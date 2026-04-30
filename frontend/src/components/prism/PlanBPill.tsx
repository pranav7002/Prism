import { motion, AnimatePresence } from "framer-motion";
import { useDemoMode } from "@/store/demoMode";
import { useWsEvents } from "@/lib/wsClient";
import { lastSettlePath } from "@/lib/derivedState";

/**
 * Small "PLAN-B" pill that lights up yellow when the most-recent epoch was
 * settled via `settleEpochThreeProof` (i.e. the orchestrator emitted
 * `EpochSettledViaPlanB` rather than `EpochSettled`). Hidden when the live
 * stream is in demo mode or has only seen the Groth16 happy path.
 *
 * The Plan-B path runs three sub-proof verifications instead of a single
 * Groth16 wrap — slower and ~3× the gas, but available when the recursive
 * aggregator or its Groth16 wrapper times out on demo hardware.
 */
const PlanBPill = () => {
  const { demo, wsUrl } = useDemoMode();
  const { events } = useWsEvents(wsUrl, !demo);
  const path = lastSettlePath(events);

  const visible = !demo && path === "plan-b";

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          key="plan-b-pill"
          initial={{ opacity: 0, y: -6 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -6 }}
          transition={{ duration: 0.25 }}
          title="Last epoch settled via three sub-proofs (Groth16 wrap fallback)."
          className="mono text-[10px] uppercase tracking-[0.18em] px-3 py-1.5 rounded-full"
          style={{
            color: "hsl(48 96% 60%)",
            background: "hsl(48 96% 60% / 0.10)",
            border: "1px solid hsl(48 96% 60% / 0.45)",
            boxShadow: "0 0 18px hsl(48 96% 60% / 0.30)",
          }}
        >
          ▲ PLAN-B
        </motion.div>
      )}
    </AnimatePresence>
  );
};

export default PlanBPill;
