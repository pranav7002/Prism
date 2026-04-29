"""
Agent γ — Frag Healer brain.

Role: Consolidate scattered liquidity across pools into optimal tier.
Actions: BatchConsolidate
Tripwire: > 3 stale positions in wrong tier.

Calm → Consolidate 1 stale 0.05% position into 0.30%
Opportunity → Sweep 3 stale positions across tiers
Crisis → Consolidate into wide-range safety position on 0.05%
"""


import logging
from dataclasses import dataclass

from common.constants import (
    AGENT_GAMMA,
    POOL_USDC_WETH_005,
    POOL_USDC_WETH_030,
    POOL_USDC_WETH_060,
    TARGET_PROTOCOL,
)
from common.market_reader import PoolState, get_market_reader
from common.schemas import (
    AgentIntentWire,
    BatchConsolidateAction,
    ConsolidateRemoveItem,
    ConsolidateAddItem,
)

logger = logging.getLogger(__name__)


@dataclass
class GammaConfig:
    agent_id: str = AGENT_GAMMA
    calm_priority: int = 50
    opportunity_priority: int = 55
    crisis_priority: int = 80
    max_slippage_bps: int = 50


class GammaBrain:
    """
    Frag Healer decision engine.

    Scans for stale/fragmented LP positions and consolidates them
    into the optimal fee tier.
    """

    def __init__(self, config: GammaConfig | None = None):
        self.config = config or GammaConfig()
        self.market = get_market_reader(use_mock=True)

    def decide(self, epoch: int) -> AgentIntentWire:
        state = self.market.get_pool_state(POOL_USDC_WETH_030)
        scenario = self._classify(epoch)
        logger.info(f"γ epoch={epoch} scenario={scenario}")

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
        """Consolidate 1 stale 0.05% position into 0.30%."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=BatchConsolidateAction(
                removes=[
                    ConsolidateRemoveItem(
                        pool=POOL_USDC_WETH_005,
                        liquidity="45000000000",
                    ),
                ],
                adds=[
                    ConsolidateAddItem(
                        pool=POOL_USDC_WETH_030,
                        amount0="45000000000",
                        amount1="15000000000000000000",
                        tick_lower=196_800,
                        tick_upper=200_400,
                    ),
                ],
            ),
            priority=self.config.calm_priority,
            max_slippage_bps=self.config.max_slippage_bps,
            expected_profit_bps=30,
            salt=AgentIntentWire.random_salt(),
        )

    def _opportunity_intent(self, epoch: int) -> AgentIntentWire:
        """Sweep 3 stale positions from all tiers into 0.30%."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=BatchConsolidateAction(
                removes=[
                    ConsolidateRemoveItem(
                        pool=POOL_USDC_WETH_005,
                        liquidity="80000000000",
                    ),
                    ConsolidateRemoveItem(
                        pool=POOL_USDC_WETH_030,
                        liquidity="45000000000",
                    ),
                    ConsolidateRemoveItem(
                        pool=POOL_USDC_WETH_060,
                        liquidity="60000000000",
                    ),
                ],
                adds=[
                    ConsolidateAddItem(
                        pool=POOL_USDC_WETH_030,
                        amount0="150000000000",
                        amount1="50000000000000000000",
                        tick_lower=200_400,
                        tick_upper=203_400,
                    ),
                ],
            ),
            priority=self.config.opportunity_priority,
            max_slippage_bps=75,
            expected_profit_bps=80,
            salt=AgentIntentWire.random_salt(),
        )

    def _crisis_intent(self, epoch: int) -> AgentIntentWire:
        """Consolidate into wide-range safety position on 0.05%."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=BatchConsolidateAction(
                removes=[
                    ConsolidateRemoveItem(
                        pool=POOL_USDC_WETH_030,
                        liquidity="200000000000",
                    ),
                ],
                adds=[
                    ConsolidateAddItem(
                        pool=POOL_USDC_WETH_005,
                        amount0="200000000000",
                        amount1="66666666666666666666",
                        tick_lower=190_000,
                        tick_upper=210_000,
                    ),
                ],
            ),
            priority=self.config.crisis_priority,
            max_slippage_bps=500,
            expected_profit_bps=0,
            salt=AgentIntentWire.random_salt(),
        )


