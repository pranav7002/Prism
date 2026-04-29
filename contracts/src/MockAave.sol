// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

/// @title MockAave
/// @notice Minimal Aave mock for Agent ε's cross-protocol delta hedge.
///         Only supports borrow/repay — not a full lending pool.
contract MockAave {
    // ─── Events ──────────────────────────────────────────────────
    event Borrowed(
        address indexed borrower,
        address indexed asset,
        uint256 amount
    );
    event Repaid(
        address indexed borrower,
        address indexed asset,
        uint256 amount
    );

    // ─── State ───────────────────────────────────────────────────
    /// borrower → asset → outstanding amount
    mapping(address => mapping(address => uint256)) public debt;

    /// @notice Borrow `amount` of `asset`. No collateral check (mock).
    function borrow(address asset, uint256 amount) external {
        debt[msg.sender][asset] += amount;
        emit Borrowed(msg.sender, asset, amount);
    }

    /// @notice Repay `amount` of `asset`.
    function repay(address asset, uint256 amount) external {
        uint256 owed = debt[msg.sender][asset];
        uint256 actual = amount > owed ? owed : amount;
        debt[msg.sender][asset] = owed - actual;
        emit Repaid(msg.sender, asset, actual);
    }

    /// @notice View outstanding debt.
    function getDebt(
        address borrower,
        address asset
    ) external view returns (uint256) {
        return debt[borrower][asset];
    }
}
