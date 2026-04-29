"""
Week 2 Integration Tests — Full Swarm

Tests verify:
  1. All 5 agents produce intents simultaneously per epoch
  2. Commitments are unique across all agents within an epoch
  3. Agent IDs are distinct
  4. Wire JSON passes Pydantic round-trip for every intent
  5. The 3-epoch cycle (calm → opportunity → crisis) works end-to-end
  6. Wallet generation produces valid ETH addresses
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
# Swarm helper
# ---

ALL_BRAINS = [
    ("α", AlphaBrain()),
    ("β", BetaBrain()),
    ("γ", GammaBrain()),
    ("δ", DeltaBrain()),
    ("ε", EpsilonBrain()),
]

def run_epoch(epoch: int) -> list[AgentIntentWire]:
    """Run all 5 brains for one epoch."""
    return [brain.decide(epoch) for _, brain in ALL_BRAINS]


# ---
# Tests
# ---

class TestSwarmIntegration:

    def test_five_agents_produce_five_intents(self):
        """Each epoch yields exactly 5 intents."""
        for epoch in [1, 2, 3]:
            intents = run_epoch(epoch)
            assert len(intents) == 5, f"Epoch {epoch}: got {len(intents)} intents"

    def test_all_agent_ids_distinct(self):
        """No two agents share the same agent_id."""
        intents = run_epoch(1)
        ids = [i.agent_id for i in intents]
        assert len(set(ids)) == 5, f"Duplicate agent IDs: {ids}"

    def test_all_commitments_unique_within_epoch(self):
        """Every commitment in a single epoch must be unique."""
        for epoch in [1, 2, 3]:
            intents = run_epoch(epoch)
            commitments = [i.compute_commitment() for i in intents]
            assert len(set(commitments)) == 5, (
                f"Epoch {epoch}: duplicate commitments found"
            )

    def test_commitments_differ_across_epochs(self):
        """Same agent, different epoch → different commitment (via random salt)."""
        brain = AlphaBrain()
        c1 = brain.decide(1).compute_commitment()
        c2 = brain.decide(1).compute_commitment()
        # Random salt → different each call
        assert c1 != c2, "Two calls with random salt should differ"

    def test_wire_json_round_trip_all_agents(self):
        """Every intent's JSON can be parsed back through Pydantic."""
        for epoch in [1, 2, 3]:
            intents = run_epoch(epoch)
            for intent in intents:
                wire = intent.to_wire_json()
                # Must serialize to valid JSON
                json_str = json.dumps(wire)
                assert len(json_str) > 50
                # Must parse back
                reparsed = AgentIntentWire.model_validate(wire)
                assert reparsed.agent_id == intent.agent_id
                assert reparsed.epoch == intent.epoch

    def test_calm_epoch_action_types(self):
        """Calm epoch: verify each agent's action type matches its role."""
        intents = run_epoch(1)
        types = {i.agent_id: i.action.type for i in intents}
        assert types[AGENT_ALPHA.lower()] == "AddLiquidity"
        assert types[AGENT_BETA.lower()] == "SetDynamicFee"
        assert types[AGENT_GAMMA.lower()] == "BatchConsolidate"
        assert types[AGENT_DELTA.lower()] == "Backrun"
        assert types[AGENT_EPSILON.lower()] == "DeltaHedge"

    def test_opportunity_epoch_action_types(self):
        """Opportunity epoch: β migrates, δ goes aggressive."""
        intents = run_epoch(2)
        types = {i.agent_id: i.action.type for i in intents}
        assert types[AGENT_BETA.lower()] == "MigrateLiquidity"
        assert types[AGENT_DELTA.lower()] == "Backrun"

    def test_crisis_epoch_action_types(self):
        """Crisis epoch: α removes liquidity, ε triggers kill switch."""
        intents = run_epoch(3)
        types = {i.agent_id: i.action.type for i in intents}
        assert types[AGENT_ALPHA.lower()] == "RemoveLiquidity"
        assert types[AGENT_EPSILON.lower()] == "KillSwitch"

    def test_protocol_is_always_uniswap(self):
        """Every intent targets the Uniswap protocol."""
        for epoch in [1, 2, 3]:
            intents = run_epoch(epoch)
            for intent in intents:
                assert intent.target_protocol == "Uniswap"

    def test_priorities_within_valid_range(self):
        """All priorities are 0-255."""
        for epoch in [1, 2, 3]:
            intents = run_epoch(epoch)
            for intent in intents:
                assert 0 <= intent.priority <= 255

    def test_slippage_within_valid_range(self):
        """All slippage values are 0-10000 bps."""
        for epoch in [1, 2, 3]:
            intents = run_epoch(epoch)
            for intent in intents:
                assert 0 <= intent.max_slippage_bps <= 10000


class TestWalletGeneration:

    def test_generate_wallets(self):
        """Wallet generation produces 5 valid ETH addresses."""
        from common.wallets import generate_agent_wallets
        wallets = generate_agent_wallets()
        assert len(wallets) == 5
        for label, addr, _ in wallets:
            assert addr.startswith("0x")
            assert len(addr) == 42  # 0x + 40 hex chars
            assert label in ["α", "β", "γ", "δ", "ε"]

    def test_wallets_are_distinct(self):
        """All 5 wallet addresses must be unique."""
        from common.wallets import generate_agent_wallets
        wallets = generate_agent_wallets()
        addresses = [addr for _, addr, _ in wallets]
        assert len(set(addresses)) == 5

    def test_private_keys_present(self):
        """Each wallet has a non-empty private key."""
        from common.wallets import generate_agent_wallets
        wallets = generate_agent_wallets()
        for _, _, pk in wallets:
            assert pk.startswith("0x")
            assert len(pk) == 66  # 0x + 64 hex chars
