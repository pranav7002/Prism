# prism-signals

Real-time dashboard for the PRISM Protocol — a swarm of five autonomous agents coordinating Uniswap V4 liquidity with ZK-verified Shapley value payouts.

## Stack

- React 18 + Vite + TypeScript
- Tailwind CSS + Shadcn UI + Recharts
- Framer Motion
- WebSocket live data from the Rust orchestrator

## Pages

| Route | Description |
|-------|-------------|
| `/` | Landing — agent orbit animation, protocol overview |
| `/overview` | Swarm command center — live agent telemetry |
| `/epoch/live` | Epoch pipeline — ZK proof stages, signal ledger |
| `/settlement` | Settlement — Shapley distribution, MEV capture |

## Running locally

```bash
npm install
npm run dev       # starts on http://localhost:8080
```

Set environment variables in `.env`:

```
VITE_WS_URL=ws://localhost:8765
VITE_PRISM_HOOK_ADDRESS=0x0B9Ae4690F8b6EAbB1511a6e1C64C948b9edCFC0
VITE_RPC_URL=https://sepolia.unichain.org
VITE_CHAIN_ID=1301
```

## WebSocket events

The dashboard connects to the PRISM orchestrator WebSocket at `VITE_WS_URL` and consumes 12 event variants:

- `epoch_started`, `intent_received`, `conflicts_detected`
- `proof_progress`, `epoch_settled`, `epoch_settled_via_plan_b`
- `agent_registered`, `kill_switch_triggered`, `dynamic_fee_updated`
- ...and more — see `src/lib/wsClient.ts`

## Testing

```bash
npm test
```
