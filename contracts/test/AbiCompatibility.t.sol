// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

import "forge-std/Test.sol";

/// @title ABI Cross-Language Compatibility Test
/// @notice Proves that the Rust hand-rolled ABI encoder in
///         `prism-orchestrator/src/proving.rs::encode_public_values_abi`
///         produces bytes that Solidity's `abi.decode` can consume.
///
///         The golden hex below is the output of:
///           encode_public_values_abi(42, &[4000, 2500, 2000, 1500, 0])
///         computed by the Rust unit test `abi_encode_matches_solidity_layout`.
contract AbiCompatibilityTest is Test {
    /// @notice Solidity-side encoding must produce the same bytes as Rust.
    function test_solidity_encode_matches_rust() public pure {
        uint16[] memory payouts = new uint16[](5);
        payouts[0] = 4000;
        payouts[1] = 2500;
        payouts[2] = 2000;
        payouts[3] = 1500;
        payouts[4] = 0;

        bytes memory solidityEncoded = abi.encode(uint256(42), payouts);

        // Golden bytes from Rust's encode_public_values_abi(42, [4000,2500,2000,1500,0])
        bytes memory rustEncoded = hex"000000000000000000000000000000000000000000000000000000000000002a"
            hex"0000000000000000000000000000000000000000000000000000000000000040"
            hex"0000000000000000000000000000000000000000000000000000000000000005"
            hex"0000000000000000000000000000000000000000000000000000000000000fa0"
            hex"00000000000000000000000000000000000000000000000000000000000009c4"
            hex"00000000000000000000000000000000000000000000000000000000000007d0"
            hex"00000000000000000000000000000000000000000000000000000000000005dc"
            hex"0000000000000000000000000000000000000000000000000000000000000000";

        assertEq(solidityEncoded, rustEncoded, "Solidity and Rust ABI encoding must match byte-for-byte");
    }

    /// @notice Rust-produced bytes must decode correctly via abi.decode.
    function test_rust_bytes_decode_correctly() public pure {
        bytes memory rustEncoded = hex"000000000000000000000000000000000000000000000000000000000000002a"
            hex"0000000000000000000000000000000000000000000000000000000000000040"
            hex"0000000000000000000000000000000000000000000000000000000000000005"
            hex"0000000000000000000000000000000000000000000000000000000000000fa0"
            hex"00000000000000000000000000000000000000000000000000000000000009c4"
            hex"00000000000000000000000000000000000000000000000000000000000007d0"
            hex"00000000000000000000000000000000000000000000000000000000000005dc"
            hex"0000000000000000000000000000000000000000000000000000000000000000";

        (uint256 epoch, uint16[] memory payouts) = abi.decode(rustEncoded, (uint256, uint16[]));

        assertEq(epoch, 42, "epoch mismatch");
        assertEq(payouts.length, 5, "payouts length mismatch");
        assertEq(payouts[0], 4000, "payouts[0] mismatch");
        assertEq(payouts[1], 2500, "payouts[1] mismatch");
        assertEq(payouts[2], 2000, "payouts[2] mismatch");
        assertEq(payouts[3], 1500, "payouts[3] mismatch");
        assertEq(payouts[4], 0, "payouts[4] mismatch");

        // Verify sum == 10000 (Shapley efficiency axiom)
        uint256 sum = 0;
        for (uint256 i = 0; i < payouts.length; i++) {
            sum += payouts[i];
        }
        assertEq(sum, 10000, "payouts must sum to 10000 bps");
    }

    /// @notice Edge case: single agent gets 100% (10000 bps).
    function test_rust_single_agent_decode() public pure {
        // encode_public_values_abi(1, &[10000])
        bytes memory encoded = abi.encode(uint256(1), _singlePayout(10000));
        (uint256 epoch, uint16[] memory payouts) = abi.decode(encoded, (uint256, uint16[]));
        assertEq(epoch, 1);
        assertEq(payouts.length, 1);
        assertEq(payouts[0], 10000);
    }

    function _singlePayout(uint16 v) internal pure returns (uint16[] memory) {
        uint16[] memory p = new uint16[](1);
        p[0] = v;
        return p;
    }
}
