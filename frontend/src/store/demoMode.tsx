import { createContext, useContext, useState, useEffect, type ReactNode } from "react";
import type { EpochData } from "@/lib/derivedState";

const DEFAULT_WS_URL =
  (import.meta.env.VITE_WS_URL as string | undefined) ?? "ws://localhost:8765";

interface Ctx {
  demo: boolean;
  toggle: () => void;
  wsUrl: string;
  demoPhaseIdx: number;
  demoHistory: EpochData[];
}

const DemoCtx = createContext<Ctx>({
  demo: true,
  toggle: () => {},
  wsUrl: DEFAULT_WS_URL,
  demoPhaseIdx: 0,
  demoHistory: [],
});

export const DemoModeProvider = ({ children }: { children: ReactNode }) => {
  const [demo, setDemo] = useState(true);
  const [demoPhaseIdx, setDemoPhaseIdx] = useState(0);
  const [demoHistory, setDemoHistory] = useState<EpochData[]>([]);
  const [epochCounter, setEpochCounter] = useState(8492);

  useEffect(() => {
    if (!demo) return;
    const t = setInterval(() => {
      setDemoPhaseIdx((i) => {
        const next = (i + 1) % 5;
        if (next === 0) {
          // A full epoch just completed! Let's push it to history.
          setEpochCounter((prev) => {
            const epochId = prev;
            setDemoHistory((h) => [
              {
                epochId,
                startTime: Date.now() - 40000,
                planHash: "0x" + Array.from({ length: 64 }, () => Math.floor(Math.random() * 16).toString(16)).join(""),
                txHash: "0x" + Array.from({ length: 64 }, () => Math.floor(Math.random() * 16).toString(16)).join(""),
                gasUsed: Math.floor(400000 + Math.random() * 50000),
                path: Math.random() > 0.8 ? "plan-b" : "groth16",
                shapley: (() => {
                  const raw = Array.from({ length: 5 }, () => 1000 + Math.random() * 2000);
                  const total = raw.reduce((a, b) => a + b, 0);
                  const bps = raw.map((r) => Math.round((r / total) * 10000));
                  // Ensure exact sum to 10000
                  const diff = 10000 - bps.reduce((a, b) => a + b, 0);
                  bps[0] += diff;
                  return bps;
                })(),
                intentsProcessed: Math.floor(120 + Math.random() * 300),
                volumeUsd: Math.floor(50000 + Math.random() * 200000),
                baseFee: Math.floor(15 + Math.random() * 20),
              },
              ...h,
            ]);
            return prev + 1;
          });
        }
        return next;
      });
    }, 8000); // 8 seconds per phase
    return () => clearInterval(t);
  }, [demo]);

  return (
    <DemoCtx.Provider
      value={{ demo, toggle: () => setDemo((d) => !d), wsUrl: DEFAULT_WS_URL, demoPhaseIdx, demoHistory }}
    >
      {children}
    </DemoCtx.Provider>
  );
};

export const useDemoMode = () => useContext(DemoCtx);