import { motion, AnimatePresence } from "framer-motion";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";

/**
 * Header pill that shows current mode and WebSocket connection status.
 *
 * States:
 *  - demo=true  → "DEMO"
 *  - demo=false, connected=true  → "LIVE ●" (animated dot)
 *  - demo=false, connected=false → "LIVE ✕"
 */
const DemoToggle = () => {
  const { demo, toggle, wsUrl } = useDemoMode();
  const { connected } = useWsEvents(wsUrl, !demo);

  const isLive = !demo;
  const label = demo ? "DEMO" : connected ? "LIVE" : "LIVE";

  // Colour tokens matching SiteShell / SignalLedger aesthetic
  const color = demo
    ? "hsl(var(--agent-delta))"
    : connected
    ? "hsl(var(--agent-beta))"
    : "hsl(var(--muted-foreground))";

  const bg = demo
    ? "hsl(var(--agent-delta) / 0.08)"
    : connected
    ? "hsl(var(--agent-beta) / 0.08)"
    : "hsl(var(--surface-2) / 0.5)";

  const border = demo
    ? "1px solid hsl(var(--agent-delta) / 0.4)"
    : connected
    ? "1px solid hsl(var(--agent-beta) / 0.4)"
    : "1px solid hsl(var(--foreground) / 0.12)";

  return (
    <button
      onClick={toggle}
      aria-pressed={!demo}
      aria-label={demo ? "Switch to live mode" : "Switch to demo mode"}
      className="flex items-center gap-1.5 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-foreground/30 rounded-full"
    >
      <AnimatePresence mode="wait">
        <motion.div
          key={demo ? "demo" : connected ? "live-ok" : "live-err"}
          initial={{ opacity: 0, scale: 0.9 }}
          animate={{ opacity: 1, scale: 1 }}
          exit={{ opacity: 0, scale: 0.9 }}
          transition={{ duration: 0.2 }}
          className="mono text-[10px] uppercase tracking-[0.18em] px-3 py-1.5 rounded-full flex items-center gap-1.5"
          style={{ color, background: bg, border }}
        >
          {/* Status indicator */}
          {isLive && connected && (
            <motion.span
              animate={{ opacity: [1, 0.2, 1] }}
              transition={{ duration: 1.4, repeat: Infinity, ease: "easeInOut" }}
              className="inline-block h-1.5 w-1.5 rounded-full"
              style={{ background: color }}
            />
          )}
          {isLive && !connected && (
            <span className="inline-block text-[11px] leading-none" style={{ color }}>
              ✕
            </span>
          )}
          {demo && (
            <span
              className="inline-block h-1.5 w-1.5 rounded-full"
              style={{ background: color }}
            />
          )}
          {label}
        </motion.div>
      </AnimatePresence>
    </button>
  );
};

export default DemoToggle;
