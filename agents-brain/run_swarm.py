#!/usr/bin/env python3
"""
PRISM Swarm Runner — runs all 5 agents and broadcasts to the orchestrator.

Modes:
    --offline       Log intents to stdout (no WS connection needed)
    --live          Connect to ws://localhost:8765 and broadcast live
    --ws-url URL    Custom WebSocket URL

Usage:
    PYTHONPATH=. python run_swarm.py                          # offline, epochs 1-3
    PYTHONPATH=. python run_swarm.py --live                   # live WS, epochs 1-3
    PYTHONPATH=. python run_swarm.py --live --epochs 1 2 3 4 5 6
    PYTHONPATH=. python run_swarm.py --ws-url ws://host:8765 --epochs 1 2 3
"""


import argparse
import asyncio
import json
import logging
import sys
import time

from alpha.brain import AlphaBrain
from beta.brain import BetaBrain
from gamma.brain import GammaBrain
from delta.brain import DeltaBrain
from epsilon.brain import EpsilonBrain
from common.broadcaster import get_broadcaster, BroadcastResult
from common.schemas import AgentIntentWire
from common.constants import WS_DEFAULT_URL

logger = logging.getLogger("swarm")


SCENARIO_NAMES = {1: "calm", 2: "opportunity", 0: "crisis"}


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
) -> list[BroadcastResult]:
    """Run all 5 brains, broadcast, and print summary."""
    scenario = SCENARIO_NAMES.get(epoch % 3, "unknown")
    print(f"\n{'═' * 60}")
    print(f"  EPOCH {epoch} — {scenario.upper()}")
    print(f"{'═' * 60}\n")

    pairs = gen_epoch_intents(epoch)
    intents = [intent for _, intent in pairs]

    # Broadcast
    results = await broadcaster.send_epoch_intents(intents)

    # Summary table
    print(f"  {'Agent':<4} {'Action':<22} {'Pri':<5} {'Slip':<6} {'Status':<8} {'Commitment'}")
    print(f"  {'─' * 4} {'─' * 22} {'─' * 5} {'─' * 6} {'─' * 8} {'─' * 18}")

    for (label, intent), result in zip(pairs, results):
        status = "✓ sent" if result.success else f"✗ {result.error or 'fail'}"
        commitment = result.commitment[:18] + "..."
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

    if args.live:
        connected = await broadcaster.connect()
        if not connected:
            print("⚠ Could not connect to orchestrator, falling back to offline mode")
            broadcaster = get_broadcaster(offline=True)
            await broadcaster.connect()
    else:
        await broadcaster.connect()

    all_results = []
    for epoch in args.epochs:
        results = await run_epoch_async(epoch, broadcaster)
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

    args = parser.parse_args()

    logging.basicConfig(
        level=logging.INFO,
        format="%(name)s %(message)s",
    )

    asyncio.run(main_async(args))


if __name__ == "__main__":
    main()
