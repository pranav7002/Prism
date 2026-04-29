"""
Python port of AgentIntent::compute_commitment() from prism-types.

Canonical encoding is big-endian packed:

    agent_id (20B) || epoch (u64 BE, 8B)
    || target_protocol_len (u32 BE, 4B) || target_protocol (UTF-8)
    || action_discriminator (1B) || action_fields (BE packed)
    || priority (u8, 1B) || max_slippage_bps (u16 BE, 2B)
    || salt (32B)

Hashed with keccak-256. The Rust source (lib.rs) is the ground truth;
if this module disagrees, the Rust side wins.
"""


import struct
from typing import Any

from Crypto.Hash import keccak as _pycryptodome_keccak

from .constants import (
    DISC_ADD_LIQUIDITY,
    DISC_BACKRUN,
    DISC_BATCH_CONSOLIDATE,
    DISC_CROSS_PROTOCOL_HEDGE,
    DISC_DELTA_HEDGE,
    DISC_KILL_SWITCH,
    DISC_MIGRATE_LIQUIDITY,
    DISC_REMOVE_LIQUIDITY,
    DISC_SET_DYNAMIC_FEE,
    DISC_SWAP,
)


# --- Hashing & hex helpers ---

def keccak256(data: bytes) -> bytes:
    """Compute keccak-256 using PyCryptodome (Crypto.Hash.keccak)."""
    h = _pycryptodome_keccak.new(digest_bits=256)
    h.update(data)
    return h.digest()


def hex_to_bytes(h: str, expected_len: int, field: str = "") -> bytes:
    """Decode a 0x-prefixed hex string to exact-length bytes."""
    raw = h.removeprefix("0x").removeprefix("0X")
    b = bytes.fromhex(raw)
    if len(b) != expected_len:
        raise ValueError(
            f"Bad length for {field or 'field'}: "
            f"expected {expected_len}, got {len(b)}"
        )
    return b


def bytes_to_hex(b: bytes) -> str:
    """Encode bytes as 0x-prefixed lowercase hex."""
    return "0x" + b.hex()


# --- Action discriminator lookup ---

_ACTION_DISCRIMINATORS = {
    "Swap":               DISC_SWAP,
    "AddLiquidity":       DISC_ADD_LIQUIDITY,
    "RemoveLiquidity":    DISC_REMOVE_LIQUIDITY,
    "Backrun":            DISC_BACKRUN,
    "DeltaHedge":         DISC_DELTA_HEDGE,
    "MigrateLiquidity":   DISC_MIGRATE_LIQUIDITY,
    "BatchConsolidate":   DISC_BATCH_CONSOLIDATE,
    "SetDynamicFee":      DISC_SET_DYNAMIC_FEE,
    "CrossProtocolHedge": DISC_CROSS_PROTOCOL_HEDGE,
    "KillSwitch":         DISC_KILL_SWITCH,
}


# --- Action packed encoding ---

def _encode_action_packed(action: dict[str, Any]) -> bytes:
    """
    Encode an action dict (wire JSON shape) into the packed byte format
    used by the Rust commitment.

    The dict must have a ``"type"`` key (PascalCase) plus the variant's
    fields. Amounts are decimal strings; addresses/hashes are hex strings.
    """
    action_type = action["type"]
    disc = _ACTION_DISCRIMINATORS.get(action_type)
    if disc is None:
        raise ValueError(f"Unknown action type: {action_type}")

    buf = bytearray()
    buf.append(disc)

    if action_type == "Swap":
        buf += hex_to_bytes(action["pool"], 20, "pool")
        buf += hex_to_bytes(action["token_in"], 20, "token_in")
        buf += hex_to_bytes(action["token_out"], 20, "token_out")
        buf += int(action["amount_in"]).to_bytes(16, "big")
        buf += int(action["min_out"]).to_bytes(16, "big")

    elif action_type == "AddLiquidity":
        buf += hex_to_bytes(action["pool"], 20, "pool")
        buf += int(action["amount0"]).to_bytes(16, "big")
        buf += int(action["amount1"]).to_bytes(16, "big")
        buf += struct.pack(">i", action["tick_lower"])
        buf += struct.pack(">i", action["tick_upper"])

    elif action_type == "RemoveLiquidity":
        buf += hex_to_bytes(action["pool"], 20, "pool")
        buf += int(action["liquidity"]).to_bytes(16, "big")

    elif action_type == "Backrun":
        buf += hex_to_bytes(action["target_tx"], 32, "target_tx")
        buf += hex_to_bytes(action["profit_token"], 20, "profit_token")

    elif action_type == "DeltaHedge":
        buf += struct.pack(">Q", action["position_id"])   # u64 BE
        buf += struct.pack(">q", action["delta"])          # i64 BE

    elif action_type == "MigrateLiquidity":
        buf += hex_to_bytes(action["from_pool"], 20, "from_pool")
        buf += hex_to_bytes(action["to_pool"], 20, "to_pool")
        buf += int(action["amount"]).to_bytes(16, "big")
        buf += struct.pack(">i", action["tick_lower"])
        buf += struct.pack(">i", action["tick_upper"])

    elif action_type == "BatchConsolidate":
        removes = action["removes"]
        adds = action["adds"]

        buf += struct.pack(">I", len(removes))
        for r in removes:
            buf += hex_to_bytes(r["pool"], 20, "pool")
            buf += int(r["liquidity"]).to_bytes(16, "big")

        buf += struct.pack(">I", len(adds))
        for a in adds:
            buf += hex_to_bytes(a["pool"], 20, "pool")
            buf += int(a["amount0"]).to_bytes(16, "big")
            buf += int(a["amount1"]).to_bytes(16, "big")
            buf += struct.pack(">i", a["tick_lower"])
            buf += struct.pack(">i", a["tick_upper"])

    elif action_type == "SetDynamicFee":
        buf += hex_to_bytes(action["pool"], 20, "pool")
        buf += struct.pack(">I", action["new_fee_ppm"])

    elif action_type == "CrossProtocolHedge":
        buf += hex_to_bytes(action["aave_borrow_asset"], 20, "aave_borrow_asset")
        buf += int(action["aave_borrow_amount"]).to_bytes(16, "big")
        buf += hex_to_bytes(action["uniswap_pool"], 20, "uniswap_pool")
        buf += hex_to_bytes(action["uniswap_token_in"], 20, "uniswap_token_in")
        buf += hex_to_bytes(action["uniswap_token_out"], 20, "uniswap_token_out")
        buf += int(action["uniswap_amount_in"]).to_bytes(16, "big")

    elif action_type == "KillSwitch":
        reason_bytes = action["reason"].encode("utf-8")
        buf += struct.pack(">I", len(reason_bytes))
        buf += reason_bytes

    return bytes(buf)


# --- Compute commitment ---

def compute_commitment(
    agent_id: str,    # "0x..." 20-byte hex
    epoch: int,       # u64
    target_protocol: str,
    action: dict[str, Any],
    priority: int,    # u8
    max_slippage_bps: int,  # u16
    salt: str,        # "0x..." 32-byte hex
) -> bytes:
    """
    Reproduce AgentIntent::compute_commitment from prism-types.
    Returns the 32-byte keccak-256 hash.
    """
    buf = bytearray()

    # agent_id (20 bytes)
    buf += hex_to_bytes(agent_id, 20, "agent_id")

    # epoch (u64 big-endian, 8 bytes)
    buf += struct.pack(">Q", epoch)

    # target_protocol: len (u32 BE) + UTF-8 bytes
    proto_bytes = target_protocol.encode("utf-8")
    buf += struct.pack(">I", len(proto_bytes))
    buf += proto_bytes

    # action: discriminator + packed fields
    buf += _encode_action_packed(action)

    # priority (u8), max_slippage_bps (u16 BE), salt (32B)
    buf.append(priority & 0xFF)
    buf += struct.pack(">H", max_slippage_bps)
    buf += hex_to_bytes(salt, 32, "salt")

    return keccak256(bytes(buf))


def compute_commitment_hex(
    agent_id: str,
    epoch: int,
    target_protocol: str,
    action: dict[str, Any],
    priority: int,
    max_slippage_bps: int,
    salt: str,
) -> str:
    """Same as compute_commitment but returns 0x-prefixed hex string."""
    return bytes_to_hex(
        compute_commitment(
            agent_id, epoch, target_protocol, action,
            priority, max_slippage_bps, salt,
        )
    )
