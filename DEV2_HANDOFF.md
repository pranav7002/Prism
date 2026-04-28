# Dev 2 — Handoff (Solidity / Unichain)

> **Audience:** Dev 2 (PrismHook + Unichain Sepolia deploy).
> **Author:** Dev 1 (Rust + SP1).
> **As of:** 2026-04-28, branch `rust-zk-integration` @ commit `7a94b12`.
> **Read alongside:** `walkthrough.md` (architecture overview), `AGGREGATOR_VKEY.txt` (current vkey).

This file is self-contained. You should not need to ping me to start work.

---

## TL;DR — what the contract needs and why

1. **PrismHook is currently undeployable on V4.** Missing `getHookPermissions()`. Vanilla `new PrismHook(...)` deploy. First `PoolManager.initialize` call on Unichain Sepolia will revert with `Hooks.HookAddressNotValid`. **This is the demo blocker.** (C1 in the audit.)
2. **`AGGREGATOR_VKEY` is stable** at `0x0071dbaffa0632707d8274ee31d554d9d233f8373c49eb4fb7970607e2c39a52`. Phase 2's SP1 fixes (M4, M5, H5) did not rotate it because the aggregator program reads sub-program vkeys via stdin rather than baking them in. **You do not need to redeploy** to pick up Phase 2 of the audit fixes.
3. **`settleEpoch(bytes proof, bytes publicValues)`** consumes `publicValues = abi.encode(uint256 epoch, uint16[] payouts)` — exactly what's emitted by the aggregator program after C2's fix. Already wired on the contract side (`PrismHook.sol:163-194`); leave it alone.
4. **Several high-severity contract issues are still open** — the audit calls them out, and the demo will technically work without fixing them, but production won't. See "Audit findings in your scope" below.

---

## Current state of `contracts/` (your branch)

This branch (`rust-zk-integration`) does **not** contain `contracts/`. Your work lives on the `contracts` branch. Last known state of your branch from when I checked the remote: `6f0e548` on `origin/contracts` ahead of `ac8f8c9`. Pull and continue from there.

The `prism-vihaan` audit-snapshot repo has a copy of the contracts that I audited — refer to `/home/pratham/Sarnav/Prism/prism-vihaan/contracts/` for the full audited tree. **Do not edit prism-vihaan** — it's a snapshot, not the source of truth.

---

## What you need from me (the Rust/SP1 side) — already on this branch

| Artifact | Where | Status |
|---|---|---|
| `AGGREGATOR_VKEY` (bytes32) | `AGGREGATOR_VKEY.txt`, root | ✅ stable — `0x0071dba…39a52` |
| `publicValues` ABI shape | `crates/prism-orchestrator/src/proving.rs:691-727` (`encode_public_values_abi`); also matching encoder inside `sp1-programs/aggregator/src/main.rs:34-61` | ✅ both byte-identical, golden-vector tested in Foundry's `AbiCompatibility.t.sol` |
| Commitment encoding (for `commitIntent`) | `crates/prism-types/src/lib.rs::AgentIntent::compute_commitment` (keccak256 over canonical packed bytes) | ✅ |
| Test-vector printer for cross-language parity | `cargo run --example print_test_vector -p prism-types` | ✅ added in commit `8ece670` |
| ELF reproducibility | All four SP1 ELFs gitignored under `sp1-programs/<name>/elf/`. Lockfiles for SP1 programs committed (commit `7a94b12`). | 🟡 you can rebuild via `cargo prove build` in each SP1 dir |

---

## Audit findings in YOUR scope — ordered by demo-criticality

Numbering matches `/home/pratham/Sarnav/Prism/prism-vihaan/AUDIT_REPORT_2026-04-28.md`.

### 🔴 C1 — PrismHook is undeployable on V4

V4 hooks must encode their callback set in the lower 14 bits of the contract's address. The contract today:

- has no `getHookPermissions() returns (Hooks.Permissions memory)` (grep across `contracts/`: zero hits)
- does not inherit `BaseHook`
- is deployed via `new PrismHook(...)` — vanilla CREATE, no `HookMiner.find(...)` salt loop in `script/DeployPrismHook.s.sol`

Foundry tests pass only because they pass `address(1)` as the PoolManager. The first `PoolManager.initialize` call on Unichain Sepolia reverts with `Hooks.HookAddressNotValid`.

**Fix:**

1. Add this method (matches v2 §7.1 exactly):

   ```solidity
   function getHookPermissions() public pure virtual returns (Hooks.Permissions memory) {
       return Hooks.Permissions({
           beforeInitialize: false,
           afterInitialize: false,
           beforeAddLiquidity: true,
           afterAddLiquidity: true,
           beforeRemoveLiquidity: true,
           afterRemoveLiquidity: true,
           beforeSwap: true,
           afterSwap: true,
           beforeDonate: false,
           afterDonate: false,
           beforeSwapReturnDelta: false,
           afterSwapReturnDelta: false,
           afterAddLiquidityReturnDelta: false,
           afterRemoveLiquidityReturnDelta: false
       });
   }
   ```

2. Either (a) inherit from `v4-periphery/BaseHook` (it validates the address-to-permissions match in its constructor — easiest), OR (b) keep the inheritance as-is and add `HookMiner.find(deployer, flags, creationCode, args)` + `new PrismHook{salt: minedSalt}(...)` in `DeployPrismHook.s.sol`. The v4-template uses the HookMiner approach — recommend matching that.

### 🟠 H8 — `setDynamicFee` is unbounded; β can DoS the pool

`PrismHook.sol:142` accepts `uint24` (max 16,777,215). V4's `LPFeeLibrary.MAX_LP_FEE = 1_000_000`. A β intent that sets fee above that makes every swap revert in fee validation until ε's kill-switch fires + manual settlement.

**Fix:** `require(newFee <= LPFeeLibrary.MAX_LP_FEE);` at the top of `setDynamicFee`.

### 🟠 H9 — Capability gating is asymmetric

`beforeAddLiquidity` checks only `if (registeredAgents[sender])`. Unregistered addresses can LP freely; registered agents lacking `canLP` (e.g., δ) can also LP. Setting capabilities at registration is therefore advisory-only.

**Fix:** in every restricted callback, require **both** `registeredAgents[sender]` **and** the matching capability flag. Sketch:

```solidity
function beforeAddLiquidity(address sender, ...) external {
    if (!registeredAgents[sender]) revert NotRegisteredAgent();
    if (!agentCaps[sender].canLP) revert NotAuthorized();
    if (commitments[currentEpoch][sender] == bytes32(0)) revert NoCommitmentThisEpoch();
    ...
}
```

Same pattern for `beforeSwap` (`canSwap`) — but careful: `beforeSwap` runs for *every* user swap, not just agent swaps; you want the cap-check only when `sender` is one of the registered agents.

### 🟠 H10 — Owner has no transfer / renounce path; kill-switch has no override

`owner` is set in the constructor and never written. Lost key = permanently frozen agent registry. `triggerKillSwitch` is cleared only by `settleEpoch`; if the orchestrator never produces a settling proof, the pool stays bricked.

**Fix:** add OZ Ownable's `transferOwnership` + an owner-only `clearKillSwitch()`.

### 🟠 H17 — `settleEpoch` is permissionless / front-runnable

Anyone holding a valid proof + matching public values can settle. Replay across epochs is blocked (monotonic counter), but a mempool watcher can copy the orchestrator's tx and settle first; orchestrator's tx then reverts. Harmless today (no token flows) but a real-money version needs an operator allowlist or commit-then-settle.

**Fix (optional for demo, required for production):** add an `OPERATOR_ROLE`-style check, or accept the front-running and emit a `settledBy` indexed event for tracking.

### 🟡 M6 — No invariant ties `payouts.length == agentList.length` in `settleEpoch`

Test at `PrismHook.t.sol:233` settles with `[10000]` against 5 registered agents — payouts for the other 4 are silently dropped. Production-blocking once token flows land.

**Fix:** `require(payouts.length == agentList.length)` in `settleEpoch`.

### 🟡 M10 — `MockSP1Verifier` has no chainid guard

Could be deployed to a real chain by accident. Add `require(block.chainid == 31337)` or rename to `LocalOnlyMockSP1Verifier` with a constructor revert on non-anvil chains.

---

## Future ask: Plan-B 3-proof signature (v2 §7.3)

Coming in Phase 3 of my plan. **Will require a contract-side change** so flagging now:

If the SP1 aggregator can't prove recursion of the three base proofs in time for the demo, my orchestrator will fall back to producing three separate Groth16 proofs. I'll need a second settle entry-point on the hook:

```solidity
function settleEpochThreeProof(
    bytes calldata proofA, bytes calldata pvA,
    bytes calldata proofB, bytes calldata pvB,
    bytes calldata proofC, bytes calldata pvC
) external;
```

Internally: verify each proof against its respective sub-program vkey (you'll need 3 immutable vkeys instead of just `AGGREGATOR_VKEY`), then assert epoch + cross-consistency manually. Gas ~780k vs ~27k for the recursive path — only used if the recursive path fails the W4 gate.

I'll send sub-program vkeys (`SOLVER_VKEY`, `EXECUTION_VKEY`, `SHAPLEY_VKEY`) when I push that change. **Don't build this yet** — I want to confirm the recursive aggregator works end-to-end first. Just keep the design space in mind so the entry-point is easy to add when needed.

---

## Coordination notes

- **No vkey rotation expected from me through Phase 2.** If a future commit on this branch *does* rotate the vkey, I'll Slack you the new value before pushing. Look for commit subjects starting with `chore: AGGREGATOR_VKEY rotated…`.
- **Capability schema is locked.** Don't change `AgentCapabilities` struct field order — the deploy script (`DeployPrismHook.s.sol`) and Dev 3's wallet-binding both depend on the current shape.
- **`agentList` order matters.** It defines the index space for `payouts`. Don't re-sort it; rely on registration order. Document this in a comment if you haven't already.
- **I will need from you, when ready:**
  - The Unichain Sepolia deploy address of `PrismHook` (set as `PRISM_HOOK_ADDRESS` env var)
  - The verifier address — `SP1_GATEWAY_ADDRESS` for production, `MockSP1Verifier` address for tests
- **Run cross-language ABI compat test** after any change to `settleEpoch`'s decode shape: `forge test --match-contract AbiCompatibility`. Three tests cover the byte-equality with my Rust `encode_public_values_abi`.

---

## Quick sanity-check checklist before you merge

- [ ] `getHookPermissions()` returns the v2 §7.1 set
- [ ] HookMiner salt loop in deploy script (or BaseHook inheritance)
- [ ] `setDynamicFee` bounded by `LPFeeLibrary.MAX_LP_FEE`
- [ ] `beforeAddLiquidity` checks `canLP`; `beforeSwap` checks `canSwap` for agents only
- [ ] `transferOwnership` + `clearKillSwitch()` exist
- [ ] `payouts.length == agentList.length` enforced in `settleEpoch`
- [ ] `MockSP1Verifier` cannot be deployed off-anvil
- [ ] `AbiCompatibility.t.sol` still passes (all 3 tests)
- [ ] `forge fmt` + `forge test` clean
- [ ] `AGGREGATOR_VKEY` constructor arg matches `AGGREGATOR_VKEY.txt` on `rust-zk-integration` at the time you deploy

When you're ready to merge, the order is: `contracts` → `rust-zk-integration` → `agents-brain`. (See "Merge-time choreography" in my Phase 1 plan note.) Ping me when your branch is feature-complete and I'll do the cross-branch integration test on a local Anvil before main hits.

— Dev 1
