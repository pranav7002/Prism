import { useEffect, useMemo, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { PieChart, Pie, Cell, ResponsiveContainer, BarChart, Bar, XAxis, YAxis, Tooltip } from "recharts";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";
import { lastShapley } from "@/lib/derivedState";
import { AGENTS } from "@/lib/agents";
import { Activity } from "lucide-react";

// Demo-only seeded payout generator. Memoized via Map so the same seed always
// yields the same row, instead of recomputing 500 sin() values every tick.
const epochCache = new Map<number, ReturnType<typeof buildEpoch>>();
function buildEpoch(seed: number) {
  const rand = (i: number) => (Math.sin(seed * 7.91 + i * 3.13) + 1) / 2;
  const raw = AGENTS.map((_, i) => 0.1 + rand(i) * 0.4);
  const total = raw.reduce((a, b) => a + b, 0);
  return AGENTS.map((a, i) => ({
    ...a,
    value: Math.round((raw[i] / total) * 100),
    bps: Math.round((raw[i] / total) * 1000),
    usd: Math.round(rand(i) * 200 + 50),
  }));
}
function genEpoch(seed: number) {
  const cached = epochCache.get(seed);
  if (cached) return cached;
  const built = buildEpoch(seed);
  epochCache.set(seed, built);
  return built;
}

/** Convert EpochSettled.shapley (Vec<u16> summing to 10000 bps) into chart-ready data. */
function shapleyToChartData(shapley: number[]) {
  const padded = Array.from({ length: AGENTS.length }, (_, i) => shapley[i] ?? 0);
  return AGENTS.map((a, i) => ({
    ...a,
    value: Math.round(padded[i] / 100),
    bps: padded[i],
    usd: Math.round(padded[i] / 10),
  }));
}

const AwaitingSettlement = () => (
  <div
    className="glass p-8 flex flex-col items-center justify-center text-center"
    style={{ minHeight: 480 }}
  >
    <Activity className="w-8 h-8 text-[hsl(var(--primary))] mb-4 animate-pulse" />
    <p className="mono text-[11px] uppercase tracking-[0.14em] text-[hsl(var(--primary))]">
      Awaiting first settlement
    </p>
    <p className="text-sm text-muted-foreground mt-2 max-w-sm">
      The Shapley distribution will appear after the first <code className="mono">EpochSettled</code> event arrives over WebSocket.
    </p>
  </div>
);

const ShapleyBreakdown = () => {
  const { demo, wsUrl } = useDemoMode();
  const { events } = useWsEvents(wsUrl, !demo);

  const liveShapley = useMemo(
    () => (!demo ? lastShapley(events) : null),
    [demo, events],
  );
  const liveData = useMemo(
    () => (liveShapley ? shapleyToChartData(liveShapley) : null),
    [liveShapley],
  );

  const [epochOffset, setEpochOffset] = useState(19);
  const [autoTick, setAutoTick] = useState(0);

  useEffect(() => {
    if (!demo) return;
    const t = setInterval(() => setAutoTick((c) => c + 1), 5000);
    return () => clearInterval(t);
  }, [demo]);

  const history = useMemo(() => {
    if (!demo) return [];
    return Array.from({ length: 20 }).map((_, i) => {
      const data = genEpoch(8473 + i + autoTick);
      const obj: Record<string, string | number> = { epoch: `#${8473 + i}` };
      data.forEach((d) => (obj[d.key] = d.value));
      return obj;
    });
  }, [autoTick, demo]);

  // currentData: live shapley if available, else demo seeded data, else null.
  // Critically: in live mode we NEVER fall through to genEpoch().
  const currentData = useMemo(() => {
    if (liveData) return liveData;
    if (demo) return genEpoch(8473 + epochOffset + autoTick);
    return null;
  }, [liveData, demo, epochOffset, autoTick]);

  // Live mode + no settlement event yet → show "awaiting settlement" instead
  // of fabricated chart data.
  if (!demo && !currentData) {
    return (
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <AwaitingSettlement />
        <AwaitingSettlement />
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <div className="glass p-8" style={{ minHeight: 480 }}>
        <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
          {liveData ? "Live · Last settled epoch" : "Demo · Seeded epoch"}
        </p>
        <h2 className="display text-2xl md:text-3xl mt-2 mb-6">Shapley Distribution</h2>

        <div className="relative" style={{ height: 280 }}>
          <AnimatePresence mode="wait">
            <motion.div
              key={liveData ? "live" : String(epochOffset + "-" + autoTick)}
              initial={{ scale: 0, rotate: -180, opacity: 0 }}
              animate={{ scale: 1, rotate: 0, opacity: 1 }}
              exit={{ scale: 0.6, opacity: 0 }}
              transition={{ type: "spring", stiffness: 90, damping: 14 }}
              className="absolute inset-0"
            >
              <ResponsiveContainer width="100%" height="100%">
                <PieChart>
                  <Pie
                    data={currentData ?? []}
                    dataKey="value"
                    innerRadius={70}
                    outerRadius={110}
                    paddingAngle={3}
                    stroke="hsl(var(--background))"
                    strokeWidth={2}
                  >
                    {(currentData ?? []).map((d, i) => (
                      <Cell key={i} fill={d.color} />
                    ))}
                  </Pie>
                  <Tooltip
                    contentStyle={{
                      background: "hsl(var(--surface-2) / 0.9)",
                      border: "1px solid hsl(var(--foreground) / 0.1)",
                      borderRadius: 12,
                      backdropFilter: "blur(24px)",
                    }}
                    formatter={(v: number, _n: string, p: { payload: NonNullable<typeof currentData>[number] }) => [
                      `${v}% · ${p.payload.bps} BPS · $${p.payload.usd}`,
                      p.payload.longName,
                    ]}
                  />
                </PieChart>
              </ResponsiveContainer>
            </motion.div>
          </AnimatePresence>
        </div>

        <div className="mt-4 grid grid-cols-5 gap-2">
          {(currentData ?? []).map((d) => (
            <div key={d.key} className="text-center">
              <span className="block h-1 w-full rounded" style={{ background: d.color }} />
              <span className="mono text-[9px] uppercase tracking-[0.1em] text-muted-foreground mt-1 block">
                {d.symbol}
              </span>
              <span className="mono text-[10px] tabular text-foreground/80">{d.value}%</span>
            </div>
          ))}
        </div>
      </div>

      <div className="glass p-8" style={{ minHeight: 480 }}>
        <div className="flex items-end justify-between mb-6">
          <div>
            <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
              {demo ? "Historical · 20 demo epochs" : "Last settled"}
            </p>
            <h2 className="display text-2xl md:text-3xl mt-2">MEV Captured</h2>
          </div>
          {demo && <p className="mono text-[10px] tabular text-foreground/70">Epoch #{8473 + epochOffset}</p>}
        </div>

        {demo ? (
          <>
            <div style={{ height: 280 }}>
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={history} barCategoryGap={6}>
                  <XAxis
                    dataKey="epoch"
                    tick={{ fontSize: 9, fill: "hsl(var(--muted-foreground))", fontFamily: "var(--font-mono)" }}
                    axisLine={false}
                    tickLine={false}
                    interval={2}
                  />
                  <YAxis hide />
                  <Tooltip
                    cursor={{ fill: "hsl(var(--foreground) / 0.04)" }}
                    contentStyle={{
                      background: "hsl(var(--surface-2) / 0.9)",
                      border: "1px solid hsl(var(--foreground) / 0.1)",
                      borderRadius: 12,
                    }}
                  />
                  {AGENTS.map((a) => (
                    <Bar key={a.key} dataKey={a.key} stackId="x" fill={a.color} radius={[2, 2, 0, 0]} />
                  ))}
                </BarChart>
              </ResponsiveContainer>
            </div>

            <div className="mt-6">
              <input
                type="range"
                min={0}
                max={19}
                value={epochOffset}
                onChange={(e) => setEpochOffset(parseInt(e.target.value))}
                className="w-full accent-[hsl(var(--primary))]"
                aria-label="Scrub historical epoch"
              />
              <div className="flex justify-between mono text-[9px] uppercase tracking-[0.12em] text-muted-foreground mt-2">
                <span>#8473</span>
                <span>#8492</span>
              </div>
            </div>
          </>
        ) : (
          <div className="space-y-3">
            {(currentData ?? []).map((d) => (
              <div key={d.key} className="flex items-center justify-between p-3 rounded-md border border-white/5 bg-black/20">
                <div className="flex items-center gap-3">
                  <span className="h-2.5 w-2.5 rounded-full" style={{ background: d.color }} />
                  <span className="mono text-[12px]" style={{ color: d.color }}>{d.longName}</span>
                </div>
                <div className="text-right">
                  <p className="mono text-sm tabular">{d.value}%</p>
                  <p className="mono text-[9px] text-muted-foreground tabular">{d.bps} bps</p>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default ShapleyBreakdown;
