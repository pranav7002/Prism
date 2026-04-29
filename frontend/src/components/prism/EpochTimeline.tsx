import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";
import { currentEpoch } from "@/lib/derivedState";

const phases = ["Commit", "Reveal", "Solve", "Prove", "Settle"] as const;
type Phase = typeof phases[number];

const phaseDuration = 8; // seconds each

const EpochTimeline = () => {
  const { demo, wsUrl } = useDemoMode();
  const { events } = useWsEvents(wsUrl, !demo);
  const liveEpoch = !demo ? currentEpoch(events) : null;

  const [activeIdx, setActiveIdx] = useState(2);
  const [countdown, setCountdown] = useState(phaseDuration);

  useEffect(() => {
    const t = setInterval(() => {
      setCountdown((c) => {
        if (c <= 1) {
          setActiveIdx((i) => (i + 1) % phases.length);
          return phaseDuration;
        }
        return c - 1;
      });
    }, 1000);
    return () => clearInterval(t);
  }, []);

  return (
    <div className="glass p-8">
      <div className="mb-8 flex items-end justify-between">
        <div>
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
            Epoch · {liveEpoch !== null ? `#${liveEpoch}` : "Live"}
          </p>
          <h2 className="display text-2xl md:text-3xl mt-2">Lifecycle Stepper</h2>
        </div>
        <p className="mono text-[11px] uppercase tracking-[0.12em] text-muted-foreground">
          Phase {activeIdx + 1}/{phases.length}
        </p>
      </div>

      <div className="relative">
        {/* connector track */}
        <div className="absolute left-5 right-5 top-5 h-px bg-foreground/10" />
        <motion.div
          className="absolute left-5 top-5 h-px"
          style={{ background: "var(--gradient-prism)" }}
          initial={false}
          animate={{ width: `calc((100% - 40px) * ${activeIdx / (phases.length - 1)})` }}
          transition={{ duration: 0.6, ease: [0.16, 1, 0.3, 1] }}
        />
        <div className="relative flex items-start justify-between">
        {phases.map((p, i) => {
          const isComplete = i < activeIdx;
          const isActive = i === activeIdx;
          return (
            <div key={p} className="relative flex flex-col items-center" style={{ minWidth: 60 }}>
              <motion.div
                animate={{ scale: isActive ? 1.15 : 1 }}
                transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
                className="grid h-10 w-10 place-items-center rounded-full"
                style={{
                  background: isComplete
                    ? "var(--gradient-prism)"
                    : isActive
                    ? "hsl(var(--surface-2))"
                    : "hsl(var(--surface-1) / 0.5)",
                  border: `1px solid ${
                    isComplete
                      ? "hsl(var(--primary))"
                      : isActive
                      ? "hsl(var(--primary) / 0.6)"
                      : "hsl(var(--foreground) / 0.1)"
                  }`,
                  boxShadow: isActive ? "0 0 32px hsl(var(--primary) / 0.5)" : "none",
                }}
              >
                {isComplete ? (
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="hsl(var(--background))" strokeWidth="2.5">
                    <path d="M5 12l5 5L20 7" />
                  </svg>
                ) : (
                  <span className="mono text-[10px]" style={{ color: isActive ? "hsl(var(--primary))" : "hsl(var(--foreground) / 0.3)" }}>
                    {String(i + 1).padStart(2, "0")}
                  </span>
                )}
                {isActive && (
                  <motion.span
                    className="absolute inset-0 rounded-full"
                    animate={{ opacity: [0.4, 0, 0.4], scale: [1, 1.6, 1] }}
                    transition={{ duration: 2, repeat: Infinity }}
                    style={{ border: "1px solid hsl(var(--primary))" }}
                  />
                )}
              </motion.div>
              <span className="mt-3 text-xs font-medium">{p}</span>
              {isActive && (
                <span className="mono text-[10px] mt-1 tabular" style={{ color: "hsl(var(--primary))" }}>
                  00:{String(countdown).padStart(2, "0")}s
                </span>
              )}
              {!isActive && (
                <span className="mono text-[10px] mt-1 text-muted-foreground/50 tabular">
                  {isComplete ? "done" : "—"}
                </span>
              )}

            </div>
          );
        })}
        </div>
      </div>
    </div>
  );
};

export default EpochTimeline;