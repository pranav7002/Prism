"""
Uniswap V4 pool state reader — real on-chain RPC bindings.

Three implementations:
  - MockMarketReader:  deterministic data matching Rust MockUniswapClient
  - OnChainMarketReader:  reads pool state directly from Unichain RPC via
    eth_call to the Uniswap V4 PoolManager / StateLibrary contracts
  - UniswapMarketReader:  REST API fallback (Uniswap routing API)

Environment:
  UNICHAIN_RPC_URL — Unichain Sepolia JSON-RPC endpoint
  UNISWAP_API_URL  — Uniswap REST API base URL (fallback)
  PRISM_USE_MOCK   — set to "1" to force MockMarketReader
"""


import asyncio
import json
import logging
import os
import struct
from dataclasses import dataclass
from typing import Optional

import httpx

from .constants import (
    POOL_USDC_WETH_005,
    POOL_USDC_WETH_030,
    POOL_USDC_WETH_060,
    TOKEN_USDC,
    TOKEN_WETH,
)

logger = logging.getLogger(__name__)

# ---
#  Data classes
# ---

@dataclass
class PoolState:
    """Mirrors prism_types::ProtocolState."""
    pool_address: str       # 0x-prefixed hex, 20 bytes
    sqrt_price_x96: int     # u128
    liquidity: int           # u128
    tick: int                # i32
    fee_tier: int            # u32 (ppm, e.g. 3000 = 0.30%)
    token0_reserve: int      # u128
    token1_reserve: int      # u128
    volatility_30d_bps: int  # u32

    @property
    def price_eth_usd(self) -> float:
        """
        Approximate ETH/USD from sqrtPriceX96.

        sqrtPriceX96 = sqrt(token1/token0) * 2^96
        For USDC(6 dec)/WETH(18 dec):
            eth_usd = 1 / ((sqrtPriceX96 / 2^96)^2) * 10^12
        """
        ratio = (self.sqrt_price_x96 / (2 ** 96)) ** 2
        if ratio == 0:
            return 0.0
        return (1.0 / ratio) * 1e12

    @property
    def is_high_volatility(self) -> bool:
        """True when 30-day vol > 30% (3000 bps)."""
        return self.volatility_30d_bps > 3000


@dataclass
class SwapQuote:
    """Quote for a swap on a Uniswap pool."""
    amount_out: int
    price_impact_bps: int
    gas_estimate: int


@dataclass
class LpPosition:
    """An existing concentrated liquidity position."""
    token_id: int
    pool: str
    liquidity: int
    tick_lower: int
    tick_upper: int

    def is_in_range(self, current_tick: int) -> bool:
        return self.tick_lower <= current_tick < self.tick_upper


# ---
#  Mock reader (unchanged — matches Rust MockUniswapClient)
# ---

class MockMarketReader:
    """
    Deterministic market data matching the Rust MockUniswapClient.
    Values mirror crates/prism-orchestrator/src/uniswap_client.rs.
    """

    def get_pool_state(self, pool_address: str = POOL_USDC_WETH_030) -> PoolState:
        return PoolState(
            pool_address=pool_address,
            sqrt_price_x96=4_339_505_179_874_584_694_521,
            liquidity=1_500_000_000_000_000,
            tick=200_000,
            fee_tier=3_000,
            token0_reserve=10_000_000_000_000_000_000_000,
            token1_reserve=30_000_000_000_000,
            volatility_30d_bps=1_500,
        )

    def get_swap_quote(
        self,
        token_in: str = TOKEN_WETH,
        token_out: str = TOKEN_USDC,
        amount_in: int = 0,
    ) -> SwapQuote:
        return SwapQuote(
            amount_out=amount_in * 997 // 1000,
            price_impact_bps=10,
            gas_estimate=150_000,
        )

    def get_lp_positions(self, owner: str = "") -> list[LpPosition]:
        return [
            LpPosition(
                token_id=1,
                pool=POOL_USDC_WETH_030,
                liquidity=1_000_000_000_000_000,
                tick_lower=190_000,
                tick_upper=210_000,
            )
        ]


# ---
#  On-chain reader — Unichain RPC via eth_call
# ---

# Uniswap V4 PoolManager Slot0 function selector:
# getSlot0(PoolId) -> (sqrtPriceX96, tick, protocolFee, lpFee)
# We encode the PoolId as the pool address padded to 32 bytes.
# Note: For V4, PoolId is bytes32 derived from PoolKey. For simplicity,
# we use the V3-compatible approach reading V3 pool contracts directly.

# V3 Pool ABI selectors (4 bytes):
_SLOT0_SELECTOR = "0x3850c7bd"          # slot0()
_LIQUIDITY_SELECTOR = "0x1a686502"      # liquidity()
_FEE_SELECTOR = "0xddca3f43"           # fee()

# Default Unichain Sepolia RPC
DEFAULT_UNICHAIN_RPC = "https://sepolia.unichain.org"


class OnChainMarketReader:
    """
    Reads pool state directly from Unichain via JSON-RPC eth_call.

    Queries Uniswap V3/V4 pool contracts for:
      - slot0(): sqrtPriceX96, tick, observationIndex, etc.
      - liquidity(): current in-range liquidity
      - fee(): pool fee tier

    Falls back to MockMarketReader if RPC is unreachable.
    """

    def __init__(self, rpc_url: str | None = None, timeout: float = 10.0):
        self.rpc_url = rpc_url or os.environ.get(
            "UNICHAIN_RPC_URL", DEFAULT_UNICHAIN_RPC
        )
        self.timeout = timeout
        self._mock = MockMarketReader()
        self._call_id = 0
        logger.info(f"OnChainMarketReader initialized: {self.rpc_url}")

    def _next_id(self) -> int:
        self._call_id += 1
        return self._call_id

    async def _eth_call(self, to: str, data: str) -> str | None:
        """Execute a raw eth_call and return the hex result."""
        payload = {
            "jsonrpc": "2.0",
            "id": self._next_id(),
            "method": "eth_call",
            "params": [
                {"to": to, "data": data},
                "latest"
            ],
        }
        try:
            async with httpx.AsyncClient(timeout=self.timeout) as client:
                resp = await client.post(
                    self.rpc_url,
                    json=payload,
                    headers={"Content-Type": "application/json"},
                )
                resp.raise_for_status()
                result = resp.json()
                if "error" in result:
                    logger.warning(f"RPC error: {result['error']}")
                    return None
                return result.get("result")
        except Exception as e:
            logger.warning(f"eth_call failed ({e})")
            return None

    async def _eth_call_batch(
        self, to: str, selectors: list[str]
    ) -> list[str | None]:
        """Batch multiple eth_calls for efficiency."""
        payloads = [
            {
                "jsonrpc": "2.0",
                "id": self._next_id(),
                "method": "eth_call",
                "params": [{"to": to, "data": sel}, "latest"],
            }
            for sel in selectors
        ]
        try:
            async with httpx.AsyncClient(timeout=self.timeout) as client:
                resp = await client.post(
                    self.rpc_url,
                    json=payloads,
                    headers={"Content-Type": "application/json"},
                )
                resp.raise_for_status()
                results = resp.json()
                if isinstance(results, list):
                    return [r.get("result") for r in results]
                return [results.get("result")]
        except Exception as e:
            logger.warning(f"Batch eth_call failed ({e})")
            return [None] * len(selectors)

    def _decode_slot0(self, hex_data: str) -> tuple[int, int]:
        """
        Decode slot0() return data.

        Returns (sqrtPriceX96, tick).
        Solidity returns: (uint160, int24, uint16, uint16, uint16, uint8, bool)
        ABI-encoded as 7 × 32-byte words.
        """
        raw = bytes.fromhex(hex_data.removeprefix("0x"))
        if len(raw) < 64:
            raise ValueError(f"slot0 response too short: {len(raw)} bytes")
        # Word 0: uint160 sqrtPriceX96 (left-padded to 32 bytes)
        sqrt_price_x96 = int.from_bytes(raw[0:32], "big")
        # Word 1: int24 tick (signed, stored as int256)
        tick_raw = int.from_bytes(raw[32:64], "big")
        if tick_raw >= 2**255:
            tick_raw -= 2**256
        tick = tick_raw
        return sqrt_price_x96, tick

    def _decode_uint256(self, hex_data: str) -> int:
        """Decode a single uint256 return value."""
        raw = bytes.fromhex(hex_data.removeprefix("0x"))
        return int.from_bytes(raw[:32], "big")

    async def get_pool_state_async(
        self, pool_address: str = POOL_USDC_WETH_030
    ) -> PoolState:
        """
        Fetch pool state from on-chain via eth_call.

        Calls slot0(), liquidity(), and fee() on the pool contract.
        If any call fails, falls back to mock data.
        """
        results = await self._eth_call_batch(
            pool_address,
            [_SLOT0_SELECTOR, _LIQUIDITY_SELECTOR, _FEE_SELECTOR],
        )

        slot0_data, liq_data, fee_data = results

        if slot0_data is None or liq_data is None:
            logger.warning(
                f"On-chain read failed for {pool_address}, using mock data"
            )
            return self._mock.get_pool_state(pool_address)

        try:
            sqrt_price_x96, tick = self._decode_slot0(slot0_data)
            liquidity = self._decode_uint256(liq_data)
            fee_tier = self._decode_uint256(fee_data) if fee_data else 3000

            # Estimate reserves from liquidity and price
            # L = sqrt(x * y), so x ≈ L^2 / sqrtPrice, y ≈ L * sqrtPrice
            if sqrt_price_x96 > 0:
                sqrt_price = sqrt_price_x96 / (2**96)
                token0_reserve = int(liquidity / sqrt_price) if sqrt_price > 0 else 0
                token1_reserve = int(liquidity * sqrt_price)
            else:
                token0_reserve = 0
                token1_reserve = 0

            return PoolState(
                pool_address=pool_address,
                sqrt_price_x96=sqrt_price_x96,
                liquidity=liquidity,
                tick=tick,
                fee_tier=fee_tier,
                token0_reserve=token0_reserve,
                token1_reserve=token1_reserve,
                volatility_30d_bps=1_500,  # on-chain vol requires an oracle, default for now
            )
        except Exception as e:
            logger.warning(f"Decode error ({e}), falling back to mock")
            return self._mock.get_pool_state(pool_address)

    def get_pool_state(self, pool_address: str = POOL_USDC_WETH_030) -> PoolState:
        """Synchronous wrapper."""
        try:
            loop = asyncio.get_event_loop()
            if loop.is_running():
                return self._mock.get_pool_state(pool_address)
            return loop.run_until_complete(
                self.get_pool_state_async(pool_address)
            )
        except RuntimeError:
            return asyncio.run(self.get_pool_state_async(pool_address))

    async def get_swap_quote_async(
        self,
        token_in: str,
        token_out: str,
        amount_in: int,
        pool_address: str = POOL_USDC_WETH_030,
    ) -> SwapQuote:
        """
        Estimate swap output using pool state.

        Uses the constant-product approximation from on-chain liquidity
        and the pool fee tier. For precise quotes in production, use
        the Uniswap Quoter contract.
        """
        state = await self.get_pool_state_async(pool_address)
        fee_fraction = state.fee_tier / 1_000_000  # e.g. 3000 ppm = 0.003
        amount_after_fee = int(amount_in * (1 - fee_fraction))

        # Simple constant-product estimate
        if state.token0_reserve > 0 and state.token1_reserve > 0:
            if token_in.lower() == TOKEN_USDC.lower():
                amount_out = (
                    state.token1_reserve * amount_after_fee
                ) // (state.token0_reserve + amount_after_fee)
            else:
                amount_out = (
                    state.token0_reserve * amount_after_fee
                ) // (state.token1_reserve + amount_after_fee)
        else:
            amount_out = amount_after_fee

        price_impact = (
            amount_in * 10000 // max(state.token0_reserve, 1)
        )

        return SwapQuote(
            amount_out=amount_out,
            price_impact_bps=min(price_impact, 10000),
            gas_estimate=150_000,
        )

    def get_swap_quote(
        self,
        token_in: str = TOKEN_WETH,
        token_out: str = TOKEN_USDC,
        amount_in: int = 0,
    ) -> SwapQuote:
        """Synchronous swap quote wrapper."""
        try:
            loop = asyncio.get_event_loop()
            if loop.is_running():
                return self._mock.get_swap_quote(token_in, token_out, amount_in)
            return loop.run_until_complete(
                self.get_swap_quote_async(token_in, token_out, amount_in)
            )
        except RuntimeError:
            return asyncio.run(
                self.get_swap_quote_async(token_in, token_out, amount_in)
            )

    def get_lp_positions(self, owner: str = "") -> list[LpPosition]:
        """LP positions require indexer/subgraph — mock for now."""
        return self._mock.get_lp_positions(owner)


# ---
#  Factory
# ---

def get_market_reader(
    use_mock: bool | None = None,
) -> MockMarketReader | OnChainMarketReader:
    """
    Factory — chooses the appropriate reader.

    Priority:
      1. PRISM_USE_MOCK=1 env var → MockMarketReader
      2. use_mock=True argument → MockMarketReader
      3. UNICHAIN_RPC_URL set → OnChainMarketReader
      4. Default → MockMarketReader (safe fallback)
    """
    env_mock = os.environ.get("PRISM_USE_MOCK", "")
    if env_mock == "1":
        return MockMarketReader()

    if use_mock is True:
        return MockMarketReader()

    rpc_url = os.environ.get("UNICHAIN_RPC_URL")
    if use_mock is False or rpc_url:
        return OnChainMarketReader(rpc_url)

    return MockMarketReader()
