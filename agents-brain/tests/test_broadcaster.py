"""
Tests for the WebSocket intent broadcaster.

Uses OfflineBroadcaster (no actual WS needed) to verify:
  - Intents are correctly serialized
  - Commitments are attached
  - All 5 intents per epoch are captured
"""

import sys
import os
import asyncio

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from common.broadcaster import OfflineBroadcaster, get_broadcaster
from common.schemas import AgentIntentWire, SwapAction

from alpha.brain import AlphaBrain
from beta.brain import BetaBrain
from gamma.brain import GammaBrain
from delta.brain import DeltaBrain
from epsilon.brain import EpsilonBrain


def _run(coro):
    """Helper to run async code in tests."""
    return asyncio.get_event_loop().run_until_complete(coro)


class TestOfflineBroadcaster:

    def test_send_single_intent(self):
        broadcaster = OfflineBroadcaster()
        _run(broadcaster.connect())

        intent = AlphaBrain().decide(1)
        result = _run(broadcaster.send_intent(intent))

        assert result.success
        assert result.agent_id == intent.agent_id
        assert result.commitment.startswith("0x")
        assert len(result.commitment) == 66
        assert broadcaster.sent_count == 1

    def test_send_epoch_all_five(self):
        broadcaster = OfflineBroadcaster()
        _run(broadcaster.connect())

        brains = [AlphaBrain(), BetaBrain(), GammaBrain(), DeltaBrain(), EpsilonBrain()]
        intents = [b.decide(1) for b in brains]
        results = _run(broadcaster.send_epoch_intents(intents))

        assert len(results) == 5
        assert all(r.success for r in results)
        assert broadcaster.sent_count == 5

        # All commitments unique
        commitments = [r.commitment for r in results]
        assert len(set(commitments)) == 5

    def test_intents_stored_with_commitment(self):
        broadcaster = OfflineBroadcaster()
        _run(broadcaster.connect())

        intent = AlphaBrain().decide(2)
        _run(broadcaster.send_intent(intent))

        stored = broadcaster.intents[0]
        assert "commitment" in stored
        assert stored["commitment"].startswith("0x")
        assert stored["agent_id"] == intent.agent_id

    def test_close_is_safe(self):
        broadcaster = OfflineBroadcaster()
        _run(broadcaster.connect())
        _run(broadcaster.close())
        assert broadcaster.sent_count == 0

    def test_factory_offline_mode(self):
        broadcaster = get_broadcaster(offline=True)
        assert isinstance(broadcaster, OfflineBroadcaster)

    def test_three_epoch_cycle(self):
        broadcaster = OfflineBroadcaster()
        _run(broadcaster.connect())

        brains = [AlphaBrain(), BetaBrain(), GammaBrain(), DeltaBrain(), EpsilonBrain()]

        total_sent = 0
        for epoch in [1, 2, 3]:
            intents = [b.decide(epoch) for b in brains]
            results = _run(broadcaster.send_epoch_intents(intents))
            assert all(r.success for r in results)
            total_sent += 5

        assert broadcaster.sent_count == 15
        assert len(broadcaster.intents) == 15

        # All 15 commitments unique
        all_commitments = [i["commitment"] for i in broadcaster.intents]
        assert len(set(all_commitments)) == 15
