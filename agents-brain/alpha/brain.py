"""
Agent α — Predictive LP brain.

Role: Pre-position concentrated liquidity before the price moves.
Actions: AddLiquidity, RemoveLiquidity
Tripwire: Vol model forecasts move > 1% in 10 blocks.

This module reads market state, decides a liquidity action, and produces
a validated AgentIntentWire ready for the orchestrator.
"""


import logging
from dataclasses import dataclass

from common.constants import (
    AGENT_ALPHA,
    POOL_USDC_WETH_030,
    TARGET_PROTOCOL,
)
from common.market_reader import MockMarketReader, PoolState, get_market_reader
from common.schemas import (
    AddLiquidityAction,
    AgentIntentWire,
    RemoveLiquidityAction,
)

logger = logging.getLogger(__name__)


# ---
#  Configuration
# ---

@dataclass
class AlphaConfig:
    """Tunable parameters for the Predictive LP agent."""
    agent_id: str = AGENT_ALPHA
    target_pool: str = POOL_USDC_WETH_030
    # Liquidity amounts (USDC / WETH in token-smallest-units)
    default_amount0: int = 300_000_000_000        # 300k USDC (6 decimals)
    default_amount1: int = 100_000_000_000_000_000_000  # 100 WETH (18 decimals)
    # Tick range width (concentrated LP)
    tick_range_width: int = 3600    # ~6% range
    # Volatility threshold for repositioning (in bps)
    vol_threshold_bps: int = 2000   # 20% — above this, widen the range
    # Priority
    calm_priority: int = 70
    opportunity_priority: int = 85
    # Slippage
    max_slippage_bps: int = 50


# ---
#  Decision logic
# ---

class AlphaBrain:
    """
    Predictive LP decision engine.

    Reads pool state and decides whether to:
      - AddLiquidity in a tight range (calm market)
      - Reposition to a new range (opportunity — price moved)
      - RemoveLiquidity (crisis — vol spike, withdraw to safety)
    """

    def __init__(self, config: AlphaConfig | None = None):
        self.config = config or AlphaConfig()
        self.market = get_market_reader(use_mock=True)
        self._last_tick: int | None = None

    def decide(self, epoch: int) -> AgentIntentWire:
        """
        Produce one AgentIntentWire for this epoch.

        Uses the epoch-mod-3 pattern (calm/opportunity/crisis) to mirror
        the Rust mock_intents generator for demo purposes. In production,
        this would use the actual market state from the reader.
        """
        state = self.market.get_pool_state(self.config.target_pool)

        scenario = self._classify_scenario(epoch, state)
        logger.info(f"α epoch={epoch} scenario={scenario} tick={state.tick}")

        if scenario == "calm":
            return self._calm_intent(epoch, state)
        elif scenario == "opportunity":
            return self._opportunity_intent(epoch, state)
        else:
            return self._crisis_intent(epoch, state)

    def _classify_scenario(self, epoch: int, state: PoolState) -> str:
        """
        Classify the current epoch.

        For the demo, we follow the Rust mock_intents.rs pattern:
        epoch % 3 == 1 → calm, == 2 → opportunity, == 0 → crisis.

        In production, this would use vol forecasting + tick movement.
        """
        mod = epoch % 3
        if mod == 1:
            return "calm"
        elif mod == 2:
            return "opportunity"
        else:
            return "crisis"

    def _calm_intent(self, epoch: int, state: PoolState) -> AgentIntentWire:
        """Add concentrated liquidity in a tight range around current tick."""
        tick_lower = state.tick - self.config.tick_range_width // 2
        tick_upper = state.tick + self.config.tick_range_width // 2
        # Align to 60-tick spacing (0.30% fee tier)
        tick_lower = (tick_lower // 60) * 60
        tick_upper = (tick_upper // 60) * 60

        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=AddLiquidityAction(
                pool=self.config.target_pool,
                amount0=str(self.config.default_amount0),
                amount1=str(self.config.default_amount1),
                tick_lower=tick_lower,
                tick_upper=tick_upper,
            ),
            priority=self.config.calm_priority,
            max_slippage_bps=self.config.max_slippage_bps,
            expected_profit_bps=50,
            salt=AgentIntentWire.random_salt(),
        )

    def _opportunity_intent(self, epoch: int, state: PoolState) -> AgentIntentWire:
        """Reposition LP into a higher range (price moved up)."""
        # Shift range up by half the width
        offset = self.config.tick_range_width // 2
        tick_lower = state.tick + offset
        tick_upper = tick_lower + self.config.tick_range_width
        tick_lower = (tick_lower // 60) * 60
        tick_upper = (tick_upper // 60) * 60

        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=AddLiquidityAction(
                pool=self.config.target_pool,
                amount0=str(self.config.default_amount0),
                amount1=str(self.config.default_amount1),
                tick_lower=tick_lower,
                tick_upper=tick_upper,
            ),
            priority=self.config.opportunity_priority,
            max_slippage_bps=self.config.max_slippage_bps,
            expected_profit_bps=150,
            salt=AgentIntentWire.random_salt(),
        )

    def _crisis_intent(self, epoch: int, state: PoolState) -> AgentIntentWire:
        """Remove liquidity — vol spike, withdraw to safety."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=RemoveLiquidityAction(
                pool=self.config.target_pool,
                liquidity=str(self.config.default_amount0),  # withdraw all
            ),
            priority=95,   # high priority in crisis
            max_slippage_bps=500,  # wider slippage tolerance in crisis
            expected_profit_bps=0,
            salt=AgentIntentWire.random_salt(),
        )


# ---
#  CLI entry point
# ---

