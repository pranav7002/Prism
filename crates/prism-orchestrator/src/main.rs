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

mod mock_intents;
mod proving;
mod settlement;
mod uniswap_client;

use std::net::SocketAddr;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use prism_types::{AgentIntent, AgentIntentWire, WsEvent};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, Duration};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::mock_intents::{generate_mock_intents, scenario_for};
use crate::proving::{prove_epoch, ProverConfig};
use crate::settlement::SettlementConfig;
use crate::uniswap_client::MockUniswapClient;

// ---------------------------------------------------------------------------
// Incoming intent message from agents (Dev 3's Python swarm)
// ---------------------------------------------------------------------------

/// Wire-format message from agent → orchestrator over WebSocket.
///
/// The Python broadcaster sends:
/// ```json
/// { "type": "SubmitIntent", "intent": { ...AgentIntentWire... } }
/// ```
#[derive(Debug, Deserialize)]
struct WsIncoming {
    #[serde(rename = "type")]
    msg_type: String,
    intent: Option<AgentIntentWire>,
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
}

impl OrchestratorConfig {
    pub fn from_env() -> Self {
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

    let pool_address = "0x8ad599c3A0ff1De082011EFDDc58f1908eb6e6D8";
    let mock_market = MockUniswapClient;

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
        // -------------------------------------------------------------------
        let mut live_intents: Vec<AgentIntent> = Vec::new();
        let mut discarded_epoch_mismatch: u32 = 0;
        while let Ok(intent) = intent_rx.try_recv() {
            if intent.epoch == epoch {
                live_intents.push(intent);
            } else {
                discarded_epoch_mismatch += 1;
                warn!(
                    "epoch {}: discarded intent from agent {:?} (intent.epoch={}, expected={})",
                    epoch, intent.agent_id, intent.epoch, epoch,
                );
            }
        }
        if discarded_epoch_mismatch > 0 {
            info!(
                "epoch {}: discarded {} intents with wrong epoch",
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

        let protocol_state = mock_market.get_pool_state(pool_address);

        match prove_epoch(&prover, intents, protocol_state, event_tx.clone()).await {
            Ok(proof) => {
                // Gas is constant O(1) — ~260k for the Groth16 pairing check
                // plus small settlement logic.
                let gas_used: u64 = 260_000;
                // Use actual Shapley weights from the solver's execution plan
                // instead of hardcoded values.
                let shapley: Vec<u16> = proof
                    .shapley_weights
                    .iter()
                    .map(|(_, w)| *w)
                    .collect();

                // Attempt on-chain settlement if configured, else mock.
                let tx_hash_hex = if let Some(sc) = &settlement_config {
                    match settlement::settle_epoch_onchain(
                        sc, epoch, &proof.proof_bytes, &proof.public_values,
                    ).await {
                        Ok(hash) => hash,
                        Err(e) => {
                            error!("epoch {}: on-chain settlement failed: {} — using mock tx", epoch, e);
                            let mock = mock_tx_hash(epoch, &proof.proof_bytes);
                            format!("0x{}", hex::encode(mock))
                        }
                    }
                } else {
                    let mock = mock_tx_hash(epoch, &proof.proof_bytes);
                    format!("0x{}", hex::encode(mock))
                };

                let settled = WsEvent::EpochSettled {
                    tx_hash: tx_hash_hex.clone(),
                    gas_used,
                    shapley: shapley.clone(),
                };
                let _ = event_tx.send(settled);
                info!(
                    "epoch {} settled (tx {}): proof={}B gas={} shapley={:?}",
                    epoch,
                    tx_hash_hex,
                    proof.proof_bytes.len(),
                    gas_used,
                    shapley,
                );
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

