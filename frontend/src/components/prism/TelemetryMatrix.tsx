import { useMemo } from "react";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";
import {
  currentEpoch,
  proofProgress,
  solverConflicts,
  lastShapley,
  lastSettlePath,
} from "@/lib/derivedState";
import { GitMerge, ArrowRightLeft, Layers, Cpu, Activity } from "lucide-react";
import { motion } from "framer-motion";

/**
 * Find the most-recent settlement event's gas_used, or null if none seen.
 * Mirrors lastShapley/lastSettlePath shape but for gas. Pure, not memoized
 * here because the caller already wraps it in useMemo.
 */
function lastGasUsed(events: ReturnType<typeof useWsEvents>["events"]): number | null {
  for (const e of events) {
    if (e.type === "epoch_settled" || e.type === "epoch_settled_via_plan_b") return e.gas_used;
  }
  return null;
}

const TelemetryMatrix = () => {
  const { demo, demoPhaseIdx, wsUrl } = useDemoMode();
  const { events, connected } = useWsEvents(wsUrl, !demo);

  const live = useMemo(
    () => ({
      epoch: !demo ? currentEpoch(events) : null,
      conflicts: !demo ? solverConflicts(events) : 0,
      progressKeys: !demo ? Object.keys(proofProgress(events)).length : 0,
      gasUsed: !demo ? lastGasUsed(events) : null,
      shapleySettled: !demo ? lastShapley(events) !== null : false,
      path: !demo ? lastSettlePath(events) : null,
    }),
    [demo, events],
  );

  // Live mode without any data yet → loading panel.
  if (!demo && !live.epoch && !live.shapleySettled) {
    return (
      <div
        className="glass p-8 md:p-10 mb-6 flex flex-col items-center justify-center text-center"
        style={{ minHeight: 280 }}
      >
        <Activity className="w-8 h-8 text-[hsl(var(--primary))] mb-4 animate-pulse" />
        <p className="mono text-[11px] uppercase tracking-[0.14em] text-[hsl(var(--primary))]">
          {connected ? "Connected · awaiting first epoch" : "Connecting to orchestrator…"}
        </p>
        <p className="text-sm text-muted-foreground mt-2 max-w-sm">
          Live telemetry will populate from <code className="mono">epoch_start</code>,{" "}
          <code className="mono">solver_running</code> and <code className="mono">epoch_settled</code> events.
        </p>
      </div>
    );
  }

  // Demo metrics — frozen plausible values. NEVER derived from live events
  // and never animated with random churn.
  const demoMetrics = {
    tickRange: "[-887220, 887220]",
    ilProjection: "-1.2%",
    jitLiquidity: "+14,020 USDC",
    targetTick: "204,512",
    densityDelta: "Δ +2.04e18",
    feeTier: "DYNAMIC (3-12 BPS)",
    surplus: "+ 0.042 ETH",
    stateRoot: "0x4a9b…2c1d",
  };

  // Live metrics — derived directly from WsEvent stream via selectors.
  const liveMetrics = {
    epochLabel: live.epoch !== null ? `#${live.epoch}` : "—",
    conflicts: String(live.conflicts),
    progressKeys: `${live.progressKeys} / 4`,
    gasUsed: live.gasUsed !== null ? live.gasUsed.toLocaleString() : "—",
    settlePath: live.path === "plan-b" ? "PLAN-B" : live.path === "groth16" ? "GROTH16" : "—",
  };

  return (
    <div className="glass p-8 md:p-10 mb-6">
      <div className="mb-8 flex items-end justify-between">
        <div>
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
            Uniswap V4 Hooks
          </p>
          <h2 className="display text-2xl md:text-3xl mt-2">Dynamic Pool Mechanics</h2>
        </div>
        <p className="mono text-[11px] uppercase tracking-[0.12em] text-[hsl(var(--primary))] hidden sm:block">
          {demo ? "Active Pool: USDC/ETH" : `Live · ${connected ? "connected" : "reconnecting"}`}
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        {/* Card 1 — Solver / beforeSwap analogue */}
        <motion.div
          animate={{ borderColor: demoPhaseIdx === 2 && demo ? "hsl(var(--agent-alpha)/0.4)" : "rgba(255,255,255,0.05)" }}
          className="bg-black/20 border rounded-xl p-5 relative overflow-hidden transition-colors duration-1000"
        >
          <div className="flex justify-between items-start mb-4">
            <span className="mono text-[10px] uppercase tracking-[0.1em] text-[hsl(var(--agent-alpha))] bg-[hsl(var(--agent-alpha))/0.1] px-2 py-1 rounded">
              {demo ? "beforeSwap" : "Solver"}
            </span>
            <ArrowRightLeft className="w-4 h-4 text-muted-foreground" />
          </div>
          <div className="space-y-3">
            <Row label="Epoch" value={demo ? "#8492" : liveMetrics.epochLabel} />
            <Row label={demo ? "IL Projection" : "Conflicts"} value={demo ? demoMetrics.ilProjection : liveMetrics.conflicts} />
            <Row
              label={demo ? "JIT Liquidity" : "Tick Range"}
              value={demo ? demoMetrics.jitLiquidity : demoMetrics.tickRange}
              accent="hsl(var(--agent-alpha))"
              last
            />
          </div>
        </motion.div>

        {/* Card 2 — Curator / beforeAddLiquidity analogue */}
        <motion.div
          animate={{ borderColor: demoPhaseIdx === 1 && demo ? "hsl(var(--agent-beta)/0.4)" : "rgba(255,255,255,0.05)" }}
          className="bg-black/20 border rounded-xl p-5 relative overflow-hidden transition-colors duration-1000"
        >
          <div className="flex justify-between items-start mb-4">
            <span className="mono text-[10px] uppercase tracking-[0.1em] text-[hsl(var(--agent-beta))] bg-[hsl(var(--agent-beta))/0.1] px-2 py-1 rounded">
              {demo ? "beforeAddLiquidity" : "Proof Pipeline"}
            </span>
            <Layers className="w-4 h-4 text-muted-foreground" />
          </div>
          <div className="space-y-3">
            <Row label={demo ? "Target Tick" : "Programs Running"} value={demo ? demoMetrics.targetTick : liveMetrics.progressKeys} />
            <Row label={demo ? "Density Delta" : "Fee Tier"} value={demo ? demoMetrics.densityDelta : "DYNAMIC (3-12 BPS)"} />
            <Row label="Fee Tier" value={demoMetrics.feeTier} last />
          </div>
        </motion.div>

        {/* Card 3 — Settlement / afterSwap analogue */}
        <motion.div
          animate={{ borderColor: demoPhaseIdx === 3 && demo ? "hsl(var(--agent-gamma)/0.4)" : "rgba(255,255,255,0.05)" }}
          className="bg-black/20 border rounded-xl p-5 relative overflow-hidden transition-colors duration-1000"
        >
          <div className="flex justify-between items-start mb-4">
            <span className="mono text-[10px] uppercase tracking-[0.1em] text-[hsl(var(--agent-gamma))] bg-[hsl(var(--agent-gamma))/0.1] px-2 py-1 rounded">
              {demo ? "afterSwap" : "Settlement"}
            </span>
            <GitMerge className="w-4 h-4 text-muted-foreground" />
          </div>
          <div className="space-y-3">
            <Row
              label={demo ? "Action" : "Path"}
              value={demo ? "REBALANCED" : liveMetrics.settlePath}
              accent="hsl(var(--agent-gamma))"
              icon
            />
            <Row
              label={demo ? "Surplus Captured" : "Last Gas"}
              value={demo ? demoMetrics.surplus : liveMetrics.gasUsed}
              accent="hsl(var(--agent-delta))"
            />
            <Row label="State Root" value={demoMetrics.stateRoot} last truncate />
          </div>
        </motion.div>
      </div>
    </div>
  );
};

interface RowProps {
  label: string;
  value: string;
  accent?: string;
  last?: boolean;
  truncate?: boolean;
  icon?: boolean;
}
const Row = ({ label, value, accent, last, truncate, icon }: RowProps) => (
  <div className={`flex justify-between ${last ? "" : "border-b border-white/5 pb-2"}`}>
    <span className="mono text-[10px] text-muted-foreground uppercase flex items-center gap-2">
      {icon && <Cpu className="w-3 h-3" />}
      {label}
    </span>
    <span
      className={`mono text-[10px] tabular ${truncate ? "truncate w-24" : ""}`}
      style={{ color: accent || "white" }}
    >
      {value}
    </span>
  </div>
);

export default TelemetryMatrix;
