"""
Cross-language commitment verification tests.

These tests reproduce the exact inputs from the Rust test suite
(crates/prism-types/examples/print_test_vector.rs) and verify that
the Python compute_commitment output matches byte-for-byte.

If any test fails, the Python commitment encoding has drifted from the
Rust source of truth — fix commitment.py, not these tests.
"""

import sys
import os

# Ensure the agent-brains root is on the path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from common.commitment import compute_commitment, bytes_to_hex
from common.schemas import (
    AgentIntentWire,
    SwapAction,
    AddLiquidityAction,
    KillSwitchAction,
)


# ---
#  Test vectors extracted from Rust via cargo run --example print_test_vector
# ---

# Vector 1: Swap (sample_intent from lib.rs tests)
SWAP_AGENT_ID = "0x" + "aa" * 20
SWAP_EPOCH = 42
SWAP_PROTOCOL = "Uniswap"
SWAP_ACTION = {
    "type": "Swap",
    "pool": "0x" + "dd" * 20,
    "token_in": "0x" + "11" * 20,
    "token_out": "0x" + "22" * 20,
    "amount_in": "1000000000000000000",
    "min_out": "330000000000000000",
}
SWAP_PRIORITY = 80
SWAP_SLIPPAGE = 50
SWAP_SALT = "0x" + "55" * 32
SWAP_EXPECTED = "0xf24824f303950f96e2be944f499483d2f81cb6926e6d3b058018c15059f8eafc"

# Vector 2: AddLiquidity (calm epoch α from mock_intents)
ADD_LIQ_AGENT_ID = "0x" + "a0" * 20
ADD_LIQ_EPOCH = 1
ADD_LIQ_PROTOCOL = "Uniswap"
ADD_LIQ_ACTION = {
    "type": "AddLiquidity",
    "pool": "0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8",
    "amount0": "300000000000",
    "amount1": "100000000000000000000",
    "tick_lower": 196800,
    "tick_upper": 200400,
}
ADD_LIQ_PRIORITY = 70
ADD_LIQ_SLIPPAGE = 50
# salt_for(0, 1): s[0] = 0, s[24..32] = 1u64.to_be_bytes()
ADD_LIQ_SALT = "0x" + "00" * 24 + "0000000000000001" + "00" * 0
# Needs exactly 32 bytes: byte 0 = 0x00, bytes 24-31 = epoch 1 BE
ADD_LIQ_SALT = "0x" + "00" * 24 + "0000000000000001"
ADD_LIQ_EXPECTED = "0x6d2d8844f4ecbee63a49ad89dd88c67fda335db05d1b4e98dcf17968f9019a9c"

# Vector 3: KillSwitch (crisis epoch ε from mock_intents)
KILL_AGENT_ID = "0x" + "a4" * 20
KILL_EPOCH = 3
KILL_PROTOCOL = "Uniswap"
KILL_ACTION = {
    "type": "KillSwitch",
    "reason": "swarm_IL_exceeds_2.5%_threshold",
}
KILL_PRIORITY = 100
KILL_SLIPPAGE = 0
# salt_for(5, 3): s[0] = 5, s[24..32] = 3u64.to_be_bytes()
KILL_SALT = "0x05" + "00" * 23 + "0000000000000003"
KILL_EXPECTED = "0x34893be3504733528a0777e503c98e272a871a4a4897e9adcb3d45a2da7f3359"


# ---
#  Tests
# ---

def test_swap_commitment_matches_rust():
    """Swap commitment must match Rust sample_intent() byte-for-byte."""
    result = compute_commitment(
        agent_id=SWAP_AGENT_ID,
        epoch=SWAP_EPOCH,
        target_protocol=SWAP_PROTOCOL,
        action=SWAP_ACTION,
        priority=SWAP_PRIORITY,
        max_slippage_bps=SWAP_SLIPPAGE,
        salt=SWAP_SALT,
    )
    assert bytes_to_hex(result) == SWAP_EXPECTED, (
        f"Swap mismatch:\n  got:      {bytes_to_hex(result)}\n  expected: {SWAP_EXPECTED}"
    )


def test_add_liquidity_commitment_matches_rust():
    """AddLiquidity commitment must match Rust calm epoch α."""
    result = compute_commitment(
        agent_id=ADD_LIQ_AGENT_ID,
        epoch=ADD_LIQ_EPOCH,
        target_protocol=ADD_LIQ_PROTOCOL,
        action=ADD_LIQ_ACTION,
        priority=ADD_LIQ_PRIORITY,
        max_slippage_bps=ADD_LIQ_SLIPPAGE,
        salt=ADD_LIQ_SALT,
    )
    assert bytes_to_hex(result) == ADD_LIQ_EXPECTED, (
        f"AddLiquidity mismatch:\n  got:      {bytes_to_hex(result)}\n  expected: {ADD_LIQ_EXPECTED}"
    )


def test_killswitch_commitment_matches_rust():
    """KillSwitch commitment must match Rust crisis epoch ε."""
    result = compute_commitment(
        agent_id=KILL_AGENT_ID,
        epoch=KILL_EPOCH,
        target_protocol=KILL_PROTOCOL,
        action=KILL_ACTION,
        priority=KILL_PRIORITY,
        max_slippage_bps=KILL_SLIPPAGE,
        salt=KILL_SALT,
    )
    assert bytes_to_hex(result) == KILL_EXPECTED, (
        f"KillSwitch mismatch:\n  got:      {bytes_to_hex(result)}\n  expected: {KILL_EXPECTED}"
    )


def test_commitment_via_schema_model():
    """AgentIntentWire.compute_commitment() must also match."""
    intent = AgentIntentWire(
        agent_id=SWAP_AGENT_ID,
        epoch=SWAP_EPOCH,
        target_protocol=SWAP_PROTOCOL,
        action=SwapAction(**SWAP_ACTION),
        priority=SWAP_PRIORITY,
        max_slippage_bps=SWAP_SLIPPAGE,
        expected_profit_bps=0,
        salt=SWAP_SALT,
    )
    assert intent.compute_commitment() == SWAP_EXPECTED


def test_commitment_changes_with_salt():
    """Different salt → different commitment (anti-grinding)."""
    c1 = compute_commitment(
        SWAP_AGENT_ID, SWAP_EPOCH, SWAP_PROTOCOL,
        SWAP_ACTION, SWAP_PRIORITY, SWAP_SLIPPAGE,
        SWAP_SALT,
    )
    c2 = compute_commitment(
        SWAP_AGENT_ID, SWAP_EPOCH, SWAP_PROTOCOL,
        SWAP_ACTION, SWAP_PRIORITY, SWAP_SLIPPAGE,
        "0x" + "66" * 32,
    )
    assert c1 != c2, "Different salts must produce different commitments"


def test_schema_rejects_bad_hex():
    """Pydantic must reject agent_id with wrong length."""
    import pytest
    with pytest.raises(Exception):
        AgentIntentWire(
            agent_id="0xDEAD",  # too short
            epoch=1,
            action=SwapAction(**SWAP_ACTION),
            priority=80,
            max_slippage_bps=50,
            salt=SWAP_SALT,
        )


def test_schema_rejects_missing_pool():
    """Swap without pool field must fail (schema drift guard)."""
    import pytest
    with pytest.raises(Exception):
        SwapAction(
            token_in="0x" + "11" * 20,
            token_out="0x" + "22" * 20,
            amount_in="1000",
            min_out="900",
            # no pool!
        )
