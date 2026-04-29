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


# Helper: salt_with(byte0, epoch) — mirrors Rust `salt_with` in
# print_test_vector.rs. Byte 0 = byte0; bytes 24..32 = epoch (u64 BE).
def _salt_with(byte0: int, epoch: int) -> str:
    s = bytearray(32)
    s[0] = byte0
    s[24:32] = epoch.to_bytes(8, "big")
    return "0x" + s.hex()


# --- M9: Vectors 4-10 — closes cross-language parity for the remaining
# 7 Action variants. Inputs mirror crates/prism-types/examples/print_test_vector.rs ---

# Vector 4: RemoveLiquidity
REMOVE_LIQ = dict(
    agent_id="0x" + "a1" * 20,
    epoch=2,
    target_protocol="Uniswap",
    action={
        "type": "RemoveLiquidity",
        "pool": "0x" + "de" * 20,
        "liquidity": "50000000000",
    },
    priority=65,
    max_slippage_bps=100,
    salt=_salt_with(1, 2),
    expected="0xe9101649b8bb2daf87fae069f5edc20dd35c1fc0700c17514497fe0b156857fd",
)

# Vector 5: Backrun
BACKRUN = dict(
    agent_id="0x" + "a2" * 20,
    epoch=2,
    target_protocol="Uniswap",
    action={
        "type": "Backrun",
        "target_tx": "0x" + "be" * 32,
        "profit_token": "0x" + "11" * 20,
    },
    priority=90,
    max_slippage_bps=200,
    salt=_salt_with(2, 2),
    expected="0xcfc3974ea358c0018a541bf236dc3b91304d40502c0d054fc2aafe473af16397",
)

# Vector 6: DeltaHedge
DELTA_HEDGE = dict(
    agent_id="0x" + "a3" * 20,
    epoch=2,
    target_protocol="Uniswap",
    action={
        "type": "DeltaHedge",
        "position_id": 0xCAFEBABEDEADBEEF,
        "delta": -123_456_789_012_345,
    },
    priority=40,
    max_slippage_bps=50,
    salt=_salt_with(3, 2),
    expected="0xbb7e3c9bbf3a4677d456741c161d21116a262414830fd5c5dceaa16551c13ec6",
)

# Vector 7: MigrateLiquidity
MIGRATE_LIQ = dict(
    agent_id="0x" + "a5" * 20,
    epoch=2,
    target_protocol="Uniswap",
    action={
        "type": "MigrateLiquidity",
        "from_pool": "0x" + "cc" * 20,
        "to_pool": "0x" + "ee" * 20,
        "amount": "200000000000",
        "tick_lower": 200_400,
        "tick_upper": 203_400,
    },
    priority=75,
    max_slippage_bps=75,
    salt=_salt_with(4, 2),
    expected="0x81b6afef5fac2d4350985e62e47758dccc819593823f6ce3c9d11faa38da2751",
)

# Vector 8: BatchConsolidate
BATCH_CONSOLIDATE = dict(
    agent_id="0x" + "a6" * 20,
    epoch=2,
    target_protocol="Uniswap",
    action={
        "type": "BatchConsolidate",
        "removes": [
            {"pool": "0x" + "10" * 20, "liquidity": "15000000000"},
            {"pool": "0x" + "20" * 20, "liquidity": "30000000000"},
        ],
        "adds": [
            {
                "pool": "0x" + "30" * 20,
                "amount0": "10000000000",
                "amount1": "5000000000000000000",
                "tick_lower": 199_800,
                "tick_upper": 201_000,
            },
        ],
    },
    priority=55,
    max_slippage_bps=100,
    salt=_salt_with(5, 2),
    expected="0x7557a6d19ee882b94798070c3ef68db526684af70633e00655da67877bbcba83",
)

# Vector 9: SetDynamicFee
SET_DYNAMIC_FEE = dict(
    agent_id="0x" + "a7" * 20,
    epoch=1,
    target_protocol="Uniswap",
    action={
        "type": "SetDynamicFee",
        "pool": "0x" + "dd" * 20,
        "new_fee_ppm": 6_000,
    },
    priority=65,
    max_slippage_bps=20,
    salt=_salt_with(6, 1),
    expected="0xa0cdc03be3df872b26a6ee2f354c9e408b9728a3dc156e065c02e6691ba67ba2",
)

# Vector 10: CrossProtocolHedge
CROSS_PROTOCOL_HEDGE = dict(
    agent_id="0x" + "a8" * 20,
    epoch=3,
    target_protocol="Uniswap",
    action={
        "type": "CrossProtocolHedge",
        "aave_borrow_asset": "0x" + "44" * 20,
        "aave_borrow_amount": "6200000000000000000",
        "uniswap_pool": "0x" + "dd" * 20,
        "uniswap_token_in": "0x" + "44" * 20,
        "uniswap_token_out": "0x" + "55" * 20,
        "uniswap_amount_in": "6200000000000000000",
    },
    priority=85,
    max_slippage_bps=500,
    salt=_salt_with(7, 3),
    expected="0x50d7afed09c2dc8464bf2654ec87a5391c08d2b839be85eeafdc45e12136635b",
)


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


# --- M9 parametrized cases — all 7 remaining variants, byte-for-byte vs Rust ---

import pytest

_M9_CASES = [
    pytest.param(REMOVE_LIQ, id="RemoveLiquidity_0x03"),
    pytest.param(BACKRUN, id="Backrun_0x04"),
    pytest.param(DELTA_HEDGE, id="DeltaHedge_0x05"),
    pytest.param(MIGRATE_LIQ, id="MigrateLiquidity_0x06"),
    pytest.param(BATCH_CONSOLIDATE, id="BatchConsolidate_0x07"),
    pytest.param(SET_DYNAMIC_FEE, id="SetDynamicFee_0x08"),
    pytest.param(CROSS_PROTOCOL_HEDGE, id="CrossProtocolHedge_0x09"),
]


@pytest.mark.parametrize("vec", _M9_CASES)
def test_commitment_matches_rust_for_remaining_actions(vec):
    """Cross-language commitment parity for all 7 actions M9 left untested."""
    result = compute_commitment(
        agent_id=vec["agent_id"],
        epoch=vec["epoch"],
        target_protocol=vec["target_protocol"],
        action=vec["action"],
        priority=vec["priority"],
        max_slippage_bps=vec["max_slippage_bps"],
        salt=vec["salt"],
    )
    got = bytes_to_hex(result)
    assert got == vec["expected"], (
        f"\n  action: {vec['action']['type']}"
        f"\n  got:      {got}"
        f"\n  expected: {vec['expected']}"
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
