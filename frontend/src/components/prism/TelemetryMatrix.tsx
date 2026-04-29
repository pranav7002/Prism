import { useEffect, useState } from "react";
import { useDemoMode } from "@/store/demoMode";
import { GitMerge, ArrowRightLeft, Layers, Cpu, Activity } from "lucide-react";
import { motion } from "framer-motion";

const TelemetryMatrix = () => {
  const { demo, demoPhaseIdx } = useDemoMode();
  
  // Fake tick data to look active
  const [randomDelta, setRandomDelta] = useState(0);
  
  useEffect(() => {
    if (!demo) return;
    const t = setInterval(() => {
      setRandomDelta(Math.floor(Math.random() * 9999));
    }, 4000);
    return () => clearInterval(t);
  }, [demo]);

  if (!demo) {
    return (
      <div className="glass p-8 md:p-10 mb-6 flex flex-col items-center justify-center text-center" style={{ minHeight: 280 }}>
        <Activity className="w-8 h-8 text-[hsl(var(--primary))] mb-4 animate-pulse" />
        <p className="mono text-[11px] uppercase tracking-[0.14em] text-[hsl(var(--primary))]">Awaiting Live Hook Telemetry</p>
        <p className="text-sm text-muted-foreground mt-2 max-w-sm">
          Awaiting real-time Uniswap V4 pool state metrics from the orchestrator.
        </p>
      </div>
    );
  }

  return (
    <div className="glass p-8 md:p-10 mb-6">
      <div className="mb-8 flex items-end justify-between">
        <div>
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Uniswap V4 Hooks</p>
          <h2 className="display text-2xl md:text-3xl mt-2">Dynamic Pool Mechanics</h2>
        </div>
        <p className="mono text-[11px] uppercase tracking-[0.12em] text-[hsl(var(--primary))] hidden sm:block">
          Active Pool: USDC/ETH
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        {/* beforeSwap */}
        <motion.div 
          animate={{ borderColor: demoPhaseIdx === 2 ? "hsl(var(--agent-alpha)/0.4)" : "rgba(255,255,255,0.05)" }}
          className="bg-black/20 border rounded-xl p-5 relative overflow-hidden transition-colors duration-1000"
        >
          <div className="flex justify-between items-start mb-4">
            <span className="mono text-[10px] uppercase tracking-[0.1em] text-[hsl(var(--agent-alpha))] bg-[hsl(var(--agent-alpha))/0.1] px-2 py-1 rounded">beforeSwap</span>
            <ArrowRightLeft className="w-4 h-4 text-muted-foreground" />
          </div>
          <div className="space-y-3">
            <div className="flex justify-between border-b border-white/5 pb-2">
              <span className="mono text-[10px] text-muted-foreground uppercase">TickRange</span>
              <span className="mono text-[10px] text-white">[-887220, 887220]</span>
            </div>
            <div className="flex justify-between border-b border-white/5 pb-2">
              <span className="mono text-[10px] text-muted-foreground uppercase">IL Projection</span>
              <span className="mono text-[10px] text-white tabular">-1.{Math.floor(randomDelta / 100)}%</span>
            </div>
            <div className="flex justify-between">
              <span className="mono text-[10px] text-muted-foreground uppercase">JIT Liquidity</span>
              <span className="mono text-[10px] text-[hsl(var(--agent-alpha))] tabular">+14,020 USDC</span>
            </div>
          </div>
        </motion.div>

        {/* beforeAddLiquidity */}
        <motion.div 
          animate={{ borderColor: demoPhaseIdx === 1 ? "hsl(var(--agent-beta)/0.4)" : "rgba(255,255,255,0.05)" }}
          className="bg-black/20 border rounded-xl p-5 relative overflow-hidden transition-colors duration-1000"
        >
          <div className="flex justify-between items-start mb-4">
            <span className="mono text-[10px] uppercase tracking-[0.1em] text-[hsl(var(--agent-beta))] bg-[hsl(var(--agent-beta))/0.1] px-2 py-1 rounded">beforeAddLiquidity</span>
            <Layers className="w-4 h-4 text-muted-foreground" />
          </div>
          <div className="space-y-3">
            <div className="flex justify-between border-b border-white/5 pb-2">
              <span className="mono text-[10px] text-muted-foreground uppercase">Target Tick</span>
              <span className="mono text-[10px] text-white tabular">204{String(randomDelta).slice(0,3)}</span>
            </div>
            <div className="flex justify-between border-b border-white/5 pb-2">
              <span className="mono text-[10px] text-muted-foreground uppercase">Density Delta</span>
              <span className="mono text-[10px] text-white tabular">Δ +2.{String(randomDelta).slice(1,3)}e18</span>
            </div>
            <div className="flex justify-between">
              <span className="mono text-[10px] text-muted-foreground uppercase">Fee Tier</span>
              <span className="mono text-[10px] text-white tabular">DYNAMIC (3-12 BPS)</span>
            </div>
          </div>
        </motion.div>

        {/* afterSwap */}
        <motion.div 
          animate={{ borderColor: demoPhaseIdx === 3 ? "hsl(var(--agent-gamma)/0.4)" : "rgba(255,255,255,0.05)" }}
          className="bg-black/20 border rounded-xl p-5 relative overflow-hidden transition-colors duration-1000"
        >
          <div className="flex justify-between items-start mb-4">
            <span className="mono text-[10px] uppercase tracking-[0.1em] text-[hsl(var(--agent-gamma))] bg-[hsl(var(--agent-gamma))/0.1] px-2 py-1 rounded">afterSwap</span>
            <GitMerge className="w-4 h-4 text-muted-foreground" />
          </div>
          <div className="space-y-3">
             <div className="flex justify-between border-b border-white/5 pb-2">
              <span className="mono text-[10px] text-muted-foreground uppercase flex items-center gap-2"><Cpu className="w-3 h-3"/> Action</span>
              <span className="mono text-[10px] text-[hsl(var(--agent-gamma))]">REBALANCED</span>
            </div>
            <div className="flex justify-between border-b border-white/5 pb-2">
              <span className="mono text-[10px] text-muted-foreground uppercase">Surplus Captured</span>
              <span className="mono text-[10px] text-[hsl(var(--agent-delta))] tabular">+ 0.04{String(randomDelta).slice(2,3)} ETH</span>
            </div>
            <div className="flex justify-between">
              <span className="mono text-[10px] text-muted-foreground uppercase">State Root</span>
              <span className="mono text-[10px] text-white tabular truncate w-24">0x4a9b{randomDelta}...</span>
            </div>
          </div>
        </motion.div>
      </div>
    </div>
  );
};

export default TelemetryMatrix;
