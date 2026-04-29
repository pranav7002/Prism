// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

/// @title MockSP1Verifier
/// @notice Always-passing verifier for dev/testing. Swap for the real
///         SP1VerifierGateway once the AGGREGATOR_VKEY is available.
contract MockSP1Verifier is ISP1Verifier {
    constructor() {
        require(
            block.chainid == 31337,
            "Mock verifier restricted to local anvil network"
        );
    }


    function verifyProof(
        bytes32 /* programVKey */,
        bytes calldata /* publicValues */,
        bytes calldata /* proofBytes */
    ) external pure override {
        // Always succeeds — no-op.
    }
}
