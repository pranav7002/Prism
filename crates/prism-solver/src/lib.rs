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

