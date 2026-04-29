// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

import "forge-std/Test.sol";
import {PrismHook} from "../src/PrismHook.sol";
import {MockSP1Verifier} from "../src/MockSP1Verifier.sol";
import {MockAave} from "../src/MockAave.sol";
import {IPoolManager} from "v4-core/src/interfaces/IPoolManager.sol";

/// @title PrismHook Test Suite
/// @notice Unit + fuzz tests for PrismHook, MockAave, and MockSP1Verifier.
///         Uses address(1) as a stand-in PoolManager (hook callbacks are tested
///         via direct calls from address(1) as msg.sender).
contract PrismHookTest is Test {
    PrismHook public hook;
    MockSP1Verifier public verifier;
    MockAave public aave;

    address owner = address(this);
    address alpha = address(0xf2E96F75a19443c17E88f2cd8e85a188A37D1EFF);
    address beta = address(0x9E8C1Bc1D077Cb1aBb60FAa3CB80491e217FBC59);
    address gamma = address(0xd01F4f010DcB7C878B807B0273A8e3bAA1D1f22D);
    address delta = address(0x0bfF21FB77Fc98068b02B9821Cc2E8306c55F459);
    address epsilon = address(0x932aE7e2CA55Ff664699fD4936Ae61AeC487BAB5);

    // We use address(1) as a dummy PoolManager.
    address constant POOL_MANAGER = address(1);

    bytes32 constant MOCK_VKEY = bytes32(uint256(0xCAFE));

    function setUp() public {
        verifier = new MockSP1Verifier();
        aave = new MockAave();
        hook = new PrismHook(
            IPoolManager(POOL_MANAGER),
            verifier,
            MOCK_VKEY,
            address(this)
        );

        // Register 5 agents with appropriate capabilities.
        hook.registerAgent(
            alpha,
            PrismHook.AgentCapabilities({
                canLP: true,
                canSwap: false,
                canBackrun: false,
                canHedge: false,
                canSetFee: false,
                canKillSwitch: false
            })
        );
        hook.registerAgent(
            beta,
            PrismHook.AgentCapabilities({
                canLP: false,
                canSwap: false,
                canBackrun: false,
                canHedge: false,
                canSetFee: true,
                canKillSwitch: false
            })
        );
        hook.registerAgent(
            gamma,
            PrismHook.AgentCapabilities({
                canLP: true,
                canSwap: false,
                canBackrun: false,
                canHedge: false,
                canSetFee: false,
                canKillSwitch: false
            })
        );
        hook.registerAgent(
            delta,
            PrismHook.AgentCapabilities({
                canLP: false,
                canSwap: true,
                canBackrun: true,
                canHedge: false,
                canSetFee: false,
                canKillSwitch: false
            })
        );
        hook.registerAgent(
            epsilon,
            PrismHook.AgentCapabilities({
                canLP: false,
                canSwap: false,
                canBackrun: false,
                canHedge: true,
                canSetFee: false,
                canKillSwitch: true
            })
        );
    }

    // ═══════════════════════════════════════════════════════════════
    //  Agent Registration
    // ═══════════════════════════════════════════════════════════════

    function test_agentCount() public view {
        assertEq(hook.agentCount(), 5);
    }

    function test_registerAgent_onlyOwner() public {
        address rando = address(0xBEEF);
        vm.prank(rando);
        vm.expectRevert(PrismHook.NotAuthorized.selector);
        hook.registerAgent(
            address(0xDEAD),
            PrismHook.AgentCapabilities({
                canLP: true,
                canSwap: false,
                canBackrun: false,
                canHedge: false,
                canSetFee: false,
                canKillSwitch: false
            })
        );
    }

    function test_registerAgent_noDuplicate() public {
        vm.expectRevert(PrismHook.AgentAlreadyRegistered.selector);
        hook.registerAgent(
            alpha,
            PrismHook.AgentCapabilities({
                canLP: true,
                canSwap: false,
                canBackrun: false,
                canHedge: false,
                canSetFee: false,
                canKillSwitch: false
            })
        );
    }

    // ═══════════════════════════════════════════════════════════════
    //  commitIntent
    // ═══════════════════════════════════════════════════════════════

    function test_commitIntent() public {
        bytes32 commitment = keccak256("test_intent");
        vm.prank(alpha);
        hook.commitIntent(commitment);

        assertEq(hook.commitments(1, alpha), commitment);
    }

    function test_commitIntent_notRegistered() public {
        vm.prank(address(0xDEAD));
        vm.expectRevert(PrismHook.NotRegisteredAgent.selector);
        hook.commitIntent(bytes32(0));
    }

    // ═══════════════════════════════════════════════════════════════
    //  setDynamicFee
    // ═══════════════════════════════════════════════════════════════

    function test_setDynamicFee() public {
        vm.prank(beta);
        hook.setDynamicFee(6000); // 0.60%
        assertEq(hook.currentDynamicFee(), 6000);
    }

    function test_setDynamicFee_unauthorized() public {
        vm.prank(alpha); // α doesn't have canSetFee
        vm.expectRevert(PrismHook.NotAuthorized.selector);
        hook.setDynamicFee(6000);
    }

    // ═══════════════════════════════════════════════════════════════
    //  triggerKillSwitch
    // ═══════════════════════════════════════════════════════════════

    function test_triggerKillSwitch() public {
        vm.prank(epsilon);
        hook.triggerKillSwitch();
        assertTrue(hook.killSwitchActive());
    }

    function test_triggerKillSwitch_unauthorized() public {
        vm.prank(delta); // δ doesn't have canKillSwitch
        vm.expectRevert(PrismHook.NotAuthorized.selector);
        hook.triggerKillSwitch();
    }

    // ═══════════════════════════════════════════════════════════════
    //  settleEpoch
    // ═══════════════════════════════════════════════════════════════

    function test_settleEpoch() public {
        uint16[] memory payouts = new uint16[](5);
        payouts[0] = 4000; // α
        payouts[1] = 2500; // β
        payouts[2] = 2000; // γ
        payouts[3] = 1500; // δ
        payouts[4] = 0; // ε

        bytes memory publicValues = abi.encode(uint256(1), payouts);
        bytes memory proof = hex"CAFE";

        hook.settleEpoch(proof, publicValues);

        assertEq(hook.currentEpoch(), 2);
        assertFalse(hook.killSwitchActive());

        uint16[] memory stored = hook.getPayouts(1);
        assertEq(stored.length, 5);
        assertEq(stored[0], 4000);
        assertEq(stored[4], 0);
    }

    function test_settleEpoch_wrongEpoch() public {
        uint16[] memory payouts = new uint16[](5);
        payouts[0] = 10000;
        // epoch 99 doesn't match currentEpoch (1)
        bytes memory publicValues = abi.encode(uint256(99), payouts);
        bytes memory proof = hex"CAFE";

        vm.expectRevert(PrismHook.EpochMismatch.selector);
        hook.settleEpoch(proof, publicValues);
    }

    function test_settleEpoch_payoutSumMismatch() public {
        uint16[] memory payouts = new uint16[](5);
        payouts[0] = 5000;
        payouts[1] = 4999; // sum = 9999, not 10000

        bytes memory publicValues = abi.encode(uint256(1), payouts);
        bytes memory proof = hex"CAFE";

        vm.expectRevert(PrismHook.PayoutSumMismatch.selector);
        hook.settleEpoch(proof, publicValues);
    }

    function test_settleEpoch_clearsKillSwitch() public {
        vm.prank(epsilon);
        hook.triggerKillSwitch();
        assertTrue(hook.killSwitchActive());

        // Settle clears kill-switch.
        uint16[] memory payouts = new uint16[](5);
        payouts[0] = 10000;
        bytes memory publicValues = abi.encode(uint256(1), payouts);
        hook.settleEpoch(hex"CAFE", publicValues);

        assertFalse(hook.killSwitchActive());
    }

    // ═══════════════════════════════════════════════════════════════
    //  Fuzz: payout sum must be exactly 10000
    // ═══════════════════════════════════════════════════════════════

    function testFuzz_settleEpoch_payoutSum(
        uint16 a,
        uint16 b,
        uint16 c,
        uint16 d
    ) public {
        // Bound to prevent sum > 10000 overflow.
        a = uint16(bound(a, 0, 10000));
        b = uint16(bound(b, 0, 10000 - a));
        c = uint16(bound(c, 0, 10000 - a - b));
        d = uint16(bound(d, 0, 10000 - a - b - c));
        uint16 e = 10000 - a - b - c - d;

        uint16[] memory payouts = new uint16[](5);
        payouts[0] = a;
        payouts[1] = b;
        payouts[2] = c;
        payouts[3] = d;
        payouts[4] = e;

        bytes memory publicValues = abi.encode(uint256(1), payouts);
        hook.settleEpoch(hex"CAFE", publicValues);

        assertEq(hook.currentEpoch(), 2);
        uint16[] memory stored = hook.getPayouts(1);
        uint256 sum = 0;
        for (uint256 i = 0; i < stored.length; i++) {
            sum += stored[i];
        }
        assertEq(sum, 10000);
    }

    // ═══════════════════════════════════════════════════════════════
    //  MockAave
    // ═══════════════════════════════════════════════════════════════

    function test_mockAave_borrowRepay() public {
        address asset = address(0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2); // WETH
        vm.startPrank(epsilon);
        aave.borrow(asset, 1 ether);
        assertEq(aave.getDebt(epsilon, asset), 1 ether);

        aave.repay(asset, 0.5 ether);
        assertEq(aave.getDebt(epsilon, asset), 0.5 ether);

        aave.repay(asset, 999 ether); // overpay capped
        assertEq(aave.getDebt(epsilon, asset), 0);
        vm.stopPrank();
    }

    // ═══════════════════════════════════════════════════════════════
    //  Full epoch cycle
    // ═══════════════════════════════════════════════════════════════

    function test_fullEpochCycle() public {
        // 1. Agents commit.
        vm.prank(alpha);
        hook.commitIntent(keccak256("alpha_intent"));
        vm.prank(beta);
        hook.commitIntent(keccak256("beta_intent"));
        vm.prank(gamma);
        hook.commitIntent(keccak256("gamma_intent"));
        vm.prank(delta);
        hook.commitIntent(keccak256("delta_intent"));
        vm.prank(epsilon);
        hook.commitIntent(keccak256("epsilon_intent"));

        // 2. β adjusts fee.
        vm.prank(beta);
        hook.setDynamicFee(6000);

        // 3. Settle epoch.
        uint16[] memory payouts = new uint16[](5);
        payouts[0] = 3500;
        payouts[1] = 2000;
        payouts[2] = 2000;
        payouts[3] = 1500;
        payouts[4] = 1000;

        bytes memory publicValues = abi.encode(uint256(1), payouts);
        hook.settleEpoch(hex"CAFE", publicValues);

        // 4. Verify state advanced.
        assertEq(hook.currentEpoch(), 2);

        uint16[] memory stored = hook.getPayouts(1);
        assertEq(stored[0], 3500);
        assertEq(stored[4], 1000);
    }
}
