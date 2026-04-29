import { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import type { AgentKey } from "./AgentGlyph";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";

interface Orb {
  agent: AgentKey;
  symbol: string;
  name: string;
  role: string;
  angle: number;
  color: string;
}

const orbs: Orb[] = [
  { agent: "alpha", symbol: "α", name: "Predictive", role: "Forecasts tick ranges.", angle: -90, color: "hsl(var(--agent-alpha))" },
  { agent: "beta", symbol: "β", name: "Curator", role: "Rebalances liquidity.", angle: -18, color: "hsl(var(--agent-beta))" },
  { agent: "gamma", symbol: "γ", name: "Healer", role: "Detects position drift.", angle: 54, color: "hsl(var(--agent-gamma))" },
  { agent: "delta", symbol: "δ", name: "Backrunner", role: "Captures cooperative MEV.", angle: 126, color: "hsl(var(--agent-delta))" },
  { agent: "epsilon", symbol: "ε", name: "Guardian", role: "Attests every ZK proof.", angle: 198, color: "hsl(var(--agent-epsilon))" },
];

const intentStream = [
  { from: "α", to: "β", payload: { intent: "forecast", pool: "USDC/ETH", conf: 0.94 } },
  { from: "β", to: "γ", payload: { intent: "rebalance", weights: [0.4, 0.6], slip: 0.01 } },
  { from: "γ", to: "δ", payload: { intent: "heal_drift", target: "stETH", delta: "+4.1" } },
  { from: "δ", to: "ε", payload: { intent: "capture_mev", hex: "0x4f2...", surplus: "0.4" } },
  { from: "ε", to: "ALL", payload: { intent: "attest_zk", proof: "77a8", verified: true } },
];

const SwarmAbstraction = ({ onSelect }: { onSelect?: (a: AgentKey) => void }) => {
  const { demo, wsUrl } = useDemoMode();
  const { events } = useWsEvents(wsUrl, !demo);

  const ref = useRef<HTMLDivElement>(null);
  const [mouse, setMouse] = useState({ x: 0, y: 0 });
  const [active, setActive] = useState<AgentKey | null>(null);
  const [intentIdx, setIntentIdx] = useState(0);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      const r = ref.current?.getBoundingClientRect();
      if (!r) return;
      setMouse({ x: (e.clientX - (r.left + r.width / 2)) / (r.width / 2), y: (e.clientY - (r.top + r.height / 2)) / (r.height / 2) });
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
  
  const liveIntent = events.length > 0 ? {
    from: "SWARM",
    to: "ORCHESTRATOR",
    payload: { ...events[0] }
  } : null;

  const currentIntent = demo ? intentStream[intentIdx] : liveIntent;

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
            <span className="w-1.5 h-1.5 rounded-full bg-green-500/50 animate-pulse" />
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
            </>
          ) : (
            <p className="mono text-[10px] text-muted-foreground text-center">Listening for Live P2P Intents...</p>
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
          
          const isActiveLine = i === intentIdx;

          return (
            <motion.line
              key={i}
              x1={x1} y1={y1} x2={x2} y2={y2}
              stroke="url(#glow)"
              strokeWidth={isActiveLine ? 3 : 1}
              strokeOpacity={isActiveLine ? 1 : 0.2}
              strokeDasharray={isActiveLine ? "4 4" : "none"}
              animate={{
                strokeDashoffset: isActiveLine ? [0, -20] : 0
              }}
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
        const isActive = active === o.agent;
        return (
          <motion.button
            key={o.agent}
            onClick={() => {
              setActive(o.agent);
              onSelect?.(o.agent);
            }}
            onMouseEnter={() => setActive(o.agent)}
            onMouseLeave={() => setActive(null)}
            className="absolute left-1/2 top-1/2 group z-20"
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
                <p className="text-[11px] text-foreground/70 mt-1 leading-snug">{o.role}</p>
              </motion.div>
            )}
          </motion.button>
        );
      })}
    </div>
  );
};

export default SwarmAbstraction;