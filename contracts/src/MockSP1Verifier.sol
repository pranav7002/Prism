// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

/// @title MockSP1Verifier
/// @notice Always-passing verifier for dev/testing AND demo theater. Swap for the
///         real SP1VerifierGateway in production.
/// @dev Allowed chains: 31337 (anvil), 1301 (Unichain Sepolia — for parallel
///      demo-mode hook). Mainnet (chainid 1) is hard-blocked.
contract MockSP1Verifier is ISP1Verifier {
    constructor() {
        require(
            block.chainid == 31337 || block.chainid == 1301,
            "Mock verifier restricted to anvil/unichain-sepolia"
        );
    }

    function verifyProof(
        bytes32, /* programVKey */
        bytes calldata, /* publicValues */
        bytes calldata /* proofBytes */
    ) external pure override {
        // Always succeeds — no-op.
    }
}
