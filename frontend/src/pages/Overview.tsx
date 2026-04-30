import { useEffect, useMemo, useState } from "react";
import AgentCard from "@/components/prism/AgentCard";
import TelemetryMatrix from "@/components/prism/TelemetryMatrix";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";
import { lastShapley, currentEpoch } from "@/lib/derivedState";
import { AGENTS } from "@/lib/agents";

const Overview = () => {
  const { demo, wsUrl } = useDemoMode();
  const { events } = useWsEvents(wsUrl, !demo);

  const liveShapley = useMemo(
    () => (!demo ? lastShapley(events) : null),
    [demo, events],
  );
  const liveEpoch = useMemo(
    () => (!demo ? currentEpoch(events) : null),
    [demo, events],
  );

  // Demo-only ticker. NEVER consulted in live mode.
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!demo) return;
    const t = setInterval(() => setTick((x) => x + 1), 4000);
    return () => clearInterval(t);
  }, [demo]);

  const demoState = useMemo(() => {
    const seed = tick;
    const rand = (i: number) => (Math.sin(seed * 4.13 + i * 2.71) + 1) / 2;
    const raw = AGENTS.map((_, i) => 0.1 + rand(i) * 0.5);
    const total = raw.reduce((a, b) => a + b, 0);
    const payouts = raw.map((r) => Math.round((r / total) * 100));
    return AGENTS.map((a, i) => ({
      isActive: (seed + i) % 3 !== 0,
      action: a.actions[(seed + i) % a.actions.length],
      payout: payouts[i],
    }));
  }, [tick]);

  const liveState = useMemo(() => {
    if (!liveShapley) {
      return AGENTS.map(() => ({ isActive: false, action: "—", payout: 0 }));
    }
    return AGENTS.map((_, i) => ({
      isActive: (liveShapley[i] ?? 0) > 0,
      action: (liveShapley[i] ?? 0) > 0 ? "Settled" : "Idle",
      payout: Math.round((liveShapley[i] ?? 0) / 100),
    }));
  }, [liveShapley]);

  const state = demo ? demoState : liveState;

  return (
    <>
      <section className="container mx-auto pt-12 pb-8">
        <TelemetryMatrix />
      </section>

      <section className="container mx-auto pb-28">
        <div className="mb-10 flex items-end justify-between">
          <div>
            <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
              Route · /overview {!demo && liveEpoch !== null ? `· Epoch #${liveEpoch}` : ""}
            </p>
            <h1 className="display text-4xl md:text-5xl mt-2">Swarm Command</h1>
            {!demo && !liveShapley && (
              <p className="mono text-[11px] uppercase tracking-[0.12em] text-[hsl(var(--primary))] mt-3">
                Awaiting first settlement · payouts populate from on-chain Shapley split
              </p>
            )}
          </div>
          <p className="mono text-[11px] uppercase tracking-[0.12em] text-muted-foreground hidden md:block">
            5 agents · 1 protocol
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-12 gap-6">
          {AGENTS.map((a, i) => (
            <AgentCard
              key={a.key}
              className={a.colSpan}
              agent={a.key}
              symbol={a.symbol}
              name={a.name}
              description={a.description}
              uptime={a.uptime}
              priority={a.priority}
              status={state[i].isActive ? "active" : "idle"}
              lastAction={state[i].action}
              targetPayout={state[i].payout}
            />
          ))}
        </div>
      </section>
    </>
  );
};

export default Overview;
