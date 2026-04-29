// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

import {IHooks} from "v4-core/src/interfaces/IHooks.sol";
import {IPoolManager} from "v4-core/src/interfaces/IPoolManager.sol";
import {PoolKey} from "v4-core/src/types/PoolKey.sol";
import {BalanceDelta} from "v4-core/src/types/BalanceDelta.sol";
import {
    BeforeSwapDelta,
    BeforeSwapDeltaLibrary
} from "v4-core/src/types/BeforeSwapDelta.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {Hooks} from "v4-core/src/libraries/Hooks.sol";
import {LPFeeLibrary} from "v4-core/src/libraries/LPFeeLibrary.sol";

/// @title PrismHook
/// @notice Uniswap V4 Hook that coordinates a swarm of 5 AI agents.
///         Verifies ZK proofs (SP1 Groth16) and distributes Shapley-fair
///         payouts each epoch.
///
///         Replaces the old PrismCoordinator + PaymentSplitter + AgentRegistry.
contract PrismHook is IHooks {
    // ─── Errors ──────────────────────────────────────────────────
    error NotPoolManager();
    error NotRegisteredAgent();
    error KillSwitchActive();
    error NoCommitmentThisEpoch();
    error NotAuthorized();
    error PayoutSumMismatch();
    error EpochMismatch();
    error LengthMismatch();
    error AgentAlreadyRegistered();

    // ─── Events ──────────────────────────────────────────────────
    event AgentRegistered(address indexed agent);
    event IntentCommitted(
        uint256 indexed epoch,
        address indexed agent,
        bytes32 commitment
    );
    event DynamicFeeUpdated(uint256 indexed epoch, uint24 newFee);
    event KillSwitchTriggered(uint256 indexed epoch, address indexed agent);
    event EpochSettled(
        uint256 indexed epoch,
        uint16[] payouts,
        address indexed settledBy
    );
    event SwapTracked(uint256 indexed epoch, address indexed sender);
    event LiquidityTracked(
        uint256 indexed epoch,
        address indexed sender,
        bool isAdd
    );

    // ─── Immutables ──────────────────────────────────────────────
    IPoolManager public immutable poolManager;
    ISP1Verifier public immutable zkVerifier;
    bytes32 public immutable AGGREGATOR_VKEY;

    // ─── Agent Capabilities ──────────────────────────────────────
    struct AgentCapabilities {
        bool canLP; // α, γ
        bool canSwap; // δ
        bool canBackrun; // δ
        bool canHedge; // ε
        bool canSetFee; // β
        bool canKillSwitch; // ε
    }

    // ─── State ───────────────────────────────────────────────────
    address public owner;
    uint256 public currentEpoch;
    bool public killSwitchActive;
    uint24 public currentDynamicFee;

    mapping(address => bool) public registeredAgents;
    mapping(address => AgentCapabilities) public agentCaps;
    address[] public agentList; // ordered for payout indexing

    /// epoch → agent → commitment hash
    mapping(uint256 => mapping(address => bytes32)) public commitments;

    /// epoch → Shapley basis-point payouts (sum == 10000)
    mapping(uint256 => uint16[]) public epochPayouts;

    // ─── Modifiers ───────────────────────────────────────────────
    modifier onlyPoolManager() {
        if (msg.sender != address(poolManager)) revert NotPoolManager();
        _;
    }

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotAuthorized();
        _;
    }

    modifier onlyRegistered() {
        if (!registeredAgents[msg.sender]) revert NotRegisteredAgent();
        _;
    }

    // ─── Constructor ─────────────────────────────────────────────
    constructor(
        IPoolManager _poolManager,
        ISP1Verifier _zkVerifier,
        bytes32 _aggregatorVkey,
        address initialOwner
    ) {
        poolManager = _poolManager;
        zkVerifier = _zkVerifier;
        AGGREGATOR_VKEY = _aggregatorVkey;
        owner = initialOwner;
        currentEpoch = 1;
        currentDynamicFee = 3000; // 0.30% default
    }

    // ═══════════════════════════════════════════════════════════════
    //  AGENT MANAGEMENT
    // ═══════════════════════════════════════════════════════════════

    function getHookPermissions() public pure returns (Hooks.Permissions memory) {
        return Hooks.Permissions({
            beforeInitialize: true,
            afterInitialize: true,
            beforeAddLiquidity: true,
            afterAddLiquidity: true,
            beforeRemoveLiquidity: true,
            afterRemoveLiquidity: true,
            beforeSwap: true,
            afterSwap: true,
            beforeDonate: true,
            afterDonate: true,
            beforeSwapReturnDelta: false,
            afterSwapReturnDelta: false,
            afterAddLiquidityReturnDelta: false,
            afterRemoveLiquidityReturnDelta: false
        });
    }

    function transferOwnership(address newOwner) external onlyOwner {
        owner = newOwner;
    }

    function clearKillSwitch() external onlyOwner {
        killSwitchActive = false;
    }

    /// @notice Register an agent with specific capabilities.
    function registerAgent(
        address agent,
        AgentCapabilities calldata caps
    ) external onlyOwner {
        if (registeredAgents[agent]) revert AgentAlreadyRegistered();
        registeredAgents[agent] = true;
        agentCaps[agent] = caps;
        agentList.push(agent);
        emit AgentRegistered(agent);
    }

    /// @notice Returns the number of registered agents.
    function agentCount() external view returns (uint256) {
        return agentList.length;
    }

    // ═══════════════════════════════════════════════════════════════
    //  AGENT ACTIONS
    // ═══════════════════════════════════════════════════════════════

    /// @notice Commit an intent hash for the current epoch.
    ///         Matches keccak256 from `AgentIntent::compute_commitment`.
    function commitIntent(bytes32 commitment) external onlyRegistered {
        commitments[currentEpoch][msg.sender] = commitment;
        emit IntentCommitted(currentEpoch, msg.sender, commitment);
    }

    /// @notice β sets the dynamic fee (in hundredths of a bip, e.g. 3000 = 0.30%).
    function setDynamicFee(uint24 newFee) external onlyRegistered {
        if (!agentCaps[msg.sender].canSetFee) revert NotAuthorized();
        require(newFee <= LPFeeLibrary.MAX_LP_FEE, "Fee exceeds max");
        currentDynamicFee = newFee;
        emit DynamicFeeUpdated(currentEpoch, newFee);
    }

    /// @notice ε triggers the kill-switch — all subsequent swaps revert.
    function triggerKillSwitch() external onlyRegistered {
        if (!agentCaps[msg.sender].canKillSwitch) revert NotAuthorized();
        killSwitchActive = true;
        emit KillSwitchTriggered(currentEpoch, msg.sender);
    }

    // ═══════════════════════════════════════════════════════════════
    //  EPOCH SETTLEMENT (ZK-VERIFIED)
    // ═══════════════════════════════════════════════════════════════

    /// @notice Settles one epoch by verifying the aggregated ZK proof and
    ///         recording Shapley payouts.
    /// @param proof  Groth16 proof bytes from SP1.
    /// @param publicValues  abi.encode(uint256 epoch, uint16[] payouts).
    function settleEpoch(
        bytes calldata proof,
        bytes calldata publicValues
    ) external {
        // 1. Verify the ZK proof.
        zkVerifier.verifyProof(AGGREGATOR_VKEY, publicValues, proof);

        // 2. Decode public values.
        (uint256 epoch, uint16[] memory payouts) = abi.decode(
            publicValues,
            (uint256, uint16[])
        );

        // 3. Epoch must match.
        if (epoch != currentEpoch) revert EpochMismatch();

        // 4. Payouts length must match registered agents (M6 Fix).
        if (payouts.length != agentList.length) revert LengthMismatch();

        // 5. Payouts must sum to exactly 10000 bps.
        uint256 sum = 0;
        for (uint256 i = 0; i < payouts.length; i++) {
            sum += payouts[i];
        }
        if (sum != 10000) revert PayoutSumMismatch();

        // 5. Store payouts.
        epochPayouts[currentEpoch] = payouts;

        // 7. Advance epoch, clear kill-switch.
        currentEpoch++;
        killSwitchActive = false;

        emit EpochSettled(epoch, payouts, msg.sender);
    }

    /// @notice Read the Shapley payouts for a given epoch.
    function getPayouts(uint256 epoch) external view returns (uint16[] memory) {
        return epochPayouts[epoch];
    }

    // ═══════════════════════════════════════════════════════════════
    //  V4 HOOK CALLBACKS
    // ═══════════════════════════════════════════════════════════════

    // ── beforeSwap ───────────────────────────────────────────────
    function beforeSwap(
        address sender,
        PoolKey calldata,
        IPoolManager.SwapParams calldata,
        bytes calldata
    )
        external
        virtual
        onlyPoolManager
        returns (bytes4, BeforeSwapDelta, uint24)
    {
        // 1. Kill-switch blocks ALL swaps.
        if (killSwitchActive) revert KillSwitchActive();

        if (registeredAgents[sender]) {
            if (!agentCaps[sender].canSwap && !agentCaps[sender].canBackrun && !agentCaps[sender].canHedge) revert NotAuthorized();
        }

        // 2. Return the dynamic fee set by β.
        return (
            IHooks.beforeSwap.selector,
            BeforeSwapDeltaLibrary.ZERO_DELTA,
            currentDynamicFee
        );
    }

    // ── afterSwap ────────────────────────────────────────────────
    function afterSwap(
        address sender,
        PoolKey calldata,
        IPoolManager.SwapParams calldata,
        BalanceDelta,
        bytes calldata
    ) external virtual onlyPoolManager returns (bytes4, int128) {
        emit SwapTracked(currentEpoch, sender);
        return (IHooks.afterSwap.selector, 0);
    }

    // ── beforeAddLiquidity ───────────────────────────────────────
    function beforeAddLiquidity(
        address sender,
        PoolKey calldata,
        IPoolManager.ModifyLiquidityParams calldata,
        bytes calldata
    ) external virtual onlyPoolManager returns (bytes4) {
        // Registered agents must have committed this epoch.
        if (registeredAgents[sender]) {
            if (!agentCaps[sender].canLP) revert NotAuthorized();
            if (commitments[currentEpoch][sender] == bytes32(0)) {
                revert NoCommitmentThisEpoch();
            }
        }
        return IHooks.beforeAddLiquidity.selector;
    }

    // ── afterAddLiquidity ────────────────────────────────────────
    function afterAddLiquidity(
        address sender,
        PoolKey calldata,
        IPoolManager.ModifyLiquidityParams calldata,
        BalanceDelta,
        BalanceDelta,
        bytes calldata
    ) external virtual onlyPoolManager returns (bytes4, BalanceDelta) {
        emit LiquidityTracked(currentEpoch, sender, true);
        return (IHooks.afterAddLiquidity.selector, BalanceDelta.wrap(0));
    }

    // ── beforeRemoveLiquidity ────────────────────────────────────
    function beforeRemoveLiquidity(
        address sender,
        PoolKey calldata,
        IPoolManager.ModifyLiquidityParams calldata,
        bytes calldata
    ) external virtual onlyPoolManager returns (bytes4) {
        if (killSwitchActive) revert KillSwitchActive();
        if (registeredAgents[sender]) {
            if (!agentCaps[sender].canLP) revert NotAuthorized();
        }
        return IHooks.beforeRemoveLiquidity.selector;
    }

    // ── afterRemoveLiquidity ─────────────────────────────────────
    function afterRemoveLiquidity(
        address sender,
        PoolKey calldata,
        IPoolManager.ModifyLiquidityParams calldata,
        BalanceDelta,
        BalanceDelta,
        bytes calldata
    ) external virtual onlyPoolManager returns (bytes4, BalanceDelta) {
        emit LiquidityTracked(currentEpoch, sender, false);
        return (IHooks.afterRemoveLiquidity.selector, BalanceDelta.wrap(0));
    }

    // ── Unused callbacks (no-op) ─────────────────────────────────
    function beforeInitialize(
        address,
        PoolKey calldata,
        uint160
    ) external virtual onlyPoolManager returns (bytes4) {
        return IHooks.beforeInitialize.selector;
    }

    function afterInitialize(
        address,
        PoolKey calldata,
        uint160,
        int24
    ) external virtual onlyPoolManager returns (bytes4) {
        return IHooks.afterInitialize.selector;
    }

    function beforeDonate(
        address,
        PoolKey calldata,
        uint256,
        uint256,
        bytes calldata
    ) external virtual onlyPoolManager returns (bytes4) {
        return IHooks.beforeDonate.selector;
    }

    function afterDonate(
        address,
        PoolKey calldata,
        uint256,
        uint256,
        bytes calldata
    ) external virtual onlyPoolManager returns (bytes4) {
        return IHooks.afterDonate.selector;
    }
}
