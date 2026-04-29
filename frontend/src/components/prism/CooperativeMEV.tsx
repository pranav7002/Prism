import { useEffect, useState } from "react";
import { motion } from "framer-motion";

interface Intent {
  id: string;
  label: string;
  size: string;
}

const seedIntents: Intent[] = [
  { id: "i1", label: "Swap 10 ETH → USDC", size: "$36,420" },
  { id: "i2", label: "Provide Liq WETH/USDT", size: "$112,000" },
  { id: "i3", label: "Swap 250k USDC → ETH", size: "$250,000" },
  { id: "i4", label: "Remove Liq stETH/ETH", size: "$48,200" },
  { id: "i5", label: "Swap 1.2M USDT → ETH", size: "$1,200,000" },
  { id: "i6", label: "Swap 4 ETH → DAI", size: "$14,560" },
];

// Optimal reorder (deterministic for demo): big trade backrun-able first
const optimal = ["i5", "i3", "i1", "i6", "i2", "i4"];

const CooperativeMEV = () => {
  const [reordered, setReordered] = useState(false);

  useEffect(() => {
    const t = setInterval(() => setReordered((r) => !r), 4500);
    return () => clearInterval(t);
  }, []);

  const right = reordered
    ? optimal.map((id) => seedIntents.find((s) => s.id === id)!)
    : seedIntents;

  return (
    <div className="glass p-8" style={{ minHeight: 520 }}>
      <div className="mb-8 flex items-end justify-between">
        <div>
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Solver · MEV</p>
          <h2 className="display text-2xl md:text-3xl mt-2">Cooperative Reordering</h2>
        </div>
        <p className="mono text-[11px] uppercase tracking-[0.12em] text-muted-foreground capitalize">
          {reordered ? "Optimal Order" : "Intent Queue"}
        </p>
      </div>

      <div className="grid grid-cols-2 gap-8">
        <div>
          <p className="mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground mb-3">Incoming</p>
          <div className="space-y-2">
            {seedIntents.map((it) => (
              <div
                key={it.id}
                className="rounded-lg px-4 py-3 flex items-center justify-between"
                style={{ background: "hsl(var(--surface-1) / 0.5)", border: "1px solid hsl(var(--foreground) / 0.06)" }}
              >
                <span className="text-xs">{it.label}</span>
                <span className="mono text-[10px] text-muted-foreground tabular">{it.size}</span>
              </div>
            ))}
          </div>
        </div>

        <div>
          <p className="mono text-[10px] uppercase tracking-[0.14em] mb-3" style={{ color: "hsl(var(--agent-beta))" }}>
            Solver Output
          </p>
          <motion.div className="space-y-2" layout>
            {right.map((it, idx) => {
              const isMevSlot = reordered && idx === 1;
              return (
                <motion.div
                  key={it.id}
                  layout
                  transition={{ type: "spring", stiffness: 120, damping: 18 }}
                  className="relative rounded-lg px-4 py-3 flex items-center justify-between"
                  style={{
                    background: "hsl(var(--surface-2) / 0.6)",
                    border: `1px solid ${isMevSlot ? "hsl(var(--agent-beta) / 0.5)" : "hsl(var(--foreground) / 0.06)"}`,
                    boxShadow: isMevSlot ? "0 0 24px hsl(var(--agent-beta) / 0.3)" : "none",
                  }}
                >
                  <span className="text-xs">{it.label}</span>
                  <span className="mono text-[10px] text-muted-foreground tabular">{it.size}</span>
                  {isMevSlot && (
                    <motion.span
                      initial={{ opacity: 0, y: -4 }}
                      animate={{ opacity: 1, y: 0 }}
                      className="absolute -top-2 right-3 mono text-[9px] uppercase tracking-[0.14em] px-2 py-0.5 rounded-full"
                      style={{
                        color: "hsl(var(--agent-beta))",
                        background: "hsl(var(--background))",
                        border: "1px solid hsl(var(--agent-beta) / 0.6)",
                      }}
                    >
                      Yield +$420
                    </motion.span>
                  )}
                </motion.div>
              );
            })}
          </motion.div>
        </div>
      </div>
    </div>
  );
};

export default CooperativeMEV;