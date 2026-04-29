import { useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import type { AgentKey } from "./AgentGlyph";

interface Orb {
  agent: AgentKey;
  symbol: string;
  name: string;
  role: string;
  angle: number;
  color: string;
}

const orbs: Orb[] = [
  { agent: "alpha", symbol: "α", name: "Predictive", role: "Forecasts tick ranges and volatility regimes.", angle: -90, color: "hsl(var(--agent-alpha))" },
  { agent: "beta", symbol: "β", name: "Curator", role: "Rebalances liquidity weights across pools.", angle: -18, color: "hsl(var(--agent-beta))" },
  { agent: "gamma", symbol: "γ", name: "Healer", role: "Detects drift and auto-rebalances positions.", angle: 54, color: "hsl(var(--agent-gamma))" },
  { agent: "delta", symbol: "δ", name: "Backrunner", role: "Captures cooperative MEV for LPs.", angle: 126, color: "hsl(var(--agent-delta))" },
  { agent: "epsilon", symbol: "ε", name: "Guardian", role: "Attests every action with a ZK proof.", angle: 198, color: "hsl(var(--agent-epsilon))" },
];

const SwarmAbstraction = ({ onSelect }: { onSelect?: (a: AgentKey) => void }) => {
  const ref = useRef<HTMLDivElement>(null);
  const [mouse, setMouse] = useState({ x: 0, y: 0 });
  const [active, setActive] = useState<AgentKey | null>(null);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      const r = ref.current?.getBoundingClientRect();
      if (!r) return;
      setMouse({ x: (e.clientX - (r.left + r.width / 2)) / (r.width / 2), y: (e.clientY - (r.top + r.height / 2)) / (r.height / 2) });
    };
    window.addEventListener("mousemove", onMove);
    return () => window.removeEventListener("mousemove", onMove);
  }, []);

  const radius = 180;

  return (
    <div ref={ref} className="relative mx-auto h-[460px] w-[460px] max-w-full">
      {/* central core */}
      <motion.div
        className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 grid place-items-center"
        animate={{ scale: [1, 1.05, 1] }}
        transition={{ duration: 4, repeat: Infinity, ease: "easeInOut" }}
      >
        <div
          className="h-20 w-20 rounded-full"
          style={{
            background: "radial-gradient(circle, hsl(var(--primary) / 0.6) 0%, hsl(var(--primary-glow) / 0.2) 60%, transparent 100%)",
            boxShadow: "0 0 80px hsl(var(--primary) / 0.5)",
          }}
        />
        <div className="absolute h-3 w-3 rounded-full bg-foreground" style={{ boxShadow: "0 0 24px hsl(var(--foreground))" }} />
      </motion.div>

      {/* orbital ring */}
      <div
        className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full border border-foreground/[0.06]"
        style={{ height: radius * 2, width: radius * 2 }}
      />

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
            className="absolute left-1/2 top-1/2 group"
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
                className="absolute left-1/2 top-full -translate-x-1/2 mt-3 w-48 text-center"
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