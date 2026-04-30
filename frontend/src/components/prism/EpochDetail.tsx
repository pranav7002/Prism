import { ExternalLink, FileCode, Code2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { AGENTS } from "@/lib/agents";
import type { EpochData } from "@/lib/derivedState";

/**
 * Expanded inspector panel for a single epoch. Renders the on-chain tx,
 * the SP1 proof commitment with a JSON-inspect dialog, the human-readable
 * settlement-path explainer, and the per-agent Shapley breakdown.
 */
const EpochDetail = ({ epoch }: { epoch: EpochData }) => {
  return (
    <div className="p-5 bg-[hsl(var(--surface-2)/0.3)] grid gap-6 md:grid-cols-2">
      {/* Transaction Hash */}
      <div className="space-y-2">
        <p className="mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground flex items-center gap-2">
          <ExternalLink className="w-3 h-3" /> On-Chain Settlement
        </p>
        {epoch.txHash ? (
          <a
            href={`https://sepolia.uniscan.xyz/tx/${epoch.txHash}`}
            target="_blank"
            rel="noreferrer"
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
            <Dialog>
              <DialogTrigger asChild>
                <button
                  className="text-[10px] uppercase opacity-50 group-hover:opacity-100 transition-opacity bg-white/10 px-2 py-1 rounded whitespace-nowrap inline-flex items-center gap-1"
                  onClick={(e) => e.stopPropagation()}
                >
                  <Code2 className="w-3 h-3" /> Inspect
                </button>
              </DialogTrigger>
              <DialogContent
                className="max-w-2xl bg-[hsl(var(--surface-1))] border border-white/10"
                onClick={(e) => e.stopPropagation()}
              >
                <DialogHeader>
                  <DialogTitle className="mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
                    Epoch #{epoch.epochId} · Raw record
                  </DialogTitle>
                </DialogHeader>
                <pre className="mono text-[11px] leading-relaxed bg-black/60 border border-white/5 rounded-md p-4 max-h-[60vh] overflow-auto whitespace-pre-wrap break-all">
                  {JSON.stringify(epoch, null, 2)}
                </pre>
              </DialogContent>
            </Dialog>
          </div>
        ) : (
          <div className="p-3 rounded-md font-mono text-xs border border-white/5 opacity-50">
            Generating proof...
          </div>
        )}
      </div>

      {/* Settlement path note */}
      <div className="col-span-full pt-4 border-t border-white/5 space-y-2">
        <p className="mono text-[10px] uppercase tracking-[0.14em] text-[hsl(var(--primary))]">
          Settlement path
        </p>
        <p className="text-sm text-foreground/70 leading-relaxed">
          {epoch.path === "plan-b"
            ? "Three sub-proofs (solver, execution, shapley) verified independently via settleEpochThreeProof. Used when the recursive aggregator's Groth16 wrap times out — same safety, larger calldata."
            : epoch.path === "groth16"
            ? "260-byte recursively-aggregated Groth16 proof verified via settleEpoch(bytes,bytes). Single SP1Verifier call on Unichain Sepolia."
            : "Awaiting settlement — proof generation in progress."}
        </p>
      </div>

      {/* Payout breakdown */}
      {epoch.shapley && (
        <div className="col-span-full pt-4 border-t border-white/5">
          <p className="mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground mb-4">
            Shapley value distribution (basis points · sums to 10,000)
          </p>

          <div className="flex w-full h-1.5 rounded-full overflow-hidden mb-5 bg-black/50">
            {epoch.shapley.map((bps, i) => (
              <div
                key={i}
                className="transition-all duration-500"
                style={{ width: `${bps / 100}%`, backgroundColor: AGENTS[i]?.color || "gray" }}
              />
            ))}
          </div>

          <div className="grid grid-cols-2 md:grid-cols-5 gap-3">
            {epoch.shapley.map((bps, i) => {
              const a = AGENTS[i];
              return (
                <div
                  key={i}
                  className="relative overflow-hidden bg-black/20 rounded-lg p-3 text-center border border-white/5 hover:bg-white/5 transition-colors"
                >
                  <div
                    className="absolute inset-x-0 bottom-0 h-1 opacity-80"
                    style={{ backgroundColor: a?.color || "gray" }}
                  />
                  <span className="mono text-[10px] text-muted-foreground whitespace-nowrap block mb-1">
                    {a?.longName || `Agent ${i + 1}`}
                  </span>
                  <span
                    className="mono text-[14px] font-medium block"
                    style={{ color: a?.color || "white" }}
                  >
                    {bps}
                  </span>
                  <span className="mono text-[9px] text-muted-foreground opacity-50 block mt-0.5">bps</span>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
};

export default EpochDetail;
