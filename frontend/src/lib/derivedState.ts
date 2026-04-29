import type { WsEvent } from "./wsClient";

// All functions are pure — no hooks, no side-effects.
// WsEvent is an internally-tagged union (`type` field is the discriminator)
// — see wsClient.ts.

/** Last EpochStart.epoch seen, or null if none. */
export function currentEpoch(events: WsEvent[]): number | null {
  for (const e of events) {
    if (e.type === "epoch_start") return e.epoch;
  }
  return null;
}

/** Title-case a snake/lower program label so existing UI keys ("Solver",
 *  "Execution", "Shapley", "Aggregator") keep matching even though the wire
 *  format is lowercase. */
function titleCaseProgram(p: string): string {
  if (!p) return p;
  return p[0].toUpperCase() + p.slice(1).toLowerCase();
}

/**
 * Latest ProofProgress percentage per program name.
 * Keys are PascalCase ("Solver" | "Execution" | "Shapley" | "Aggregator")
 * for backwards-compat with existing consumers, normalized from the
 * lowercase wire shape.
 */
export function proofProgress(events: WsEvent[]): Record<string, number> {
  const result: Record<string, number> = {};
  for (const e of events) {
    if (e.type === "proof_progress") {
      const program = titleCaseProgram(e.program);
      if (!(program in result)) {
        result[program] = e.pct;
      }
    }
  }
  return result;
}

/**
 * Last EpochSettled.shapley array (Vec<u16>, sums to 10000 bps), or null if none.
 * Callers divide by 100 to get percentages. Also reads the Plan-B variant so
 * the dashboard renders payouts regardless of which path settled the epoch.
 */
export function lastShapley(events: WsEvent[]): number[] | null {
  for (const e of events) {
    if (e.type === "epoch_settled") return e.shapley;
    if (e.type === "epoch_settled_via_plan_b") return e.shapley;
  }
  return null;
}

/**
 * Path that settled the most-recent epoch.
 *   - "groth16" — happy-path recursive aggregation + 260-byte Groth16 wrap.
 *   - "plan-b"  — fallback: three sub-proofs verified independently on-chain.
 *   - null      — no settlement event seen yet.
 */
export function lastSettlePath(events: WsEvent[]): "groth16" | "plan-b" | null {
  for (const e of events) {
    if (e.type === "epoch_settled") return "groth16";
    if (e.type === "epoch_settled_via_plan_b") return "plan-b";
  }
  return null;
}

/** Last SolverRunning.conflicts_detected count, or 0 if none seen. */
export function solverConflicts(events: WsEvent[]): number {
  for (const e of events) {
    if (e.type === "solver_running") return e.conflicts_detected;
  }
  return 0;
}

/** Last n events in chronological order (most recent first). */
export function recentEvents(events: WsEvent[], n: number): WsEvent[] {
  return events.slice(0, n);
}

/** Helper: get a human-readable label for any WsEvent variant. */
export function eventLabel(e: WsEvent): string {
  switch (e.type) {
    case "epoch_start":
      return `Epoch #${e.epoch} started`;
    case "intents_received":
      return `${e.count} intents received`;
    case "solver_running":
      return `Solver running (${e.conflicts_detected} conflicts)`;
    case "solver_complete":
      return `Solver complete — plan ${e.plan_hash.slice(0, 10)}…`;
    case "proof_progress":
      return `${titleCaseProgram(e.program)} proof ${e.pct}%`;
    case "proof_complete":
      return `${titleCaseProgram(e.program)} proof done (${e.time_ms}ms)`;
    case "aggregation_start":
      return "Aggregation started";
    case "aggregation_complete":
      return `Aggregation complete (${e.time_ms}ms)`;
    case "groth16_wrapping":
      return `Groth16 wrapping ${e.pct}%`;
    case "epoch_settled":
      return `Epoch settled — ${e.tx_hash.slice(0, 10)}…`;
    case "epoch_settled_via_plan_b":
      return `Epoch settled via Plan-B — ${e.tx_hash.slice(0, 10)}…`;
    case "error":
      return `Error: ${e.message}`;
  }
}
