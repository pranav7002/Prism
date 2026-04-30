import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

interface Props {
  value: string | number;
  className?: string;
}

/**
 * Per-character odometer rolling. Each char animates vertically when it changes.
 */
const Odometer = ({ value, className }: Props) => {
  const [chars, setChars] = useState(String(value).split(""));
  useEffect(() => {
    setChars(String(value).split(""));
  }, [value]);

  return (
    <span className={`inline-flex tabular ${className ?? ""}`} aria-label={String(value)}>
      {chars.map((c, i) => (
        <span key={i} className="relative inline-block overflow-hidden" style={{ height: "1em", lineHeight: "1em" }}>
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span
              key={c + "-" + i}
              initial={{ y: "100%", opacity: 0 }}
              animate={{ y: "0%", opacity: 1 }}
              exit={{ y: "-100%", opacity: 0 }}
              transition={{ duration: 0.35, ease: [0.16, 1, 0.3, 1] }}
              className="block"
            >
              {c}
            </motion.span>
          </AnimatePresence>
        </span>
      ))}
    </span>
  );
};

export default Odometer;