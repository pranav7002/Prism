type AgentKey = "alpha" | "beta" | "gamma" | "delta" | "epsilon";

const colorVar: Record<AgentKey, string> = {
  alpha: "hsl(var(--agent-alpha))",
  beta: "hsl(var(--agent-beta))",
  gamma: "hsl(var(--agent-gamma))",
  delta: "hsl(var(--agent-delta))",
  epsilon: "hsl(var(--agent-epsilon))",
};

interface Props { agent: AgentKey; size?: number; }

// Lightweight SVG "Lottie-like" idle-breathing geometric glyph (bespoke per agent)
const AgentGlyph = ({ agent, size = 140 }: Props) => {
  const stroke = colorVar[agent];
  const common = { stroke, strokeWidth: 1, fill: "none", strokeLinecap: "round" as const, strokeLinejoin: "round" as const };

  return (
    <div className="relative grid place-items-center animate-breathe" style={{ width: size, height: size }}>
      <div
        className="absolute inset-0 rounded-full"
        style={{ background: `radial-gradient(circle at 50% 50%, ${stroke}26 0%, transparent 65%)` }}
      />
      <svg viewBox="0 0 120 120" width={size} height={size} className="relative">
        <defs>
          <radialGradient id={`g-${agent}`} cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor={stroke} stopOpacity="0.9" />
            <stop offset="100%" stopColor={stroke} stopOpacity="0.2" />
          </radialGradient>
        </defs>

        {agent === "alpha" && (
          <g {...common}>
            <circle cx="60" cy="60" r="34" opacity="0.5" />
            <circle cx="60" cy="60" r="22" opacity="0.8" />
            <path d="M26 60 L94 60 M60 26 L60 94" opacity="0.4" />
            <circle cx="60" cy="60" r="3" fill={stroke} stroke="none" />
          </g>
        )}
        {agent === "beta" && (
          <g {...common}>
            <polygon points="60,20 96,46 82,90 38,90 24,46" opacity="0.7" />
            <polygon points="60,36 84,52 74,80 46,80 36,52" opacity="0.4" />
            <circle cx="60" cy="60" r="4" fill={stroke} stroke="none" />
          </g>
        )}
        {agent === "gamma" && (
          <g {...common}>
            <path d="M30 80 Q60 20 90 80" opacity="0.7" />
            <path d="M30 80 Q60 100 90 80" opacity="0.5" />
            <circle cx="60" cy="60" r="14" opacity="0.6" />
          </g>
        )}
        {agent === "delta" && (
          <g {...common}>
            <rect x="28" y="28" width="64" height="64" rx="2" opacity="0.4" transform="rotate(45 60 60)" />
            <rect x="38" y="38" width="44" height="44" rx="2" opacity="0.7" />
            <circle cx="60" cy="60" r="3" fill={stroke} stroke="none" />
          </g>
        )}
        {agent === "epsilon" && (
          <g {...common}>
            <circle cx="60" cy="60" r="38" opacity="0.5" />
            <path d="M22 60 Q60 22 98 60 Q60 98 22 60Z" opacity="0.7" />
            <circle cx="60" cy="60" r="5" fill={`url(#g-${agent})`} stroke="none" />
          </g>
        )}
      </svg>
    </div>
  );
};

export default AgentGlyph;
export type { AgentKey };
