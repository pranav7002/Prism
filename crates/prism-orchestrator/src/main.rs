//! PRISM orchestrator — epoch loop + WebSocket broadcast server.
//!
//! Layout:
//!   - `run_epoch_loop` drives one epoch every `epoch_duration_secs`.
//!   - WebSocket server at `ws_bind_addr` accepts frontend connections and
//!     forwards `WsEvent`s as JSON text frames.
//!   - Events flow through a `tokio::sync::broadcast` channel so multiple
//!     clients receive the same stream without backpressure coupling.
//!   - Agents (Dev 3's Python swarm) submit intents via WS as JSON; the
//!     orchestrator parses them into `AgentIntent` and feeds them to the
//!     solver. When no live intents arrive, mock intents are used.

mod aave_client;
mod mock_intents;
mod proving;
mod settlement;
mod uniswap_client;

use std::net::SocketAddr;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use prism_types::{AgentIntent, AgentIntentWire, ProtocolState, WsEvent};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, Duration};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::aave_client::AaveClient;
use crate::mock_intents::{generate_mock_intents, scenario_for};
use crate::proving::{prove_epoch, ProverConfig};
use crate::settlement::SettlementConfig;
use crate::uniswap_client::{MockUniswapClient, UniswapClient};

// ---------------------------------------------------------------------------
// Incoming intent message from agents (Dev 3's Python swarm)
// ---------------------------------------------------------------------------

/// Wire-format message from agent → orchestrator over WebSocket.
///
/// The Python broadcaster sends:
/// ```json
/// { "type": "SubmitIntent", "intent": { ...AgentIntentWire... }, "commitment": "0x…" }
/// ```
///
/// `commitment` is the agent-supplied keccak commitment over the intent
/// payload. The orchestrator MUST verify this against the value it
/// recomputes from the wire intent — that's what makes the reveal step
/// binding (without it, an agent could submit one set of fields and claim
/// any commitment, which is the bug C7 in the audit report).
#[derive(Debug, Deserialize)]
struct WsIncoming {
    #[serde(rename = "type")]
    msg_type: String,
    intent: Option<AgentIntentWire>,
    /// 0x-prefixed 32-byte hex string. Required for `SubmitIntent`.
    commitment: Option<String>,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct OrchestratorConfig {
    pub ws_bind_addr: String,
    pub epoch_duration_secs: u64,
    pub use_mock_prover: bool,
    pub uniswap_api_url: String,
    /// Pool address fed to the (Mock|Real)UniswapClient. Required — must be
    /// set per chain (Unichain Sepolia ≠ mainnet ≠ local). No safe default.
    pub pool_address: String,
    /// Aave V3 Pool contract address. When `None`, the epoch loop uses
    /// `aave_client::fallback_healthy()` (HF=2.0) instead of an RPC call.
    pub aave_pool_address: Option<String>,
    /// JSON-RPC endpoint for the chain Aave V3 is deployed on. Defaults to
    /// `UNICHAIN_RPC_URL` for local dev, but production should point this at
    /// Sepolia / OP Sepolia / mainnet (Aave is not on Unichain).
    pub aave_rpc_url: String,
    /// User account whose health factor we monitor. `None` ⇒ fallback.
    pub aave_user_address: Option<String>,
}

impl OrchestratorConfig {
    pub fn from_env() -> Self {
        let unichain_rpc = std::env::var("UNICHAIN_RPC_URL").unwrap_or_default();
        Self {
            ws_bind_addr: std::env::var("WS_BIND_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8765".into()),
            epoch_duration_secs: std::env::var("EPOCH_DURATION_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(12),
            use_mock_prover: std::env::var("USE_MOCK_PROVER")
                .map(|v| v != "0" && v.to_lowercase() != "false")
                .unwrap_or(true),
            uniswap_api_url: std::env::var("UNISWAP_API_URL")
                .unwrap_or_else(|_| "https://api.uniswap.org".into()),
            pool_address: std::env::var("POOL_ADDRESS").unwrap_or_else(|_| {
                // Sentinel zero address. Real deployments MUST set
                // POOL_ADDRESS — the orchestrator logs a warning at startup
                // when it sees this default. Using mainnet USDC/WETH as a
                // silent fallback (the previous behavior) is wrong on every
                // testnet and produced misleading mock pool data.
                "0x0000000000000000000000000000000000000000".into()
            }),
            aave_pool_address: std::env::var("AAVE_POOL_ADDRESS").ok(),
            aave_rpc_url: std::env::var("AAVE_RPC_URL").unwrap_or(unichain_rpc),
            aave_user_address: std::env::var("AAVE_USER_ADDRESS").ok(),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Arc::new(OrchestratorConfig::from_env());
    info!(
        "PRISM Orchestrator starting — ws={} epoch={}s mock_prover={}",
        config.ws_bind_addr, config.epoch_duration_secs, config.use_mock_prover
    );

    let (event_tx, _event_rx) = broadcast::channel::<WsEvent>(100);

    // Channel for live agent intents submitted over WebSocket.
    // Buffer 64 intents per epoch — more than enough for 5 agents.
    let (intent_tx, intent_rx) = mpsc::channel::<AgentIntent>(64);

    // On-chain settlement config (None = mock mode).
    let settlement_config = SettlementConfig::from_env().map(Arc::new);
    if settlement_config.is_none() {
        warn!("settlement: PRISM_HOOK_ADDRESS / PRIVATE_KEY / UNICHAIN_RPC_URL not set — running in mock settlement mode");
    }

    // Spawn the epoch loop.
    {
        let config = config.clone();
        let event_tx = event_tx.clone();
        let sc = settlement_config.clone();
        tokio::spawn(async move {
            run_epoch_loop(config, event_tx, intent_rx, sc).await;
        });
    }

    // WebSocket server.
    let listener = TcpListener::bind(&config.ws_bind_addr).await?;
    info!("WebSocket server listening on {}", config.ws_bind_addr);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let rx = event_tx.subscribe();
                let itx = intent_tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_ws_connection(stream, addr, rx, itx).await {
                        warn!("ws connection {} closed with error: {}", addr, e);
                    }
                });
            }
            Err(e) => {
                error!("accept failed: {}", e);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Epoch loop
// ---------------------------------------------------------------------------

async fn run_epoch_loop(
    config: Arc<OrchestratorConfig>,
    event_tx: broadcast::Sender<WsEvent>,
    mut intent_rx: mpsc::Receiver<AgentIntent>,
    settlement_config: Option<Arc<SettlementConfig>>,
) {
    let prover = ProverConfig::from_compiled(config.use_mock_prover);
    let mut tick = interval(Duration::from_secs(config.epoch_duration_secs));
    let mut epoch: u64 = 1;

    let pool_address = config.pool_address.as_str();
    if pool_address == "0x0000000000000000000000000000000000000000" {
        warn!("POOL_ADDRESS not set — running on a sentinel zero address; live UniswapClient calls will fail and the orchestrator will fall back to mock pool state every epoch");
    }
    let market = UniswapClient::new(&config.uniswap_api_url);
    let mock_market = MockUniswapClient; // cold-start / hard-failure fallback
    let mut last_known_state: Option<ProtocolState> = None;

    let aave_client_opt: Option<AaveClient> = config
        .aave_pool_address
        .as_deref()
        .map(|addr| AaveClient::new(&config.aave_rpc_url, addr));

    // Intents that arrived during epoch N tagged with epoch N+1 — held
    // until that tick fires. Without this buffer, agents that broadcast
    // slightly ahead of the orchestrator's tick get their intents
    // silently dropped (H1 in AUDIT_REPORT). Cleared at start of each
    // tick after merging into the current epoch's live_intents.
    let mut next_epoch_buffer: Vec<AgentIntent> = Vec::new();

    loop {
        tick.tick().await;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        info!("=== epoch {} starting ({}) ===", epoch, scenario_for(epoch));
        let _ = event_tx.send(WsEvent::EpochStart { epoch, timestamp });

        // -------------------------------------------------------------------
        // Drain live intents from the WS channel; fall back to mock if none.
        //
        // Commit window: accept intent.epoch ∈ {epoch, epoch+1}. The current
        // epoch goes into live_intents now; the next-epoch ones get buffered
        // for the next tick. Anything outside that two-epoch window is a
        // genuine misbehavior or a long network stall — log and drop.
        // -------------------------------------------------------------------
        let mut live_intents: Vec<AgentIntent> = std::mem::take(&mut next_epoch_buffer)
            .into_iter()
            .filter(|i| i.epoch == epoch)
            .collect();
        let mut discarded_epoch_mismatch: u32 = 0;
        while let Ok(intent) = intent_rx.try_recv() {
            if intent.epoch == epoch {
                live_intents.push(intent);
            } else if intent.epoch == epoch + 1 {
                // Agent ran slightly ahead — hold for next tick.
                next_epoch_buffer.push(intent);
            } else {
                discarded_epoch_mismatch += 1;
                warn!(
                    "epoch {}: discarded intent from agent {:?} (intent.epoch={}, expected {} or {})",
                    epoch, intent.agent_id, intent.epoch, epoch, epoch + 1,
                );
            }
        }
        if !next_epoch_buffer.is_empty() {
            info!(
                "epoch {}: buffered {} intent(s) for epoch {}",
                epoch, next_epoch_buffer.len(), epoch + 1,
            );
        }
        if discarded_epoch_mismatch > 0 {
            info!(
                "epoch {}: discarded {} intents with out-of-window epoch",
                epoch, discarded_epoch_mismatch,
            );
        }

        let intents = if live_intents.is_empty() {
            info!("epoch {}: no live intents — using mock intents ({})", epoch, scenario_for(epoch));
            generate_mock_intents(epoch)
        } else {
            info!("epoch {}: using {} live intents from agents", epoch, live_intents.len());
            live_intents
        };

        // Role labels follow the emission order of `generate_mock_intents`.
        // In crisis epochs ε emits two intents (CrossProtocolHedge, then
        // KillSwitch), so the list is one longer.
        let base_labels = ["α", "β", "γ", "δ", "ε"];
        let agent_labels: Vec<String> = intents
            .iter()
            .enumerate()
            .map(|(i, _)| base_labels.get(i).copied().unwrap_or("ε").to_string())
            .collect();
        let _ = event_tx.send(WsEvent::IntentsReceived {
            count: intents.len() as u32,
            agents: agent_labels,
        });

        let protocol_state = match market.get_pool_state(pool_address).await {
            Ok(s) => {
                last_known_state = Some(s.clone());
                s
            }
            Err(e) => {
                warn!(
                    "epoch {}: pool state fetch failed: {} — using fallback",
                    epoch, e
                );
                last_known_state
                    .clone()
                    .unwrap_or_else(|| mock_market.get_pool_state(pool_address))
            }
        };

        let health = match (&aave_client_opt, &config.aave_user_address) {
            (Some(aave), Some(user)) => aave.get_health_factor(user).await.unwrap_or_else(|e| {
                warn!(
                    "epoch {}: HF fetch failed: {} — falling back to healthy",
                    epoch, e
                );
                aave_client::fallback_healthy()
            }),
            _ => aave_client::fallback_healthy(),
        };

        // `real_prover` arg for Agent B's new signature: always `None` from
        // the orchestrator side. Construction lives behind the `real-prover`
        // feature inside proving.rs (Agent B owns it). Passing `None` lets
        // the call-site type-infer to whatever Option<Arc<RealProver>> Agent
        // B's signature declares — works for both feature configurations.
        match prove_epoch(
            &prover,
            None,
            intents,
            protocol_state,
            health,
            event_tx.clone(),
        )
        .await
        {
            Ok(outcome) => {
                let shapley: Vec<u16> = outcome
                    .shapley_weights()
                    .iter()
                    .map(|(_, w)| *w)
                    .collect();

                match outcome {
                    proving::AggregateOutcome::Wrapped(proof) => {
                        // Gas is constant O(1) — ~260k for the Groth16 pairing
                        // check plus small settlement logic.
                        let gas_used: u64 = 260_000;
                        let tx_hash_hex = if let Some(sc) = &settlement_config {
                            match settlement::settle_epoch_onchain(
                                sc, epoch, &proof.proof_bytes, &proof.public_values,
                            ).await {
                                Ok(hash) => hash,
                                Err(e) => {
                                    error!(
                                        "epoch {}: on-chain settlement failed: {} — using mock tx",
                                        epoch, e
                                    );
                                    let mock = mock_tx_hash(epoch, &proof.proof_bytes);
                                    format!("0x{}", hex::encode(mock))
                                }
                            }
                        } else {
                            let mock = mock_tx_hash(epoch, &proof.proof_bytes);
                            format!("0x{}", hex::encode(mock))
                        };

                        let _ = event_tx.send(WsEvent::EpochSettled {
                            tx_hash: tx_hash_hex.clone(),
                            gas_used,
                            shapley: shapley.clone(),
                        });
                        info!(
                            "epoch {} settled (tx {}): proof={}B gas={} shapley={:?}",
                            epoch,
                            tx_hash_hex,
                            proof.proof_bytes.len(),
                            gas_used,
                            shapley,
                        );
                    }
                    proving::AggregateOutcome::PlanB(p) => {
                        // Plan-B settles via three independent sub-proof
                        // verifications. Gas is ~3× the Groth16 path.
                        let gas_used: u64 = 480_000;
                        let tx_hash_hex = if let Some(sc) = &settlement_config {
                            match settlement::settle_epoch_three_proof_onchain(
                                sc,
                                p.epoch,
                                &p.solver.proof_bytes, &p.solver.public_values,
                                &p.execution.proof_bytes, &p.execution.public_values,
                                &p.shapley.proof_bytes, &p.shapley.public_values,
                                &p.payouts_bps,
                            ).await {
                                Ok(hash) => hash,
                                Err(e) => {
                                    error!(
                                        "epoch {}: Plan-B settlement failed: {} — using mock tx",
                                        epoch, e
                                    );
                                    let mock = mock_tx_hash(epoch, &p.solver.proof_bytes);
                                    format!("0x{}", hex::encode(mock))
                                }
                            }
                        } else {
                            let mock = mock_tx_hash(epoch, &p.solver.proof_bytes);
                            format!("0x{}", hex::encode(mock))
                        };

                        let _ = event_tx.send(WsEvent::EpochSettledViaPlanB {
                            tx_hash: tx_hash_hex.clone(),
                            gas_used,
                            shapley: shapley.clone(),
                        });
                        info!(
                            "epoch {} settled via Plan-B (tx {}): solver={}B exec={}B shapley_proof={}B gas={} shapley={:?}",
                            epoch,
                            tx_hash_hex,
                            p.solver.proof_bytes.len(),
                            p.execution.proof_bytes.len(),
                            p.shapley.proof_bytes.len(),
                            gas_used,
                            shapley,
                        );
                    }
                }
            }
            Err(e) => {
                error!("epoch {} failed: {}", epoch, e);
                let _ = event_tx.send(WsEvent::Error {
                    message: format!("epoch {} failed: {}", epoch, e),
                });
            }
        }

        epoch += 1;
    }
}

fn mock_tx_hash(epoch: u64, proof_bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"prism-mock-tx");
    h.update(epoch.to_be_bytes());
    h.update(proof_bytes);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

// ---------------------------------------------------------------------------
// WebSocket connection handler
// ---------------------------------------------------------------------------

async fn handle_ws_connection(
    stream: tokio::net::TcpStream,
    addr: SocketAddr,
    mut rx: broadcast::Receiver<WsEvent>,
    intent_tx: mpsc::Sender<AgentIntent>,
) -> anyhow::Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    info!("ws client connected: {}", addr);

    let (mut ws_sink, mut ws_source) = ws_stream.split();

    loop {
        tokio::select! {
            incoming = ws_source.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => {
                        info!("ws client disconnected: {}", addr);
                        return Ok(());
                    }
                    Some(Ok(Message::Ping(p))) => {
                        ws_sink.send(Message::Pong(p)).await?;
                    }
                    Some(Ok(Message::Text(text))) => {
                        handle_incoming_text(&text, addr, &intent_tx).await;
                    }
                    Some(Ok(_)) => {
                        // Binary / other frames — ignore.
                    }
                    Some(Err(e)) => return Err(e.into()),
                }
            }
            event = rx.recv() => {
                match event {
                    Ok(e) => {
                        ws_sink.send(Message::Text(e.to_json())).await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("ws client {} lagged, dropped {} events", addr, n);
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Parse an incoming text message from a WS client. If it's a valid
/// `SubmitIntent`, convert the wire-format intent to an internal
/// `AgentIntent` and push it into the mpsc channel for the epoch loop.
async fn handle_incoming_text(
    text: &str,
    addr: SocketAddr,
    intent_tx: &mpsc::Sender<AgentIntent>,
) {
    let msg: WsIncoming = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            warn!("ws {}: unparseable message: {}", addr, e);
            return;
        }
    };

    if msg.msg_type != "SubmitIntent" {
        // Silently ignore non-intent messages (e.g. pings, heartbeats).
        return;
    }

    let wire = match msg.intent {
        Some(w) => w,
        None => {
            warn!("ws {}: SubmitIntent missing 'intent' field", addr);
            return;
        }
    };

    let agent_id = wire.agent_id.clone();

    // Parse the envelope-level commitment the agent supplied. We compare it
    // against the value `to_internal` recomputes from the wire fields. If
    // the agent sent fields that don't hash to the commitment they claimed,
    // reject — that's an integrity failure or a different intent being
    // smuggled in under a previously-revealed commitment.
    let claimed_commitment: [u8; 32] = match msg.commitment.as_deref() {
        Some(hex_str) => {
            let stripped = hex_str.strip_prefix("0x").unwrap_or(hex_str);
            match hex::decode(stripped).ok().and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()) {
                Some(arr) => arr,
                None => {
                    warn!(
                        "ws {}: intent from {} has malformed commitment hex — rejected",
                        addr, agent_id
                    );
                    return;
                }
            }
        }
        None => {
            warn!(
                "ws {}: SubmitIntent from {} missing 'commitment' field — rejected",
                addr, agent_id
            );
            return;
        }
    };

    match wire.to_internal() {
        Ok(internal) => {
            if internal.commitment != claimed_commitment {
                warn!(
                    "ws {}: intent from {} commitment mismatch — claimed=0x{} computed=0x{} — rejected",
                    addr,
                    agent_id,
                    hex::encode(claimed_commitment),
                    hex::encode(internal.commitment),
                );
                return;
            }
            info!(
                "ws {}: accepted intent from agent {} (epoch={}, priority={})",
                addr, agent_id, internal.epoch, internal.priority
            );
            if let Err(e) = intent_tx.send(internal).await {
                error!("ws {}: intent channel send failed: {}", addr, e);
            }
        }
        Err(e) => {
            warn!("ws {}: invalid wire intent from {}: {}", addr, agent_id, e);
        }
    }
}
