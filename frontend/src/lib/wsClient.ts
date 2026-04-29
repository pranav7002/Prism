import { useEffect, useRef, useState } from "react";

// ── WsEvent discriminated union ────────────────────────────────────────────
// Mirrors prism-types::WsEvent exactly. The Rust enum is serde-internally-
// tagged with `#[serde(tag = "type", rename_all = "snake_case")]`, so the
// wire shape is `{ "type": "epoch_settled", "tx_hash": "...", ... }` —
// the discriminator is the `type` field, NOT a wrapper key.
//
// Earlier this type used externally-tagged `{ EpochSettled: {...} }`, which
// never matched real orchestrator output — every selector returned null in
// live mode. Fixed in Tier C smoke test.

export type WsEvent =
  | { type: "epoch_start"; epoch: number; timestamp: number }
  | { type: "intents_received"; count: number; agents: string[] }
  | { type: "solver_running"; conflicts_detected: number }
  | { type: "solver_complete"; plan_hash: string; dropped: string[] }
  | { type: "proof_progress"; program: string; pct: number }
  | { type: "proof_complete"; program: string; time_ms: number }
  | { type: "aggregation_start" }
  | { type: "aggregation_complete"; time_ms: number }
  | { type: "groth16_wrapping"; pct: number }
  | { type: "epoch_settled"; tx_hash: string; gas_used: number; shapley: number[] }
  | { type: "epoch_settled_via_plan_b"; tx_hash: string; gas_used: number; shapley: number[] }
  | { type: "error"; message: string };

// ── Hook ───────────────────────────────────────────────────────────────────

const MAX_EVENTS = 100;
const BASE_BACKOFF_MS = 1_000;
const MAX_BACKOFF_MS = 30_000;

export function useWsEvents(
  url: string,
  enabled: boolean
): { events: WsEvent[]; connected: boolean } {
  const [events, setEvents] = useState<WsEvent[]>([]);
  const [connected, setConnected] = useState(false);

  // Use refs so effect closure always sees latest values without re-running
  const wsRef = useRef<WebSocket | null>(null);
  const backoffRef = useRef(BASE_BACKOFF_MS);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;

    if (!enabled) {
      // Tear down any live connection when switching back to demo
      if (wsRef.current) {
        wsRef.current.onclose = null;
        wsRef.current.close();
        wsRef.current = null;
      }
      if (retryTimerRef.current) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      setConnected(false);
      setEvents([]);
      return;
    }

    const connect = () => {
      if (!mountedRef.current) return;

      const ws = new WebSocket(url);
      wsRef.current = ws;

      ws.onopen = () => {
        if (!mountedRef.current) { ws.close(); return; }
        backoffRef.current = BASE_BACKOFF_MS; // reset on success
        setConnected(true);
      };

      ws.onmessage = (evt: MessageEvent) => {
        if (!mountedRef.current) return;
        try {
          const parsed = JSON.parse(evt.data as string) as WsEvent;
          if (!parsed || !parsed.type) return; // Drop malformed payloads to prevent selector crashes
          setEvents((prev) => [parsed, ...prev].slice(0, MAX_EVENTS));
        } catch {
          // Ignore malformed messages
        }
      };

      ws.onerror = () => {
        // onclose fires right after onerror; handle reconnect there
      };

      ws.onclose = () => {
        if (!mountedRef.current) return;
        setConnected(false);
        wsRef.current = null;

        // Exponential backoff reconnect
        const delay = backoffRef.current;
        backoffRef.current = Math.min(delay * 2, MAX_BACKOFF_MS);
        retryTimerRef.current = setTimeout(connect, delay);
      };
    };

    connect();

    return () => {
      mountedRef.current = false;
      if (retryTimerRef.current) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      if (wsRef.current) {
        wsRef.current.onclose = null; // prevent retry on intentional teardown
        wsRef.current.close();
        wsRef.current = null;
      }
    };
  }, [url, enabled]);

  return { events, connected };
}
