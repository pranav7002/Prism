//! PRISM shared types.
//!
//! This crate defines the on-the-wire shape of everything that crosses the
//! boundary between agents, the orchestrator, the SP1 zkVM programs, and the
//! WebSocket frontend. Keep these types dependency-light and Serde-friendly.
//!
//! The SP1 programs duplicate the minimal subset they need (see each
//! `sp1-programs/*/src/main.rs`) because the RISC-V zkVM target prefers small,
//! no-std-friendly code. The duplicated structs MUST stay field-compatible
//! with the definitions in this crate.

use serde::{Deserialize, Serialize};
use tiny_keccak::{Hasher, Keccak};

/// 20-byte Ethereum address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub [u8; 20]);

impl AgentId {
    pub const ZERO: AgentId = AgentId([0u8; 20]);

    pub fn to_hex(&self) -> String {
        format!("0x{}", hex::encode(self.0))
    }
}

/// One leg of a `BatchConsolidate` removal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConsolidateRemove {
    pub pool: [u8; 20],
    pub liquidity: u128,
}

/// One leg of a `BatchConsolidate` addition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConsolidateAdd {
    pub pool: [u8; 20],
    pub amount0: u128,
    pub amount1: u128,
    pub tick_lower: i32,
    pub tick_upper: i32,
}

/// Things an agent can ask to do in one epoch.
///
/// Uses serde's default externally-tagged representation — this is the one
/// format that round-trips cleanly through both JSON (for the frontend) and
/// bincode (for the SP1 zkVM, which does not support internally-tagged
/// enums). The JSON wire shape is `{"Swap": { ... }}`.
///
/// Variants are Uniswap-V4-native per the pivot strategy:
/// - `Swap`: single-pool swap targeted by pool address and enforced via `min_out`.
/// - `AddLiquidity` / `RemoveLiquidity`: α, γ single-position ops.
/// - `MigrateLiquidity`: β moves liquidity between fee tiers (from_pool→to_pool).
/// - `BatchConsolidate`: γ removes from N stale positions and adds to M optimal ones.
/// - `SetDynamicFee`: β sets the V4 pool dynamic fee (ppm, e.g. 3000 = 0.30%).
/// - `Backrun`: δ cooperative backrun after a target tx.
/// - `CrossProtocolHedge`: ε borrows on Aave + swaps on Uniswap atomically.
/// - `DeltaHedge`: ε generic inventory hedge (kept for simple cases).
/// - `KillSwitch`: ε halts the pool when swarm IL exceeds threshold.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Swap {
        pool: [u8; 20],
        token_in: [u8; 20],
        token_out: [u8; 20],
        amount_in: u128,
        min_out: u128,
    },
    AddLiquidity {
        pool: [u8; 20],
        amount0: u128,
        amount1: u128,
        tick_lower: i32,
        tick_upper: i32,
    },
    RemoveLiquidity {
        pool: [u8; 20],
        liquidity: u128,
    },
    Backrun {
        target_tx: [u8; 32],
        profit_token: [u8; 20],
    },
    DeltaHedge {
        position_id: u64,
        delta: i64,
    },
    MigrateLiquidity {
        from_pool: [u8; 20],
        to_pool: [u8; 20],
        amount: u128,
        tick_lower: i32,
        tick_upper: i32,
    },
    BatchConsolidate {
        removes: Vec<ConsolidateRemove>,
        adds: Vec<ConsolidateAdd>,
    },
    SetDynamicFee {
        pool: [u8; 20],
        new_fee_ppm: u32,
    },
    CrossProtocolHedge {
        aave_borrow_asset: [u8; 20],
        aave_borrow_amount: u128,
        uniswap_pool: [u8; 20],
        uniswap_token_in: [u8; 20],
        uniswap_token_out: [u8; 20],
        uniswap_amount_in: u128,
    },
    KillSwitch {
        reason: String,
    },
}

impl Action {
    /// Discriminator byte used in commitment encoding. Stable across
    /// versions — do not reorder.
    pub fn discriminator(&self) -> u8 {
        match self {
            Action::Swap { .. } => 0x01,
            Action::AddLiquidity { .. } => 0x02,
            Action::RemoveLiquidity { .. } => 0x03,
            Action::Backrun { .. } => 0x04,
            Action::DeltaHedge { .. } => 0x05,
            Action::MigrateLiquidity { .. } => 0x06,
            Action::BatchConsolidate { .. } => 0x07,
            Action::SetDynamicFee { .. } => 0x08,
            Action::CrossProtocolHedge { .. } => 0x09,
            Action::KillSwitch { .. } => 0xFF,
        }
    }

    fn encode_packed(&self, out: &mut Vec<u8>) {
        out.push(self.discriminator());
        match self {
            Action::Swap {
                pool,
                token_in,
                token_out,
                amount_in,
                min_out,
            } => {
                out.extend_from_slice(pool);
                out.extend_from_slice(token_in);
                out.extend_from_slice(token_out);
                out.extend_from_slice(&amount_in.to_be_bytes());
                out.extend_from_slice(&min_out.to_be_bytes());
            }
            Action::AddLiquidity {
                pool,
                amount0,
                amount1,
                tick_lower,
                tick_upper,
            } => {
                out.extend_from_slice(pool);
                out.extend_from_slice(&amount0.to_be_bytes());
                out.extend_from_slice(&amount1.to_be_bytes());
                out.extend_from_slice(&tick_lower.to_be_bytes());
                out.extend_from_slice(&tick_upper.to_be_bytes());
            }
            Action::RemoveLiquidity { pool, liquidity } => {
                out.extend_from_slice(pool);
                out.extend_from_slice(&liquidity.to_be_bytes());
            }
            Action::Backrun {
                target_tx,
                profit_token,
            } => {
                out.extend_from_slice(target_tx);
                out.extend_from_slice(profit_token);
            }
            Action::DeltaHedge { position_id, delta } => {
                out.extend_from_slice(&position_id.to_be_bytes());
                out.extend_from_slice(&delta.to_be_bytes());
            }
            Action::MigrateLiquidity {
                from_pool,
                to_pool,
                amount,
                tick_lower,
                tick_upper,
            } => {
                out.extend_from_slice(from_pool);
                out.extend_from_slice(to_pool);
                out.extend_from_slice(&amount.to_be_bytes());
                out.extend_from_slice(&tick_lower.to_be_bytes());
                out.extend_from_slice(&tick_upper.to_be_bytes());
            }
            Action::BatchConsolidate { removes, adds } => {
                out.extend_from_slice(&(removes.len() as u32).to_be_bytes());
                for r in removes {
                    out.extend_from_slice(&r.pool);
                    out.extend_from_slice(&r.liquidity.to_be_bytes());
                }
                out.extend_from_slice(&(adds.len() as u32).to_be_bytes());
                for a in adds {
                    out.extend_from_slice(&a.pool);
                    out.extend_from_slice(&a.amount0.to_be_bytes());
                    out.extend_from_slice(&a.amount1.to_be_bytes());
                    out.extend_from_slice(&a.tick_lower.to_be_bytes());
                    out.extend_from_slice(&a.tick_upper.to_be_bytes());
                }
            }
            Action::SetDynamicFee { pool, new_fee_ppm } => {
                out.extend_from_slice(pool);
                out.extend_from_slice(&new_fee_ppm.to_be_bytes());
            }
            Action::CrossProtocolHedge {
                aave_borrow_asset,
                aave_borrow_amount,
                uniswap_pool,
                uniswap_token_in,
                uniswap_token_out,
                uniswap_amount_in,
            } => {
                out.extend_from_slice(aave_borrow_asset);
                out.extend_from_slice(&aave_borrow_amount.to_be_bytes());
                out.extend_from_slice(uniswap_pool);
                out.extend_from_slice(uniswap_token_in);
                out.extend_from_slice(uniswap_token_out);
                out.extend_from_slice(&uniswap_amount_in.to_be_bytes());
            }
            Action::KillSwitch { reason } => {
                let bytes = reason.as_bytes();
                out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                out.extend_from_slice(bytes);
            }
        }
    }
}

/// One agent's commitment for one epoch.
///
/// `commitment` is `keccak256` over a fixed, big-endian-packed encoding of the
/// other fields — see `compute_commitment`. Keeping it EVM-compatible lets the
/// on-chain commit-reveal contract verify the commitment with a single
/// `keccak256(abi.encodePacked(...))` call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentIntent {
    pub agent_id: AgentId,
    pub epoch: u64,
    pub target_protocol: String,
    pub action: Action,
    pub priority: u8,
    pub max_slippage_bps: u16,
    pub salt: [u8; 32],
    pub commitment: [u8; 32],
}

impl AgentIntent {
    /// Deterministic commitment hash over (agent_id, epoch, target_protocol,
    /// action, priority, max_slippage_bps, salt). Uses keccak256 for EVM
    /// compatibility.
    pub fn compute_commitment(&self) -> [u8; 32] {
        let mut buf: Vec<u8> = Vec::with_capacity(128);
        buf.extend_from_slice(&self.agent_id.0);
        buf.extend_from_slice(&self.epoch.to_be_bytes());
        let proto = self.target_protocol.as_bytes();
        buf.extend_from_slice(&(proto.len() as u32).to_be_bytes());
        buf.extend_from_slice(proto);
        self.action.encode_packed(&mut buf);
        buf.push(self.priority);
        buf.extend_from_slice(&self.max_slippage_bps.to_be_bytes());
        buf.extend_from_slice(&self.salt);

        let mut hasher = Keccak::v256();
        hasher.update(&buf);
        let mut out = [0u8; 32];
        hasher.finalize(&mut out);
        out
    }

    /// Returns true iff `self.commitment` matches the recomputed hash.
    pub fn verify_commitment(&self) -> bool {
        self.compute_commitment() == self.commitment
    }

    /// Construct an intent with the commitment filled in from the other
    /// fields. Convenience for tests and the mock-intents generator.
    pub fn new_with_commitment(
        agent_id: AgentId,
        epoch: u64,
        target_protocol: String,
        action: Action,
        priority: u8,
        max_slippage_bps: u16,
        salt: [u8; 32],
    ) -> Self {
        let mut intent = AgentIntent {
            agent_id,
            epoch,
            target_protocol,
            action,
            priority,
            max_slippage_bps,
            salt,
            commitment: [0u8; 32],
        };
        intent.commitment = intent.compute_commitment();
        intent
    }
}

/// An ordered execution plan: output of the off-chain solver and input to
/// execution-proof / shapley-proof.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub epoch: u64,
    pub ordered_intents: Vec<AgentIntent>,
    pub cooperative_mev_value: u128,
    /// Basis points per agent; MUST sum to 10000.
    pub shapley_weights: Vec<(AgentId, u16)>,
}

/// Snapshot of Uniswap v3/v4 pool state for one epoch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProtocolState {
    pub pool_address: [u8; 20],
    pub sqrt_price_x96: u128,
    pub liquidity: u128,
    pub tick: i32,
    pub fee_tier: u32,
    pub token0_reserve: u128,
    pub token1_reserve: u128,
    pub volatility_30d_bps: u32,
}

/// Aave-style health factor. `collateral_usd` and `debt_usd` are expressed in
/// plain USD units (u128 to accommodate large positions). `value` / `is_safe`
/// fall back to f64 only for the ratio display.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthFactor {
    pub collateral_usd: u128,
    pub debt_usd: u128,
}

impl HealthFactor {
    /// Ratio of collateral to debt. Returns `f64::INFINITY` when debt is zero.
    pub fn value(&self) -> f64 {
        if self.debt_usd == 0 {
            f64::INFINITY
        } else {
            self.collateral_usd as f64 / self.debt_usd as f64
        }
    }

    /// True when the position is above the 1.05 safety threshold.
    pub fn is_safe(&self) -> bool {
        self.value() > 1.05
    }
}

/// Request sent to the proving pipeline. Externally-tagged so bincode can
/// round-trip it into the SP1 zkVM stdin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ProofRequest {
    Solver {
        intents: Vec<AgentIntent>,
        protocol_state: ProtocolState,
    },
    Execution {
        plan: ExecutionPlan,
        protocol_state: ProtocolState,
    },
    Shapley {
        plan: ExecutionPlan,
        mev_value: u128,
    },
    Aggregate {
        solver_proof: Vec<u8>,
        execution_proof: Vec<u8>,
        shapley_proof: Vec<u8>,
    },
}

/// Events broadcast over the orchestrator WebSocket. The JSON representation
/// is an internally-tagged enum with `"type"` as the discriminator and
/// snake_case variant names, matching Dev 3's frontend consumer in
/// `INTERFACES_FOR_DEV3.md` §3.2.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    /// Sent at the start of every epoch. `timestamp` is seconds since epoch.
    EpochStart {
        epoch: u64,
        timestamp: u64,
    },
    /// Sent after collecting intents for the current epoch.
    IntentsReceived {
        count: u32,
        agents: Vec<String>,
    },
    /// Sent when the off-chain solver begins reconciliation.
    SolverRunning {
        conflicts_detected: u32,
    },
    /// Sent after the solver produces an `ExecutionPlan`.
    SolverComplete {
        plan_hash: String,
        dropped: Vec<String>,
    },
    /// Per-program proving progress (0..=100).
    ProofProgress {
        program: String,
        pct: u8,
    },
    /// A sub-proof finished. `time_ms` is wall-clock elapsed.
    ProofComplete {
        program: String,
        time_ms: u64,
    },
    /// Recursive aggregation begins.
    AggregationStart,
    /// Recursive aggregation finished.
    AggregationComplete {
        time_ms: u64,
    },
    /// Groth16 wrap progress (0..=100).
    Groth16Wrapping {
        pct: u8,
    },
    /// Settlement landed on chain (or mocked). `tx_hash` is hex string
    /// `"0x..."`. `shapley` is a vector of basis-point weights summing to
    /// 10000.
    EpochSettled {
        tx_hash: String,
        gas_used: u64,
        shapley: Vec<u16>,
    },
    /// Fatal epoch-level error.
    Error {
        message: String,
    },
}

impl WsEvent {
    /// Serialize to JSON. Panics only if serde derives are wrong (which would
    /// be a compile-time check in practice) — safe to unwrap here.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("WsEvent must serialize")
    }
}

// ---------------------------------------------------------------------------
// Wire types — JSON-facing intent shape produced by Dev 3's agent brains.
//
// `AgentIntentWire` is the shape described in `INTERFACES_FOR_DEV3.md` §3.1:
// hex-encoded byte fields, decimal-string u128 amounts, internally-tagged
// action with PascalCase `"type"` discriminator, and `expected_profit_bps`.
//
// The SP1 zkVM cannot deserialize internally-tagged enums via bincode, so we
// keep `AgentIntent` (the externally-tagged internal form) for the proving
// pipeline and convert at the agent-facing boundary.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
