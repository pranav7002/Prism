import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import type { WsEvent } from "@/lib/wsClient";

/**
 * The "live mode no fake data" invariant tests.
 *
 * These mount each page with `demo: false` and `events: []` and assert that
 * NO fabricated payouts, hashes, or metric numbers reach the DOM. If a future
 * contributor adds a `Math.random() / Math.sin()` default to a live render
 * path, these tests break loud.
 *
 * The first audit's DK-1, DK-3, DK-4 (Overview, ShapleyBreakdown,
 * TelemetryMatrix) are the components most at risk of this regression.
 */

// ── Mocks ─────────────────────────────────────────────────────────────────
// We mock the two hooks at the module boundary so each test can inject its
// own (demo, events, connected) state without touching the actual store or
// WebSocket transport.

const mockState = {
  demo: false,
  events: [] as WsEvent[],
  connected: true,
  demoPhaseIdx: 0,
  demoHistory: [] as unknown[],
  wsUrl: "ws://test",
};

vi.mock("@/store/demoMode", () => ({
  useDemoMode: () => mockState,
  DemoModeProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

vi.mock("@/lib/wsClient", async (orig) => {
  const real = await orig<typeof import("@/lib/wsClient")>();
  return {
    ...real,
    useWsEvents: () => ({ events: mockState.events, connected: mockState.connected }),
  };
});

// Recharts uses ResponsiveContainer that needs a non-zero parent in jsdom.
vi.mock("recharts", async (orig) => {
  const real = await orig<typeof import("recharts")>();
  // ResponsiveContainer measures parent. In jsdom it's 0 — wrap children
  // with a fixed-size div so charts mount.
  return {
    ...real,
    ResponsiveContainer: ({ children }: { children: React.ReactNode }) => (
      <div style={{ width: 400, height: 400 }}>{children}</div>
    ),
  };
});

const reset = () => {
  mockState.demo = false;
  mockState.events = [];
  mockState.connected = true;
  mockState.demoHistory = [];
};

// Lazy-import pages AFTER the vi.mock factory has been registered.
const importOverview = () => import("@/pages/Overview").then((m) => m.default);
const importShapley = () =>
  import("@/components/prism/ShapleyBreakdown").then((m) => m.default);
const importTelemetry = () =>
  import("@/components/prism/TelemetryMatrix").then((m) => m.default);

// ── Tests ─────────────────────────────────────────────────────────────────

describe("live mode (demo=false, events=[])", () => {
  beforeEach(reset);

  it("Overview shows '—' / 0% / 'Awaiting first settlement' instead of fabricated payouts", async () => {
    const Overview = await importOverview();
    render(
      <MemoryRouter>
        <Overview />
      </MemoryRouter>,
    );

    // The "Awaiting first settlement" notice must be visible.
    expect(screen.getByText(/Awaiting first settlement/i)).toBeInTheDocument();

    // Every agent card must show "—" for last action.
    // There are 5 cards, so we expect at least 5 "—" tokens.
    const dashes = screen.getAllByText("—");
    expect(dashes.length).toBeGreaterThanOrEqual(5);

    // No agent card should show "Active" status — without a settlement
    // event, every agent is idle.
    expect(screen.queryByText("Active")).toBeNull();
    const idleBadges = screen.getAllByText("Idle");
    expect(idleBadges.length).toBeGreaterThanOrEqual(5);

    // The Odometer renders the targetPayout per character but also exposes
    // the full value via aria-label. Each agent should report "0%".
    const zeroPayouts = screen.getAllByLabelText("0%");
    expect(zeroPayouts.length).toBeGreaterThanOrEqual(5);
  });

  it("ShapleyBreakdown shows the 'Awaiting first settlement' empty state", async () => {
    const ShapleyBreakdown = await importShapley();
    render(<ShapleyBreakdown />);

    // Two awaiting panels (one for the pie, one for the historical area).
    const awaiting = screen.getAllByText(/Awaiting first settlement/i);
    expect(awaiting.length).toBeGreaterThanOrEqual(1);

    // Critically: no pie chart cells, no agent legends. The bar chart and
    // its slider must NOT be rendered (they live behind the demo branch).
    expect(screen.queryByRole("slider")).toBeNull();
  });

  it("TelemetryMatrix shows 'awaiting first epoch' in live mode without events", async () => {
    const TelemetryMatrix = await importTelemetry();
    render(<TelemetryMatrix />);

    expect(screen.getByText(/awaiting first epoch/i)).toBeInTheDocument();

    // No fake "IL Projection -1.X%" or "+14,020 USDC" copy from demo.
    expect(screen.queryByText(/IL Projection/i)).toBeNull();
    expect(screen.queryByText(/14,020/i)).toBeNull();
  });

  it("TelemetryMatrix surfaces real selectors once an epoch_settled event arrives", async () => {
    mockState.events = [
      { type: "epoch_start", epoch: 100, timestamp: 1700000000 },
      { type: "solver_running", conflicts_detected: 7 },
      {
        type: "epoch_settled",
        tx_hash: "0xabc123",
        gas_used: 261234,
        shapley: [3000, 2000, 2000, 1500, 1500],
      },
    ];
    const TelemetryMatrix = await importTelemetry();
    render(<TelemetryMatrix />);

    // Real epoch number + real conflicts + real path + real gas.
    expect(screen.getByText(/Solver/)).toBeInTheDocument();
    expect(screen.getByText("#100")).toBeInTheDocument();
    expect(screen.getByText("7")).toBeInTheDocument(); // conflicts
    expect(screen.getByText("GROTH16")).toBeInTheDocument();
    expect(screen.getByText("261,234")).toBeInTheDocument();
  });
});

describe("demo mode (demo=true)", () => {
  beforeEach(reset);

  it("Overview renders 'Swarm Command' even with no live data", async () => {
    mockState.demo = true;
    const Overview = await importOverview();
    render(
      <MemoryRouter>
        <Overview />
      </MemoryRouter>,
    );
    expect(screen.getByText("Swarm Command")).toBeInTheDocument();
    // Must NOT show the awaiting-settlement notice in demo mode.
    expect(screen.queryByText(/Awaiting first settlement/i)).toBeNull();
  });
});
