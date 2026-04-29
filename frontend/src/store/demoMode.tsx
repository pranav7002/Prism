import { createContext, useContext, useState, type ReactNode } from "react";

const DEFAULT_WS_URL =
  (import.meta.env.VITE_WS_URL as string | undefined) ?? "ws://localhost:8765";

interface Ctx {
  demo: boolean;
  toggle: () => void;
  wsUrl: string;
}

const DemoCtx = createContext<Ctx>({
  demo: true,
  toggle: () => {},
  wsUrl: DEFAULT_WS_URL,
});

export const DemoModeProvider = ({ children }: { children: ReactNode }) => {
  const [demo, setDemo] = useState(true);
  return (
    <DemoCtx.Provider
      value={{ demo, toggle: () => setDemo((d) => !d), wsUrl: DEFAULT_WS_URL }}
    >
      {children}
    </DemoCtx.Provider>
  );
};

export const useDemoMode = () => useContext(DemoCtx);