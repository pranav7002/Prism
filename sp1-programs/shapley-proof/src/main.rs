// PRISM SP1 Program: shapley-proof
// Verification key must be extracted after compilation: `cargo prove build`
// inside this directory.
//
// Purpose: proves that `cooperative_mev_value` was split by a deterministic
// priority-weighted distribution over the agents' priorities, then verifies
// the efficiency, non-negativity, and symmetry axioms.
//
// Naming note (H11 in Audit report): this used to be framed as "Monte Carlo
// Shapley" but the marginal-contribution formula collapses to closed-form
// proportional weighting because v(S) = sum-of-priorities is additive — the
// permutation shuffle is mathematically a no-op. The 1,000-sample loop is
// retained for SP1↔solver byte-parity (both sides do the same shuffle work)
// but the framing has been corrected to `priority_weighted_split`. If a
// future v(S) becomes non-additive (e.g. cooperative MEV that's actually
// sub-additive in coalition size), revisit the algorithm in lockstep on
// both sides.
//
// Algorithm:
//   LCG seed advance: next = state * 6364136223846793005 + 1442695040888963407
//   For `num_samples` iterations:
//     - Fisher-Yates shuffle the agent index vector.
//     - Accumulate priority[i] for each agent.
//   Average, renormalize so sum == cooperative_mev_value (efficiency).

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

// ----------------------------------------------------------------------------
// LCG / Fisher-Yates
// ----------------------------------------------------------------------------

const LCG_MUL: u64 = 6_364_136_223_846_793_005;
const LCG_INC: u64 = 1_442_695_040_888_963_407;

fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(LCG_MUL).wrapping_add(LCG_INC);
    *state
}

fn fisher_yates_shuffle(arr: &mut [usize], state: &mut u64) {
    let n = arr.len();
    if n < 2 {
        return;
    }
    for i in (1..n).rev() {
        let r = lcg_next(state);
        let j = (r as usize) % (i + 1);
        arr.swap(i, j);
    }
}

// ----------------------------------------------------------------------------

fn hash_distribution(payouts: &[(AgentId, u128)]) -> [u8; 32] {
    let mut h = Sha256::new();
    for (id, v) in payouts {
        h.update(id.0);
        h.update(v.to_be_bytes());
    }
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

pub fn main() {
    // M3: removed unused `_mev_value_in: u128` stdin read. Was previously
    // consumed and discarded — wasted RV cycles, obscured intent. The
    // cooperative_mev_value is sourced from `plan.cooperative_mev_value`.
    let plan: ExecutionPlan = sp1_zkvm::io::read();
    let random_seed: u64 = sp1_zkvm::io::read();
    let num_samples: u32 = sp1_zkvm::io::read();

    let n = plan.ordered_intents.len();
    assert!(n > 0, "empty plan");
    assert!(num_samples > 0, "num_samples must be > 0");

    // Shapley accumulator in f64 is OK inside the zkVM (RISC-V has no hw fp,
    // but the rustc soft-float path is deterministic). We convert to u128
    // token units at the end.
    let mut totals: Vec<u128> = Vec::with_capacity(n);
    for _ in 0..n {
        totals.push(0u128);
    }

    let priorities: Vec<u128> = plan
        .ordered_intents
        .iter()
        .map(|i| i.priority as u128)
        .collect();

    let mut indices: Vec<usize> = (0..n).collect();
    let mut rng_state = random_seed ^ 0xDEAD_BEEF_CAFE_BABE;

    for _ in 0..num_samples {
        fisher_yates_shuffle(&mut indices, &mut rng_state);

        // Walk the permutation accumulating priority_sum. Each agent's
        // marginal contribution at their position is their own priority —
        // this reduces to `priority[i]` averaged across permutations,
        // which for the current additive v(S) is identical to a closed-form
        // priority-proportional split. We retain the shuffle loop for SP1
        // ↔ solver byte-parity and as scaffolding for a future non-additive
        // v(S) (see H11 in Audit report).
        let mut running: u128 = 0;
        for &idx in &indices {
            let before = running;
            running = running.saturating_add(priorities[idx]);
            let marginal = running - before;
            totals[idx] = totals[idx].saturating_add(marginal);
        }
    }

    // Average.
    let mut avg: Vec<u128> = totals
        .iter()
        .map(|t| *t / num_samples as u128)
        .collect();

    // Normalize so sum == cooperative_mev_value (efficiency axiom).
    let target = plan.cooperative_mev_value;
    let sum_avg: u128 = avg.iter().copied().fold(0u128, |a, b| a.saturating_add(b));
    let mut final_payouts: Vec<(AgentId, u128)> = Vec::with_capacity(n);

    if target == 0 || sum_avg == 0 {
        // Nothing to distribute — all zeros.
        for intent in &plan.ordered_intents {
            final_payouts.push((intent.agent_id, 0));
        }
    } else {
        let mut running_assigned: u128 = 0;
        for i in 0..n {
            let share = (avg[i].saturating_mul(target)) / sum_avg;
            running_assigned = running_assigned.saturating_add(share);
            final_payouts.push((plan.ordered_intents[i].agent_id, share));
        }
        // Rounding dust goes to the first agent so the sum equals `target`
        // exactly (efficiency axiom).
        if running_assigned < target {
            let dust = target - running_assigned;
            final_payouts[0].1 = final_payouts[0].1.saturating_add(dust);
        }
        // Clear unused.
        let _ = &mut avg;
    }

    // Efficiency axiom.
    let sum: u128 = final_payouts
        .iter()
        .map(|(_, v)| *v)
        .fold(0u128, |a, b| a.saturating_add(b));
    assert!(
        sum == target,
        "efficiency violated: sum {} != target {}",
        sum,
        target
    );

    // Non-negativity axiom — u128 is unsigned, so this is automatic; the
    // check is here for spec parity and to catch overflow-induced dust bugs.
    for (_, v) in &final_payouts {
        assert!(*v <= target, "non-negativity / overflow guard");
    }

    // Symmetry axiom: agents with identical priority must receive payouts
    // within 5% of each other (after normalization). Tolerance is generous
    // to accommodate rounding dust assigned to index 0.
    for i in 0..n {
        for j in (i + 1)..n {
            if plan.ordered_intents[i].priority == plan.ordered_intents[j].priority {
                let a = final_payouts[i].1;
                let b = final_payouts[j].1;
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                if hi == 0 {
                    continue;
                }
                let diff = hi - lo;
                // Allow <=5% relative OR a small absolute dust window.
                assert!(
                    diff.saturating_mul(100) <= hi.saturating_mul(5) || diff <= 1_000,
                    "symmetry violated between agents with equal priority"
                );
            }
        }
    }

    let dist_hash = hash_distribution(&final_payouts);

    sp1_zkvm::io::commit(&final_payouts);
    sp1_zkvm::io::commit(&dist_hash);
}
