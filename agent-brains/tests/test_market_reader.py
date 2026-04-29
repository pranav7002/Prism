"""
Tests for the on-chain market reader.

Includes mock-mode tests and an optional live RPC test
(skipped if UNICHAIN_RPC_URL is not set).
"""

import sys
import os
import asyncio

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import pytest
from common.market_reader import (
    MockMarketReader,
    OnChainMarketReader,
    get_market_reader,
    PoolState,
)


class TestMockReader:
    """Existing mock tests."""

    def test_mock_pool_state_matches_rust(self):
        reader = MockMarketReader()
        state = reader.get_pool_state("0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8")
        assert state.fee_tier == 3_000
        assert state.liquidity == 1_500_000_000_000_000
        assert state.tick == 200_000

    def test_mock_swap_quote_applies_03_fee(self):
        reader = MockMarketReader()
        quote = reader.get_swap_quote(amount_in=1_000_000)
        assert quote.amount_out == 997_000


class TestOnChainReader:
    """Tests for the OnChainMarketReader."""

    def test_decode_slot0_valid(self):
        """Test slot0 decoding with known data."""
        reader = OnChainMarketReader(rpc_url="http://dummy:8545")

        # 7 words of 32 bytes each = 224 bytes
        # Word 0: sqrtPriceX96 = 1000 (big-endian, 32 bytes)
        # Word 1: tick = -100 (signed i256)
        sqrt_price = (1000).to_bytes(32, "big")
        tick_neg = (2**256 - 100).to_bytes(32, "big")  # -100 as 2's complement
        padding = b"\x00" * (32 * 5)  # 5 more empty words
        hex_data = "0x" + (sqrt_price + tick_neg + padding).hex()

        sp, tick = reader._decode_slot0(hex_data)
        assert sp == 1000
        assert tick == -100

    def test_decode_uint256(self):
        reader = OnChainMarketReader(rpc_url="http://dummy:8545")
        val = (42).to_bytes(32, "big")
        assert reader._decode_uint256("0x" + val.hex()) == 42

    def test_fallback_on_bad_rpc(self):
        """OnChainMarketReader should fall back to mock on unreachable RPC."""
        reader = OnChainMarketReader(
            rpc_url="http://127.0.0.1:19999",  # no server here
            timeout=1.0,
        )
        state = reader.get_pool_state()
        # Should return mock data (not crash)
        assert state.fee_tier == 3_000
        assert state.liquidity == 1_500_000_000_000_000

    def test_factory_respects_mock_env(self):
        """PRISM_USE_MOCK=1 forces mock reader."""
        os.environ["PRISM_USE_MOCK"] = "1"
        try:
            reader = get_market_reader(use_mock=None)
            assert isinstance(reader, MockMarketReader)
        finally:
            del os.environ["PRISM_USE_MOCK"]

    def test_factory_creates_onchain_when_requested(self):
        reader = get_market_reader(use_mock=False)
        assert isinstance(reader, OnChainMarketReader)


class TestPoolIdEncoding:
    """H13-full: validate PoolKey → keccak256 derivation."""

    def test_pool_id_for_tracked_pool_matches_canonical_v4_encoding(self):
        """
        Independent re-encoding of PoolKey via eth-abi should match the
        helper's output. This pins the call shape so any drift in eth-abi's
        struct encoding becomes a visible test failure rather than a silent
        on-chain RPC miss.
        """
        from common.market_reader import pool_id_for
        from common.constants import (
            POOL_USDC_WETH_030,
            TRACKED_POOLS,
        )
        from eth_abi import encode as abi_encode
        from eth_utils import keccak as keccak256

        key = TRACKED_POOLS[POOL_USDC_WETH_030]
        expected = keccak256(
            abi_encode(
                ["address", "address", "uint24", "int24", "address"],
                [key.currency0, key.currency1, key.fee, key.tick_spacing, key.hooks],
            )
        )
        actual = pool_id_for(POOL_USDC_WETH_030)
        assert actual == expected
        assert len(actual) == 32

    def test_pool_id_for_untracked_pool_falls_back_to_padded_address(self):
        """Pools without a registered PoolKey use the legacy zero-padded shape."""
        from common.market_reader import pool_id_for

        addr = "0x" + "ab" * 20
        result = pool_id_for(addr)
        assert len(result) == 32
        # First 12 bytes are zero padding, last 20 are the address.
        assert result[:12] == b"\x00" * 12
        assert result[12:].hex() == "ab" * 20

    def test_pool_id_is_deterministic(self):
        from common.market_reader import pool_id_for
        from common.constants import POOL_USDC_WETH_005

        a = pool_id_for(POOL_USDC_WETH_005)
        b = pool_id_for(POOL_USDC_WETH_005)
        assert a == b


class TestLiveRPC:
    """Live RPC tests — only run when UNICHAIN_RPC_URL is set."""

    @pytest.mark.skipif(
        not os.environ.get("UNICHAIN_RPC_URL"),
        reason="UNICHAIN_RPC_URL not set"
    )
    def test_live_pool_state(self):
        """Fetch real pool state from Unichain."""
        reader = OnChainMarketReader()
        state = asyncio.run(reader.get_pool_state_async(
            "0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8"
        ))
        assert state.sqrt_price_x96 > 0
        assert state.liquidity > 0
        print(f"Live state: tick={state.tick} liq={state.liquidity}")
