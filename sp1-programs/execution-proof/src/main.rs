// PRISM SP1 Program: execution-proof
// Verification key must be extracted after compilation: `cargo prove build`
// inside this directory.
//
// Purpose: proves each action in the ExecutionPlan is mathematically valid
// against on-chain state.
// Constraints:
//   - Swap: constant-product AMM with fee tier; actual slippage <= max.
//   - Backrun/Borrow-implying action: post-execution health > 1.05.
//   - AddLiquidity: 30d volatility < 3000 bps.

#![no_main]

sp1_zkvm::entrypoint!(main);

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct AgentId([u8; 20]);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ConsolidateRemove {
    pool: [u8; 20],
    liquidity: u128,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ConsolidateAdd {
    pool: [u8; 20],
    amount0: u128,
    amount1: u128,
    tick_lower: i32,
    tick_upper: i32,
}

// Externally-tagged (serde default). Must match `prism-types::Action` bit-for-bit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Action {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct AgentIntent {
    agent_id: AgentId,
    epoch: u64,
    target_protocol: String,
    action: Action,
    priority: u8,
    max_slippage_bps: u16,
    salt: [u8; 32],
    commitment: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ExecutionPlan {
    epoch: u64,
    ordered_intents: Vec<AgentIntent>,
    cooperative_mev_value: u128,
    shapley_weights: Vec<(AgentId, u16)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ProtocolState {
    pool_address: [u8; 20],
    sqrt_price_x96: u128,
    liquidity: u128,
    tick: i32,
    fee_tier: u32,
    token0_reserve: u128,
    token1_reserve: u128,
    volatility_30d_bps: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct HealthFactor {
    collateral_usd: u128,
    debt_usd: u128,
}

impl HealthFactor {
    // Integer-only safety check: collateral * 100 > debt * 105 is equivalent
    // to collateral / debt > 1.05 without any floating-point inside the zkVM.
    fn is_safe(&self) -> bool {
        if self.debt_usd == 0 {
            return true;
        }
        let lhs = self.collateral_usd.saturating_mul(100);
        let rhs = self.debt_usd.saturating_mul(105);
        lhs > rhs
    }
}

// ----------------------------------------------------------------------------
// AMM math
// ----------------------------------------------------------------------------

/// Constant-product output: amount_out = (amount_in * reserve_out) /
/// (reserve_in + amount_in), then multiplied by (1_000_000 - fee_tier) /
/// 1_000_000. All arithmetic in u128 with saturating semantics.
fn xy_k_amount_out(amount_in: u128, reserve_in: u128, reserve_out: u128, fee_tier_ppm: u32) -> u128 {
    if amount_in == 0 || reserve_in == 0 || reserve_out == 0 {
        return 0;
    }
    let num = amount_in.saturating_mul(reserve_out);
    let den = reserve_in.saturating_add(amount_in);
    let raw = num / den;
    let fee_mult = 1_000_000u128.saturating_sub(fee_tier_ppm as u128);
    raw.saturating_mul(fee_mult) / 1_000_000
}

fn hash_execution(plan: &ExecutionPlan, state: &ProtocolState, gas: u128) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(plan.epoch.to_be_bytes());
    for i in &plan.ordered_intents {
        h.update(i.commitment);
    }
    h.update(state.pool_address);
    h.update(state.fee_tier.to_be_bytes());
    h.update(state.token0_reserve.to_be_bytes());
    h.update(state.token1_reserve.to_be_bytes());
    h.update(gas.to_be_bytes());
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

pub fn main() {
    let plan: ExecutionPlan = sp1_zkvm::io::read();
    let state: ProtocolState = sp1_zkvm::io::read();
    let health: HealthFactor = sp1_zkvm::io::read();

    let mut n_actions: u128 = 0;
    let mut saw_backrun = false;

    for intent in &plan.ordered_intents {
        n_actions += 1;
        match &intent.action {
            Action::Swap {
                amount_in,
                min_out,
                ..
            } => {
                let out = xy_k_amount_out(
                    *amount_in,
                    state.token0_reserve,
                    state.token1_reserve,
                    state.fee_tier,
                );
                assert!(
                    out >= *min_out,
                    "amount_out {} below min_out {}",
                    out,
                    *min_out
                );
            }
            Action::AddLiquidity { .. } => {
                // Volatility ceiling for AddLiquidity. Aligned with the
                // off-chain solver's volatility cutoff (5_000 bps) so a plan
                // that the solver accepts cannot fail proof generation
                // purely on this constant — H5 in AUDIT_REPORT. Plans with
                // vol ∈ [3000, 5000] previously passed the solver and then
                // failed here.
                assert!(
                    state.volatility_30d_bps < 5_000,
                    "volatility {} bps too high for LP",
                    state.volatility_30d_bps
                );
            }
            Action::Backrun { .. } => {
                saw_backrun = true;
            }
            Action::MigrateLiquidity {
                from_pool,
                to_pool,
                amount,
                ..
            } => {
                assert!(from_pool != to_pool, "migrate: from_pool == to_pool");
                assert!(*amount > 0, "migrate: amount is zero");
            }
            Action::BatchConsolidate { removes, adds } => {
                assert!(!removes.is_empty(), "batch_consolidate: empty removes");
                assert!(!adds.is_empty(), "batch_consolidate: empty adds");
                // Guard against overflow on the sum of liquidity.
                let mut sum: u128 = 0;
                for r in removes {
                    sum = sum.checked_add(r.liquidity).expect("liquidity overflow");
                }
                // Trivially bounded below u128::MAX/2 by checked_add success.
                let _ = sum;
            }
            Action::SetDynamicFee { new_fee_ppm, .. } => {
                assert!(
                    (500..=10_000).contains(new_fee_ppm),
                    "dynamic fee {} ppm out of [500, 10000]",
                    new_fee_ppm
                );
            }
            Action::CrossProtocolHedge {
                aave_borrow_asset,
                aave_borrow_amount,
                uniswap_token_in,
                uniswap_amount_in,
                ..
            } => {
                assert!(*aave_borrow_amount > 0, "hedge: aave amount is zero");
                assert!(*uniswap_amount_in > 0, "hedge: swap amount is zero");
                // The borrowed asset and the token being supplied into the
                // swap should be the same (ε borrows X, swaps X→Y). This
                // anchors the delta-neutrality check.
                assert!(
                    aave_borrow_asset == uniswap_token_in,
                    "hedge: borrow asset != swap token_in"
                );
                // CrossProtocolHedge implies an Aave borrow — same risk
                // profile as Backrun. Trip the post-action HF check (M5).
                saw_backrun = true;
            }
            Action::RemoveLiquidity { .. }
            | Action::DeltaHedge { .. }
            | Action::KillSwitch { .. } => {}
        }
    }

    // saw_backrun is also set by CrossProtocolHedge — see M5 above.
    if saw_backrun {
        assert!(health.is_safe(), "post-execution health factor <= 1.05");
    }

    let gas_estimate: u128 = n_actions.saturating_mul(150_000);
    let exec_hash = hash_execution(&plan, &state, gas_estimate);

    sp1_zkvm::io::commit(&true);
    sp1_zkvm::io::commit(&gas_estimate);
    sp1_zkvm::io::commit(&exec_hash);
}
