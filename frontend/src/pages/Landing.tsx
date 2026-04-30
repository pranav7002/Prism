import { useNavigate } from "react-router-dom";
import TelemetryMatrix from "@/components/prism/TelemetryMatrix";
import SwarmAbstraction from "@/components/prism/SwarmAbstraction";
import { AGENTS } from "@/lib/agents";

const Landing = () => {
  const nav = useNavigate();
  return (
    <>
      {/* Hero with side-by-side swarm */}
      <section className="container mx-auto pt-10 pb-16">
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-10 lg:gap-6 items-center">
          {/* Left — Hero copy */}
          <div className="lg:col-span-7 text-left">
            <div className="inline-flex items-center gap-2 rounded-full glass-2 px-4 py-1.5 mb-8 animate-fade-up">
              <span className="h-1.5 w-1.5 rounded-full bg-agent-beta animate-soft-pulse"
                    style={{ boxShadow: "0 0 12px hsl(var(--agent-beta))" }} />
              <span className="mono text-[11px] uppercase tracking-[0.14em] text-foreground/70">
                Live on Uniswap V4
              </span>
            </div>

            <h1 className="display text-5xl md:text-6xl lg:text-7xl max-w-2xl animate-fade-up"
                style={{ animationDelay: "60ms" }}>
              The Autonomous Layer of{" "}
              <em className="text-gradient italic" style={{ fontStyle: "italic" }}>DeFi</em>
            </h1>

            <p className="mt-6 max-w-lg text-base md:text-lg text-foreground/60 leading-relaxed animate-fade-up"
               style={{ animationDelay: "140ms" }}>
              A swarm of five autonomous agents coordinating Uniswap V4 liquidity. Trustless. Unstoppable.
            </p>

            <div className="mt-8 flex items-center gap-4 animate-fade-up" style={{ animationDelay: "200ms" }}>
              <button
                onClick={() => nav("/overview")}
                className="group relative inline-flex items-center gap-2 rounded-full px-6 py-3 text-sm font-medium overflow-hidden transition-all duration-300"
                style={{ background: "hsl(var(--surface-2) / 0.6)", border: "1px solid hsl(var(--primary) / 0.6)", boxShadow: "var(--shadow-glow)" }}
              >
                <span className="absolute inset-0 opacity-0 group-hover:opacity-100 transition-opacity duration-300"
                      style={{ background: "var(--gradient-prism)" }} />
                <span className="relative z-10 transition-colors duration-300 group-hover:text-background">
                  Initialize Protocol
                </span>
                <svg className="relative z-10 transition-colors duration-300 group-hover:text-background" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6">
                  <path d="M5 12h14M13 6l6 6-6 6" />
                </svg>
              </button>
            </div>
          </div>

          {/* Right — Swarm orbit */}
          <div className="lg:col-span-5 flex items-center justify-center animate-fade-up" style={{ animationDelay: "260ms" }}>
            <div className="scale-90 lg:scale-100">
              <SwarmAbstraction onSelect={() => nav("/overview")} />
            </div>
          </div>
        </div>
      </section>

      {/* Agent descriptions — always visible */}
      <section className="container mx-auto pb-16">
        <div className="mb-8">
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Section · 02 · The Swarm</p>
          <h2 className="display text-3xl md:text-4xl mt-2">Five agents. One protocol.</h2>
        </div>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-5 gap-3">
          {AGENTS.map((a) => (
            <div
              key={a.key}
              className="glass p-5 transition-all duration-300 hover:-translate-y-0.5"
              style={{ borderColor: "hsl(var(--foreground) / 0.06)" }}
            >
              <div className="flex items-center gap-3 mb-3">
                <span
                  className="grid h-9 w-9 place-items-center rounded-full"
                  style={{
                    background: `radial-gradient(circle, ${a.color}33 0%, ${a.color}10 60%, transparent 100%)`,
                    border: `1px solid ${a.color}55`,
                  }}
                >
                  <span className="display text-lg" style={{ color: a.color }}>{a.symbol}</span>
                </span>
                <span className="mono text-[10px] uppercase tracking-[0.14em]" style={{ color: a.color }}>
                  {a.name}
                </span>
              </div>
              <p className="text-[12.5px] text-foreground/65 leading-relaxed">{a.description}</p>
            </div>
          ))}
        </div>

        <div className="mt-10">
          <TelemetryMatrix />
        </div>
      </section>
    </>
  );
};

export default Landing;