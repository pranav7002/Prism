import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";
import { proofProgress } from "@/lib/derivedState";

type Track = { key: string; label: string; color: string };
// Track keys match WsEvent ProofProgress program names (PascalCase)
const tracks: Track[] = [
  { key: "Solver", label: "Solver", color: "hsl(var(--agent-alpha))" },
  { key: "Execution", label: "Execution", color: "hsl(var(--agent-beta))" },
  { key: "Shapley", label: "Shapley", color: "hsl(var(--agent-delta))" },
];

type Phase = "generating" | "merging" | "wrapping" | "verified";

const EMPTY_PROGRESS: Record<string, number> = { Solver: 0, Execution: 0, Shapley: 0 };

const ProofPipeline = () => {
  const { demo, wsUrl } = useDemoMode();
  const { events } = useWsEvents(wsUrl, !demo);

  // Live-derived progress (only used when !demo)
  const liveProgress = !demo ? proofProgress(events) : null;
  const hasLiveData = liveProgress !== null && Object.keys(liveProgress).length > 0;

  const [phase, setPhase] = useState<Phase>("generating");
  const [demoProgress, setDemoProgress] = useState<Record<string, number>>(EMPTY_PROGRESS);

  // Demo animation — only runs when demo=true
  useEffect(() => {
    if (!demo) return;

    let raf: ReturnType<typeof setTimeout>;
    let cancelled = false;

    const cycle = () => {
      setPhase("generating");
      setDemoProgress(EMPTY_PROGRESS);
      const tick = () => {
        if (cancelled) return;
        setDemoProgress((p) => {
          const next = {
            Solver: Math.min(100, p.Solver + Math.random() * 4),
            Execution: Math.min(100, p.Execution + Math.random() * 3.2),
            Shapley: Math.min(100, p.Shapley + Math.random() * 4.6),
          };
          if (next.Solver >= 100 && next.Execution >= 100 && next.Shapley >= 100) {
            setPhase("merging");
            setTimeout(() => !cancelled && setPhase("wrapping"), 900);
            setTimeout(() => !cancelled && setPhase("verified"), 1800);
            setTimeout(() => !cancelled && cycle(), 4200);
            return next;
          }
          raf = setTimeout(tick, 80);
          return next;
        });
      };
      tick();
    };

    cycle();
    return () => {
      cancelled = true;
      clearTimeout(raf);
    };
  }, [demo]);

  // Determine which progress values to display
  const progress = hasLiveData ? liveProgress! : demoProgress;

  const liveString = tracks
    .map((t) => `${t.label} ${Math.floor(progress[t.key] ?? 0)}%`)
    .join(", ");

  return (
    <div className="glass p-8 md:p-10" style={{ minHeight: 460 }}>
      <div className="mb-8 flex items-end justify-between">
        <div>
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Cryptographic · Pipeline</p>
          <h2 className="display text-2xl md:text-3xl mt-2">Proof Generation</h2>
        </div>
        <p className="mono text-[11px] uppercase tracking-[0.12em] text-muted-foreground capitalize">
          {phase}
        </p>
      </div>

      <div className="grid grid-cols-12 gap-6 items-stretch" style={{ minHeight: 280 }}>
        {/* Tracks */}
        <div className="col-span-12 md:col-span-5 flex flex-col justify-between">
          {tracks.map((t) => {
            const pct = Math.min(100, Math.floor(progress[t.key] ?? 0));
            const merging = phase === "merging" || phase === "wrapping" || phase === "verified";
            return (
              <motion.div
                key={t.key}
                animate={{ opacity: merging && demo ? 0.4 : 1, x: merging && demo ? 40 : 0 }}
                transition={{ duration: 0.6, ease: [0.16, 1, 0.3, 1] }}
                className="py-1"
              >
                <div className="flex items-center justify-between mb-1.5">
                  <span className="mono text-[10px] uppercase tracking-[0.12em] text-muted-foreground">
                    {t.label}
                  </span>
                  <span className="mono text-[10px] tabular" style={{ color: t.color }}>
                    {pct}%
                  </span>
                </div>
                <div
                  className="relative h-3 rounded-full overflow-hidden"
                  style={{
                    background: "hsl(var(--surface-2) / 0.6)",
                    boxShadow: "inset 0 1px 2px rgba(0,0,0,0.4)",
                  }}
                  role="progressbar"
                  aria-valuenow={pct}
                  aria-valuemin={0}
                  aria-valuemax={100}
                >
                  <div
                    className="absolute inset-y-0 left-0 transition-[width] duration-200"
                    style={{
                      width: `${pct}%`,
                      background: `linear-gradient(90deg, ${t.color}55, ${t.color})`,
                      boxShadow: `0 0 16px ${t.color}, inset 0 0 0 1px ${t.color}aa`,
                    }}
                  />
                  {/* diagonal stripes */}
                  <div
                    className="absolute inset-0 opacity-30 mix-blend-overlay"
                    style={{
                      width: `${pct}%`,
                      backgroundImage:
                        "repeating-linear-gradient(45deg, rgba(255,255,255,0.15) 0 4px, transparent 4px 8px)",
                      animation: "stripe-pan 1.2s linear infinite",
                    }}
                  />
                </div>
              </motion.div>
            );
          })}
        </div>

        {/* Bezier curves SVG */}
        <div className="col-span-12 md:col-span-2 hidden md:block relative">
          <svg viewBox="0 0 100 100" className="w-full h-full absolute inset-0" preserveAspectRatio="none">
            {tracks.map((t, i) => {
              // Match bars: 3 rows evenly distributed, centers at ~16.7, 50, 83.3
              const y1 = 16.7 + i * 33.3;
              return (
                <path
                  key={t.key}
                  d={`M0 ${y1} C 50 ${y1}, 50 50, 100 50`}
                  fill="none"
                  stroke={t.color}
                  strokeWidth="0.6"
                  strokeOpacity={phase === "generating" ? 0.3 : 0.85}
                  vectorEffect="non-scaling-stroke"
                  style={{ transition: "stroke-opacity 0.6s" }}
                />
              );
            })}
          </svg>
        </div>

        {/* Aggregator + Groth16 */}
        <div className="col-span-12 md:col-span-5 space-y-4">
          <motion.div
            animate={{
              scale: phase === "merging" ? 1.05 : 1,
              boxShadow:
                phase === "merging" || phase === "wrapping"
                  ? "0 0 60px hsl(var(--primary) / 0.6)"
                  : "0 0 0 transparent",
            }}
            transition={{ duration: 0.5 }}
            className="rounded-2xl p-5"
            style={{
              background: "hsl(var(--surface-2) / 0.6)",
              border: "1px solid hsl(var(--primary) / 0.4)",
            }}
          >
            <p className="mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">Aggregator</p>
            <p className="display text-xl mt-1">Compressing 3 → 1</p>
          </motion.div>

          <motion.div
            animate={{
              x: phase === "wrapping" ? [0, -2, 2, -2, 2, 0] : 0,
            }}
            transition={{ duration: 0.5 }}
            className="rounded-2xl p-5 relative overflow-hidden"
            style={{
              background: "hsl(var(--surface-1) / 0.6)",
              border: `1px solid ${phase === "verified" ? "hsl(var(--agent-beta) / 0.7)" : "hsl(var(--foreground) / 0.08)"}`,
              boxShadow: phase === "verified" ? "0 0 40px hsl(var(--agent-beta) / 0.4)" : "none",
            }}
          >
            <div className="flex items-center justify-between">
              <div>
                <p className="mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">Groth16</p>
                <p className="display text-xl mt-1">On-Chain Settlement</p>
              </div>
              <AnimatePresence>
                {phase === "verified" && (
                  <motion.span
                    initial={{ scale: 0, opacity: 0 }}
                    animate={{ scale: 1, opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{ type: "spring", stiffness: 200, damping: 14 }}
                    className="mono text-[10px] uppercase tracking-[0.14em] px-3 py-1.5 rounded-full"
                    style={{
                      color: "hsl(var(--agent-beta))",
                      background: "hsl(var(--agent-beta) / 0.12)",
                      border: "1px solid hsl(var(--agent-beta) / 0.5)",
                    }}
                  >
                    ✓ Verified
                  </motion.span>
                )}
              </AnimatePresence>
            </div>
          </motion.div>
        </div>
      </div>

      <div aria-live="polite" className="sr-only">{liveString}</div>

      <style>{`
        @keyframes stripe-pan {
          from { background-position: 0 0; }
          to   { background-position: 16px 0; }
        }
      `}</style>
    </div>
  );
};

export default ProofPipeline;
