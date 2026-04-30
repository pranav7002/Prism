// Single source of truth for the 5-agent metadata shared across pages and
// components. Replaces the triplicated `baseAgents` arrays previously copied
// in Landing.tsx / Overview.tsx / Settlement.tsx and the `colorVar` records
// in AgentGlyph / AgentCard / SignalLedger.

export type AgentKey = "alpha" | "beta" | "gamma" | "delta" | "epsilon";

export interface AgentMeta {
  key: AgentKey;
  symbol: string;          // α β γ δ ε
  name: string;            // "Predictive"
  longName: string;        // "Predictive α"
  description: string;
  color: string;           // CSS hsl(var(--agent-…)) reference
  // Illustrative chrome (no orchestrator event maps to these — they are
  // static decoration around the live data on /overview).
  uptime: string;
  priority: string;
  colSpan: string;
  actions: string[];
}

export const AGENTS: AgentMeta[] = [
  {
    key: "alpha",
    symbol: "α",
    name: "Predictive",
    longName: "Predictive α",
    description:
      "Forecasts tick ranges and volatility regimes ahead of every epoch using ZK-verified models.",
    color: "hsl(var(--agent-alpha))",
    uptime: "99.9%",
    priority: "8.4",
    colSpan: "md:col-span-6",
    actions: ["Forecast", "Recompute", "Tick+1", "Snap"],
  },
  {
    key: "beta",
    symbol: "β",
    name: "Curator",
    longName: "Curator β",
    description:
      "Rebalances liquidity weights across pools to maximize fee yield under risk constraints.",
    color: "hsl(var(--agent-beta))",
    uptime: "99.8%",
    priority: "7.6",
    colSpan: "md:col-span-4",
    actions: ["AddLiq", "Rebal", "Withdraw", "Hold"],
  },
  {
    key: "gamma",
    symbol: "γ",
    name: "Healer",
    longName: "Healer γ",
    description:
      "Detects drift and auto-rebalances drained or impaired positions across the swarm.",
    color: "hsl(var(--agent-gamma))",
    uptime: "99.6%",
    priority: "7.2",
    colSpan: "md:col-span-4",
    actions: ["Heal", "Scan", "Rotate", "—"],
  },
  {
    key: "delta",
    symbol: "δ",
    name: "Backrunner",
    longName: "Backrunner δ",
    description:
      "Captures cooperative MEV and routes the surplus back to LPs via Shapley settlement.",
    color: "hsl(var(--agent-delta))",
    uptime: "99.7%",
    priority: "9.1",
    colSpan: "md:col-span-6",
    actions: ["Backrun", "Bundle", "Submit", "Idle"],
  },
  {
    key: "epsilon",
    symbol: "ε",
    name: "Guardian",
    longName: "Guardian ε",
    description:
      "Attests every action with a ZK proof and enforces protocol-level risk thresholds.",
    color: "hsl(var(--agent-epsilon))",
    uptime: "100.0%",
    priority: "9.8",
    colSpan: "md:col-span-4",
    actions: ["Attest", "Verify", "Sign", "Lock"],
  },
];

export const AGENT_COLORS: Record<AgentKey, string> = AGENTS.reduce(
  (acc, a) => ({ ...acc, [a.key]: a.color }),
  {} as Record<AgentKey, string>,
);

/** Convenience: get the meta for a given index (used to map shapley[i] → agent). */
export function agentAtIndex(i: number): AgentMeta | undefined {
  return AGENTS[i];
}
