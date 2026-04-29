#!/usr/bin/env python3
"""
PRISM Swarm Runner — runs all 5 agents and broadcasts to the orchestrator.

Modes:
    --offline           Log intents to stdout (no WS connection needed)
    --live              Connect to ws://localhost:8765 and broadcast live
    --ws-url URL        Custom WebSocket URL
    --commit-on-chain   Submit commitments to PrismHook on-chain before WS broadcast
    --rpc-url URL       Unichain JSON-RPC endpoint (default: https://sepolia.unichain.org)
    --hook-address ADDR PrismHook contract address (or set PRISM_HOOK_ADDRESS env var)

Usage:
    PYTHONPATH=. python run_swarm.py                          # offline, epochs 1-3
    PYTHONPATH=. python run_swarm.py --live                   # live WS, epochs 1-3
    PYTHONPATH=. python run_swarm.py --live --epochs 1 2 3 4 5 6
    PYTHONPATH=. python run_swarm.py --ws-url ws://host:8765 --epochs 1 2 3
    PYTHONPATH=. python run_swarm.py --offline --commit-on-chain  # on-chain only
"""


import argparse
import asyncio
import json
import logging
import os
import sys
import time

from alpha.brain import AlphaBrain
from beta.brain import BetaBrain
from gamma.brain import GammaBrain
from delta.brain import DeltaBrain
from epsilon.brain import EpsilonBrain
from common.broadcaster import get_broadcaster, BroadcastResult
from common.chain_client import ChainClient, ChainClientError
from common.schemas import AgentIntentWire
from common.constants import WS_DEFAULT_URL

logger = logging.getLogger("swarm")


SCENARIO_NAMES = {1: "calm", 2: "opportunity", 0: "crisis"}

# Per-agent private key env var names (read from environment at runtime)
AGENT_KEY_ENV_VARS = {
    "α": "AGENT_ALPHA_KEY",
    "β": "AGENT_BETA_KEY",
    "γ": "AGENT_GAMMA_KEY",
    "δ": "AGENT_DELTA_KEY",
    "ε": "AGENT_EPSILON_KEY",
}

ALL_BRAINS = [
    ("α", AlphaBrain()),
    ("β", BetaBrain()),
    ("γ", GammaBrain()),
    ("δ", DeltaBrain()),
    ("ε", EpsilonBrain()),
]


def gen_epoch_intents(epoch: int) -> list[tuple[str, AgentIntentWire]]:
    """Generate intents from all 5 brains."""
    return [(label, brain.decide(epoch)) for label, brain in ALL_BRAINS]


async def run_epoch_async(
    epoch: int,
    broadcaster,
    chain_client: ChainClient | None = None,
    agent_keys: dict[str, str] | None = None,
) -> list[BroadcastResult]:
    """Run all 5 brains, optionally commit on-chain, broadcast, and print summary."""
    scenario = SCENARIO_NAMES.get(epoch % 3, "unknown")
    print(f"\n{'═' * 60}")
    print(f"  EPOCH {epoch} — {scenario.upper()}")
    print(f"{'═' * 60}\n")

    pairs = gen_epoch_intents(epoch)
    intents = [intent for _, intent in pairs]

    # Optional on-chain commit step (C5)
    tx_hashes: dict[str, str] = {}
    if chain_client is not None and agent_keys:
        for label, intent in pairs:
            pk = agent_keys.get(label)
            if not pk:
                logger.warning(f"No private key for agent {label}, skipping on-chain commit")
                continue
            commitment_hex = intent.compute_commitment()
            commitment_bytes = bytes.fromhex(commitment_hex.removeprefix("0x"))
            try:
                tx_hash = await chain_client.commit_intent(pk, commitment_bytes)
                tx_hashes[label] = tx_hash
                logger.info(f"Agent {label} committed on-chain: {tx_hash}")
            except ChainClientError as exc:
                logger.error(f"On-chain commit failed for {label}: {exc}")
                tx_hashes[label] = f"ERR:{type(exc).__name__}"

    # Broadcast over WebSocket
    results = await broadcaster.send_epoch_intents(intents)

    # Summary table — wider when tx hashes are present
    if tx_hashes:
        print(f"  {'Agent':<4} {'Action':<22} {'Pri':<5} {'Slip':<6} {'Status':<8} {'Commitment':<21} {'TxHash'}")
        print(f"  {'─' * 4} {'─' * 22} {'─' * 5} {'─' * 6} {'─' * 8} {'─' * 21} {'─' * 18}")
    else:
        print(f"  {'Agent':<4} {'Action':<22} {'Pri':<5} {'Slip':<6} {'Status':<8} {'Commitment'}")
        print(f"  {'─' * 4} {'─' * 22} {'─' * 5} {'─' * 6} {'─' * 8} {'─' * 18}")

    for (label, intent), result in zip(pairs, results):
        status = "✓ sent" if result.success else f"✗ {result.error or 'fail'}"
        commitment = result.commitment[:18] + "..."
        if tx_hashes:
            raw_tx = tx_hashes.get(label, "—")
            tx_display = (raw_tx[:18] + "...") if len(raw_tx) > 21 else raw_tx
            print(
                f"  {label:<4} {intent.action.type:<22} "
                f"{intent.priority:<5} {intent.max_slippage_bps:<6} "
                f"{status:<8} {commitment:<21} {tx_display}"
            )
        else:
            print(
                f"  {label:<4} {intent.action.type:<22} "
                f"{intent.priority:<5} {intent.max_slippage_bps:<6} "
                f"{status:<8} {commitment}"
            )

    # Verify uniqueness
    commitments = [r.commitment for r in results]
    if len(set(commitments)) == len(commitments):
        print(f"\n  ✓ All {len(results)} commitments unique")
    else:
        print(f"\n  ⚠ DUPLICATE COMMITMENTS detected!")

    return results


async def main_async(args):
    """Async main — connect, run epochs, close."""
    broadcaster = get_broadcaster(
        ws_url=args.ws_url,
        offline=not args.live,
    )

    # C6: live connection failure is fatal — no silent fallback
    if args.live:
        connected = await broadcaster.connect()
        if not connected:
            print("✗ FATAL: Could not connect to orchestrator at", args.ws_url)
            print("  Use --offline to run without a WS server.")
            sys.exit(1)
    else:
        await broadcaster.connect()

    # C5: build ChainClient and collect per-agent private keys (if requested)
    chain_client: ChainClient | None = None
    agent_keys: dict[str, str] = {}
    if args.commit_on_chain:
        hook_address = args.hook_address or os.environ.get("PRISM_HOOK_ADDRESS", "")
        if not hook_address:
            print("✗ --commit-on-chain requires PRISM_HOOK_ADDRESS env var or --hook-address flag")
            sys.exit(1)
        chain_client = ChainClient(
            rpc_url=args.rpc_url,
            hook_address=hook_address,
        )
        for label, env_var in AGENT_KEY_ENV_VARS.items():
            pk = os.environ.get(env_var, "")
            if pk:
                agent_keys[label] = pk
            else:
                logger.warning(f"Private key env var {env_var} not set for agent {label}")
        logger.info(
            f"On-chain commits enabled: hook={hook_address} "
            f"rpc={args.rpc_url} agents_with_keys={len(agent_keys)}"
        )

    all_results = []
    for epoch in args.epochs:
        results = await run_epoch_async(
            epoch, broadcaster,
            chain_client=chain_client,
            agent_keys=agent_keys if agent_keys else None,
        )
        all_results.extend(results)

        # Wait between epochs in live mode (simulates 12s epoch cadence)
        if args.live and epoch != args.epochs[-1]:
            print(f"\n  ⏳ Waiting {args.interval}s for next epoch...")
            await asyncio.sleep(args.interval)

    # Summary
    total = len(all_results)
    sent = sum(1 for r in all_results if r.success)
    failed = total - sent
    print(f"\n{'═' * 60}")
    print(f"  SUMMARY: {sent}/{total} intents broadcast ({failed} failed)")
    print(f"{'═' * 60}")

    # Wire JSON for last epoch
    if args.epochs and args.dump_json:
        last_epoch = args.epochs[-1]
        print(f"\n{'─' * 60}")
        print(f"  Wire JSON for epoch {last_epoch}:")
        print(f"{'─' * 60}")
        last_pairs = gen_epoch_intents(last_epoch)
        for label, intent in last_pairs:
            print(f"\n  ── {label} ──")
            print(json.dumps(intent.to_wire_json(), indent=4))

    await broadcaster.close()


def main():
    parser = argparse.ArgumentParser(description="PRISM Agent Swarm Runner")
    parser.add_argument(
        "--live", action="store_true",
        help="Connect to orchestrator WebSocket and broadcast live"
    )
    parser.add_argument(
        "--ws-url", default=WS_DEFAULT_URL,
        help=f"WebSocket URL (default: {WS_DEFAULT_URL})"
    )
    parser.add_argument(
        "--epochs", type=int, nargs="+", default=[1, 2, 3],
        help="Epoch numbers to run (default: 1 2 3)"
    )
    parser.add_argument(
        "--interval", type=float, default=3.0,
        help="Seconds between epochs in live mode (default: 3.0)"
    )
    parser.add_argument(
        "--dump-json", action="store_true", default=True,
        help="Print wire JSON for last epoch (default: true)"
    )
    parser.add_argument(
        "--no-dump-json", dest="dump_json", action="store_false",
        help="Don't print wire JSON"
    )
    parser.add_argument(
        "--commit-on-chain", action="store_true", default=False,
        help="Submit each commitment to PrismHook.commitIntent() on-chain before WS broadcast"
    )
    parser.add_argument(
        "--rpc-url", default="https://sepolia.unichain.org",
        help="Unichain JSON-RPC endpoint (default: https://sepolia.unichain.org)"
    )
    parser.add_argument(
        "--hook-address", default=os.environ.get("PRISM_HOOK_ADDRESS", ""),
        help="PrismHook contract address (default: $PRISM_HOOK_ADDRESS env var)"
    )

    args = parser.parse_args()

    logging.basicConfig(
        level=logging.INFO,
        format="%(name)s %(message)s",
    )

    asyncio.run(main_async(args))


if __name__ == "__main__":
    main()
