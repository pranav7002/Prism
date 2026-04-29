const AmbientBackground = () => (
  <div className="ambient-orbs" aria-hidden>
    <span style={{ top: "-10%", left: "-5%", background: "hsl(var(--agent-alpha))" }} />
    <span style={{ top: "20%", right: "-10%", background: "hsl(var(--primary))" }} />
    <span style={{ bottom: "-15%", left: "20%", background: "hsl(var(--agent-epsilon))" }} />
    <span style={{ bottom: "10%", right: "10%", background: "hsl(var(--agent-beta))", width: 360, height: 360 }} />
  </div>
);

export default AmbientBackground;
