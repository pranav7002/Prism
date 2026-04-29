// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

import "forge-std/Script.sol";
import {SP1VerifierGateway} from "@sp1-contracts/SP1VerifierGateway.sol";
import {SP1Verifier as SP1VerifierGroth16} from "@sp1-contracts/v3.0.0/SP1VerifierGroth16.sol";

/// @title DeployVerifier
/// @notice Deploys SP1VerifierGateway + SP1VerifierGroth16 (v3.0.0) and
///         registers the Groth16 route. Use when no pre-deployed Succinct
///         gateway exists on the target chain.
///
/// Usage:
///   PRIVATE_KEY=0x... \
///   forge script script/DeployVerifier.s.sol \
///     --rpc-url $UNICHAIN_RPC_URL --broadcast
///
/// Outputs the gateway address; pass it as SP1_GATEWAY_ADDRESS to the
/// PrismHook deploy script afterwards.
contract DeployVerifier is Script {
    function run() external {
        uint256 deployerKey = vm.envUint("PRIVATE_KEY");
        address deployer = vm.addr(deployerKey);

        vm.startBroadcast(deployerKey);

        // 1. Deploy the Groth16 verifier (v3.0.0 — matches the SP1 SDK 3.x
        //    proofs that PRISM produces today).
        SP1VerifierGroth16 groth16 = new SP1VerifierGroth16();
        console.log("SP1VerifierGroth16 (v3.0.0):", address(groth16));

        // 2. Deploy the gateway, owned by the deployer so we can add the
        //    Groth16 route.
        SP1VerifierGateway gateway = new SP1VerifierGateway(deployer);
        console.log("SP1VerifierGateway:         ", address(gateway));

        // 3. Register the Groth16 verifier as the default route.
        gateway.addRoute(address(groth16));
        console.log("Groth16 route registered.");

        vm.stopBroadcast();
    }
}
