"""
Real Ethereum wallet generation for the 5 PRISM agents.

Uses eth-account for proper secp256k1 ECDSA key derivation.
Address = keccak256(uncompressed_pubkey)[12:] — real ETH addresses
that can be registered in PrismHook.sol and sign on-chain transactions.
"""


import os
import secrets
from typing import NamedTuple

from eth_account import Account


class AgentWallet(NamedTuple):
    label: str       # α, β, γ, δ, ε
    address: str     # 0x-prefixed, EIP-55 checksummed
    private_key: str # 0x-prefixed, 32-byte hex


AGENT_LABELS = ["α", "β", "γ", "δ", "ε"]

# Deterministic seed-derived private keys for reproducible dev builds.
# These are SHA-256 hashes of "prism-agent-{role}-v1" — stable across runs.
_DETERMINISTIC_SEEDS = {
    "α": "prism-agent-alpha-predictive-lp-v1",
    "β": "prism-agent-beta-fee-curator-v1",
    "γ": "prism-agent-gamma-frag-healer-v1",
    "δ": "prism-agent-delta-backrunner-v1",
    "ε": "prism-agent-epsilon-guardian-v1",
}


def generate_agent_wallets(seed: str | None = "deterministic") -> list[AgentWallet]:
    """
    Generate 5 real Ethereum wallets (one per agent).

    Args:
        seed: If "deterministic", uses stable seed phrases for reproducibility.
              If None, generates cryptographically random keys.
              If a custom string, derives keys from SHA-256(seed + label).

    Returns:
        List of 5 AgentWallet with real EIP-55 checksummed addresses
        derived via secp256k1 ECDSA (eth-account).
    """
    import hashlib
    wallets: list[AgentWallet] = []

    for label in AGENT_LABELS:
        if seed == "deterministic":
            # Stable deterministic keys for dev
            seed_str = _DETERMINISTIC_SEEDS[label]
            pk_bytes = hashlib.sha256(seed_str.encode("utf-8")).digest()
        elif seed is not None:
            pk_bytes = hashlib.sha256(f"{seed}:{label}".encode("utf-8")).digest()
        else:
            pk_bytes = secrets.token_bytes(32)

        pk_hex = "0x" + pk_bytes.hex()
        acct = Account.from_key(pk_hex)

        wallets.append(AgentWallet(
            label=label,
            address=acct.address,       # EIP-55 checksummed
            private_key=pk_hex,
        ))

    return wallets


def write_wallet_file(wallets: list[AgentWallet], path: str):
    """Write AGENT_WALLETS.md for Dev 2 handoff."""
    role_map = {
        "α": "Predictive LP",
        "β": "Fee Curator",
        "γ": "Frag Healer",
        "δ": "Backrunner",
        "ε": "Guardian",
    }
    capability_map = {
        "α": "canLP=true",
        "β": "canSetFee=true",
        "γ": "canLP=true",
        "δ": "canSwap=true, canBackrun=true",
        "ε": "canHedge=true, canKillSwitch=true",
    }

    lines = [
        "# PRISM Agent Wallets",
        "",
        "Real Ethereum EOAs generated via `eth-account` (secp256k1 ECDSA).",
        "Use these addresses in `DeployPrismHook.s.sol` `registerAgent()` calls.",
        "",
        "## Addresses for `PrismHook.registerAgent()`",
        "",
        "| Agent | Role | Address | Capabilities |",
        "|-------|------|---------|-------------|",
    ]

    for w in wallets:
        role = role_map.get(w.label, "Unknown")
        caps = capability_map.get(w.label, "")
        lines.append(f"| {w.label} | {role} | `{w.address}` | {caps} |")

    lines.extend([
        "",
        "## Foundry Script Snippet",
        "",
        "```solidity",
        "// Paste into DeployPrismHook.s.sol",
    ])
    for w in wallets:
        role = role_map.get(w.label, "")
        lines.append(f'address agent_{w.label} = {w.address}; // {role}')
    lines.extend([
        "```",
        "",
        "## Private Keys (DEV ONLY — do NOT commit to prod)",
        "",
        "```",
    ])
    for w in wallets:
        lines.append(f"{w.label}: {w.private_key}")
    lines.extend(["```", ""])

    with open(path, "w") as f:
        f.write("\n".join(lines))


def main():
    """CLI: generate wallets and print/save them."""
    wallets = generate_agent_wallets(seed="deterministic")

    print("═══ PRISM Agent Wallets (Real ECDSA EOAs) ═══\n")
    for w in wallets:
        print(f"  {w.label}  {w.address}")
        print(f"       pk: {w.private_key}\n")

    # Write handoff file
    project_root = os.path.join(os.path.dirname(__file__), "..", "..")
    wallet_path = os.path.join(project_root, "AGENT_WALLETS.md")
    write_wallet_file(wallets, wallet_path)
    print(f"✓ Wrote {os.path.abspath(wallet_path)}")


if __name__ == "__main__":
    main()
