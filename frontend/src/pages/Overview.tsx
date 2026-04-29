import { useEffect, useState } from "react";
import AgentCard from "@/components/prism/AgentCard";
import TelemetryMatrix from "@/components/prism/TelemetryMatrix";

const baseAgents = [
  { agent: "alpha" as const, symbol: "α", name: "Predictive", description: "Forecasts tick ranges and volatility regimes ahead of every epoch using ZK-verified models.", uptime: "99.9%", priority: "8.4", colSpan: "md:col-span-6", actions: ["Forecast", "Recompute", "Tick+1", "Snap"] },
  { agent: "delta" as const, symbol: "δ", name: "Backrunner", description: "Captures cooperative MEV and routes the surplus back to LPs via Shapley settlement.", uptime: "99.7%", priority: "9.1", colSpan: "md:col-span-6", actions: ["Backrun", "Bundle", "Submit", "Idle"] },
  { agent: "beta" as const, symbol: "β", name: "Curator", description: "Rebalances liquidity weights across pools to maximize fee yield under risk constraints.", uptime: "99.8%", priority: "7.6", colSpan: "md:col-span-4", actions: ["AddLiq", "Rebal", "Withdraw", "Hold"] },
  { agent: "gamma" as const, symbol: "γ", name: "Healer", description: "Detects drift and auto-rebalances drained or impaired positions across the swarm.", uptime: "99.6%", priority: "7.2", colSpan: "md:col-span-4", actions: ["Heal", "Scan", "Rotate", "—"] },
  { agent: "epsilon" as const, symbol: "ε", name: "Guardian", description: "Attests every action with a ZK proof and enforces protocol-level risk thresholds.", uptime: "100.0%", priority: "9.8", colSpan: "md:col-span-4", actions: ["Attest", "Verify", "Sign", "Lock"] },
];

const Overview = () => {
  const [tick, setTick] = useState(0);
  // Throttled to ~4Hz per PRD
  useEffect(() => {
    const t = setInterval(() => setTick((x) => x + 1), 250);
    return () => clearInterval(t);
  }, []);

  // derive payouts that sum to 100, deterministic per tick window
  const seed = Math.floor(tick / 16); // change every ~4s
  const rand = (i: number) => (Math.sin(seed * 4.13 + i * 2.71) + 1) / 2;
  const raw = baseAgents.map((_, i) => 0.1 + rand(i) * 0.5);
  const total = raw.reduce((a, b) => a + b, 0);
  const payouts = raw.map((r) => Math.round((r / total) * 100));

  return (
    <>
      <section className="container mx-auto pt-12 pb-8">
        <TelemetryMatrix />
      </section>

      <section className="container mx-auto pb-28">
        <div className="mb-10 flex items-end justify-between">
          <div>
            <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Route · /overview</p>
            <h1 className="display text-4xl md:text-5xl mt-2">Swarm Command</h1>
          </div>
          <p className="mono text-[11px] uppercase tracking-[0.12em] text-muted-foreground hidden md:block">
            5 agents · 1 protocol
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-12 gap-6">
          {baseAgents.map((a, i) => {
            const isActive = (seed + i) % 3 !== 0;
            const action = a.actions[(seed + i) % a.actions.length];
            return (
              <AgentCard
                key={a.agent}
                className={a.colSpan}
                agent={a.agent}
                symbol={a.symbol}
                name={a.name}
                description={a.description}
                uptime={a.uptime}
                priority={a.priority}
                status={isActive ? "active" : "idle"}
                lastAction={action}
                targetPayout={payouts[i]}
              />
            );
          })}
        </div>
      </section>
    </>
  );
};

export default Overview;