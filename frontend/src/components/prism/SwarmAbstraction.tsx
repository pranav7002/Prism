import { useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";
import type { WsEvent } from "@/lib/wsClient";
import { eventLabel } from "@/lib/derivedState";
import { AGENTS, type AgentKey } from "@/lib/agents";

// Orbit angles for the 5 agent glyphs (degrees, clockwise from top).
const ORBIT_ANGLES: Record<AgentKey, number> = {
  alpha: -90,
  beta: -18,
  gamma: 54,
  delta: 126,
  epsilon: 198,
};

const orbs = AGENTS.map((a) => ({ ...a, angle: ORBIT_ANGLES[a.key] }));

// Demo intent stream — explicitly cosmetic, used only when `demo` is true.
const intentStream = [
  { from: "α", to: "β", payload: { intent: "forecast", pool: "USDC/ETH", conf: 0.94 } },
  { from: "β", to: "γ", payload: { intent: "rebalance", weights: [0.4, 0.6], slip: 0.01 } },
  { from: "γ", to: "δ", payload: { intent: "heal_drift", target: "stETH", delta: "+4.1" } },
  { from: "δ", to: "ε", payload: { intent: "capture_mev", hex: "0x4f2…", surplus: "0.4" } },
  { from: "ε", to: "ALL", payload: { intent: "attest_zk", proof: "77a8…", verified: true } },
];

/**
 * Project a real WsEvent into a small, judgement-friendly view object. Strips
 * fields that don't belong (e.g., we never display tx_hash as if it were an
 * "intent payload" — we describe what kind of event arrived).
 */
function projectLiveEvent(e: WsEvent): { from: string; to: string; payload: Record<string, unknown> } {
  const base = { from: "ORCHESTRATOR", to: "DASHBOARD", payload: {} as Record<string, unknown> };
  switch (e.type) {
    case "epoch_start":
      return { ...base, from: "ORCHESTRATOR", to: "SWARM", payload: { type: e.type, epoch: e.epoch } };
    case "intents_received":
      return { from: "SWARM", to: "ORCHESTRATOR", payload: { type: e.type, count: e.count, agents: e.agents } };
    case "solver_running":
      return { ...base, payload: { type: e.type, conflicts: e.conflicts_detected } };
    case "solver_complete":
      return { ...base, payload: { type: e.type, dropped: e.dropped.length } };
    case "proof_progress":
      return { ...base, payload: { type: e.type, program: e.program, pct: e.pct } };
    case "proof_complete":
      return { ...base, payload: { type: e.type, program: e.program, time_ms: e.time_ms } };
    case "aggregation_start":
    case "aggregation_complete":
      return { ...base, payload: { type: e.type } };
    case "groth16_wrapping":
      return { ...base, payload: { type: e.type, pct: e.pct } };
    case "epoch_settled":
    case "epoch_settled_via_plan_b":
      return { from: "ORCHESTRATOR", to: "CHAIN", payload: { type: e.type, gas: e.gas_used } };
    case "error":
      return { ...base, payload: { type: e.type, message: e.message } };
  }
}

const SwarmAbstraction = ({ onSelect }: { onSelect?: (a: AgentKey) => void }) => {
  const { demo, wsUrl } = useDemoMode();
  const { events, connected } = useWsEvents(wsUrl, !demo);

  const ref = useRef<HTMLDivElement>(null);
  const [mouse, setMouse] = useState({ x: 0, y: 0 });
  const [active, setActive] = useState<AgentKey | null>(null);
  const [intentIdx, setIntentIdx] = useState(0);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      const r = ref.current?.getBoundingClientRect();
      if (!r) return;
      setMouse({
        x: (e.clientX - (r.left + r.width / 2)) / (r.width / 2),
        y: (e.clientY - (r.top + r.height / 2)) / (r.height / 2),
      });
    };
    window.addEventListener("mousemove", onMove);
    return () => window.removeEventListener("mousemove", onMove);
  }, []);

  useEffect(() => {
    if (!demo) return;
    const t = setInterval(() => {
      setIntentIdx((i) => (i + 1) % intentStream.length);
    }, 3000);
    return () => clearInterval(t);
  }, [demo]);

  const radius = 200;

  // Live: project the most-recent event into a labelled view, plus a human
  // label below for context. Demo: rotate through the cosmetic stream.
  const liveProjection = !demo && events.length > 0 ? projectLiveEvent(events[0]) : null;
  const liveLabel = !demo && events.length > 0 ? eventLabel(events[0]) : null;

  const currentIntent = demo ? intentStream[intentIdx] : liveProjection;

  return (
    <div ref={ref} className="relative mx-auto h-[520px] w-[520px] max-w-full">
      {/* Central Intent Terminal */}
      <motion.div
        className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-48 h-48 bg-black/40 border border-white/10 rounded-xl flex flex-col overflow-hidden z-10"
        style={{ boxShadow: "0 0 40px rgba(0,0,0,0.5)" }}
      >
        <div className="bg-white/5 border-b border-white/10 p-2 flex justify-between items-center">
          <span className="mono text-[8px] uppercase text-muted-foreground tracking-widest">Topology P2P</span>
          <div className="flex gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-red-500/50" />
            <span className="w-1.5 h-1.5 rounded-full bg-yellow-500/50" />
            <span
              className={`w-1.5 h-1.5 rounded-full ${demo || connected ? "bg-green-500/70 animate-pulse" : "bg-red-500/70"}`}
              title={demo ? "Demo mode" : connected ? "WebSocket connected" : "Reconnecting…"}
            />
          </div>
        </div>
        <div className="flex-1 p-3 flex flex-col justify-center">
          {currentIntent ? (
            <>
              <p className="mono text-[10px] text-muted-foreground mb-2">
                Routing: <span className="text-white">{currentIntent.from} → {currentIntent.to}</span>
              </p>
              <pre className="mono text-[9px] text-[hsl(var(--primary))] leading-relaxed overflow-hidden">
                {JSON.stringify(currentIntent.payload, null, 2)}
              </pre>
              {!demo && liveLabel && (
                <p className="mono text-[9px] text-muted-foreground mt-2 truncate">{liveLabel}</p>
              )}
            </>
          ) : (
            <p className="mono text-[10px] text-muted-foreground text-center">
              {demo ? "Cycling demo intents…" : connected ? "Connected · awaiting events…" : "Connecting…"}
            </p>
          )}
        </div>
      </motion.div>

      {/* SVG Connection Lines */}
      <svg className="absolute inset-0 w-full h-full pointer-events-none" style={{ zIndex: 0 }}>
        <defs>
          <radialGradient id="glow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="hsl(var(--primary))" stopOpacity="0.4" />
            <stop offset="100%" stopColor="hsl(var(--primary))" stopOpacity="0" />
          </radialGradient>
        </defs>
        {orbs.map((o, i) => {
          const nextOrb = orbs[(i + 1) % orbs.length];
          const rad1 = (o.angle * Math.PI) / 180;
          const rad2 = (nextOrb.angle * Math.PI) / 180;
          const x1 = 260 + Math.cos(rad1) * radius;
          const y1 = 260 + Math.sin(rad1) * radius;
          const x2 = 260 + Math.cos(rad2) * radius;
          const y2 = 260 + Math.sin(rad2) * radius;
          const isActiveLine = demo && i === intentIdx;

          return (
            <motion.line
              key={i}
              x1={x1}
              y1={y1}
              x2={x2}
              y2={y2}
              stroke="url(#glow)"
              strokeWidth={isActiveLine ? 3 : 1}
              strokeOpacity={isActiveLine ? 1 : 0.2}
              strokeDasharray={isActiveLine ? "4 4" : "none"}
              animate={{ strokeDashoffset: isActiveLine ? [0, -20] : 0 }}
              transition={{ duration: 1, repeat: Infinity, ease: "linear" }}
            />
          );
        })}
      </svg>

      {/* Orbs */}
      {orbs.map((o) => {
        const rad = (o.angle * Math.PI) / 180;
        const baseX = Math.cos(rad) * radius;
        const baseY = Math.sin(rad) * radius;
        const dx = mouse.x * 24;
        const dy = mouse.y * 24;
        const isActive = active === o.key;
        return (
          <motion.button
            key={o.key}
            type="button"
            aria-pressed={isActive}
            aria-label={`${o.longName} — ${o.description}`}
            onClick={() => {
              setActive(o.key);
              onSelect?.(o.key);
            }}
            onMouseEnter={() => setActive(o.key)}
            onMouseLeave={() => setActive(null)}
            onFocus={() => setActive(o.key)}
            onBlur={() => setActive(null)}
            className="absolute left-1/2 top-1/2 group z-20 focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-[hsl(var(--primary))] rounded-full"
            initial={false}
            animate={{ x: baseX + dx - 28, y: baseY + dy - 28 }}
            transition={{ type: "spring", stiffness: 80, damping: 14, mass: 0.8 }}
          >
            <span
              className="grid h-14 w-14 place-items-center rounded-full glass-2 transition-all duration-300"
              style={{
                background: `radial-gradient(circle, ${o.color}33 0%, ${o.color}10 60%, transparent 100%)`,
                border: `1px solid ${isActive ? o.color : `${o.color}55`}`,
                boxShadow: isActive ? `0 0 40px ${o.color}` : `0 0 20px ${o.color}55`,
                transform: isActive ? "scale(1.15)" : "scale(1)",
              }}
            >
              <span className="display text-2xl" style={{ color: o.color }}>
                {o.symbol}
              </span>
            </span>
            {isActive && (
              <motion.div
                initial={{ opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                className="absolute left-1/2 top-full -translate-x-1/2 mt-3 w-48 text-center bg-black/60 backdrop-blur-md p-2 rounded border border-white/10"
              >
                <p className="mono text-[10px] uppercase tracking-[0.14em]" style={{ color: o.color }}>
                  {o.name}
                </p>
                <p className="text-[11px] text-foreground/70 mt-1 leading-snug">{o.description}</p>
              </motion.div>
            )}
          </motion.button>
        );
      })}
    </div>
  );
};

export default SwarmAbstraction;
