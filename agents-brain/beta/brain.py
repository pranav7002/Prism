"""
Agent β — Fee Curator brain.

Role: Migrate liquidity to optimal fee tier as volatility shifts.
Actions: MigrateLiquidity, SetDynamicFee
Tripwire: Realized 50-block vol crosses fee-tier band.

Calm → SetDynamicFee (maintain 0.30%)
Opportunity → MigrateLiquidity (0.30% → 0.60% on vol spike)
Crisis → SetDynamicFee (ramp to 1.00% to capture vol premium)
"""


import logging
from dataclasses import dataclass

from common.constants import (
    AGENT_BETA,
    POOL_USDC_WETH_030,
    POOL_USDC_WETH_060,
    TARGET_PROTOCOL,
)
from common.market_reader import PoolState, get_market_reader
from common.schemas import (
    AgentIntentWire,
    MigrateLiquidityAction,
    SetDynamicFeeAction,
)

logger = logging.getLogger(__name__)


@dataclass
class BetaConfig:
    """Tunable parameters for Fee Curator."""
    agent_id: str = AGENT_BETA
    target_pool: str = POOL_USDC_WETH_030
    alt_pool: str = POOL_USDC_WETH_060
    # Fee tiers (in ppm = parts per million)
    calm_fee_ppm: int = 3_000       # 0.30%
    crisis_fee_ppm: int = 10_000    # 1.00%
    # Migration amount
    migrate_amount: int = 200_000_000_000  # 200k USDC units
    # Priority
    calm_priority: int = 65
    opportunity_priority: int = 75
    crisis_priority: int = 90
    # Vol threshold for migration (bps)
    vol_migrate_threshold: int = 2_500  # 25%


class BetaBrain:
    """
    Fee Curator decision engine.

    Reads volatility and adjusts fee tier or migrates liquidity
    to the optimal pool.
    """

    def __init__(self, config: BetaConfig | None = None):
        self.config = config or BetaConfig()
        self.market = get_market_reader(use_mock=True)

    def decide(self, epoch: int) -> AgentIntentWire:
        state = self.market.get_pool_state(self.config.target_pool)
        scenario = self._classify(epoch, state)
        logger.info(
            f"β epoch={epoch} scenario={scenario} "
            f"vol={state.volatility_30d_bps}bps"
        )

        if scenario == "calm":
            return self._calm_intent(epoch, state)
        elif scenario == "opportunity":
            return self._opportunity_intent(epoch, state)
        else:
            return self._crisis_intent(epoch, state)

    def _classify(self, epoch: int, state: PoolState) -> str:
        mod = epoch % 3
        if mod == 1:
            return "calm"
        elif mod == 2:
            return "opportunity"
        else:
            return "crisis"

    def _calm_intent(self, epoch: int, state: PoolState) -> AgentIntentWire:
        """Maintain 0.30% fee — keep the tier stable."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=SetDynamicFeeAction(
                pool=self.config.target_pool,
                new_fee_ppm=self.config.calm_fee_ppm,
            ),
            priority=self.config.calm_priority,
            max_slippage_bps=0,
            expected_profit_bps=20,
            salt=AgentIntentWire.random_salt(),
        )

    def _opportunity_intent(self, epoch: int, state: PoolState) -> AgentIntentWire:
        """Migrate liquidity from 0.30% to 0.60% pool on vol spike."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=MigrateLiquidityAction(
                from_pool=self.config.target_pool,
                to_pool=self.config.alt_pool,
                amount=str(self.config.migrate_amount),
                tick_lower=200_400,
                tick_upper=203_400,
            ),
            priority=self.config.opportunity_priority,
            max_slippage_bps=75,
            expected_profit_bps=100,
            salt=AgentIntentWire.random_salt(),
        )

    def _crisis_intent(self, epoch: int, state: PoolState) -> AgentIntentWire:
        """Ramp fee to 1.00% to capture volatility premium."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=SetDynamicFeeAction(
                pool=self.config.target_pool,
                new_fee_ppm=self.config.crisis_fee_ppm,
            ),
            priority=self.config.crisis_priority,
            max_slippage_bps=500,
            expected_profit_bps=0,
            salt=AgentIntentWire.random_salt(),
        )


