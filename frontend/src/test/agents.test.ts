import { describe, it, expect } from "vitest";
import { AGENTS, AGENT_COLORS, agentAtIndex } from "@/lib/agents";

describe("agents constant", () => {
  it("has exactly 5 agents in canonical order (α β γ δ ε)", () => {
    expect(AGENTS.map((a) => a.key)).toEqual(["alpha", "beta", "gamma", "delta", "epsilon"]);
    expect(AGENTS.map((a) => a.symbol)).toEqual(["α", "β", "γ", "δ", "ε"]);
  });

  it("AGENT_COLORS has a hsl() entry for every agent key", () => {
    for (const a of AGENTS) {
      expect(AGENT_COLORS[a.key]).toBe(a.color);
      expect(AGENT_COLORS[a.key]).toMatch(/^hsl\(var\(--agent-/);
    }
  });

  it("agentAtIndex maps shapley[i] back to the right agent", () => {
    // The orchestrator's shapley[] is indexed in canonical agent order, so
    // shapley[0] is alpha's payout, shapley[4] is epsilon's. This test pins
    // that contract.
    expect(agentAtIndex(0)?.key).toBe("alpha");
    expect(agentAtIndex(4)?.key).toBe("epsilon");
    expect(agentAtIndex(99)).toBeUndefined();
  });

  it("longName is consistently '<Name> <symbol>'", () => {
    for (const a of AGENTS) {
      expect(a.longName).toBe(`${a.name} ${a.symbol}`);
    }
  });
});
