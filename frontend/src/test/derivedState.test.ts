import { describe, it, expect } from "vitest";
import {
  currentEpoch,
  proofProgress,
  lastShapley,
  lastSettlePath,
  solverConflicts,
  recentEvents,
  eventLabel,
} from "@/lib/derivedState";
import type { WsEvent } from "@/lib/wsClient";

const EPOCH_START: WsEvent = { type: "epoch_start", epoch: 42, timestamp: 1700000000 };
const SOLVER_RUNNING: WsEvent = { type: "solver_running", conflicts_detected: 3 };
const PROOF_PROGRESS_SOLVER: WsEvent = { type: "proof_progress", program: "solver", pct: 75 };
const PROOF_PROGRESS_EXECUTION: WsEvent = { type: "proof_progress", program: "execution", pct: 50 };
const EPOCH_SETTLED: WsEvent = {
  type: "epoch_settled",
  tx_hash: "0xdeadbeef",
  gas_used: 120000,
  shapley: [2000, 2500, 1500, 2000, 2000],
};
const EPOCH_SETTLED_PLAN_B: WsEvent = {
  type: "epoch_settled_via_plan_b",
  tx_hash: "0xfeedface",
  gas_used: 480000,
  shapley: [4000, 2500, 2000, 1500, 0],
};

describe("derivedState", () => {
  describe("currentEpoch", () => {
    it("returns null for empty events", () => {
      expect(currentEpoch([])).toBeNull();
    });

    it("returns the epoch from the first EpochStart event", () => {
      expect(currentEpoch([EPOCH_START, SOLVER_RUNNING])).toBe(42);
    });

    it("returns the most-recent epoch (first in array = newest)", () => {
      const older: WsEvent = { type: "epoch_start", epoch: 41, timestamp: 1699999000 };
      expect(currentEpoch([EPOCH_START, older])).toBe(42);
    });
  });

  describe("proofProgress", () => {
    it("returns empty object for empty events", () => {
      expect(proofProgress([])).toEqual({});
    });

    it("returns latest pct per program (PascalCase keys)", () => {
      const result = proofProgress([PROOF_PROGRESS_SOLVER, PROOF_PROGRESS_EXECUTION]);
      expect(result["Solver"]).toBe(75);
      expect(result["Execution"]).toBe(50);
    });

    it("takes the most-recent value when program appears multiple times", () => {
      const older: WsEvent = { type: "proof_progress", program: "solver", pct: 40 };
      // Newest first in array
      const result = proofProgress([PROOF_PROGRESS_SOLVER, older]);
      expect(result["Solver"]).toBe(75);
    });
  });

  describe("lastShapley", () => {
    it("returns null for empty events", () => {
      expect(lastShapley([])).toBeNull();
    });

    it("returns the shapley array from EpochSettled", () => {
      const result = lastShapley([EPOCH_SETTLED]);
      expect(result).toEqual([2000, 2500, 1500, 2000, 2000]);
    });

    it("also reads from EpochSettledViaPlanB", () => {
      const result = lastShapley([EPOCH_SETTLED_PLAN_B]);
      expect(result).toEqual([4000, 2500, 2000, 1500, 0]);
    });
  });

  describe("lastSettlePath", () => {
    it("returns null when no settlement event has fired", () => {
      expect(lastSettlePath([])).toBeNull();
      expect(lastSettlePath([EPOCH_START, SOLVER_RUNNING])).toBeNull();
    });

    it("returns 'groth16' on the happy path", () => {
      expect(lastSettlePath([EPOCH_SETTLED])).toBe("groth16");
    });

    it("returns 'plan-b' when the most recent settle was the fallback", () => {
      expect(lastSettlePath([EPOCH_SETTLED_PLAN_B])).toBe("plan-b");
    });

    it("uses the most-recent settlement (first in array)", () => {
      // Newest first in the events array — Plan-B should win.
      expect(lastSettlePath([EPOCH_SETTLED_PLAN_B, EPOCH_SETTLED])).toBe("plan-b");
      // And vice-versa.
      expect(lastSettlePath([EPOCH_SETTLED, EPOCH_SETTLED_PLAN_B])).toBe("groth16");
    });
  });

  describe("solverConflicts", () => {
    it("returns 0 for empty events", () => {
      expect(solverConflicts([])).toBe(0);
    });

    it("returns conflicts_detected from latest SolverRunning", () => {
      expect(solverConflicts([SOLVER_RUNNING])).toBe(3);
    });
  });

  describe("recentEvents", () => {
    it("returns at most n events", () => {
      const many = Array.from({ length: 10 }, () => EPOCH_START);
      expect(recentEvents(many, 5)).toHaveLength(5);
    });

    it("returns all events when fewer than n", () => {
      expect(recentEvents([EPOCH_START, SOLVER_RUNNING], 20)).toHaveLength(2);
    });
  });

  describe("eventLabel", () => {
    it("formats EpochStart", () => {
      expect(eventLabel(EPOCH_START)).toBe("Epoch #42 started");
    });

    it("formats EpochSettled with truncated hash", () => {
      expect(eventLabel(EPOCH_SETTLED)).toContain("settled");
    });

    it("formats EpochSettledViaPlanB with the Plan-B label", () => {
      expect(eventLabel(EPOCH_SETTLED_PLAN_B)).toContain("Plan-B");
    });

    it("formats ProofProgress with title-cased program name", () => {
      expect(eventLabel(PROOF_PROGRESS_SOLVER)).toBe("Solver proof 75%");
    });

    it("parses raw orchestrator wire shape (regression: internally-tagged)", () => {
      // This is the exact shape `ws_send(e.to_json())` produces on the
      // Rust side. The earlier externally-tagged TS type silently mismatched
      // it — captured here to prevent regression.
      const raw = JSON.parse(
        '{"type":"epoch_settled","tx_hash":"0xabc","gas_used":260000,"shapley":[10000,0,0,0,0]}'
      ) as WsEvent;
      expect(eventLabel(raw)).toContain("settled");
      expect(lastShapley([raw])).toEqual([10000, 0, 0, 0, 0]);
      expect(lastSettlePath([raw])).toBe("groth16");
    });
  });

  describe("live-mode no-fake-data invariants", () => {
    // These pin the contract that, in live mode with empty events, every
    // selector returns a falsy/empty value rather than a fabricated stub.
    // If a future selector starts returning Math.random() defaults, this
    // suite breaks.
    it("currentEpoch on [] is null (no fabricated epoch number)", () => {
      expect(currentEpoch([])).toBeNull();
    });
    it("lastShapley on [] is null (no fabricated payout split)", () => {
      expect(lastShapley([])).toBeNull();
    });
    it("lastSettlePath on [] is null (no claimed groth16/plan-b)", () => {
      expect(lastSettlePath([])).toBeNull();
    });
    it("solverConflicts on [] is 0 (no fabricated conflict count)", () => {
      expect(solverConflicts([])).toBe(0);
    });
    it("proofProgress on [] is empty object (no canned 75%)", () => {
      expect(proofProgress([])).toEqual({});
    });
  });
});
