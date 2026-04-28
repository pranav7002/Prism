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

    /// Construct a `HealthFactor` from Aave V3's actual `healthFactor`
    /// return field (1e18-scaled). This is what slot 5 of
    /// `getUserAccountData` returns — the *real* health factor that
    /// accounts for per-asset liquidation thresholds, not the naive
    /// `totalCollateralBase / totalDebtBase` ratio.
    ///
    /// Encodes the value so `value()` recovers the original HF without
    /// changing the wire shape (closes H7 in Audit report). Field
    /// names `collateral_usd` / `debt_usd` are kept for backward
    /// compatibility but contain the e18-scaled numerator/denominator
    /// rather than literal USD amounts when this constructor is used.
    pub fn from_aave_e18(hf_e18: u128) -> Self {
        Self {
            collateral_usd: hf_e18,
            debt_usd: 1_000_000_000_000_000_000, // 1e18
        }
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
pub struct AgentIntentWire {
    pub agent_id: String,
    pub epoch: u64,
    pub target_protocol: String,
    pub action: ActionWire,
    pub priority: u8,
    pub max_slippage_bps: u16,
    #[serde(default)]
    pub expected_profit_bps: u16,
    pub salt: String,
}

/// Wire form of `ConsolidateRemove`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConsolidateRemoveWire {
    pub pool: String,
    pub liquidity: String,
}

/// Wire form of `ConsolidateAdd`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConsolidateAddWire {
    pub pool: String,
    pub amount0: String,
    pub amount1: String,
    pub tick_lower: i32,
    pub tick_upper: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ActionWire {
    Swap {
        pool: String,
        token_in: String,
        token_out: String,
        amount_in: String,
        min_out: String,
    },
    AddLiquidity {
        pool: String,
        amount0: String,
        amount1: String,
        tick_lower: i32,
        tick_upper: i32,
    },
    RemoveLiquidity {
        pool: String,
        liquidity: String,
    },
    Backrun {
        target_tx: String,
        profit_token: String,
    },
    DeltaHedge {
        position_id: u64,
        delta: i64,
    },
    MigrateLiquidity {
        from_pool: String,
        to_pool: String,
        amount: String,
        tick_lower: i32,
        tick_upper: i32,
    },
    BatchConsolidate {
        removes: Vec<ConsolidateRemoveWire>,
        adds: Vec<ConsolidateAddWire>,
    },
    SetDynamicFee {
        pool: String,
        new_fee_ppm: u32,
    },
    CrossProtocolHedge {
        aave_borrow_asset: String,
        aave_borrow_amount: String,
        uniswap_pool: String,
        uniswap_token_in: String,
        uniswap_token_out: String,
        uniswap_amount_in: String,
    },
    KillSwitch {
        reason: String,
    },
}

/// Errors returned by wire → internal conversion.
#[derive(Debug)]
pub enum WireError {
    BadHex(String),
    BadLength { field: &'static str, expected: usize, got: usize },
    BadDecimal(String),
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::BadHex(s) => write!(f, "bad hex: {}", s),
            WireError::BadLength { field, expected, got } => {
                write!(f, "bad length for {}: expected {}, got {}", field, expected, got)
            }
            WireError::BadDecimal(s) => write!(f, "bad decimal: {}", s),
        }
    }
}

impl std::error::Error for WireError {}

fn decode_fixed<const N: usize>(s: &str, field: &'static str) -> Result<[u8; N], WireError> {
    let trimmed = s.trim_start_matches("0x");
    let bytes = hex::decode(trimmed).map_err(|_| WireError::BadHex(s.to_string()))?;
    if bytes.len() != N {
        return Err(WireError::BadLength {
            field,
            expected: N,
            got: bytes.len(),
        });
    }
    let mut out = [0u8; N];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn encode_hex(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn parse_u128(s: &str) -> Result<u128, WireError> {
    s.parse::<u128>().map_err(|_| WireError::BadDecimal(s.to_string()))
}

impl AgentIntentWire {
    /// Convert to the internal, bincode-friendly `AgentIntent`. The returned
    /// intent's `commitment` is recomputed from the parsed fields — the wire
    /// form does not include an explicit commitment (it is derived).
    pub fn to_internal(&self) -> Result<AgentIntent, WireError> {
        let agent_id = AgentId(decode_fixed::<20>(&self.agent_id, "agent_id")?);
        let salt = decode_fixed::<32>(&self.salt, "salt")?;
        let action = self.action.to_internal()?;
        let intent = AgentIntent::new_with_commitment(
            agent_id,
            self.epoch,
            self.target_protocol.clone(),
            action,
            self.priority,
            self.max_slippage_bps,
            salt,
        );
        Ok(intent)
    }
}

impl ActionWire {
    fn to_internal(&self) -> Result<Action, WireError> {
        Ok(match self {
            ActionWire::Swap {
                pool,
                token_in,
                token_out,
                amount_in,
                min_out,
            } => Action::Swap {
                pool: decode_fixed::<20>(pool, "pool")?,
                token_in: decode_fixed::<20>(token_in, "token_in")?,
                token_out: decode_fixed::<20>(token_out, "token_out")?,
                amount_in: parse_u128(amount_in)?,
                min_out: parse_u128(min_out)?,
            },
            ActionWire::AddLiquidity {
                pool,
                amount0,
                amount1,
                tick_lower,
                tick_upper,
            } => Action::AddLiquidity {
                pool: decode_fixed::<20>(pool, "pool")?,
                amount0: parse_u128(amount0)?,
                amount1: parse_u128(amount1)?,
                tick_lower: *tick_lower,
                tick_upper: *tick_upper,
            },
            ActionWire::RemoveLiquidity { pool, liquidity } => Action::RemoveLiquidity {
                pool: decode_fixed::<20>(pool, "pool")?,
                liquidity: parse_u128(liquidity)?,
            },
            ActionWire::Backrun {
                target_tx,
                profit_token,
            } => Action::Backrun {
                target_tx: decode_fixed::<32>(target_tx, "target_tx")?,
                profit_token: decode_fixed::<20>(profit_token, "profit_token")?,
            },
            ActionWire::DeltaHedge { position_id, delta } => Action::DeltaHedge {
                position_id: *position_id,
                delta: *delta,
            },
            ActionWire::MigrateLiquidity {
                from_pool,
                to_pool,
                amount,
                tick_lower,
                tick_upper,
            } => Action::MigrateLiquidity {
                from_pool: decode_fixed::<20>(from_pool, "from_pool")?,
                to_pool: decode_fixed::<20>(to_pool, "to_pool")?,
                amount: parse_u128(amount)?,
                tick_lower: *tick_lower,
                tick_upper: *tick_upper,
            },
            ActionWire::BatchConsolidate { removes, adds } => {
                let mut rs = Vec::with_capacity(removes.len());
                for r in removes {
                    rs.push(ConsolidateRemove {
                        pool: decode_fixed::<20>(&r.pool, "pool")?,
                        liquidity: parse_u128(&r.liquidity)?,
                    });
                }
                let mut as_ = Vec::with_capacity(adds.len());
                for a in adds {
                    as_.push(ConsolidateAdd {
                        pool: decode_fixed::<20>(&a.pool, "pool")?,
                        amount0: parse_u128(&a.amount0)?,
                        amount1: parse_u128(&a.amount1)?,
                        tick_lower: a.tick_lower,
                        tick_upper: a.tick_upper,
                    });
                }
                Action::BatchConsolidate {
                    removes: rs,
                    adds: as_,
                }
            }
            ActionWire::SetDynamicFee { pool, new_fee_ppm } => Action::SetDynamicFee {
                pool: decode_fixed::<20>(pool, "pool")?,
                new_fee_ppm: *new_fee_ppm,
            },
            ActionWire::CrossProtocolHedge {
                aave_borrow_asset,
                aave_borrow_amount,
                uniswap_pool,
                uniswap_token_in,
                uniswap_token_out,
                uniswap_amount_in,
            } => Action::CrossProtocolHedge {
                aave_borrow_asset: decode_fixed::<20>(aave_borrow_asset, "aave_borrow_asset")?,
                aave_borrow_amount: parse_u128(aave_borrow_amount)?,
                uniswap_pool: decode_fixed::<20>(uniswap_pool, "uniswap_pool")?,
                uniswap_token_in: decode_fixed::<20>(uniswap_token_in, "uniswap_token_in")?,
                uniswap_token_out: decode_fixed::<20>(uniswap_token_out, "uniswap_token_out")?,
                uniswap_amount_in: parse_u128(uniswap_amount_in)?,
            },
            ActionWire::KillSwitch { reason } => Action::KillSwitch {
                reason: reason.clone(),
            },
        })
    }
}

impl From<&AgentIntent> for AgentIntentWire {
    fn from(intent: &AgentIntent) -> Self {
        AgentIntentWire {
            agent_id: encode_hex(&intent.agent_id.0),
            epoch: intent.epoch,
            target_protocol: intent.target_protocol.clone(),
            action: (&intent.action).into(),
            priority: intent.priority,
            max_slippage_bps: intent.max_slippage_bps,
            expected_profit_bps: 0,
            salt: encode_hex(&intent.salt),
        }
    }
}

impl From<&Action> for ActionWire {
    fn from(a: &Action) -> Self {
        match a {
            Action::Swap {
                pool,
                token_in,
                token_out,
                amount_in,
                min_out,
            } => ActionWire::Swap {
                pool: encode_hex(pool),
                token_in: encode_hex(token_in),
                token_out: encode_hex(token_out),
                amount_in: amount_in.to_string(),
                min_out: min_out.to_string(),
            },
            Action::AddLiquidity {
                pool,
                amount0,
                amount1,
                tick_lower,
                tick_upper,
            } => ActionWire::AddLiquidity {
                pool: encode_hex(pool),
                amount0: amount0.to_string(),
                amount1: amount1.to_string(),
                tick_lower: *tick_lower,
                tick_upper: *tick_upper,
            },
            Action::RemoveLiquidity { pool, liquidity } => ActionWire::RemoveLiquidity {
                pool: encode_hex(pool),
                liquidity: liquidity.to_string(),
            },
            Action::Backrun {
                target_tx,
                profit_token,
            } => ActionWire::Backrun {
                target_tx: encode_hex(target_tx),
                profit_token: encode_hex(profit_token),
            },
            Action::DeltaHedge { position_id, delta } => ActionWire::DeltaHedge {
                position_id: *position_id,
                delta: *delta,
            },
            Action::MigrateLiquidity {
                from_pool,
                to_pool,
                amount,
                tick_lower,
                tick_upper,
            } => ActionWire::MigrateLiquidity {
                from_pool: encode_hex(from_pool),
                to_pool: encode_hex(to_pool),
                amount: amount.to_string(),
                tick_lower: *tick_lower,
                tick_upper: *tick_upper,
            },
            Action::BatchConsolidate { removes, adds } => ActionWire::BatchConsolidate {
                removes: removes
                    .iter()
                    .map(|r| ConsolidateRemoveWire {
                        pool: encode_hex(&r.pool),
                        liquidity: r.liquidity.to_string(),
                    })
                    .collect(),
                adds: adds
                    .iter()
                    .map(|a| ConsolidateAddWire {
                        pool: encode_hex(&a.pool),
                        amount0: a.amount0.to_string(),
                        amount1: a.amount1.to_string(),
                        tick_lower: a.tick_lower,
                        tick_upper: a.tick_upper,
                    })
                    .collect(),
            },
            Action::SetDynamicFee { pool, new_fee_ppm } => ActionWire::SetDynamicFee {
                pool: encode_hex(pool),
                new_fee_ppm: *new_fee_ppm,
            },
            Action::CrossProtocolHedge {
                aave_borrow_asset,
                aave_borrow_amount,
                uniswap_pool,
                uniswap_token_in,
                uniswap_token_out,
                uniswap_amount_in,
            } => ActionWire::CrossProtocolHedge {
                aave_borrow_asset: encode_hex(aave_borrow_asset),
                aave_borrow_amount: aave_borrow_amount.to_string(),
                uniswap_pool: encode_hex(uniswap_pool),
                uniswap_token_in: encode_hex(uniswap_token_in),
                uniswap_token_out: encode_hex(uniswap_token_out),
                uniswap_amount_in: uniswap_amount_in.to_string(),
            },
            Action::KillSwitch { reason } => ActionWire::KillSwitch {
                reason: reason.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_intent() -> AgentIntent {
        AgentIntent::new_with_commitment(
            AgentId([0xAA; 20]),
            42,
            "Uniswap".into(),
            Action::Swap {
                pool: [0xDD; 20],
                token_in: [0x11; 20],
                token_out: [0x22; 20],
                amount_in: 1_000_000_000_000_000_000u128,
                min_out: 330_000_000_000_000_000u128,
            },
            80,
            50,
            [0x55; 32],
        )
    }

    #[test]
    fn commitment_is_deterministic() {
        let a = sample_intent();
        let b = sample_intent();
        assert_eq!(a.commitment, b.commitment);
        assert!(a.verify_commitment());
    }

    #[test]
    fn commitment_changes_with_salt() {
        let a = sample_intent();
        let mut b = a.clone();
        b.salt = [0x66; 32];
        b.commitment = b.compute_commitment();
        assert_ne!(a.commitment, b.commitment);
    }

    #[test]
    fn health_factor_thresholds() {
        let safe = HealthFactor {
            collateral_usd: 2_000_000,
            debt_usd: 1_000_000,
        };
        assert!((safe.value() - 2.0).abs() < 1e-9);
        assert!(safe.is_safe());

        let borderline = HealthFactor {
            collateral_usd: 1_040_000,
            debt_usd: 1_000_000,
        };
        assert!(!borderline.is_safe());

        let zero_debt = HealthFactor {
            collateral_usd: 1,
            debt_usd: 0,
        };
        assert!(zero_debt.value().is_infinite());
        assert!(zero_debt.is_safe());
    }

    #[test]
    fn ws_event_json_shape() {
        let e = WsEvent::ProofProgress {
            program: "solver".into(),
            pct: 75,
        };
        let s = e.to_json();
        assert!(s.contains(r#""type":"proof_progress""#));
        assert!(s.contains(r#""program":"solver""#));
        assert!(s.contains(r#""pct":75"#));

        let round: WsEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(round, e);
    }

    #[test]
    fn action_json_is_externally_tagged() {
        // Dev 3 documented JSON shape: {"Swap": {...}}. Externally-tagged
        // (serde default) is required because bincode — used by the SP1
        // zkVM — cannot deserialize internally-tagged enums.
        let a = Action::Swap {
            pool: [0xDD; 20],
            token_in: [0x11; 20],
            token_out: [0x22; 20],
            amount_in: 1_000_000,
            min_out: 990_000,
        };
        let s = serde_json::to_string(&a).unwrap();
        assert!(s.starts_with(r#"{"Swap":{"pool":"#), "shape was: {}", s);
        let round: Action = serde_json::from_str(&s).unwrap();
        assert_eq!(round, a);
    }

    #[test]
    fn ws_event_epoch_settled_roundtrip() {
        let e = WsEvent::EpochSettled {
            tx_hash: "0xabcd".into(),
            gas_used: 260_000,
            shapley: vec![4000, 2500, 2000, 1500, 0],
        };
        let s = e.to_json();
        assert!(s.contains(r#""type":"epoch_settled""#));
        assert!(s.contains(r#""gas_used":260000"#));
        let round: WsEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(round, e);
    }

    #[test]
    fn ws_event_all_dev3_variants_roundtrip() {
        let cases: Vec<WsEvent> = vec![
            WsEvent::EpochStart { epoch: 5, timestamp: 1719000000 },
            WsEvent::IntentsReceived { count: 5, agents: vec!["α".into(), "β".into()] },
            WsEvent::SolverRunning { conflicts_detected: 2 },
            WsEvent::SolverComplete { plan_hash: "0xabcd".into(), dropped: vec!["ε".into()] },
            WsEvent::ProofComplete { program: "solver".into(), time_ms: 30_000 },
            WsEvent::AggregationStart,
            WsEvent::AggregationComplete { time_ms: 60_000 },
            WsEvent::Groth16Wrapping { pct: 50 },
        ];
        for e in cases {
            let s = e.to_json();
            let round: WsEvent = serde_json::from_str(&s).unwrap();
            assert_eq!(round, e, "roundtrip failed for {:?}", e);
        }
    }

    #[test]
    fn wire_intent_roundtrips_are_now_lossless() {
        // With the Uniswap V4 pivot, Swap carries `pool` + `min_out` in both
        // wire and internal forms. The wire↔internal conversion is now fully
        // lossless — commitment must survive byte-for-byte.
        let internal = sample_intent();
        let wire: AgentIntentWire = (&internal).into();
        assert!(wire.agent_id.starts_with("0x"));
        assert_eq!(wire.agent_id.len(), 2 + 40);
        assert!(wire.salt.starts_with("0x"));
        assert_eq!(wire.salt.len(), 2 + 64);
        assert!(matches!(wire.action, ActionWire::Swap { .. }));

        let round = wire.to_internal().unwrap();
        assert_eq!(round, internal);
    }

    #[test]
    fn wire_intent_type_tag_is_pascal_case() {
        let internal = sample_intent();
        let wire: AgentIntentWire = (&internal).into();
        let json = serde_json::to_string(&wire).unwrap();
        assert!(json.contains(r#""type":"Swap""#), "got: {}", json);
        assert!(json.contains(r#""agent_id":"0xaaaaaaaa"#), "got: {}", json);
        assert!(json.contains(r#""amount_in":"1000000000000000000""#), "got: {}", json);
    }

    #[test]
    fn action_discriminators_are_unique() {
        let actions = [
            Action::Swap {
                pool: [0; 20],
                token_in: [0; 20],
                token_out: [0; 20],
                amount_in: 0,
                min_out: 0,
            },
            Action::AddLiquidity {
                pool: [0; 20],
                amount0: 0,
                amount1: 0,
                tick_lower: 0,
                tick_upper: 0,
            },
            Action::RemoveLiquidity {
                pool: [0; 20],
                liquidity: 0,
            },
            Action::Backrun {
                target_tx: [0; 32],
                profit_token: [0; 20],
            },
            Action::DeltaHedge {
                position_id: 0,
                delta: 0,
            },
            Action::MigrateLiquidity {
                from_pool: [0; 20],
                to_pool: [0; 20],
                amount: 0,
                tick_lower: 0,
                tick_upper: 0,
            },
            Action::BatchConsolidate {
                removes: vec![],
                adds: vec![],
            },
            Action::SetDynamicFee {
                pool: [0; 20],
                new_fee_ppm: 0,
            },
            Action::CrossProtocolHedge {
                aave_borrow_asset: [0; 20],
                aave_borrow_amount: 0,
                uniswap_pool: [0; 20],
                uniswap_token_in: [0; 20],
                uniswap_token_out: [0; 20],
                uniswap_amount_in: 0,
            },
            Action::KillSwitch {
                reason: "".into(),
            },
        ];
        let mut seen = std::collections::HashSet::new();
        for a in &actions {
            assert!(seen.insert(a.discriminator()));
        }
        assert_eq!(actions.len(), 10);
    }

    #[test]
    fn migrate_liquidity_roundtrips() {
        let internal = AgentIntent::new_with_commitment(
            AgentId([0xA1; 20]),
            7,
            "Uniswap".into(),
            Action::MigrateLiquidity {
                from_pool: [0x11; 20],
                to_pool: [0x22; 20],
                amount: 200_000_000_000u128,
                tick_lower: 190_000,
                tick_upper: 210_000,
            },
            75,
            50,
            [0x77; 32],
        );
        let wire: AgentIntentWire = (&internal).into();
        let json = serde_json::to_string(&wire).unwrap();
        assert!(json.contains(r#""type":"MigrateLiquidity""#), "got: {}", json);
        let round = wire.to_internal().unwrap();
        assert_eq!(round, internal);
    }

    #[test]
    fn batch_consolidate_roundtrips_with_nested_vecs() {
        let internal = AgentIntent::new_with_commitment(
            AgentId([0xA2; 20]),
            7,
            "Uniswap".into(),
            Action::BatchConsolidate {
                removes: vec![
                    ConsolidateRemove {
                        pool: [0x11; 20],
                        liquidity: 100,
                    },
                    ConsolidateRemove {
                        pool: [0x22; 20],
                        liquidity: 200,
                    },
                ],
                adds: vec![ConsolidateAdd {
                    pool: [0x33; 20],
                    amount0: 1_000,
                    amount1: 2_000,
                    tick_lower: -100,
                    tick_upper: 100,
                }],
            },
            55,
            50,
            [0x88; 32],
        );
        let wire: AgentIntentWire = (&internal).into();
        let round = wire.to_internal().unwrap();
        assert_eq!(round, internal);
    }

    #[test]
    fn cross_protocol_hedge_roundtrips() {
        let internal = AgentIntent::new_with_commitment(
            AgentId([0xA4; 20]),
            7,
            "Uniswap+Aave".into(),
            Action::CrossProtocolHedge {
                aave_borrow_asset: [0x11; 20],
                aave_borrow_amount: 1_200_000_000_000_000_000u128,
                uniswap_pool: [0x22; 20],
                uniswap_token_in: [0x33; 20],
                uniswap_token_out: [0x44; 20],
                uniswap_amount_in: 1_200_000_000_000_000_000u128,
            },
            85,
            50,
            [0x99; 32],
        );
        let wire: AgentIntentWire = (&internal).into();
        let round = wire.to_internal().unwrap();
        assert_eq!(round, internal);
    }

    #[test]
    fn swap_now_requires_pool_and_min_out() {
        // Old-shape JSON (no `pool`) must fail to deserialize into the new
        // ActionWire::Swap, preventing silent schema drift from Dev 3.
        let old_shape = r#"{"type":"Swap","token_in":"0x1111111111111111111111111111111111111111","token_out":"0x2222222222222222222222222222222222222222","amount_in":"1000","min_out":"900"}"#;
        assert!(serde_json::from_str::<ActionWire>(old_shape).is_err());
    }

    #[test]
    fn set_dynamic_fee_roundtrips() {
        let internal = AgentIntent::new_with_commitment(
            AgentId([0xA5; 20]),
            7,
            "Uniswap".into(),
            Action::SetDynamicFee {
                pool: [0x11; 20],
                new_fee_ppm: 6000,
            },
            65,
            0,
            [0xAB; 32],
        );
        let wire: AgentIntentWire = (&internal).into();
        let round = wire.to_internal().unwrap();
        assert_eq!(round, internal);
    }
}
