import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";
import { epochHistory, type EpochData } from "@/lib/derivedState";
import { ExternalLink, FileCode, CheckCircle2, ChevronDown, ChevronUp } from "lucide-react";

const agents = [
  { key: "alpha", name: "Predictive α", color: "hsl(var(--agent-alpha))" },
  { key: "beta", name: "Curator β", color: "hsl(var(--agent-beta))" },
  { key: "gamma", name: "Healer γ", color: "hsl(var(--agent-gamma))" },
  { key: "delta", name: "Backrunner δ", color: "hsl(var(--agent-delta))" },
  { key: "epsilon", name: "Guardian ε", color: "hsl(var(--agent-epsilon))" },
];

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
          <div className="h-10 w-10 rounded-full flex items-center justify-center" style={{ background: "hsl(var(--surface-2))", border: "1px solid hsl(var(--foreground)/0.1)" }}>
             {isSettled ? <CheckCircle2 className="w-5 h-5 text-[hsl(var(--agent-beta))]" /> : <div className="w-2 h-2 rounded-full bg-[hsl(var(--primary))] animate-pulse" />}
          </div>
          <div>
            <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Epoch #{epoch.epochId}</p>
            <h3 className="display text-xl">{isSettled ? `Settled via ${epoch.path === "plan-b" ? "Plan-B" : "Groth16"}` : "Processing..."}</h3>
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
            <div className="p-5 bg-[hsl(var(--surface-2)/0.3)] grid gap-6 md:grid-cols-2">
              
              {/* Transaction Hash */}
              <div className="space-y-2">
                <p className="mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground flex items-center gap-2">
                  <ExternalLink className="w-3 h-3" /> On-Chain Settlement
                </p>
                {epoch.txHash ? (
                  <a 
                    href={`https://sepolia.uniscan.xyz/tx/${epoch.txHash}`}
                    target="_blank" rel="noreferrer"
                    className="block p-3 rounded-md font-mono text-xs break-all hover:bg-white/5 transition-colors border border-white/5"
                    style={{ color: "hsl(var(--primary))", wordBreak: "break-all" }}
                  >
                    {epoch.txHash}
                  </a>
                ) : (
                  <div className="p-3 rounded-md font-mono text-xs border border-white/5 opacity-50">
                    Awaiting transaction...
                  </div>
                )}
              </div>

              {/* ZK Proof Hash */}
              <div className="space-y-2">
                <p className="mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground flex items-center gap-2">
                  <FileCode className="w-3 h-3" /> SP1 Proof Commitment
                </p>
                {epoch.planHash ? (
                  <div className="p-3 rounded-md font-mono text-xs break-all border border-white/5 bg-black/20 flex justify-between items-start md:items-center flex-col md:flex-row gap-2 group">
                    <span style={{ wordBreak: "break-all" }}>{epoch.planHash}</span>
                    <button className="text-[10px] uppercase opacity-50 group-hover:opacity-100 transition-opacity bg-white/10 px-2 py-1 rounded whitespace-nowrap" onClick={(e) => { e.stopPropagation(); alert(JSON.stringify(epoch, null, 2)); }}>View JSON</button>
                  </div>
                ) : (
                  <div className="p-3 rounded-md font-mono text-xs border border-white/5 opacity-50">
                    Generating proof...
                  </div>
                )}
              </div>
              
              {/* Payout breakdown */}
              {epoch.shapley && (
                 <div className="col-span-full pt-4 border-t border-white/5">
                   <p className="mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground mb-4">Shapley Value Distribution (Basis Points)</p>
                   
                   <div className="flex w-full h-1.5 rounded-full overflow-hidden mb-5 bg-black/50">
                     {epoch.shapley.map((bps, i) => (
                       <div key={i} className="transition-all duration-500" style={{ width: `${bps / 100}%`, backgroundColor: agents[i]?.color || 'gray' }} />
                     ))}
                   </div>
                   
                   <div className="grid grid-cols-2 md:grid-cols-5 gap-3">
                      {epoch.shapley.map((bps, i) => (
                        <div key={i} className="relative overflow-hidden bg-black/20 rounded-lg p-3 text-center border border-white/5 hover:bg-white/5 transition-colors">
                          <div className="absolute inset-x-0 bottom-0 h-1 opacity-80" style={{ backgroundColor: agents[i]?.color || 'gray' }} />
                          <span className="mono text-[10px] text-muted-foreground whitespace-nowrap block mb-1">{agents[i]?.name || `Agent ${i+1}`}</span>
                          <span className="mono text-[14px] font-medium block" style={{ color: agents[i]?.color || 'white' }}>{bps}</span>
                          <span className="mono text-[9px] text-muted-foreground opacity-50 block mt-0.5">bps</span>
                        </div>
                      ))}
                   </div>
                 </div>
              )}

            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
};

const Settlement = () => {
  const { demo, wsUrl, demoHistory } = useDemoMode();
  const { events } = useWsEvents(wsUrl, !demo);
  
  const liveHistory = !demo ? epochHistory(events) : [];
  // Merge live and demo history depending on mode
  const history = demo ? demoHistory : liveHistory;

  return (
    <div className="min-h-screen pb-20">
      <section className="container mx-auto pt-12 pb-6">
        <div className="mb-8">
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Route · /settlement</p>
          <h1 className="display text-4xl md:text-5xl mt-2">Epoch History</h1>
          <p className="text-muted-foreground mt-4 max-w-2xl">
            A permanent, verifiable ledger of all completed epochs. Inspect the raw SP1 zero-knowledge proof commitments and their corresponding on-chain settlement transactions on Unichain Sepolia.
          </p>
        </div>
      </section>

      <section className="container mx-auto">
        {history.length === 0 ? (
          <div className="p-12 text-center border border-dashed rounded-xl border-white/10 text-muted-foreground">
            No epochs settled yet. Wait for the current epoch to complete.
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