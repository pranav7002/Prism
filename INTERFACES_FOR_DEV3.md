# Interfaces for Dev 3

> **Audience:** Dev 3 (Python agent swarm).
> **Author:** Dev 1 (Rust + SP1).
> **As of:** 2026-04-28, branch `rust-zk-integration` @ commit `5ee71c9`.
> **Read alongside:** `DEV3_HANDOFF.md` (broader scope), `walkthrough.md` (architecture).

This file is a contract reference — what Python ↔ Rust ↔ Solidity exchange and how. **Pinned shapes.** If anything in this document needs to change, I'll Slack you first.

---

## TL;DR

You have three integration points with the Rust orchestrator and the Solidity hook:

1. **WS reveal** — `SubmitIntent` JSON message over WebSocket (already wired)
2. **On-chain commit** — `PrismHook.commitIntent(bytes32)` from each agent's EOA (NOT yet wired — closes C5)
3. **Commitment derivation** — keccak256 over canonical packed bytes; must be byte-identical with my Rust side

---

## 1. WebSocket — `SubmitIntent`

**Endpoint:** `ws://<host>:8765` (configurable via `WS_BIND_ADDR`)

### Message shape (you → orchestrator)

```json
{
  "type": "SubmitIntent",
  "intent": {
    "agent_id": "0x<20-byte hex>",
    "epoch": 5,
    "target_protocol": "Uniswap",
    "action": { "type": "Swap", "...": "..." },
    "priority": 80,
    "max_slippage_bps": 50,
    "expected_profit_bps": 30,
    "salt": "0x<32-byte hex>"
  },
  "commitment": "0x<32-byte hex>"
}
```

The `commitment` field is **required** as of audit fix C7 (commit `8cbfa82`). The orchestrator parses it, recomputes the commitment from the wire `intent` fields, and **rejects** the message if they don't match. Missing or malformed → reject with `warn!` log.

### Action discriminators (from `prism-types::Action`)

| Variant | JSON `type` |
|---|---|
| Swap | `"Swap"` |
| AddLiquidity | `"AddLiquidity"` |
| RemoveLiquidity | `"RemoveLiquidity"` |
| Backrun | `"Backrun"` |
| DeltaHedge | `"DeltaHedge"` |
| KillSwitch | `"KillSwitch"` |
| MigrateLiquidity | `"MigrateLiquidity"` |
| BatchConsolidate | `"BatchConsolidate"` |
| SetDynamicFee | `"SetDynamicFee"` |
| CrossProtocolHedge | `"CrossProtocolHedge"` |

Tag is PascalCase. Action-specific fields are siblings of `type`.

### Epoch window

As of commit `7230ae2` the orchestrator accepts `intent.epoch ∈ {current, current + 1}`. You can broadcast slightly ahead of the orchestrator's tick — current-epoch intents go to the solver immediately; next-epoch intents are buffered and merged into the next tick's intent set. Out-of-window epochs (≥ current+2 or ≤ current−1) still warn+drop.

### Outbound events (orchestrator → you, for the frontend)

`prism-types::WsEvent` enum, JSON-tagged by variant:

```rust
EpochStart { epoch: u64, timestamp: u64 }
IntentsReceived { count: u32, agents: Vec<String> }
SolverRunning { conflicts_detected: u32 }
SolverComplete { plan_hash: String, dropped: Vec<String> }
ProofProgress { program: String, pct: u8 }
ProofComplete { program: String, time_ms: u64 }
AggregationStart
AggregationComplete { time_ms: u64 }
Groth16Wrapping { pct: u8 }
EpochSettled { tx_hash: String, gas_used: u64, shapley: Vec<u16> }
Error { message: String }
```

Round-trip tested in `prism-types::tests::ws_event_*`. 8 golden vectors. **This shape is locked.**

---

## 2. On-chain `commitIntent` (Solidity-side contract)

**This is the integration that closes C5.**

### Contract method

```solidity
function commitIntent(bytes32 commitment) external onlyRegistered;
```

- **Address:** Dev 2 will publish the deployed `PrismHook` address. Read it from env var `PRISM_HOOK_ADDRESS` (matches the orchestrator's settlement.rs convention).
- **Selector:** `0x{first 4 bytes of keccak256("commitIntent(bytes32)")}`. You can compute it once and hardcode, or derive at startup via web3.py's `Contract.functions.commitIntent(...).selector`.
- **Caller:** must be a registered agent (the contract checks `registeredAgents[msg.sender]`). Each of the 5 agents has a dedicated EOA — see `agent-brains/common/wallets.py` for the addresses; private keys must come from env (one per agent), never committed.
- **Gas budget:** ~50k. Set gas limit to 100k for headroom.
- **Reverts:**
  - `NotRegisteredAgent` (custom error, selector `0x{first 4 bytes of keccak256("NotRegisteredAgent()")}`) — caller's address isn't in the registry. Treat as fatal config error; don't retry.

### Sequencing

```
        ┌─────────────────────────────────┐
        │  agent.compute_commitment()     │
        │  → keccak256 over canonical     │
        │    packed intent bytes          │
        └──────────────┬──────────────────┘
                       │
                       ▼
        ┌─────────────────────────────────┐   ❶ on-chain commit
        │  hook.commitIntent(commitment)  │  ─────────────────►
        │  wait for receipt.status == 1   │   ~50k gas, ~12s
        └──────────────┬──────────────────┘
                       │
                       ▼ (only after on-chain confirmation)
        ┌─────────────────────────────────┐   ❷ WS reveal
        │  ws.send(SubmitIntent {         │  ─────────────────►
        │    intent, commitment           │   orchestrator
        │  })                             │   recomputes,
        └─────────────────────────────────┘   verifies match
```

### Implementation sketch (Python)

```python
# agent-brains/common/chain_client.py (new file)
from typing import Awaitable
from web3 import AsyncWeb3
from web3.providers.async_rpc import AsyncHTTPProvider
from eth_account.signers.local import LocalAccount

class ChainClient:
    """Calls PrismHook.commitIntent on Unichain Sepolia for one agent."""

    # Pre-computed once: keccak256("commitIntent(bytes32)")[:4].hex()
    _COMMIT_INTENT_SELECTOR = "0x..."  # fill in at startup

    def __init__(self, rpc_url: str, hook_address: str, account: LocalAccount):
        self.w3 = AsyncWeb3(AsyncHTTPProvider(rpc_url))
        self.hook = self.w3.eth.contract(address=hook_address, abi=self._abi())
        self.account = account

    async def commit_intent(self, commitment: bytes) -> str:
        """
        Submit commitIntent(bytes32) on-chain. Returns tx hash.
        Raises if the tx reverts or doesn't confirm within timeout.
        """
        assert len(commitment) == 32, "commitment must be 32 bytes"
        nonce = await self.w3.eth.get_transaction_count(self.account.address)
        tx = await self.hook.functions.commitIntent(commitment).build_transaction({
            "from": self.account.address,
            "nonce": nonce,
            "gas": 100_000,
            # EIP-1559 — let web3.py auto-fill maxFeePerGas/maxPriorityFeePerGas
        })
        signed = self.account.sign_transaction(tx)
        tx_hash = await self.w3.eth.send_raw_transaction(signed.rawTransaction)
        receipt = await self.w3.eth.wait_for_transaction_receipt(tx_hash, timeout=30)
        if receipt.status != 1:
            raise RuntimeError(f"commitIntent reverted: {tx_hash.hex()}")
        return tx_hash.hex()
```

Plumb into `broadcaster.send_intent` so the on-chain call **must succeed before** the WS reveal is sent. If the on-chain call fails or times out, treat it as fatal and surface the error.

### Funding

Each of the 5 agent EOAs needs a small balance on Unichain Sepolia for gas — Dev 2 will fund them as part of the deploy script (or coordinate via faucet). Coordinate the addresses:

```
α  0xf2E96F75a19443c17E88f2cd8e85a188A37D1EFF
β  0x9E8C1Bc1D077Cb1aBb60FAa3CB80491e217FBC59
γ  0xd01F4f010DcB7C878B807B0273A8e3bAA1D1f22D
δ  0x0bfF21FB77Fc98068b02B9821Cc2E8306c55F459
ε  0x932aE7e2CA55Ff664699fD4936Ae61AeC487BAB5
```

These are the same addresses the deploy script registers and that `agent-brains/common/wallets.py` already references. **Do not change them** without coordinating with Dev 2 (they're hardcoded into the deploy script).

---

## 3. Commitment derivation — keccak parity

Your Python `commitment.py` and my Rust `prism-types::AgentIntent::compute_commitment` must produce **byte-identical** output. The encoding is keccak256 over canonical packed bytes:

```
agent_id      [20 bytes]
epoch         [u64 BE → 8 bytes]
target_proto  [u32 BE length, then UTF-8 bytes]
action        [variant-specific packed bytes — see below]
priority      [1 byte]
max_slippage  [u16 BE → 2 bytes]
salt          [32 bytes]
```

Hash with keccak256 → 32 bytes.

### Action discriminators (one byte, prepend to action body)

| Variant | Byte |
|---|---|
| Swap | `0x01` |
| AddLiquidity | `0x02` |
| RemoveLiquidity | `0x03` |
| Backrun | `0x04` |
| DeltaHedge | `0x05` |
| KillSwitch | `0xFF` |
| MigrateLiquidity | `0x06` |
| BatchConsolidate | `0x07` |
| SetDynamicFee | `0x08` |
| CrossProtocolHedge | `0x09` |

**These bytes are stable. Do not reorder.** The exact same table appears in the SP1 solver-proof zkVM program; any drift causes proof rejection.

For the per-action body layout, the source of truth is `prism-types::Action::encode_packed` in `crates/prism-types/src/lib.rs`. Round-trip tests in `prism-types::tests::*_roundtrips` cover all 10 variants.

### Generating golden vectors

I added an example for this in commit `8ece670`:

```bash
cargo run --example print_test_vector -p prism-types
```

Output:
```
SWAP_COMMITMENT=0xf24824f303950f96e2be944f499483d2f81cb6926e6d3b058018c15059f8eafc
ADD_LIQ_COMMITMENT=0x6d2d8844f4ecbee63a49ad89dd88c67fda335db05d1b4e98dcf17968f9019a9c
KILLSWITCH_COMMITMENT=0x34893be3504733528a0777e503c98e272a871a4a4897e9adcb3d45a2da7f3359
```

These three are pinned in `print_test_vector.rs` (deterministic inputs, no randomness). Pin them in your Python parity tests and you'll catch encoding drift before it hits the SP1 program.

**Coverage gap (M9 in Audit report):** the printer currently emits 3 of 10 variants. Adding the other 7 is a one-line patch per variant — each follows the same `AgentIntent::new_with_commitment(...)` shape with deterministic inputs. If you want to do this yourself rather than asking me, the file is `crates/prism-types/examples/print_test_vector.rs` and it's already declared as an example in `prism-types/Cargo.toml`.

---

## Sequence diagram for one full epoch

```
   Python (per agent)              Solidity (PrismHook)             Rust (orchestrator)
   ───────────────────              ─────────────────────             ──────────────────
        │
        │ compute_commitment(intent)
        │ → 32 bytes
        │
        │  ❶ commitIntent(commitment)
        ├───────────────────────────────►
        │                                  store in commitments[epoch][agent]
        │                                  emit IntentCommitted
        │ ◄──────────────────────────── tx receipt status=1
        │
        │  ❷ WS SubmitIntent { intent, commitment }
        ├──────────────────────────────────────────────────────────────────►
        │                                                                       parse, recompute
        │                                                                       commitment, verify match
        │                                                                       — if mismatch: reject
        │                                                                       — else: queue for solver
        │
        │                                                                       (per epoch tick)
        │                                                                       run solver → SP1 proofs
        │                                                                       → settleEpoch(proof, pv)
        │                                  verify ZK proof
        │                                  decode (epoch, payouts[])
        │                                  store payouts, advance epoch
        │                                  ◄────────────── tx receipt
        │
        │ ◄──────────────────────── EpochSettled { tx_hash, gas_used, shapley }
        │   (over WS)
```

---

## Locked vs negotiable

| Surface | Locked? | Notes |
|---|---|---|
| `WsEvent` JSON shape | ✅ locked | Round-trip tested. Breaking change → I bump a version field. |
| `AgentIntentWire` JSON shape | ✅ locked | Same. |
| Action discriminator bytes | ✅ locked | Drift causes proof rejection. |
| Commitment encoding | ✅ locked | Drift causes proof rejection. |
| `PrismHook.commitIntent` selector | ✅ locked (Solidity ABI) | `bytes32 commitment` only. |
| 5 agent EOA addresses | ✅ locked | Hardcoded in deploy script. |
| `PRISM_HOOK_ADDRESS` env var name | 🟡 negotiable | We can change if it conflicts; coordinate with Dev 2. |
| Inbound WS message envelope (`type`, `intent`, `commitment`) | 🟡 minor | Adding sibling fields is fine; renaming any of the three is breaking. |

---

If anything's unclear, the source of truth is always the code:
- `crates/prism-types/src/lib.rs` — `WsEvent`, `AgentIntent`, `AgentIntentWire`, `Action`, commitment encoding
- `crates/prism-orchestrator/src/main.rs` — WS server, intent ingestion, commit verification
- (Dev 2) `contracts/src/PrismHook.sol` — `commitIntent`, `settleEpoch`

— Dev 1
