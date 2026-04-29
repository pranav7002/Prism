"""
Agent δ — Backrunner brain.

Role: Extract cooperative MEV from α/β/γ's moves.
Actions: Backrun, Swap
Tripwire: Another agent's intent creates a pool dislocation.

Calm → Low-priority idle Backrun
Opportunity → High-priority aggressive Backrun (exploit β's migration)
Crisis → Backrun on mass-exit panic trades
"""


import logging
import struct
from dataclasses import dataclass

from common.constants import (
    AGENT_DELTA,
    TOKEN_USDC,
    TARGET_PROTOCOL,
)
from common.market_reader import PoolState, get_market_reader
from common.schemas import (
    AgentIntentWire,
    BackrunAction,
)

logger = logging.getLogger(__name__)


@dataclass
class DeltaConfig:
    agent_id: str = AGENT_DELTA
    profit_token: str = TOKEN_USDC
    calm_priority: int = 50
    opportunity_priority: int = 90
    crisis_priority: int = 92
    max_slippage_bps: int = 100


def _epoch_target_tx(epoch: int, marker: bytes = b"\xbe") -> str:
    """
    Deterministic 32-byte target tx hash seeded by epoch.
    Mirrors the Rust mock_intents pattern.
    """
    buf = bytearray(32)
    buf[0:len(marker)] = marker
    buf[24:32] = epoch.to_bytes(8, "big")
    return "0x" + buf.hex()


class DeltaBrain:
    """
    Backrunner decision engine.

    Identifies another agent's intent that creates a price dislocation,
    and constructs a Backrun to capture the cooperative MEV spread.
    δ's profit is credited via Shapley distribution.
    """

    def __init__(self, config: DeltaConfig | None = None):
        self.config = config or DeltaConfig()
        self.market = get_market_reader(use_mock=True)

    def decide(self, epoch: int) -> AgentIntentWire:
        state = self.market.get_pool_state()
        scenario = self._classify(epoch)
        logger.info(f"δ epoch={epoch} scenario={scenario}")

        if scenario == "calm":
            return self._calm_intent(epoch)
        elif scenario == "opportunity":
            return self._opportunity_intent(epoch)
        else:
            return self._crisis_intent(epoch)

    def _classify(self, epoch: int) -> str:
        mod = epoch % 3
        if mod == 1:
            return "calm"
        elif mod == 2:
            return "opportunity"
        else:
            return "crisis"

    def _calm_intent(self, epoch: int) -> AgentIntentWire:
        """Low-priority idle backrun — small MEV on routine trades."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=BackrunAction(
                target_tx=_epoch_target_tx(epoch, b"\xbe"),
                profit_token=self.config.profit_token,
            ),
            priority=self.config.calm_priority,
            max_slippage_bps=self.config.max_slippage_bps,
            expected_profit_bps=30,
            salt=AgentIntentWire.random_salt(),
        )

    def _opportunity_intent(self, epoch: int) -> AgentIntentWire:
        """Aggressive backrun — exploit β's thinned 0.30% pool."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=BackrunAction(
                target_tx=_epoch_target_tx(epoch, b"\xbe\xef"),
                profit_token=self.config.profit_token,
            ),
            priority=self.config.opportunity_priority,
            max_slippage_bps=self.config.max_slippage_bps,
            expected_profit_bps=200,
            salt=AgentIntentWire.random_salt(),
        )

    def _crisis_intent(self, epoch: int) -> AgentIntentWire:
        """Backrun on mass-exit panic trades."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=BackrunAction(
                target_tx=_epoch_target_tx(epoch, b"\xde\xad"),
                profit_token=self.config.profit_token,
            ),
            priority=self.config.crisis_priority,
            max_slippage_bps=500,
            expected_profit_bps=50,
            salt=AgentIntentWire.random_salt(),
        )


