import { useEffect, useRef, useState } from "react";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";
import { recentEvents, eventLabel } from "@/lib/derivedState";
import type { WsEvent } from "@/lib/wsClient";

type AgentKey = "Predictive α" | "Curator β" | "Healer γ" | "Backrunner δ" | "Guardian ε";

interface Signal {
  id: number;
  ts: string;
  agent: AgentKey;
  color: string;
  message: string;
  hash: string;
}

const palette: Record<AgentKey, string> = {
  "Predictive α": "hsl(var(--agent-alpha))",
  "Curator β": "hsl(var(--agent-beta))",
  "Healer γ": "hsl(var(--agent-gamma))",
  "Backrunner δ": "hsl(var(--agent-delta))",
  "Guardian ε": "hsl(var(--agent-epsilon))",
};

const messages: { agent: AgentKey; message: string }[] = [
  { agent: "Guardian ε", message: "Risk parameter threshold verified." },
  { agent: "Predictive α", message: "Tick range projected for USDC/ETH pool." },
  { agent: "Curator β", message: "Liquidity weights rebalanced across 4 pools." },
  { agent: "Backrunner δ", message: "MEV opportunity captured. Cooperative payout queued." },
  { agent: "Healer γ", message: "Drift detected on stETH/ETH. Position auto-rebalanced." },
  { agent: "Predictive α", message: "Volatility regime classifier updated." },
  { agent: "Guardian ε", message: "ZK proof attested. Settlement lane open." },
  { agent: "Curator β", message: "Shapley payout vector computed for epoch." },
];

const randHash = () =>
  "0x" + Array.from({ length: 4 }, () => Math.floor(Math.random() * 0xffff).toString(16).padStart(4, "0")).join("") +
  "..." + Math.floor(Math.random() * 0xffff).toString(16).padStart(4, "0");

const fmtTime = () => {
  const d = new Date();
  const pad = (n: number, l = 2) => n.toString().padStart(l, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
};

// Round-robin agent assignment for live events (no real per-agent tagging in WsEvent)
const liveAgentRoster: AgentKey[] = [
  "Predictive α", "Curator β", "Healer γ", "Backrunner δ", "Guardian ε",
];
let liveAgentIdx = 0;

function wsEventToSignal(e: WsEvent, id: number): Signal {
  const agent = liveAgentRoster[liveAgentIdx % liveAgentRoster.length];
  liveAgentIdx += 1;
  return {
    id,
    ts: fmtTime(),
    agent,
    color: palette[agent],
    message: eventLabel(e),
    hash: randHash(),
  };
}

const SignalLedger = () => {
  const { demo, wsUrl } = useDemoMode();
  const { events, connected } = useWsEvents(wsUrl, !demo);

  const [demoSignals, setDemoSignals] = useState<Signal[]>([]);
  const idRef = useRef(0);
  const [hovered, setHovered] = useState<number | null>(null);

  // Demo mode: interval-driven fake signals
  useEffect(() => {
    if (!demo) return;

    // Seed
    const seed = Array.from({ length: 6 }).map(() => {
      const m = messages[Math.floor(Math.random() * messages.length)];
      idRef.current += 1;
      return { id: idRef.current, ts: fmtTime(), agent: m.agent, color: palette[m.agent], message: m.message, hash: randHash() };
    });
    setDemoSignals(seed.reverse());

    const t = setInterval(() => {
      const m = messages[Math.floor(Math.random() * messages.length)];
      idRef.current += 1;
      setDemoSignals(prev => [
        { id: idRef.current, ts: fmtTime(), agent: m.agent, color: palette[m.agent], message: m.message, hash: randHash() },
        ...prev,
      ].slice(0, 9));
    }, 1800);
    return () => clearInterval(t);
  }, [demo]);

  // Live mode: derive signals from recent WsEvents
  const liveSignals: Signal[] = !demo
    ? recentEvents(events, 20).map((e, i) => wsEventToSignal(e, i))
    : [];

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
                <span className="mono text-[11px] text-muted-foreground/80 text-right tabular">{s.hash}</span>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
};

export default SignalLedger;
