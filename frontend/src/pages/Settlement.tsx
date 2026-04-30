import { useState, useMemo } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";
import { epochHistory, type EpochData } from "@/lib/derivedState";
import { CheckCircle2, ChevronDown, ChevronUp } from "lucide-react";
import EpochDetail from "@/components/prism/EpochDetail";

const EpochCard = ({ epoch }: { epoch: EpochData }) => {
  const [expanded, setExpanded] = useState(false);
  const isSettled = !!epoch.txHash;

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      className="mb-4 rounded-xl overflow-hidden border"
      style={{
        background: "hsl(var(--surface-1) / 0.4)",
        borderColor: isSettled ? "hsl(var(--agent-beta) / 0.3)" : "hsl(var(--foreground) / 0.1)",
      }}
    >
      <div
        className="p-5 cursor-pointer flex items-center justify-between hover:bg-white/5 transition-colors"
        onClick={() => setExpanded(!expanded)}
      >
        <div className="flex items-center gap-4">
          <div
            className="h-10 w-10 rounded-full flex items-center justify-center"
            style={{ background: "hsl(var(--surface-2))", border: "1px solid hsl(var(--foreground)/0.1)" }}
          >
            {isSettled ? (
              <CheckCircle2 className="w-5 h-5 text-[hsl(var(--agent-beta))]" />
            ) : (
              <div className="w-2 h-2 rounded-full bg-[hsl(var(--primary))] animate-pulse" />
            )}
          </div>
          <div>
            <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
              Epoch #{epoch.epochId}
            </p>
            <h3 className="display text-xl">
              {isSettled ? `Settled via ${epoch.path === "plan-b" ? "Plan-B" : "Groth16"}` : "Processing..."}
            </h3>
          </div>
        </div>

        <div className="flex items-center gap-6">
          {isSettled && epoch.intentsProcessed !== undefined && (
            <div className="hidden md:block text-right border-r border-white/10 pr-6">
              <p className="mono text-[10px] uppercase text-muted-foreground">Intents</p>
              <p className="mono text-sm">{epoch.intentsProcessed}</p>
            </div>
          )}
          {isSettled && epoch.volumeUsd !== undefined && (
            <div className="hidden md:block text-right border-r border-white/10 pr-6">
              <p className="mono text-[10px] uppercase text-muted-foreground">Volume</p>
              <p className="mono text-sm">${epoch.volumeUsd.toLocaleString()}</p>
            </div>
          )}
          {isSettled && epoch.gasUsed !== undefined && (
            <div className="hidden md:block text-right border-r border-white/10 pr-6">
              <p className="mono text-[10px] uppercase text-muted-foreground">Gas Used</p>
              <p className="mono text-sm">{epoch.gasUsed.toLocaleString()} gas</p>
            </div>
          )}
          {isSettled && epoch.baseFee !== undefined && (
            <div className="hidden md:block text-right">
              <p className="mono text-[10px] uppercase text-muted-foreground">Base Fee</p>
              <p className="mono text-sm">{epoch.baseFee} gwei</p>
            </div>
          )}
          {expanded ? <ChevronUp className="w-5 h-5 opacity-50" /> : <ChevronDown className="w-5 h-5 opacity-50" />}
        </div>
      </div>

      <AnimatePresence>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            className="border-t border-white/5"
          >
            <EpochDetail epoch={epoch} />
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
};

const Settlement = () => {
  const { demo, wsUrl, demoHistory } = useDemoMode();
  const { events } = useWsEvents(wsUrl, !demo);

  // Trust the hook's `enabled` flag — no need to double-gate.
  const liveHistory = useMemo(() => epochHistory(events), [events]);
  const history = demo ? demoHistory : liveHistory;

  return (
    <div className="min-h-screen pb-20">
      <section className="container mx-auto pt-12 pb-6">
        <div className="mb-8">
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Route · /settlement</p>
          <h1 className="display text-4xl md:text-5xl mt-2">Epoch History</h1>
          <p className="text-muted-foreground mt-4 max-w-2xl">
            A permanent, verifiable ledger of completed epochs. Inspect SP1 proof commitments and
            their on-chain settlement transactions on Unichain Sepolia.
          </p>
        </div>
      </section>

      <section className="container mx-auto">
        {history.length === 0 ? (
          <div className="p-12 text-center border border-dashed rounded-xl border-white/10 text-muted-foreground">
            {demo
              ? "No demo epochs settled yet."
              : "No epochs settled yet — start the orchestrator and wait one epoch (≈12 s)."}
          </div>
        ) : (
          <div className="max-w-4xl mx-auto md:mx-0">
            {history.map((epoch) => (
              <EpochCard key={epoch.epochId} epoch={epoch} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
};

export default Settlement;
