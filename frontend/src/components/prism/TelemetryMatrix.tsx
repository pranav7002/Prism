const Dot = ({ color }: { color: string }) => (
  <span
    className="inline-block w-1.5 h-1.5 rounded-full animate-soft-pulse"
    style={{ background: color, boxShadow: `0 0 12px ${color}` }}
  />
);

const TelemetryMatrix = () => {
  return (
    <div className="glass mx-auto max-w-3xl h-16 grid grid-cols-3 items-center px-6 animate-fade-up"
         style={{ animationDelay: "200ms" }}>
      <div className="flex items-center gap-3 border-r border-foreground/[0.05] pr-6">
        <Dot color="hsl(var(--agent-alpha))" />
        <span className="mono text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
          Orchestrator <span className="text-foreground/80">Stable</span>
        </span>
      </div>
      <div className="flex items-center gap-3 justify-center border-r border-foreground/[0.05] px-6">
        <Dot color="hsl(var(--agent-beta))" />
        <span className="mono text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
          ZK Circuits <span className="text-foreground/80 tabular">5/5</span>
        </span>
      </div>
      <div className="flex items-center gap-3 justify-end pl-6">
        <span className="flex items-end gap-[3px] h-3" aria-hidden>
          <span className="w-[2px] h-full bg-foreground/60 origin-bottom animate-wave-1 rounded-sm" />
          <span className="w-[2px] h-full bg-foreground/60 origin-bottom animate-wave-2 rounded-sm" />
          <span className="w-[2px] h-full bg-foreground/60 origin-bottom animate-wave-3 rounded-sm" />
        </span>
        <span className="mono text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
          Listening for signals
        </span>
      </div>
    </div>
  );
};

export default TelemetryMatrix;
