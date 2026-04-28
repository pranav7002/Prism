# Dev 3 — Handoff (Python swarm + Next.js frontend)

> **Audience:** Dev 3 (5 agent brains + Next.js dashboard + FEEDBACK.md).
> **Author:** Dev 1 (Rust + SP1).
> **As of:** 2026-04-28, branch `rust-zk-integration` @ commit `5c45bc9`.
> **Read alongside:** `walkthrough.md` (architecture), `DEV2_HANDOFF.md` (contracts side).

This file is self-contained. You should not need to ping me to start work.

---

## TL;DR — what changes for you and why

1. **Your test suite cannot currently run.** `agent-brains/common/commitment.py:27` imports `Crypto` (pycryptodome) at module load time, but no `requirements.txt` / `pyproject.toml` declares it. Pytest fails on all 4 test files at collection. This is the cheapest thing on your list — fix it first. (N1 in the audit.)
2. **The Next.js frontend (v1 §9, v2 §13.3 W3-W5) does not exist.** Zero files in `prism-vihaan/` matching `front|web|next|app|dashboard|ui`. The orchestrator's WebSocket schema is locked and round-trip tested — the moment you scaffold the client, the contract is stable. (N2.)
3. **No `FEEDBACK.md`** — required Uniswap-sponsor-track deliverable per v2 §16.3. Cannot be authentic until Dev 2 closes C1 (V4 hook deploy). (N3.)
4. **β's brain emits an action it has no on-chain capability for.** β has only `canSetFee`; β's `_opportunity_intent` returns `MigrateLiquidityAction`. The on-chain commit currently goes through (no action-vs-cap check), but it'll fail any future enforcement (and is internally inconsistent). (C4 + N4.)
5. **The Python swarm never calls `commitIntent()` on-chain.** The whole commit-reveal binding is bypassed. A real fix is in your scope. (C5.)
6. **`run_swarm.py --live` silently downgrades to offline on connection failure.** This wrecks demo prep — there's no signal that the swarm isn't actually broadcasting. (C6.)

The orchestrator side of the WS contract is **stable**. Phase 2 of my work added a commitment-window for late intents (H1) — agents broadcasting slightly ahead of the orchestrator's tick are now buffered for the next epoch instead of dropped, but the WS payload shape is unchanged.

---

## Current state of `agent-brains/` (your branch)

This branch (`rust-zk-integration`) does **not** contain `agent-brains/`. Your work lives on the `agents-brain` branch. Last known state from when I checked the remote: `c86d14e` on `origin/agents-brain` ahead of `5d9658b`.

The `prism-vihaan` audit-snapshot repo has a copy of the agent brains I audited — refer to `/home/pratham/Sarnav/Prism/prism-vihaan/agent-brains/` for the full audited tree. **Do not edit prism-vihaan** — it's a snapshot, not the source of truth.

---

## What you need from me (the Rust/SP1 side) — already on this branch

| Artifact | Where | Status |
|---|---|---|
| WebSocket event schema (frontend consumer) | `crates/prism-types/src/lib.rs::WsEvent` | ✅ stable; 8 round-trip tests assert the JSON shape |
| Wire intent format (agent producer) | `crates/prism-types/src/lib.rs::AgentIntentWire` | ✅ stable |
| Commitment encoding (must match Python's keccak path) | `compute_commitment` in `prism-types/src/lib.rs` | ✅ byte-identical with Python's `commitment.py` for 3/10 variants — see N5 below |
| Test-vector printer for parity coverage | `cargo run --example print_test_vector -p prism-types` | ✅ added in commit `8ece670` (Phase 1) |
| WebSocket server endpoint | `0.0.0.0:8765` (configurable via `WS_BIND_ADDR`) | ✅ |
| Commit-reveal verification on inbound intents | orchestrator `main.rs:425-505` | ✅ now enforced (C7 in audit, fixed in commit `8cbfa82` upstream) |

### WS event shapes you'll consume

```rust
// from crates/prism-types/src/lib.rs:385+
pub enum WsEvent {
    EpochStart { epoch: u64, timestamp: u64 },
    IntentsReceived { count: u32, agents: Vec<String> },
    SolverRunning { conflicts_detected: u32 },          // ← real count after H3 fix
    SolverComplete { plan_hash: String, dropped: Vec<String> },  // ← structured drops after H2
    ProofProgress { program: String, pct: u8 },
    ProofComplete { program: String, time_ms: u64 },
    AggregationStart,
    AggregationComplete { time_ms: u64 },
    Groth16Wrapping { pct: u8 },
    EpochSettled { tx_hash: String, gas_used: u64, shapley: Vec<u16> },
    Error { message: String },
}
```

JSON tag is the variant name in PascalCase (e.g. `{"type":"EpochStart","epoch":5,"timestamp":...}`). Test it against `crates/prism-types/src/lib.rs::tests::ws_event_*` for 8 golden vectors.

### Inbound (your agents → orchestrator) shape

```json
{
  "type": "SubmitIntent",
  "intent": { ...AgentIntentWire... },
  "commitment": "0x<32-byte hex>"
}
```

The orchestrator now requires the `commitment` field and rejects on mismatch with the value it recomputes from the wire fields (audit fix C7). Make sure your broadcaster includes it.

---

## Audit findings in YOUR scope — ordered by demo-criticality

Numbering matches `/home/pratham/Sarnav/Prism/prism-vihaan/AUDIT_REPORT_2026-04-28.md`.

### 🔴 C5 — Python swarm never calls on-chain `commitIntent()`

Today: agents compute a keccak commitment locally and broadcast it inside the WS payload. The PrismHook's `commitIntent(bytes32)` is **never invoked from the swarm**. The orchestrator's WS-side check (post-C7) only verifies wire-vs-recomputed self-consistency — it has no chain-side anchor.

This means a malicious orchestrator could synthesize commitments after seeing all reveals. The "commit-reveal" story works at the message layer only.

**Fix:** add a `chain_client.py` in `agent-brains/common/`:

```python
# pseudo-sketch
from web3 import Web3
from eth_account import Account
class ChainClient:
    def __init__(self, rpc_url, private_key, hook_address):
        self.w3 = Web3(Web3.HTTPProvider(rpc_url))
        self.account = Account.from_key(private_key)
        self.hook_address = hook_address
        self.hook_abi = ...  # commitIntent(bytes32) selector

    async def commit_intent(self, commitment_bytes32: bytes) -> str:
        tx = self.hook.functions.commitIntent(commitment_bytes32).build_transaction(...)
        signed = self.account.sign_transaction(tx)
        tx_hash = self.w3.eth.send_raw_transaction(signed.rawTransaction)
        receipt = self.w3.eth.wait_for_transaction_receipt(tx_hash, timeout=10)
        if receipt.status != 1:
            raise RuntimeError(f"commitIntent reverted: {tx_hash.hex()}")
        return tx_hash.hex()
```

Plumb it into `broadcaster.send_intent()` so the on-chain commit must succeed *before* the WS reveal is sent. Each agent's EOA must be funded on Unichain Sepolia (Dev 2 will publish the deployed PrismHook address — coordinate with them).

The `bytes32` you pass is the same value your `commitment.py` computes today — already byte-identical with my Rust side for 3/10 variants (see N5).

### 🔴 C6 — `run_swarm.py --live` silently falls back to offline mode

`agent-brains/run_swarm.py:101-106`:

```python
if args.live:
    connected = await broadcaster.connect()
    if not connected:
        print("⚠ Could not connect to orchestrator, falling back to offline mode")
        broadcaster = get_broadcaster(offline=True)
        await broadcaster.connect()
```

A misconfigured WS URL or dead orchestrator yields zero exit code and "✓ sent" log lines for the rest of the run. CI / demo prep cannot tell.

**Fix:** in `--live` mode, fail fast — `sys.exit(1)`. Make `--offline` an explicit flag for the offline shim.

### 🔴 C4 / N4 — β emits `MigrateLiquidity` but β only has `canSetFee`

β's deploy-time capabilities are `{canSetFee: true, all others: false}` (see `DeployPrismHook.s.sol:100-110`). β's `_opportunity_intent` (`agent-brains/beta/brain.py:105-122`) returns a `MigrateLiquidityAction`. The on-chain commit currently goes through (no action-vs-cap check), but Dev 2 may add capability-vs-action enforcement (audit H9), and the SP1 circuit might too in a future hardening pass.

**Note: v2 spec is internally inconsistent here.** v2 §1.1 names β "Fee Curator" and v2 §10.2 calls for β to do `MigrateLiquidity 200k USDC from 0.30% → 0.60% pool + SetDynamicFee 6000 ppm` in the opportunity epoch. Either (a) β should be granted `canLP` in the deploy script (then your brain is correct), or (b) β's opportunity intent should be split into two intents, with the migrate signed by α/γ.

**Recommended:** option (a). Coordinate with Dev 2 to widen β's capabilities. Until that lands, change β's opportunity branch to emit `SetDynamicFee` with the higher fee tier — that's still spec-aligned (β is the Fee Curator) and stops being internally inconsistent.

### 🟠 H13 — `OnChainMarketReader` uses Uniswap V3 selectors

`agent-brains/common/market_reader.py:154` — `_FEE_SELECTOR = 0xddca3f43` is V3-only; V4 pools have no `fee()` view, and V4's `getSlot0()` layout differs (no observation slots).

**Fix:** rewrite reads against `IPoolManager`'s actual interface. `sqrtPrice/tick` come from `PoolManager.getSlot0(poolId)`; `fee` is a field of `PoolKey` (read it from the key, not the pool). Coordinate with Dev 2 — they own the contract side.

### 🟠 H14 — Volatility hardcoded `1500` everywhere

`agent-brains/common/market_reader.py:312` — `OnChainMarketReader.get_pool_state` always reports `volatility_30d_bps=1500`. Every brain that uses a vol threshold can't actually trip it. ε's kill-switch never fires from real conditions.

**Fix:** add a real TWAP / ATR oracle, or a historical-vol indexer. Hackathon-acceptable workaround: read from a Pyth/Chainlink feed if one's available on Unichain Sepolia, else compute rolling stddev over the last 30 epoch sqrtPriceX96 readings.

### 🟠 H15 — No mid-session WebSocket reconnect

`agent-brains/common/broadcaster.py:133-138` — `_listen_events` exits, `_connected = False` is set, no reconnect. Subsequent `send_intent` calls return `"Not connected"` and intents drop silently.

The connect path already has exponential-backoff retry (broadcaster.py:40-42, 77-113). Extend the same pattern to mid-session disconnects: when `_listen_events` exits unexpectedly, restart it with a bounded retry budget; expose a callback for "connection actually dead" so `run_swarm.py` can exit non-zero (ties into C6).

### 🟠 H16 — No retry queue for failed intent sends

`broadcaster.py:189-204`, `run_swarm.py:69-82` — failed sends are logged and forgotten.

**Fix:** persistent FIFO queue with replay on reconnect. Pair with H15 — same retry/backoff machinery handles both.

### 🔵 H18 — Production code carries `info: Any` Pydantic validators

`agent-brains/common/schemas.py:59,89,107,etc.` — using `Any` instead of `pydantic.ValidationInfo` masks type errors; Pydantic v2 type-checks validators against this signature.

**Fix:** trivial — `from pydantic import ValidationInfo` and replace.

### 🆕 N1 — Python tests are broken at collection

`from Crypto.Hash import keccak as _pycryptodome_keccak` at top of `agent-brains/common/commitment.py` requires pycryptodome. Nothing declares it.

**Fix (do this first):** add `agent-brains/requirements.txt`:

```
pycryptodome>=3.20
web3>=6.20
eth-account>=0.13
pydantic>=2.5
websockets>=12.0
pytest>=8.0
pytest-asyncio>=0.23
```

Document install in your branch's README: `pip install -r requirements.txt`.

### 🆕 N2 — Frontend missing entirely

Scaffold `frontend/` (or `web/`) as a Next.js 15 App-Router project. Components per v1 §9:

- `EpochTimeline` — renders the WS event sequence horizontally, one row per epoch, with proof-pipeline progress bars
- `AgentSwarm` — 5 cards (α/β/γ/δ/ε) animating intent submission and Shapley credit
- `ProofPipeline` — 4-stage diagram (solver → execution → shapley → aggregator → Groth16)
- `ShapleyBreakdown` — chart of `EpochSettled.shapley` percentages

The WS schema in `prism-types::WsEvent` is the contract. Round-trip tested — won't drift.

### 🆕 N3 — `FEEDBACK.md` missing

Required Uniswap-sponsor-track deliverable per v2 §16.3. Should describe **real** pain points: hook-address-mining flags, dynamic-fee integration patterns, external verifier wiring, V4 pool init quirks. Cannot be authentic until Dev 2 closes C1 (V4 deploy works); write it after you've used the deployed hook for at least one real demo run.

### 🆕 N5 / N6 — keccak fallback path + dead `eth_account` import

`commitment.py` imports both pycryptodome and (presumably) a fallback. Once N1 is fixed and tests run, run the parity test against my new `print_test_vector` example for **all 10 Action variants** (current coverage is 3/10 — the audit's M9). Mismatches mean someone's encoding drifted; same encoding must hold across Rust ↔ Python ↔ SP1 or the proof rejects the agent's intent.

`from eth_account import Account` in `wallets.py` is dead today; becomes load-bearing once C5 lands.

---

## Coordination notes

- **WS event schema is locked.** I won't change `WsEvent` shape without versioning. If a future change requires a new field, I'll add it as `Option<>` first to keep your client decoder backwards-compatible. Any breaking change → I'll Slack first.
- **Wire intent shape is locked.** `AgentIntentWire` round-trip tests cover all 10 Action variants (already passing in `prism-types` test suite). Don't change the JSON tag from PascalCase or the keccak parity will break.
- **Test vectors are easy to regenerate.** Run `cargo run --example print_test_vector -p prism-types` (added Phase 1, commit `8ece670`) — outputs golden hex strings for the canonical Action variants. Pin them in your Python tests.
- **Inbound intents now have a one-epoch grace window.** As of Phase 2 commit `7230ae2`, agents can broadcast `intent.epoch = current_epoch + 1` slightly ahead of the tick — the orchestrator buffers and processes it on the next tick. This means your broadcaster doesn't need second-precise sync with the orchestrator clock.
- **I will need from you, when ready:**
  - Confirmation that the Python swarm is calling `commitIntent` on-chain before reveal (C5 closed)
  - The 5 agent EOAs (with funded balances on Unichain Sepolia) — coordinate with Dev 2 on registration
  - Frontend deploy URL or local serve instructions for demo rehearsal

---

## Quick sanity-check checklist before you merge

- [ ] `requirements.txt` exists; `pip install -r requirements.txt && pytest agent-brains/tests/` is green
- [ ] β's brain emits a capability-aligned action in the opportunity epoch (or β is granted `canLP` and the deploy script reflects that — coordinate with Dev 2)
- [ ] `chain_client.py` calls `PrismHook.commitIntent` and waits for confirmation before WS reveal
- [ ] `run_swarm.py --live` exits non-zero on connection failure
- [ ] `OnChainMarketReader` uses V4 selectors (`PoolManager.getSlot0`, key-derived fee)
- [ ] `volatility_30d_bps` reads from a real source (oracle or rolling stddev)
- [ ] Broadcaster reconnects mid-session + retries failed sends
- [ ] All 10 Action variants have keccak commitment parity vectors against my `print_test_vector` output
- [ ] Frontend renders all 11 `WsEvent` variants
- [ ] `FEEDBACK.md` exists, written from a real V4 hook deploy experience

When you're ready to merge, the order is: `contracts` → `rust-zk-integration` → `agents-brain`. Ping me when your branch is feature-complete and I'll do the cross-branch integration test on a local Anvil before main hits.

— Dev 1
