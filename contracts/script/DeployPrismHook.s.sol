// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

import "forge-std/Script.sol";
import {PrismHook} from "../src/PrismHook.sol";
import {MockSP1Verifier} from "../src/MockSP1Verifier.sol";
import {MockAave} from "../src/MockAave.sol";
import {IPoolManager} from "v4-core/src/interfaces/IPoolManager.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

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
                abi.encodePacked(
                    bytes1(0xff),
                    deployer,
                    salt,
                    bytecodeHash
                )
            );

            if (uint160(uint256(hash)) & 0x3FFF == flags) {
                return (address(uint160(uint256(hash))), salt);
            }
        }
        revert("Salt not found");
    }
}

contract DeployPrismHook is Script {
    function run() external {
        uint256 deployerKey = vm.envOr(
            "PRIVATE_KEY",
            uint256(
                0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
            )
        );
        address poolManager = vm.envOr("POOL_MANAGER", address(1));
        bytes32 aggregatorVkey = vm.envOr(
            "AGGREGATOR_VKEY",
            bytes32(uint256(0xCAFE))
        );
        address deployer = vm.addr(deployerKey);

        bool useMockVerifier = vm.envOr("USE_MOCK_VERIFIER", false);
        address verifierAddr;

        vm.startBroadcast(deployerKey);

        if (useMockVerifier) {
            MockSP1Verifier mock = new MockSP1Verifier();
            verifierAddr = address(mock);
            console.log("MockSP1Verifier:", verifierAddr);
        } else {
            verifierAddr = vm.envAddress("SP1_GATEWAY_ADDRESS");
            console.log("SP1VerifierGateway (existing):", verifierAddr);
        }

        MockAave aave = new MockAave();
        console.log("MockAave:       ", address(aave));

        uint160 flags = uint160(
            (1 << 11) | // BEFORE_ADD_LIQUIDITY
            (1 << 10) | // AFTER_ADD_LIQUIDITY
            (1 << 9)  | // BEFORE_REMOVE_LIQUIDITY
            (1 << 8)  | // AFTER_REMOVE_LIQUIDITY
            (1 << 7)  | // BEFORE_SWAP
            (1 << 6)    // AFTER_SWAP
        );

        address CREATE2_FACTORY = 0x4e59b44847b379578588920cA78FbF26c0B4956C;
        bytes memory creationCode = type(PrismHook).creationCode;
        bytes memory constructorArgs = abi.encode(
            IPoolManager(poolManager),
            ISP1Verifier(verifierAddr),
            aggregatorVkey,
            deployer
        );

        (, bytes32 salt) = HookMiner.find(
            CREATE2_FACTORY,
            flags,
            creationCode,
            constructorArgs
        );

        PrismHook hook = new PrismHook{salt: salt}(
            IPoolManager(poolManager),
            ISP1Verifier(verifierAddr),
            aggregatorVkey,
            deployer
        );
        console.log("PrismHook:      ", address(hook));

        _registerAgents(hook);

        vm.stopBroadcast();
    }

    function _registerAgents(PrismHook hook) internal {
        address alpha = vm.envOr("AGENT_ALPHA", address(0xf2E96F75a19443c17E88f2cd8e85a188A37D1EFF));
        address beta = vm.envOr("AGENT_BETA", address(0x9E8C1Bc1D077Cb1aBb60FAa3CB80491e217FBC59));
        address gamma = vm.envOr("AGENT_GAMMA", address(0xd01F4f010DcB7C878B807B0273A8e3bAA1D1f22D));
        address delta_ = vm.envOr("AGENT_DELTA", address(0x0bfF21FB77Fc98068b02B9821Cc2E8306c55F459));
        address epsilon = vm.envOr("AGENT_EPSILON", address(0x932aE7e2CA55Ff664699fD4936Ae61AeC487BAB5));

        hook.registerAgent(alpha, PrismHook.AgentCapabilities({canLP: true, canSwap: false, canBackrun: false, canHedge: false, canSetFee: false, canKillSwitch: false}));
        hook.registerAgent(beta, PrismHook.AgentCapabilities({canLP: true, canSwap: false, canBackrun: false, canHedge: false, canSetFee: true, canKillSwitch: false}));
        hook.registerAgent(gamma, PrismHook.AgentCapabilities({canLP: true, canSwap: false, canBackrun: false, canHedge: false, canSetFee: false, canKillSwitch: false}));
        hook.registerAgent(delta_, PrismHook.AgentCapabilities({canLP: false, canSwap: true, canBackrun: true, canHedge: false, canSetFee: false, canKillSwitch: false}));
        hook.registerAgent(epsilon, PrismHook.AgentCapabilities({canLP: false, canSwap: false, canBackrun: false, canHedge: true, canSetFee: false, canKillSwitch: true}));

        console.log("5 agents registered");
    }
}
