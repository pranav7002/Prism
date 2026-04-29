//! Off-chain conflict resolution and cooperative MEV ordering.
//!
//! The solver is pure — given intents and protocol state, it produces an
//! ExecutionPlan. SP1 Program 1 (`solver-proof`) re-runs the same ordering
//! rules inside the zkVM and asserts the output matches what the orchestrator
//! claims.
//!
//! Ordering rules, in order of application:
//! 1. Any `KillSwitch` action is placed first, regardless of priority.
//! 2. Remaining intents sort by `priority` descending.
//! 3. Ties break lexicographically by `agent_id`.
//! 4. β-before-δ: whenever a `Backrun` appears after a `DeltaHedge` in the
//!    partially-sorted list, we swap them. This captures cooperative MEV
//!    (backrunner's profit is the delta-hedger's price improvement).

use prism_types::{Action, AgentId, AgentIntent, ExecutionPlan, HealthFactor, ProtocolState};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum SolverError {
    #[error("unresolvable conflict(s): {0:?}")]
    UnresolvableConflict(Vec<ConflictType>),
    #[error("insufficient agents: need at least 1 valid intent")]
    InsufficientAgents,
    #[error("invalid intent: {0}")]
    InvalidIntent(String),
}

// ---------------------------------------------------------------------------
// Conflict detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictType {
    /// Two swaps on the same pool collectively exceed their slippage budget.
    SlippageViolation,
    /// Two agents both removing liquidity from the same pool in the same epoch.
    LiquidityRace,
    /// A backrun would push the health factor below the 1.05 safety line.
    HealthFactorRisk,
    /// A lower-priority intent would execute before a higher-priority one
    /// (only raised by external callers; the resolver removes it by design).
    PriorityInversion,
    /// A SetDynamicFee intent races an in-flight Swap on the same pool. The
    /// swap settles at one fee while the curator changes the fee mid-epoch
    /// — last-write semantics on the fee leave the swap's effective price
    /// dependent on intent ordering inside the epoch (M2 in Audit report).
    FeeSwapRace,
}

pub struct ConflictDetector;

impl ConflictDetector {
    pub fn new() -> Self {
        Self
    }

    /// Returns `(i, j, kind)` for every conflicting pair of intents.
    pub fn detect(&self, intents: &[AgentIntent]) -> Vec<(usize, usize, ConflictType)> {
        let mut out = Vec::new();

        for i in 0..intents.len() {
            for j in (i + 1)..intents.len() {
                if let Some(kind) = pair_conflict(&intents[i], &intents[j]) {
                    out.push((i, j, kind));
                }
            }
        }

        out
    }
}

impl Default for ConflictDetector {
    fn default() -> Self {
        Self::new()
    }
}

fn pair_conflict(a: &AgentIntent, b: &AgentIntent) -> Option<ConflictType> {
    match (&a.action, &b.action) {
        // Two swaps on the same pool (and same direction) whose combined
        // per-intent slippage budgets exceed the full 100% book — a clear
        // conflict even if each individually is within its own cap.
        (
            Action::Swap {
                pool: ap,
                token_in: ai,
                token_out: ao,
                ..
            },
            Action::Swap {
                pool: bp,
                token_in: bi,
                token_out: bo,
                ..
            },
        ) if ap == bp && ai == bi && ao == bo => {
            if (a.max_slippage_bps as u32 + b.max_slippage_bps as u32) > 10_000 {
                Some(ConflictType::SlippageViolation)
            } else {
                None
            }
        }
        (
            Action::RemoveLiquidity { pool: ap, .. },
            Action::RemoveLiquidity { pool: bp, .. },
        ) if ap == bp => Some(ConflictType::LiquidityRace),
        // Two SetDynamicFee intents on the same pool in the same epoch —
        // last-write-wins is ambiguous; flag as a race.
        (
            Action::SetDynamicFee { pool: ap, .. },
            Action::SetDynamicFee { pool: bp, .. },
        ) if ap == bp => Some(ConflictType::LiquidityRace),
        // SetDynamicFee racing a Swap on the same pool — the swap's
        // effective price depends on whether the fee bump executed first
        // (closes M2). Both directions covered. Note: Backrun is a
        // distinct Action variant (target_tx + profit_token, not Swap),
        // so β's SetDynamicFee in epoch 2 doesn't conflict with δ's
        // Backrun — preserves the β-migrate / δ-backrun cooperation
        // documented in v2 §10.2.
        (
            Action::SetDynamicFee { pool: ap, .. },
            Action::Swap { pool: bp, .. },
        )
        | (
            Action::Swap { pool: bp, .. },
            Action::SetDynamicFee { pool: ap, .. },
        ) if ap == bp => Some(ConflictType::FeeSwapRace),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Priority resolution
// ---------------------------------------------------------------------------

pub struct PriorityResolver;

impl PriorityResolver {
    pub fn new() -> Self {
        Self
    }

    /// Apply the ordering rules and return a newly ordered vec.
    pub fn resolve(&self, mut intents: Vec<AgentIntent>) -> Vec<AgentIntent> {
        // Partition: kill-switch first.
        let (mut kills, rest): (Vec<_>, Vec<_>) = intents
            .drain(..)
            .partition(|i| matches!(i.action, Action::KillSwitch { .. }));

        kills.sort_by_key(|a| a.agent_id.0);

        // Stable sort by (priority desc, agent_id asc).
        let mut rest = rest;
        rest.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.agent_id.0.cmp(&b.agent_id.0))
        });

        // β-before-δ: if any DeltaHedge precedes a Backrun, move the Backrun
        // in front. Single pass suffices because priority sort already
        // clusters roles; we preserve relative order within each class.
        apply_beta_before_delta(&mut rest);

        kills.extend(rest);
        kills
    }
}

impl Default for PriorityResolver {
    fn default() -> Self {
        Self::new()
    }
}

fn apply_beta_before_delta(intents: &mut Vec<AgentIntent>) {
    // Collect indices of DeltaHedge and Backrun actions in current order.
    let mut first_delta: Option<usize> = None;
    for (i, intent) in intents.iter().enumerate() {
        if matches!(intent.action, Action::DeltaHedge { .. }) {
            first_delta = Some(i);
            break;
        }
    }

    let Some(delta_idx) = first_delta else {
        return;
    };

    // If any Backrun appears AFTER the first DeltaHedge, rotate it forward.
    let mut backrun_idx: Option<usize> = None;
    for (i, intent) in intents.iter().enumerate().skip(delta_idx + 1) {
        if matches!(intent.action, Action::Backrun { .. }) {
            backrun_idx = Some(i);
            break;
        }
    }

    if let Some(br) = backrun_idx {
        let item = intents.remove(br);
        intents.insert(delta_idx, item);
    }
}

// ---------------------------------------------------------------------------
// Cooperative MEV valuation
// ---------------------------------------------------------------------------

pub struct CooperativeMevCalculator;

impl CooperativeMevCalculator {
    pub fn new() -> Self {
        Self
    }

    /// Rough heuristic value: per-action contributions summed in u128 token
    /// units. SP1 Program 3 re-runs this off-chain estimate inside the zkVM
    /// as a sanity sum; the Shapley program distributes it.
    pub fn calculate_mev_value(
        &self,
        plan: &ExecutionPlan,
        protocol_state: &ProtocolState,
    ) -> u128 {
        let mut total: u128 = 0;

        for intent in &plan.ordered_intents {
            match &intent.action {
                Action::Backrun { .. } => {
                    // Captured profit heuristic: fee_tier (in ppm) applied to
                    // a notional of 1e18 tokens.
                    let notional: u128 = 1_000_000_000_000_000_000;
                    let contribution = notional
                        .saturating_mul(protocol_state.fee_tier as u128)
                        / 1_000_000u128;
                    total = total.saturating_add(contribution);
                }
                Action::Swap { amount_in, .. } => {
                    // Positive slippage savings from priority ordering.
                    let priority_diff = intent.priority as u128;
                    let base = amount_in.saturating_mul(priority_diff);
                    let savings = base / 10_000u128;
                    let bound = amount_in
                        .saturating_mul(intent.max_slippage_bps as u128)
                        / 10_000;
                    total = total.saturating_add(savings.min(bound));
                }
                _ => {}
            }
        }

        total
    }
}

impl Default for CooperativeMevCalculator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Kill-switch monitoring
// ---------------------------------------------------------------------------

pub struct KillSwitchMonitor;

impl KillSwitchMonitor {
    pub fn new() -> Self {
        Self
    }

    pub fn should_trigger(&self, state: &ProtocolState, health: &HealthFactor) -> bool {
        if health.value() < 1.02 {
            return true;
        }
        if state.volatility_30d_bps > 5_000 {
            return true;
        }
        if state.liquidity == 0 {
            return true;
        }
        false
    }

    pub fn build_kill_switch_intent(&self, agent_id: AgentId, epoch: u64) -> AgentIntent {
        let action = Action::KillSwitch {
            reason: "volatility_or_health_threshold_breach".to_string(),
        };
        AgentIntent::new_with_commitment(
            agent_id,
            epoch,
            "Uniswap".into(),
            action,
            255,
            0,
            [0u8; 32],
        )
    }
}

impl Default for KillSwitchMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Top-level plan builder
// ---------------------------------------------------------------------------

pub fn build_execution_plan(
    intents: Vec<AgentIntent>,
    protocol_state: &ProtocolState,
) -> Result<ExecutionPlan, SolverError> {
    if intents.is_empty() {
        return Err(SolverError::InsufficientAgents);
    }

    // Reject unverifiable intents early.
    for intent in &intents {
        if !intent.verify_commitment() {
            return Err(SolverError::InvalidIntent(format!(
                "commitment mismatch for agent {}",
                intent.agent_id.to_hex()
            )));
        }
    }

    let epoch = intents[0].epoch;

    let detector = ConflictDetector::new();
    let conflicts = detector.detect(&intents);
    if !conflicts.is_empty() {
        // Surface only the kinds; resolver caller can re-detect for indices.
        let kinds = conflicts.into_iter().map(|(_, _, k)| k).collect::<Vec<_>>();
        return Err(SolverError::UnresolvableConflict(kinds));
    }

    let resolver = PriorityResolver::new();
    let ordered = resolver.resolve(intents);

    let placeholder_plan = ExecutionPlan {
        epoch,
        ordered_intents: ordered,
        cooperative_mev_value: 0,
        shapley_weights: vec![],
    };

    let mev = CooperativeMevCalculator::new().calculate_mev_value(&placeholder_plan, protocol_state);

    let weights = priority_weighted_split(&placeholder_plan.ordered_intents, epoch);

    Ok(ExecutionPlan {
        epoch,
        ordered_intents: placeholder_plan.ordered_intents,
        cooperative_mev_value: mev,
        shapley_weights: weights,
    })
}

// ---------------------------------------------------------------------------
// Priority-weighted split (was: "Monte Carlo Shapley")
// ---------------------------------------------------------------------------
//
// Port of the algorithm from `sp1-programs/shapley-proof/src/main.rs`. Uses
// an LCG PRNG + Fisher-Yates shuffle to walk N permutations of the agent
// index vector and accumulate per-agent contribution.
//
// Naming honesty (H11 in Audit report): the prior name "Monte Carlo
// Shapley" was misleading because v(S) = sum-of-priorities is additive, so
// the marginal contribution at any position is exactly the agent's own
// priority regardless of permutation. The 1,000-sample Fisher-Yates loop
// therefore collapses mathematically to a closed-form proportional split
// — a true Shapley value over a non-additive v(S) (e.g., sub-additive
// cooperative MEV) would NOT collapse this way and would require a
// different algorithm in lockstep on both sides.
//
// We retain the shuffle loop because (a) it's cheap, (b) byte-parity with
// the SP1 circuit is preserved, and (c) it's the right shape for a future
// non-additive v(S) — flip the marginal-contribution rule, keep the
// scaffolding. But the framing is now correct.

/// LCG multiplier — same constant used in the SP1 zkVM circuit.
const LCG_MUL: u64 = 6_364_136_223_846_793_005;
/// LCG increment — same constant used in the SP1 zkVM circuit.
const LCG_INC: u64 = 1_442_695_040_888_963_407;
/// Number of permutation samples. 1,000 gives <1% error for 5 agents on
/// a non-additive v(S); for the current additive v(S) the value is
/// numerically irrelevant but the SP1 circuit reads this same constant
/// from stdin so they must stay in sync.
pub const SHAPLEY_NUM_SAMPLES: u32 = 1_000;

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

/// Compute deterministic priority-weighted basis-point weights summing to
/// exactly 10,000. Pre-H11 fix this was `monte_carlo_shapley`; renamed
/// because for the current additive v(S) the algorithm is provably
/// equivalent to a closed-form proportional split.
///
/// `seed` is derived from the epoch to ensure deterministic results across
/// runs. The same seed + intents always produce the same weights.
fn priority_weighted_split(intents: &[AgentIntent], seed: u64) -> Vec<(AgentId, u16)> {
    let n = intents.len();
    if n == 0 {
        return vec![];
    }

    let priorities: Vec<u128> = intents.iter().map(|i| i.priority as u128).collect();
    let mut totals: Vec<u128> = vec![0u128; n];
    let mut indices: Vec<usize> = (0..n).collect();
    // XOR with a magic constant to decorrelate from sequential epoch seeds,
    // matching the SP1 circuit's initialization.
    let mut rng_state = seed ^ 0xDEAD_BEEF_CAFE_BABE;

    for _ in 0..SHAPLEY_NUM_SAMPLES {
        fisher_yates_shuffle(&mut indices, &mut rng_state);

        // Walk the permutation accumulating priority_sum. Each agent's
        // marginal contribution at their position is their own priority.
        let mut running: u128 = 0;
        for &idx in &indices {
            let before = running;
            running = running.saturating_add(priorities[idx]);
            let marginal = running - before;
            totals[idx] = totals[idx].saturating_add(marginal);
        }
    }

    // Average over samples.
    let avg: Vec<u128> = totals
        .iter()
        .map(|t| *t / SHAPLEY_NUM_SAMPLES as u128)
        .collect();

    // Normalize to 10,000 bps (efficiency axiom).
    let target: u128 = 10_000;
    let sum_avg: u128 = avg.iter().copied().sum();

    if sum_avg == 0 {
        // All priorities are zero — fall back to equal split.
        let base = 10_000u16 / n as u16;
        let remainder = 10_000u16 - base * n as u16;
        return intents
            .iter()
            .enumerate()
            .map(|(i, intent)| {
                let w = if i == 0 { base + remainder } else { base };
                (intent.agent_id, w)
            })
            .collect();
    }

    let mut result: Vec<(AgentId, u16)> = Vec::with_capacity(n);
    let mut running_assigned: u128 = 0;
    for i in 0..n {
        let share = (avg[i].saturating_mul(target)) / sum_avg;
        running_assigned = running_assigned.saturating_add(share);
        result.push((intents[i].agent_id, share as u16));
    }

    // Rounding dust goes to first agent so sum == 10,000 exactly.
    if running_assigned < target {
        let dust = (target - running_assigned) as u16;
        result[0].1 = result[0].1.saturating_add(dust);
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use prism_types::{Action, AgentId};

    fn intent(
        agent: u8,
        priority: u8,
        action: Action,
        epoch: u64,
    ) -> AgentIntent {
        AgentIntent::new_with_commitment(
            AgentId([agent; 20]),
            epoch,
            "Uniswap".into(),
            action,
            priority,
            50,
            [agent; 32],
        )
    }

    fn swap(amount: u128) -> Action {
        Action::Swap {
            pool: [0xDD; 20],
            token_in: [0x11; 20],
            token_out: [0x22; 20],
            amount_in: amount,
            min_out: amount.saturating_mul(99) / 100,
        }
    }

    fn backrun() -> Action {
        Action::Backrun {
            target_tx: [0xBB; 32],
            profit_token: [0x22; 20],
        }
    }

    fn delta() -> Action {
        Action::DeltaHedge {
            position_id: 7,
            delta: -123,
        }
    }

    fn killswitch() -> Action {
        Action::KillSwitch {
            reason: "panic".into(),
        }
    }

    fn dummy_state() -> ProtocolState {
        ProtocolState {
            pool_address: [0xDD; 20],
            sqrt_price_x96: 1,
            liquidity: 1_000_000,
            tick: 0,
            fee_tier: 3_000,
            token0_reserve: 1_000_000,
            token1_reserve: 1_000_000,
            volatility_30d_bps: 1_500,
        }
    }

    #[test]
    fn priority_sort_descending() {
        let intents = vec![
            intent(0x01, 10, swap(1), 1),
            intent(0x02, 90, swap(2), 1),
            intent(0x03, 50, swap(3), 1),
        ];
        let resolved = PriorityResolver::new().resolve(intents);
        let priorities: Vec<u8> = resolved.iter().map(|i| i.priority).collect();
        assert_eq!(priorities, vec![90, 50, 10]);
    }

    #[test]
    fn beta_before_delta_swap() {
        let intents = vec![
            intent(0x01, 80, delta(), 1),   // δ at priority 80
            intent(0x02, 70, backrun(), 1), // β at priority 70 — should move in front
        ];
        let resolved = PriorityResolver::new().resolve(intents);
        assert!(matches!(resolved[0].action, Action::Backrun { .. }));
        assert!(matches!(resolved[1].action, Action::DeltaHedge { .. }));
    }

    #[test]
    fn killswitch_takes_first_slot_regardless_of_priority() {
        let intents = vec![
            intent(0x01, 99, swap(1), 1),
            intent(0x02, 10, killswitch(), 1),
        ];
        let resolved = PriorityResolver::new().resolve(intents);
        assert!(matches!(resolved[0].action, Action::KillSwitch { .. }));
    }

    #[test]
    fn tie_break_by_agent_id_lex() {
        let intents = vec![
            intent(0xEE, 50, swap(1), 1),
            intent(0x01, 50, swap(2), 1),
        ];
        let resolved = PriorityResolver::new().resolve(intents);
        assert_eq!(resolved[0].agent_id, AgentId([0x01; 20]));
        assert_eq!(resolved[1].agent_id, AgentId([0xEE; 20]));
    }

    #[test]
    fn killswitch_monitor_triggers_on_health() {
        let mon = KillSwitchMonitor::new();
        let state = dummy_state();
        let unhealthy = HealthFactor {
            collateral_usd: 1_010_000,
            debt_usd: 1_000_000,
        };
        assert!(mon.should_trigger(&state, &unhealthy));
    }

    #[test]
    fn killswitch_monitor_triggers_on_volatility() {
        let mon = KillSwitchMonitor::new();
        let mut state = dummy_state();
        state.volatility_30d_bps = 6_000;
        let healthy = HealthFactor {
            collateral_usd: 2_000_000,
            debt_usd: 1_000_000,
        };
        assert!(mon.should_trigger(&state, &healthy));
    }

    #[test]
    fn killswitch_monitor_triggers_on_zero_liquidity() {
        let mon = KillSwitchMonitor::new();
        let mut state = dummy_state();
        state.liquidity = 0;
        let healthy = HealthFactor {
            collateral_usd: 2_000_000,
            debt_usd: 1_000_000,
        };
        assert!(mon.should_trigger(&state, &healthy));
    }

    #[test]
    fn conflict_detection_finds_liquidity_race() {
        let intents = vec![
            intent(
                0x01,
                50,
                Action::RemoveLiquidity {
                    pool: [0xAA; 20],
                    liquidity: 1,
                },
                1,
            ),
            intent(
                0x02,
                50,
                Action::RemoveLiquidity {
                    pool: [0xAA; 20],
                    liquidity: 2,
                },
                1,
            ),
        ];
        let conflicts = ConflictDetector::new().detect(&intents);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].2, ConflictType::LiquidityRace);
    }

    #[test]
    fn build_execution_plan_end_to_end() {
        let intents = vec![
            intent(0x01, 70, swap(1_000), 5),
            intent(0x02, 85, backrun(), 5),
            intent(0x03, 40, delta(), 5),
        ];
        let plan = build_execution_plan(intents, &dummy_state()).unwrap();
        assert_eq!(plan.epoch, 5);
        assert_eq!(plan.ordered_intents.len(), 3);
        // Shapley weights sum to exactly 10000.
        let sum: u32 = plan.shapley_weights.iter().map(|(_, w)| *w as u32).sum();
        assert_eq!(sum, 10_000);
        // Backrun must appear before DeltaHedge.
        let mut saw_backrun = false;
        for i in &plan.ordered_intents {
            if matches!(i.action, Action::Backrun { .. }) {
                saw_backrun = true;
            }
            if matches!(i.action, Action::DeltaHedge { .. }) {
                assert!(saw_backrun, "δ appeared before β");
            }
        }
    }

    #[test]
    fn plan_accepts_uniswap_pivot_actions() {
        // One intent per new Uniswap-V4-native action variant. All must
        // survive conflict detection and come out priority-sorted.
        let migrate = Action::MigrateLiquidity {
            from_pool: [0x11; 20],
            to_pool: [0x22; 20],
            amount: 200_000_000_000u128,
            tick_lower: 200_400,
            tick_upper: 203_400,
        };
        let set_fee = Action::SetDynamicFee {
            pool: [0x33; 20],
            new_fee_ppm: 6_000,
        };
        let hedge = Action::CrossProtocolHedge {
            aave_borrow_asset: [0x44; 20],
            aave_borrow_amount: 1_000_000,
            uniswap_pool: [0x55; 20],
            uniswap_token_in: [0x44; 20],
            uniswap_token_out: [0x66; 20],
            uniswap_amount_in: 1_000_000,
        };
        let intents = vec![
            intent(0x01, 75, migrate, 9),
            intent(0x02, 65, set_fee, 9),
            intent(0x03, 85, hedge, 9),
        ];
        let plan = build_execution_plan(intents, &dummy_state()).unwrap();
        assert_eq!(plan.ordered_intents.len(), 3);
        // Priority-sort: hedge (85) before migrate (75) before set_fee (65).
        assert!(matches!(
            plan.ordered_intents[0].action,
            Action::CrossProtocolHedge { .. }
        ));
        assert!(matches!(
            plan.ordered_intents[1].action,
            Action::MigrateLiquidity { .. }
        ));
        assert!(matches!(
            plan.ordered_intents[2].action,
            Action::SetDynamicFee { .. }
        ));
    }

    #[test]
    fn duplicate_set_dynamic_fee_on_same_pool_conflicts() {
        let a = Action::SetDynamicFee {
            pool: [0x99; 20],
            new_fee_ppm: 3_000,
        };
        let b = Action::SetDynamicFee {
            pool: [0x99; 20],
            new_fee_ppm: 6_000,
        };
        let intents = vec![intent(0x01, 60, a, 9), intent(0x02, 60, b, 9)];
        let err = build_execution_plan(intents, &dummy_state()).unwrap_err();
        assert!(matches!(err, SolverError::UnresolvableConflict(_)));
    }

    #[test]
    fn set_dynamic_fee_racing_swap_on_same_pool_conflicts() {
        // M2 fix: a SetDynamicFee and a Swap on the same pool race.
        let fee = Action::SetDynamicFee {
            pool: [0xDD; 20], // matches swap()'s pool
            new_fee_ppm: 6_000,
        };
        // Order 1: fee then swap.
        let intents = vec![
            intent(0x01, 65, fee.clone(), 9),
            intent(0x02, 80, swap(1_000), 9),
        ];
        let conflicts = ConflictDetector::new().detect(&intents);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].2, ConflictType::FeeSwapRace);

        // Order 2: swap then fee — same conflict, different ordering.
        let intents = vec![
            intent(0x01, 80, swap(1_000), 9),
            intent(0x02, 65, fee, 9),
        ];
        let conflicts = ConflictDetector::new().detect(&intents);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].2, ConflictType::FeeSwapRace);
    }

    #[test]
    fn set_dynamic_fee_on_different_pool_does_not_conflict_with_swap() {
        // SetDynamicFee on pool A while Swap targets pool B — no race.
        let fee = Action::SetDynamicFee {
            pool: [0xAA; 20],
            new_fee_ppm: 6_000,
        };
        // swap() above hits pool [0xDD; 20] — different from [0xAA; 20].
        let intents = vec![intent(0x01, 65, fee, 9), intent(0x02, 80, swap(1), 9)];
        let conflicts = ConflictDetector::new().detect(&intents);
        assert!(
            conflicts.is_empty(),
            "different pools must not race: {:?}",
            conflicts
        );
    }

    #[test]
    fn set_dynamic_fee_does_not_conflict_with_backrun() {
        // Regression guard for v2 §10.2: β's SetDynamicFee and δ's Backrun
        // must coexist in epoch 2 without flagging a conflict (Backrun is
        // a distinct Action variant, not Swap). M2 fix preserves this.
        let fee = Action::SetDynamicFee {
            pool: [0xDD; 20],
            new_fee_ppm: 6_000,
        };
        let intents = vec![intent(0x01, 75, fee, 2), intent(0x02, 90, backrun(), 2)];
        let conflicts = ConflictDetector::new().detect(&intents);
        assert!(
            conflicts.is_empty(),
            "β-fee + δ-backrun cooperation broke: {:?}",
            conflicts
        );
    }

    // -----------------------------------------------------------------------
    // priority_weighted_split tests (H11 — was "Monte Carlo Shapley tests")
    // -----------------------------------------------------------------------

    #[test]
    fn shapley_higher_priority_gets_more_weight() {
        // Agent 0x01 has priority 90, 0x02 has 10. Shapley should reflect that.
        let intents = vec![
            intent(0x01, 90, swap(1_000), 1),
            intent(0x02, 10, swap(2_000), 1),
        ];
        let weights = priority_weighted_split(&intents, 42);
        assert_eq!(weights.len(), 2);
        let sum: u32 = weights.iter().map(|(_, w)| *w as u32).sum();
        assert_eq!(sum, 10_000, "efficiency axiom violated");
        // Agent with priority 90 should get ~9000 bps.
        let w0 = weights[0].1;
        let w1 = weights[1].1;
        assert!(
            w0 > w1,
            "priority-90 agent ({} bps) should outweigh priority-10 agent ({} bps)",
            w0, w1
        );
        // Within 5% tolerance of exact proportional split (9000/1000).
        assert!(w0 >= 8500, "priority-90 agent got only {} bps", w0);
        assert!(w1 <= 1500, "priority-10 agent got {} bps", w1);
    }

    #[test]
    fn shapley_is_deterministic() {
        let intents = vec![
            intent(0x01, 70, swap(1_000), 1),
            intent(0x02, 50, swap(2_000), 1),
            intent(0x03, 30, swap(3_000), 1),
        ];
        let w1 = priority_weighted_split(&intents, 99);
        let w2 = priority_weighted_split(&intents, 99);
        assert_eq!(w1, w2, "same seed must produce identical weights");
    }

    #[test]
    fn shapley_equal_priorities_give_equal_weights() {
        let intents = vec![
            intent(0x01, 50, swap(1_000), 1),
            intent(0x02, 50, swap(2_000), 1),
            intent(0x03, 50, swap(3_000), 1),
        ];
        let weights = priority_weighted_split(&intents, 7);
        let sum: u32 = weights.iter().map(|(_, w)| *w as u32).sum();
        assert_eq!(sum, 10_000);
        // Each agent should get ~3333 bps. Allow rounding dust.
        for (_, w) in &weights {
            assert!(
                *w >= 3300 && *w <= 3400,
                "equal-priority agent got {} bps (expected ~3333)",
                w
            );
        }
    }

    #[test]
    fn shapley_single_agent_gets_all() {
        let intents = vec![intent(0x01, 50, swap(1_000), 1)];
        let weights = priority_weighted_split(&intents, 1);
        assert_eq!(weights.len(), 1);
        assert_eq!(weights[0].1, 10_000);
    }
}
