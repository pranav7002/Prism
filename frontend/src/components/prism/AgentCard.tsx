import AgentGlyph from "./AgentGlyph";
import Odometer from "./Odometer";
import { AGENT_COLORS, type AgentKey } from "@/lib/agents";

interface Props {
  agent: AgentKey;
  symbol: string;       // α, β...
  name: string;
  description: string;
  uptime: string;
  priority: string;
  className?: string;
  status?: "idle" | "active";
  lastAction?: string;
  targetPayout?: number; // 0-100
}

const AgentCard = ({ agent, symbol, name, description, uptime, priority, className, status = "idle", lastAction = "—", targetPayout = 0 }: Props) => {
  const c = AGENT_COLORS[agent];
  return (
    <article
      className={`glass group relative p-8 transition-all duration-300 ease-out hover:bg-surface-1/60 ${className ?? ""}`}
      style={{ ...({ "--ring-c": c } as React.CSSProperties), minHeight: 380 }}
    >
      <div
        className="pointer-events-none absolute inset-0 rounded-[var(--radius)] opacity-0 transition-opacity duration-300 group-hover:opacity-100"
        style={{ boxShadow: `inset 0 0 0 1px ${c}55, 0 30px 80px -40px ${c}55` }}
      />
      <header className="relative flex items-start justify-between">
        <div className="flex items-center gap-3">
          <span
            className="grid h-9 w-9 place-items-center rounded-full glass-2 text-base"
            style={{ color: c, fontFamily: "var(--font-display)" }}
          >
            {symbol}
          </span>
          <div>
            <h3 className="text-sm font-medium tracking-tight">{name}</h3>
            <p className="mono text-[10px] uppercase tracking-[0.12em] text-muted-foreground">Agent · {agent}</p>
          </div>
        </div>

        <AgentGlyph agent={agent} size={56} />
      </header>

      <p className="relative mt-6 text-sm text-foreground/70 leading-relaxed">{description}</p>

      <div className="relative mt-6 grid grid-cols-3 gap-4 border-t border-foreground/[0.06] pt-5">
        <div>
          <p className="mono text-[9px] uppercase tracking-[0.12em] text-muted-foreground mb-1.5">Status</p>
          <span
            className="inline-flex items-center gap-1.5 mono text-[10px] uppercase tracking-[0.1em] px-2 py-1 rounded-full"
            style={{
              color: status === "active" ? c : "hsl(var(--foreground) / 0.4)",
              background: status === "active" ? `${c}15` : "hsl(var(--foreground) / 0.04)",
              border: `1px solid ${status === "active" ? `${c}55` : "hsl(var(--foreground) / 0.1)"}`,
            }}
          >
            <span
              className={`h-1.5 w-1.5 rounded-full ${status === "active" ? "animate-soft-pulse" : ""}`}
              style={{ background: status === "active" ? c : "hsl(var(--foreground) / 0.3)" }}
            />
            {status === "active" ? "Active" : "Idle"}
          </span>
        </div>
        <div>
          <p className="mono text-[9px] uppercase tracking-[0.12em] text-muted-foreground mb-1.5">Last Action</p>
          <p className="mono text-[11px] text-foreground/80 tabular">{lastAction}</p>
        </div>
        <div>
          <p className="mono text-[9px] uppercase tracking-[0.12em] text-muted-foreground mb-1.5">Target Payout</p>
          <p className="mono text-[11px] tabular" style={{ color: c }}>
            <Odometer value={`${targetPayout}%`} />
          </p>
        </div>
      </div>

      <footer className="relative mt-4 flex items-center justify-between text-muted-foreground/70">
        <div className="mono text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
          Uptime <span className="text-foreground/80 tabular ml-1">{uptime}</span>
        </div>
        <div className="mono text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
          Priority <span className="text-foreground/80 tabular ml-1">{priority}</span>
        </div>
      </footer>
    </article>
  );
};

export default AgentCard;
