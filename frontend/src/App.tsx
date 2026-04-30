import { BrowserRouter, Route, Routes } from "react-router-dom";
import { Toaster as Sonner } from "@/components/ui/sonner";
import { Toaster } from "@/components/ui/toaster";
import { TooltipProvider } from "@/components/ui/tooltip";
import NotFound from "./pages/NotFound.tsx";
import Landing from "./pages/Landing.tsx";
import Overview from "./pages/Overview.tsx";
import EpochLive from "./pages/EpochLive.tsx";
import Settlement from "./pages/Settlement.tsx";
import SiteShell from "./components/prism/SiteShell";
import { DemoModeProvider } from "./store/demoMode";

const App = () => (
  <TooltipProvider>
    <Toaster />
    <Sonner />
    <DemoModeProvider>
      <BrowserRouter>
        <Routes>
          <Route element={<SiteShell />}>
            <Route path="/" element={<Landing />} />
            <Route path="/overview" element={<Overview />} />
            <Route path="/epoch/live" element={<EpochLive />} />
            <Route path="/settlement" element={<Settlement />} />
            <Route path="*" element={<NotFound />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </DemoModeProvider>
  </TooltipProvider>
);

export default App;
