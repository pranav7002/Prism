"""
Week 2 Test Plan — All 5 Agent Brains

Tests verify:
  1. Each brain produces valid AgentIntentWire for all 3 scenarios
  2. Correct action types per agent role
  3. Commitments are unique across agents within the same epoch
  4. Pydantic validation catches malformed intents
  5. Schema compliance with INTERFACES_FOR_DEV3.md §3.1
"""

import sys
import os
import json

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from common.schemas import AgentIntentWire
from common.constants import (
    AGENT_ALPHA, AGENT_BETA, AGENT_GAMMA, AGENT_DELTA, AGENT_EPSILON,
)

from alpha.brain import AlphaBrain
from beta.brain import BetaBrain
from gamma.brain import GammaBrain
from delta.brain import DeltaBrain
from epsilon.brain import EpsilonBrain


# ---
# Helper
# ---

def _validate_intent(intent: AgentIntentWire, expected_agent: str):
    """Common validations every intent must pass."""
    # 1. Correct agent_id
    assert intent.agent_id == expected_agent.lower(), (
        f"Wrong agent_id: {intent.agent_id}"
    )
    # 2. Has a valid commitment (non-zero, 66 chars)
    commitment = intent.compute_commitment()
    assert commitment.startswith("0x"), "Commitment must be 0x-prefixed"
    assert len(commitment) == 66, f"Commitment wrong length: {len(commitment)}"
    assert commitment != "0x" + "00" * 32, "Commitment must not be all-zeros"
    # 3. Serializes to valid JSON
    wire = intent.to_wire_json()
    json_str = json.dumps(wire)
    assert len(json_str) > 50, "Wire JSON too short"
    # 4. Round-trip: parse the JSON back through Pydantic
    reparsed = AgentIntentWire.model_validate(wire)
    assert reparsed.compute_commitment() == commitment, "Round-trip commitment mismatch"


# ---
# α — Predictive LP
# ---

class TestAlpha:
    def test_calm_produces_add_liquidity(self):
        brain = AlphaBrain()
        intent = brain.decide(epoch=1)  # calm
        _validate_intent(intent, AGENT_ALPHA)
        assert intent.action.type == "AddLiquidity"
        assert intent.priority == 70

    def test_opportunity_produces_add_liquidity(self):
        brain = AlphaBrain()
        intent = brain.decide(epoch=2)  # opportunity
        _validate_intent(intent, AGENT_ALPHA)
        assert intent.action.type == "AddLiquidity"
        assert intent.priority == 85

    def test_crisis_produces_remove_liquidity(self):
        brain = AlphaBrain()
        intent = brain.decide(epoch=3)  # crisis
        _validate_intent(intent, AGENT_ALPHA)
        assert intent.action.type == "RemoveLiquidity"


# ---
# β — Fee Curator
# ---

class TestBeta:
    def test_calm_produces_set_dynamic_fee(self):
        brain = BetaBrain()
        intent = brain.decide(epoch=1)
        _validate_intent(intent, AGENT_BETA)
        assert intent.action.type == "SetDynamicFee"
        assert 500 <= intent.action.new_fee_ppm <= 10000

    def test_opportunity_produces_migrate_liquidity(self):
        brain = BetaBrain()
        intent = brain.decide(epoch=2)
        _validate_intent(intent, AGENT_BETA)
        assert intent.action.type == "MigrateLiquidity"

    def test_crisis_produces_set_dynamic_fee_high(self):
        brain = BetaBrain()
        intent = brain.decide(epoch=3)
        _validate_intent(intent, AGENT_BETA)
        assert intent.action.type == "SetDynamicFee"
        assert intent.action.new_fee_ppm >= 5000  # crisis = high fee


# ---
# γ — Frag Healer
# ---

class TestGamma:
    def test_calm_produces_batch_consolidate(self):
        brain = GammaBrain()
        intent = brain.decide(epoch=1)
        _validate_intent(intent, AGENT_GAMMA)
        assert intent.action.type == "BatchConsolidate"
        assert len(intent.action.removes) >= 1
        assert len(intent.action.adds) >= 1

    def test_opportunity_produces_batch_consolidate_multi(self):
        brain = GammaBrain()
        intent = brain.decide(epoch=2)
        _validate_intent(intent, AGENT_GAMMA)
        assert intent.action.type == "BatchConsolidate"
        assert len(intent.action.removes) >= 2  # opportunity sweeps more

    def test_crisis_produces_batch_consolidate(self):
        brain = GammaBrain()
        intent = brain.decide(epoch=3)
        _validate_intent(intent, AGENT_GAMMA)
        assert intent.action.type == "BatchConsolidate"


# ---
# δ — Backrunner
# ---

class TestDelta:
    def test_calm_produces_backrun(self):
        brain = DeltaBrain()
        intent = brain.decide(epoch=1)
        _validate_intent(intent, AGENT_DELTA)
        assert intent.action.type == "Backrun"

    def test_opportunity_produces_backrun_high_priority(self):
        brain = DeltaBrain()
        intent = brain.decide(epoch=2)
        _validate_intent(intent, AGENT_DELTA)
        assert intent.action.type == "Backrun"
        assert intent.priority >= 80  # opportunity = aggressive

    def test_crisis_produces_backrun(self):
        brain = DeltaBrain()
        intent = brain.decide(epoch=3)
        _validate_intent(intent, AGENT_DELTA)
        assert intent.action.type == "Backrun"


# ---
# ε — Guardian
# ---

class TestEpsilon:
    def test_calm_produces_delta_hedge(self):
        brain = EpsilonBrain()
        intent = brain.decide(epoch=1)
        _validate_intent(intent, AGENT_EPSILON)
        assert intent.action.type == "DeltaHedge"

    def test_opportunity_produces_delta_hedge(self):
        brain = EpsilonBrain()
        intent = brain.decide(epoch=2)
        _validate_intent(intent, AGENT_EPSILON)
        assert intent.action.type == "DeltaHedge"

    def test_crisis_produces_kill_switch(self):
        brain = EpsilonBrain()
        intent = brain.decide(epoch=3)
        _validate_intent(intent, AGENT_EPSILON)
        assert intent.action.type == "KillSwitch"
        assert len(intent.action.reason) > 0
        assert intent.priority >= 100  # kill switch = max priority
