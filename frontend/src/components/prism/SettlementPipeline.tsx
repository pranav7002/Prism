import { useEffect, useState } from "react";

const tracks = [
  { key: "solver", label: "Solver" },
  { key: "execution", label: "Execution" },
  { key: "shapley", label: "Shapley" },
] as const;

type Key = typeof tracks[number]["key"];

const SettlementPipeline = () => {
  const [progress, setProgress] = useState<Record<Key, number>>({ solver: 0, execution: 0, shapley: 0 });

  useEffect(() => {
    let cancelled = false;
    const cycle = () => {
      setProgress({ solver: 0, execution: 0, shapley: 0 });
      const tick = () => {
        if (cancelled) return;
        setProgress((p) => {
          const next: Record<Key, number> = {
            solver: Math.min(100, p.solver + 0.8 + Math.random() * 1.4),
            execution: Math.min(100, p.execution + 0.6 + Math.random() * 1.2),
            shapley: Math.min(100, p.shapley + 0.7 + Math.random() * 1.3),
          };
          if (next.solver >= 100 && next.execution >= 100 && next.shapley >= 100) {
            setTimeout(() => !cancelled && cycle(), 1800);
            return next;
          }
          setTimeout(tick, 80);
          return next;
        });
      };
      tick();
    };
    cycle();
    return () => { cancelled = true; };
  }, []);

  const allDone = progress.solver >= 100 && progress.execution >= 100 && progress.shapley >= 100;

  return (
    <div className="glass p-8 md:p-10">
      <div className="mb-8 flex items-end justify-between">
        <div>
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Settlement · Pipeline</p>
          <h2 className="display text-2xl md:text-3xl mt-2">Proof Aggregation</h2>
        </div>
        <p className="mono text-[11px] uppercase tracking-[0.12em] text-muted-foreground">
          {allDone ? "Verified" : "Generating"}
        </p>
      </div>

      <div className="grid grid-cols-12 gap-6 items-stretch">
        <div className="col-span-12 md:col-span-5 flex flex-col justify-between gap-4">
          {tracks.map((t) => {
            const pct = Math.floor(progress[t.key]);
            return (
              <div key={t.key}>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">{t.label}</span>
                  <span className="mono text-[10px] tabular text-foreground/80">{pct}%</span>
                </div>
                <div
                  className="relative h-[6px] rounded-full overflow-hidden"
                  style={{ background: "hsl(var(--foreground) / 0.06)" }}
                  role="progressbar"
                  aria-valuenow={pct}
                  aria-valuemin={0}
                  aria-valuemax={100}
                >
                  <div
                    className="absolute inset-y-0 left-0 transition-[width] duration-200"
                    style={{
                      width: `${pct}%`,
                      background: "hsl(var(--foreground) / 0.85)",
                    }}
                  />
                </div>
              </div>
            );
          })}
        </div>

        <div className="col-span-12 md:col-span-2 hidden md:block relative min-h-[140px]">
          <svg viewBox="0 0 100 100" className="absolute inset-0 w-full h-full" preserveAspectRatio="none">
            {tracks.map((_, i) => {
              const y1 = 16.7 + i * 33.3;
              return (
                <path
                  key={i}
                  d={`M0 ${y1} C 50 ${y1}, 50 50, 100 50`}
                  fill="none"
                  stroke="hsl(var(--foreground))"
                  strokeOpacity={0.3}
                  strokeWidth="0.6"
                  vectorEffect="non-scaling-stroke"
                />
              );
            })}
          </svg>
        </div>

        <div className="col-span-12 md:col-span-5 flex items-center">
          <div
            className="w-full rounded-2xl p-5 transition-all duration-500"
            style={{
              background: "hsl(var(--surface-1) / 0.6)",
              border: `1px solid ${allDone ? "hsl(var(--foreground) / 0.4)" : "hsl(var(--foreground) / 0.08)"}`,
            }}
          >
            <p className="mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">Groth16</p>
            <p className="display text-xl mt-1">On-Chain Settlement</p>
            <p className="mono text-[10px] uppercase tracking-[0.14em] text-foreground/60 mt-3">
              {allDone ? "✓ Verified" : "Awaiting aggregation"}
            </p>
          </div>
        </div>
      </div>
    </div>
  );
};

export default SettlementPipeline;