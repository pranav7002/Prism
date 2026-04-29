"""
Agent ε — Guardian brain.

Role: Delta-hedge via Aave; kill the pool if swarm IL > 2.5%.
Actions: CrossProtocolHedge, DeltaHedge, KillSwitch
Tripwire: Aggregate swarm IL crosses 2.5%.

Calm → Light DeltaHedge (inventory monitor)
Opportunity → Larger DeltaHedge (vol rising)
Crisis → CrossProtocolHedge + KillSwitch (emergency shutdown)

For the demo, crisis returns KillSwitch only (single intent per agent).
In production, ε would emit both CrossProtocolHedge and KillSwitch
as chained intents.
"""


import logging
from dataclasses import dataclass

from common.constants import (
    AGENT_EPSILON,
    POOL_USDC_WETH_030,
    TOKEN_USDC,
    TOKEN_WETH,
    TARGET_PROTOCOL,
)
from common.market_reader import PoolState, get_market_reader
from common.schemas import (
    AgentIntentWire,
    CrossProtocolHedgeAction,
    DeltaHedgeAction,
    KillSwitchAction,
)

logger = logging.getLogger(__name__)


@dataclass
class EpsilonConfig:
    agent_id: str = AGENT_EPSILON
    target_pool: str = POOL_USDC_WETH_030
    # DeltaHedge parameters
    calm_delta: int = -100      # light hedge
    opportunity_delta: int = -500  # moderate hedge
    position_id: int = 1
    # Crisis parameters
    kill_reason: str = "swarm_IL_exceeds_2.5%_threshold"
    aave_borrow_amount: int = 6_200_000_000_000_000_000  # 6.2 WETH
    # Priority
    calm_priority: int = 40
    opportunity_priority: int = 40
    crisis_priority: int = 100  # kill switch takes max priority


class EpsilonBrain:
    """
    Guardian decision engine.

    Monitors the swarm's aggregate impermanent loss and:
    - Calm: light delta-hedge to neutralize inventory risk
    - Opportunity: deeper hedge as volatility rises
    - Crisis: triggers kill switch to protect the swarm
    """

    def __init__(self, config: EpsilonConfig | None = None):
        self.config = config or EpsilonConfig()
        self.market = get_market_reader(use_mock=True)

    def decide(self, epoch: int) -> AgentIntentWire:
        state = self.market.get_pool_state(self.config.target_pool)
        scenario = self._classify(epoch, state)
        logger.info(
            f"ε epoch={epoch} scenario={scenario} "
            f"vol={state.volatility_30d_bps}bps"
        )

        if scenario == "calm":
            return self._calm_intent(epoch)
        elif scenario == "opportunity":
            return self._opportunity_intent(epoch)
        else:
            return self._crisis_intent(epoch)

    def _classify(self, epoch: int, state: PoolState) -> str:
        mod = epoch % 3
        if mod == 1:
            return "calm"
        elif mod == 2:
            return "opportunity"
        else:
            return "crisis"

    def _calm_intent(self, epoch: int) -> AgentIntentWire:
        """Light inventory hedge — monitor mode."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=DeltaHedgeAction(
                position_id=self.config.position_id,
                delta=self.config.calm_delta,
            ),
            priority=self.config.calm_priority,
            max_slippage_bps=50,
            expected_profit_bps=0,
            salt=AgentIntentWire.random_salt(),
        )

    def _opportunity_intent(self, epoch: int) -> AgentIntentWire:
        """Deeper hedge — vol is rising."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=DeltaHedgeAction(
                position_id=self.config.position_id,
                delta=self.config.opportunity_delta,
            ),
            priority=self.config.opportunity_priority,
            max_slippage_bps=50,
            expected_profit_bps=0,
            salt=AgentIntentWire.random_salt(),
        )

    def _crisis_intent(self, epoch: int) -> AgentIntentWire:
        """Kill switch — swarm IL has breached the 2.5% threshold."""
        return AgentIntentWire(
            agent_id=self.config.agent_id,
            epoch=epoch,
            target_protocol=TARGET_PROTOCOL,
            action=KillSwitchAction(
                reason=self.config.kill_reason,
            ),
            priority=self.config.crisis_priority,
            max_slippage_bps=0,
            expected_profit_bps=0,
            salt=AgentIntentWire.random_salt(),
        )


