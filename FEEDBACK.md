# PRISM — Uniswap V4 + SP1 DX Feedback

Real pain points encountered while building **PRISM**, a ZK-proven cooperative MEV coordination hook on Uniswap V4 / Unichain Sepolia. Submitted as part of the Uniswap Foundation sponsor track. All citations reference files in this repo so any reader can verify the claim against the source.

---

## 1. Hook-address mining requires a hand-rolled HookMiner

V4 hooks have to encode their permission set in the lower 14 bits of the deployed contract address. Without a canonical mining utility, every hook deploy reinvents one. Ours lives inline in our deploy script:

```solidity
// contracts/script/DeployPrismHook.s.sol
library HookMiner {
    function find(
        address deployer,
        uint160 flags,
        bytes memory creationCode,
        bytes memory constructorArgs
    ) internal pure returns (address, bytes32) {
        bytes memory bytecode = abi.encodePacked(creationCode, constructorArgs);
        bytes32 bytecodeHash = keccak256(bytecode);
        for (uint256 i = 0; i < type(uint256).max; i++) {
            bytes32 salt = bytes32(i);
            bytes32 hash = keccak256(
                abi.encodePacked(bytes1(0xff), deployer, salt, bytecodeHash)
            );
            if (uint160(uint256(hash)) & 0x3FFF == flags) {
                return (address(uint160(uint256(hash))), salt);
            }
        }
        revert("Salt not found");
    }
}
```

This is correct but undocumented — we worked it out from the `Hooks` library + Discord conversations. A canonical `HookMiner.sol` published in `v4-periphery` (similar in spirit to forge-std's `Test.sol`) would mean every hook deploy reuses one battle-tested implementation instead of pasting a copy.

**Suggestion:** ship a reusable `HookMiner` in `v4-periphery`. Bonus: a Foundry helper that takes a `Hooks.Permissions memory` struct and returns the address+salt directly, hiding the bit-mask plumbing.

---

## 2. Permission-bit semantics are easy to get wrong

Our first PrismHook ABI listed 10 active callbacks — `beforeInitialize`, `afterInitialize`, `beforeAddLiquidity`, `afterAddLiquidity`, `beforeRemoveLiquidity`, `afterRemoveLiquidity`, `beforeSwap`, `afterSwap`, `beforeDonate`, `afterDonate` — when our actual design only required 6 (no donate, no initialize). The hook still works because the no-op callbacks return their selectors, but the contract is invoked on every pool init and every donate, costing gas and widening the attack surface for no functional gain.

We later trimmed it to the canonical 6:

```solidity
// contracts/src/PrismHook.sol — getHookPermissions()
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
    // ...
});
```

There's no compile-time hint that the callback bodies and the permissions struct are out of sync, and there's no Foundry assertion you can drop into your test setup that says *"this hook should not be invoked on init/donate."* Without one, the over-permissive set ships silently.

**Suggestion:** a `Hooks.assertSubset(actualBitmap, expectedBitmap)` helper, or a Foundry cheatcode that captures hook callback invocations during a pool lifecycle and asserts only the declared subset fired.

---

## 3. `LPFeeLibrary.MAX_LP_FEE` is hidden behind two imports

Our β agent (Fee Curator) needs to set the dynamic fee. The cap is declared in `v4-core/src/libraries/LPFeeLibrary.sol::MAX_LP_FEE` (1_000_000 ppm) but it isn't re-exported from any convenience header. We didn't realise the cap existed until we got mysterious `ProtocolFeeLibrary.InvalidProtocolFee` reverts in test. The fix:

```solidity
// contracts/src/PrismHook.sol — setDynamicFee()
function setDynamicFee(uint24 newFee) external onlyRegistered {
    if (!agentCaps[msg.sender].canSetFee) revert NotAuthorized();
    require(newFee <= LPFeeLibrary.MAX_LP_FEE, "Fee exceeds max");  // <-- only added after debugging
    currentDynamicFee = newFee;
    emit DynamicFeeUpdated(currentEpoch, newFee);
}
```

Without that `require`, an out-of-range fee sails through `setDynamicFee` and only blows up much later inside `PoolManager.swap`, by which point the kill-switch hasn't fired and pricing is broken for an epoch.

**Suggestion:** a top-level `Hooks.sol` re-export of fee constants, OR a `LPFeeLibrary.assertValid(uint24)` helper that reverts with a descriptive message.

---

## 4. SP1VerifierGateway address discoverability on Unichain

We needed the address of Succinct's deployed `SP1VerifierGateway` on Unichain Sepolia (`0xeC95E0b24A0475b9afCAFD609b4D51D001380e75`). It is **not** on Succinct's main docs page. We found it by reading the deployment manifests in their `sp1-contracts` repo. That cost ~30 minutes; for a hackathon team without prior context it would be longer.

```bash
# Where the address actually lives
contracts/lib/sp1-contracts/contracts/deployments/<chainId>.json
```

**Suggestion:** Succinct could maintain a small `<chainId>-deployments.json` index at the root of the docs site, or expose `SP1VerifierGateway.findCanonical()` as a deterministic CREATE2-derived address known per-chain. Either way the answer to "what's the gateway on chain X" should be a single fetch.

---

## 5. CREATE2 + dynamic constructor args plumbing is brittle

To mine a hook salt you have to assemble exactly the same `creationCode + abi.encode(constructor args)` blob the EVM will see at deploy time. One off-by-one and the salt is invalid and the hook ends up at the "wrong" address with the right permission bits, so `PoolManager.initialize` reverts cryptically.

```solidity
// contracts/script/DeployPrismHook.s.sol
bytes memory creationCode = type(PrismHook).creationCode;
bytes memory constructorArgs = abi.encode(
    IPoolManager(poolManager),
    ISP1Verifier(verifierAddr),
    aggregatorVkey,
    deployer
);
(, bytes32 salt) = HookMiner.find(CREATE2_FACTORY, flags, creationCode, constructorArgs);
PrismHook hook = new PrismHook{salt: salt}(
    IPoolManager(poolManager),
    ISP1Verifier(verifierAddr),
    aggregatorVkey,
    deployer
);
```

The constructor args have to be type-by-type identical between the `abi.encode` call (mining input) and the actual `new PrismHook{salt: ...}(...)` call (deploy). Solidity gives you no help if you typo one of them — the constructor would still compile, the deploy would still succeed at a different address, and the next pool init would revert with `HookAddressNotValid`.

**Suggestion:** a typed constructor wrapper. Something like `HookDeployer.deploy<HookT>(salt, args)` where `args` is a struct literal that the compiler enforces matches the constructor signature. Or, more practically, a Foundry script template that generates the constructorArgs blob from the constructor signature using `abi.encodeCall`.

---

## 6. Permissionless `settleEpoch` is the obvious foot-gun

Our first cut of the hook accepted any caller for `settleEpoch(bytes proof, bytes publicValues)`:

```solidity
// Original, permissionless version — fixed in the deployed hook
function settleEpoch(
    bytes calldata proof,
    bytes calldata publicValues
) external {
    zkVerifier.verifyProof(AGGREGATOR_VKEY, publicValues, proof);
    (uint256 epoch, uint16[] memory payouts) = abi.decode(publicValues, (uint256, uint16[]));
    if (epoch != currentEpoch) revert EpochMismatch();
    // ...
}
```

Once a Groth16 proof is in the mempool, **any observer can replay it before our orchestrator gets included**. The proof bytes don't lie — but ordering does. We hardened this in the deployed version:

```solidity
// contracts/src/PrismHook.sol — current
function settleEpoch(
    bytes calldata proof,
    bytes calldata publicValues
) external onlyOperator nonReentrant {
    // ...
}
```

This is a known hook-design trade-off, and the right answer depends on the threat model. Worth flagging for hook authors who think *"if it's behind a ZK proof, it's safe"* — it's safe in the sense that the bytes don't lie, but the caller and the ordering still matter.

**Suggestion:** the v4-core hook docs should call this out explicitly in the "writing a hook" guide, and provide a snippet for the `OPERATOR_ROLE` pattern with `nonReentrant`.

---

## 7. ABI cross-language parity is invisible until you write a test for it

We have a Rust orchestrator that produces public values and a Solidity hook that decodes them. Both use the standard tuple ABI for `(uint256 epoch, uint16[] payouts)`. We initially trusted that "everyone speaks ABI" and shipped without a parity test. When we later wrote one, we caught a subtle bug: an off-by-one in our hand-rolled ABI encoder's dynamic-array offset. The test:

```solidity
// contracts/test/AbiCompatibility.t.sol
function test_solidity_encode_matches_rust() public pure {
    uint16[] memory payouts = new uint16[](5);
    payouts[0] = 4000; payouts[1] = 2500; payouts[2] = 2000; payouts[3] = 1500; payouts[4] = 0;
    bytes memory solidityEncoded = abi.encode(uint256(42), payouts);
    bytes memory rustEncoded = hex"...000000000000000000000000000000000000000000000000000000000000002a"
                               hex"0000000000000000000000000000000000000000000000000000000000000040"
                               // ... etc
                               ;
    assertEq(solidityEncoded, rustEncoded, "Solidity and Rust ABI encoding must match byte-for-byte");
}
```

Without this assertion the bug would have shipped — the orchestrator would have submitted bytes that decoded to nonsense payouts, and `settleEpoch`'s `sum != 10000` check would have rejected it AFTER we'd burned gas verifying the proof. Cheap test, expensive omission.

**Suggestion:** any hook template that crosses a language boundary should ship with a stub ABI-compat test like ours. Better: a Foundry cheatcode `vm.assertAbiBytesEq(expectedTuple, actualBytes)` that's first-class.

---

## Summary

V4 hooks are powerful, and SP1 + Groth16 wrap is genuinely production-ready. The pain we hit was almost entirely on the **scaffolding around them** — HookMiner, fee constants, gateway discovery, CREATE2 plumbing, role gating, ABI parity — pieces that any hook author hits in week 1. Most of it could be solved by a half-day investment in `v4-periphery` utilities + a *"first hook"* tutorial that walks through the gotchas in order.

The ZK math, the LCG determinism between the off-chain solver and the SP1 circuit, the recursive aggregation, the Groth16 wrap — none of that gave us trouble. The boring scaffolding around it is where time was lost.

— PRISM team
