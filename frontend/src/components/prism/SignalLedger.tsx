import { useEffect, useMemo, useRef, useState } from "react";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";
import { recentEvents, eventLabel } from "@/lib/derivedState";
import type { WsEvent } from "@/lib/wsClient";
import { AGENTS } from "@/lib/agents";

type AgentLong = typeof AGENTS[number]["longName"];
const palette: Record<AgentLong, string> = AGENTS.reduce(
  (acc, a) => ({ ...acc, [a.longName]: a.color }),
  {} as Record<AgentLong, string>,
);
const liveAgentRoster: AgentLong[] = AGENTS.map((a) => a.longName);

interface Signal {
  id: number;
  ts: string;
  agent: AgentLong;
  color: string;
  message: string;
  hash: string;
}

const messages: { agent: AgentLong; message: string }[] = [
  { agent: "Guardian ε", message: "Risk parameter threshold verified." },
  { agent: "Predictive α", message: "Tick range projected for USDC/ETH pool." },
  { agent: "Curator β", message: "Liquidity weights rebalanced across 4 pools." },
  { agent: "Backrunner δ", message: "MEV opportunity captured. Cooperative payout queued." },
  { agent: "Healer γ", message: "Drift detected on stETH/ETH. Position auto-rebalanced." },
  { agent: "Predictive α", message: "Volatility regime classifier updated." },
  { agent: "Guardian ε", message: "ZK proof attested. Settlement lane open." },
  { agent: "Curator β", message: "Shapley payout vector computed for epoch." },
];

// Stable hash generator — called ONCE per signal at creation time. Pre-hex16
// chars so it looks like a real abbreviated hash and never mutates after.
function stableHash(): string {
  return (
    "0x" +
    Array.from({ length: 4 }, () => Math.floor(Math.random() * 0xffff).toString(16).padStart(4, "0")).join("") +
    "..." +
    Math.floor(Math.random() * 0xffff).toString(16).padStart(4, "0")
  );
}

const fmtTime = () => {
  const d = new Date();
  const pad = (n: number, l = 2) => n.toString().padStart(l, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
};

/**
 * Convert a WsEvent into a Signal. The agent index is supplied by the caller
 * so each component instance can keep its own counter (no module-level state).
 */
function wsEventToSignal(e: WsEvent, id: number, agentIdx: number): Signal {
  const agent = liveAgentRoster[agentIdx % liveAgentRoster.length];
  let realHash = "—";
  if (e.type === "solver_complete") realHash = e.plan_hash;
  else if (e.type === "epoch_settled" || e.type === "epoch_settled_via_plan_b") realHash = e.tx_hash;

  return {
    id,
    ts: fmtTime(),
    agent,
    color: palette[agent],
    message: eventLabel(e),
    hash: realHash,
  };
}

const SignalLedger = () => {
  const { demo, wsUrl } = useDemoMode();
  const { events, connected } = useWsEvents(wsUrl, !demo);

  const [demoSignals, setDemoSignals] = useState<Signal[]>([]);
  const idRef = useRef(0);
  const liveAgentIdxRef = useRef(0); // per-instance, no module state
  const [hovered, setHovered] = useState<number | null>(null);

  // Demo mode: interval-driven canned signals. Hash is generated once at
  // creation and frozen on the Signal object.
  useEffect(() => {
    if (!demo) return;

    const seed = Array.from({ length: 6 }).map(() => {
      const m = messages[Math.floor(Math.random() * messages.length)];
      idRef.current += 1;
      return {
        id: idRef.current,
        ts: fmtTime(),
        agent: m.agent,
        color: palette[m.agent],
        message: m.message,
        hash: stableHash(),
      };
    });
    setDemoSignals(seed.reverse());

    const t = setInterval(() => {
      const m = messages[Math.floor(Math.random() * messages.length)];
      idRef.current += 1;
      const newSignal: Signal = {
        id: idRef.current,
        ts: fmtTime(),
        agent: m.agent,
        color: palette[m.agent],
        message: m.message,
        hash: stableHash(),
      };
      setDemoSignals((prev) => [newSignal, ...prev].slice(0, 9));
    }, 1800);
    return () => clearInterval(t);
  }, [demo]);

  // Live mode: derive signals from recent events. Memoized so we only rebuild
  // when events array changes, and per-instance counter makes the agent
  // assignment stable across renders.
  const liveSignals = useMemo<Signal[]>(() => {
    if (demo) return [];
    return recentEvents(events, 20).map((e, i) => {
      const sig = wsEventToSignal(e, i, liveAgentIdxRef.current + i);
      return sig;
    });
  }, [demo, events]);

  // Bump the agent counter once per new event batch so the rotation advances
  // monotonically across renders.
  useEffect(() => {
    if (!demo) liveAgentIdxRef.current += 1;
  }, [demo, events.length]);

  const signals = demo ? demoSignals : liveSignals;

  return (
    <section aria-label="Cryptographic Signal Ledger" className="relative">
      <div className="mb-6 flex items-end justify-between">
        <div>
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Section · 03</p>
          <h2 className="display text-3xl md:text-4xl mt-2">Cryptographic Signal Ledger</h2>
        </div>
        <p className="mono text-[11px] uppercase tracking-[0.12em] text-muted-foreground hidden md:block">
          {!demo && connected
            ? "Live · WebSocket · 4Hz throttle"
            : !demo && !connected
            ? "Live · Connecting…"
            : "Demo · Simulated · 4Hz throttle"}
        </p>
      </div>

      <div
        className="relative overflow-hidden"
        style={{
          maskImage: "linear-gradient(to bottom, transparent 0%, black 16%, black 100%)",
          WebkitMaskImage: "linear-gradient(to bottom, transparent 0%, black 16%, black 100%)",
        }}
      >
        <div className="space-y-1 py-8">
          {signals.length === 0 && !demo && (
            <p className="mono text-[11px] text-muted-foreground py-6 px-2">
              {connected ? "Awaiting events…" : "Reconnecting…"}
            </p>
          )}
          {signals.map((s) => {
            const dim = hovered !== null && hovered !== s.id;
            return (
              <div
                key={s.id}
                onMouseEnter={() => setHovered(s.id)}
                onMouseLeave={() => setHovered(null)}
                className="grid grid-cols-[110px_180px_1fr_140px] items-center gap-6 px-2 py-2 rounded-md transition-opacity duration-200 animate-ledger-in"
                style={{ opacity: dim ? 0.3 : 1 }}
              >
                <span className="mono text-[11px] text-muted-foreground tabular">[{s.ts}]</span>
                <span
                  className="mono text-[11px] uppercase tracking-[0.08em] px-2 py-1 rounded-md w-fit"
                  style={{ color: s.color, background: `${s.color}1a`, border: `1px solid ${s.color}33` }}
                >
                  {s.agent}
                </span>
                <span className="text-sm text-foreground/90 truncate">{s.message}</span>
                <span className="mono text-[11px] text-muted-foreground/80 text-right tabular truncate">
                  {s.hash}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
};

export default SignalLedger;
