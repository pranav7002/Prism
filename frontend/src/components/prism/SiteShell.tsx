import { Link, NavLink, Outlet, useLocation } from "react-router-dom";
import { AnimatePresence, motion } from "framer-motion";
import AmbientBackground from "./AmbientBackground";
import { useDemoMode } from "@/store/demoMode";
import DemoToggle from "./DemoToggle";
import PlanBPill from "./PlanBPill";

const navItems = [
  { to: "/", label: "Landing" },
  { to: "/overview", label: "Overview" },
  { to: "/epoch/live", label: "Operations" },
  { to: "/settlement", label: "Settlement" },
];

const SiteShell = () => {
  const location = useLocation();
  const { demo, toggle } = useDemoMode();

  return (
    <div className="relative min-h-screen grain overflow-hidden">
      <AmbientBackground />

      {/* Simulated Network banner */}
      <AnimatePresence>
        {demo && (
          <motion.div
            key="sim-banner"
            initial={{ y: -28, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            exit={{ y: -28, opacity: 0 }}
            transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
            className="fixed top-3 right-4 z-50"
          >
            <div
              className="mono text-[10px] uppercase tracking-[0.18em] px-3 py-1.5 rounded-full"
              style={{
                color: "hsl(var(--agent-delta))",
                background: "hsl(var(--agent-delta) / 0.08)",
                border: "1px solid hsl(var(--agent-delta) / 0.4)",
                boxShadow: "0 0 24px hsl(var(--agent-delta) / 0.35)",
              }}
            >
              ● Simulated Network
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <header className="relative z-10">
        <div className="container mx-auto flex h-16 items-center justify-between">
          <Link to="/" className="flex items-center gap-3">
            <svg width="20" height="20" viewBox="0 0 24 24" aria-hidden>
              <polygon points="12,3 22,20 2,20" fill="none" stroke="hsl(var(--foreground))" strokeWidth="1.2" />
            </svg>
            <span className="text-sm font-medium tracking-tight">PRISM</span>
            <span className="mx-3 h-3 w-px bg-foreground/15" />
            <span className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
              The Autonomous Layer
            </span>
          </Link>
          <nav className="hidden md:flex items-center gap-8 text-sm">
            {navItems.map((n) => (
              <NavLink
                key={n.to}
                to={n.to}
                end={n.to === "/"}
                className={({ isActive }) =>
                  `transition-colors ${isActive ? "text-foreground" : "text-foreground/60 hover:text-foreground"}`
                }
              >
                {n.label}
              </NavLink>
            ))}
          </nav>
          <div className="flex items-center gap-3">
            <span className="mono text-[11px] uppercase tracking-[0.12em] text-muted-foreground hidden sm:inline">
              Epoch <span className="tabular text-foreground/80">#8492</span>
            </span>
            <PlanBPill />
            <DemoToggle />
          </div>
        </div>
      </header>

      <main className="relative z-10">
        <AnimatePresence mode="wait">
          <motion.div
            key={location.pathname}
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            transition={{ duration: 0.15, ease: [0.16, 1, 0.3, 1] }}
          >
            <Outlet />
          </motion.div>
        </AnimatePresence>
      </main>

      <footer className="relative z-10 border-t border-foreground/[0.06] mt-10">
        <div className="container mx-auto h-16 flex items-center justify-between">
          <span className="mono text-[11px] uppercase tracking-[0.12em] text-muted-foreground">
            © PRISM Protocol
          </span>
          <button
            onClick={toggle}
            className="group flex items-center gap-3"
            aria-pressed={demo}
            aria-label="Toggle simulated network demo mode"
          >
            <span
              className="relative inline-flex h-5 w-9 items-center rounded-full transition-colors"
              style={{
                background: demo ? "hsl(var(--agent-delta) / 0.25)" : "hsl(var(--surface-2))",
                border: `1px solid ${demo ? "hsl(var(--agent-delta) / 0.6)" : "hsl(var(--foreground) / 0.1)"}`,
              }}
            >
              <span
                className="absolute h-3.5 w-3.5 rounded-full transition-transform"
                style={{
                  background: demo ? "hsl(var(--agent-delta))" : "hsl(var(--foreground) / 0.4)",
                  transform: demo ? "translateX(18px)" : "translateX(3px)",
                  boxShadow: demo ? "0 0 12px hsl(var(--agent-delta))" : "none",
                }}
              />
            </span>
            <span className="mono text-[11px] uppercase tracking-[0.12em] text-foreground/70 group-hover:text-foreground transition-colors">
              Demo Mode
            </span>
          </button>
        </div>
      </footer>
    </div>
  );
};

export default SiteShell;