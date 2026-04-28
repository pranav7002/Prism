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

    let weights = monte_carlo_shapley(&placeholder_plan.ordered_intents, epoch);

    Ok(ExecutionPlan {
        epoch,
        ordered_intents: placeholder_plan.ordered_intents,
        cooperative_mev_value: mev,
        shapley_weights: weights,
    })
}

// ---------------------------------------------------------------------------
// Monte Carlo Shapley value computation
// ---------------------------------------------------------------------------
//
// Port of the algorithm from `sp1-programs/shapley-proof/src/main.rs`.
// Uses an LCG PRNG + Fisher-Yates shuffle to estimate each agent's
// marginal contribution across random coalition orderings.
//
// The valuation function v(S) = sum of priorities of agents in S.
// For additive valuations, Shapley values converge to priority-proportional
// shares, but we keep the Monte Carlo loop so the algorithm generalizes
// when the valuation function becomes non-additive (e.g., cooperative MEV
// synergies between β and δ).

/// LCG multiplier — same constant used in the SP1 zkVM circuit.
const LCG_MUL: u64 = 6_364_136_223_846_793_005;
/// LCG increment — same constant used in the SP1 zkVM circuit.
const LCG_INC: u64 = 1_442_695_040_888_963_407;
/// Number of Monte Carlo permutation samples. 1000 gives <1% error for
/// 5 agents. The SP1 circuit uses the same value.
const SHAPLEY_NUM_SAMPLES: u32 = 1_000;

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

/// Compute Shapley values via Monte Carlo sampling and return basis-point
/// weights summing to exactly 10,000.
///
/// `seed` is derived from the epoch to ensure deterministic results across
/// runs. The same seed + intents always produce the same weights.
fn monte_carlo_shapley(intents: &[AgentIntent], seed: u64) -> Vec<(AgentId, u16)> {
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

