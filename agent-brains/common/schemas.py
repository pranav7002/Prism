"""
Pydantic models for the AgentIntent wire format.

Validates against INTERFACES_FOR_DEV3.md §3.1. Every agent brain must
produce JSON matching these models before sending to the orchestrator.

The wire JSON uses:
  - hex strings for addresses (20B) and hashes (32B), 0x-prefixed
  - decimal strings for u128 amounts (to preserve precision)
  - PascalCase internally-tagged "type" for actions
  - integers for ticks, fee_ppm, priority, slippage
"""


import os
import re
import secrets
from typing import Any, Literal, Union

from pydantic import BaseModel, Field, field_validator, model_validator

from .commitment import compute_commitment_hex


# ---
#  Hex validators
# ---

_HEX_RE = re.compile(r"^0x[0-9a-fA-F]+$")


def _validate_hex(v: str, byte_len: int, field_name: str) -> str:
    if not _HEX_RE.match(v):
        raise ValueError(f"{field_name} must be 0x-prefixed hex, got: {v!r}")
    raw = v[2:]
    if len(raw) != byte_len * 2:
        raise ValueError(
            f"{field_name} must be {byte_len} bytes ({byte_len*2} hex chars), "
            f"got {len(raw)//2} bytes"
        )
    return v.lower()   # normalize to lowercase


# ---
#  Action models (one per variant)
# ---

class SwapAction(BaseModel):
    type: Literal["Swap"] = "Swap"
    pool: str
    token_in: str
    token_out: str
    amount_in: str    # decimal string (u128)
    min_out: str      # decimal string (u128)

    @field_validator("pool", "token_in", "token_out")
    @classmethod
    def validate_address(cls, v: str, info: Any) -> str:
        return _validate_hex(v, 20, info.field_name)

    @field_validator("amount_in", "min_out")
    @classmethod
    def validate_amount(cls, v: str, info: Any) -> str:
        try:
            val = int(v)
            if val < 0:
                raise ValueError
        except (ValueError, TypeError):
            raise ValueError(f"{info.field_name} must be a non-negative decimal string")
        return v


class AddLiquidityAction(BaseModel):
    type: Literal["AddLiquidity"] = "AddLiquidity"
    pool: str
    amount0: str
    amount1: str
    tick_lower: int
    tick_upper: int

    @field_validator("pool")
    @classmethod
    def validate_pool(cls, v: str) -> str:
        return _validate_hex(v, 20, "pool")

    @field_validator("amount0", "amount1")
    @classmethod
    def validate_amount(cls, v: str, info: Any) -> str:
        int(v)  # will raise on bad input
        return v


class RemoveLiquidityAction(BaseModel):
    type: Literal["RemoveLiquidity"] = "RemoveLiquidity"
    pool: str
    liquidity: str

    @field_validator("pool")
    @classmethod
    def validate_pool(cls, v: str) -> str:
        return _validate_hex(v, 20, "pool")

    @field_validator("liquidity")
    @classmethod
    def validate_amount(cls, v: str) -> str:
        int(v)
        return v


class BackrunAction(BaseModel):
    type: Literal["Backrun"] = "Backrun"
    target_tx: str
    profit_token: str

    @field_validator("target_tx")
    @classmethod
    def validate_tx(cls, v: str) -> str:
        return _validate_hex(v, 32, "target_tx")

    @field_validator("profit_token")
    @classmethod
    def validate_token(cls, v: str) -> str:
        return _validate_hex(v, 20, "profit_token")


class DeltaHedgeAction(BaseModel):
    type: Literal["DeltaHedge"] = "DeltaHedge"
    position_id: int
    delta: int


class MigrateLiquidityAction(BaseModel):
    type: Literal["MigrateLiquidity"] = "MigrateLiquidity"
    from_pool: str
    to_pool: str
    amount: str
    tick_lower: int
    tick_upper: int

    @field_validator("from_pool", "to_pool")
    @classmethod
    def validate_pool(cls, v: str, info: Any) -> str:
        return _validate_hex(v, 20, info.field_name)

    @field_validator("amount")
    @classmethod
    def validate_amount(cls, v: str) -> str:
        int(v)
        return v


class ConsolidateRemoveItem(BaseModel):
    pool: str
    liquidity: str

    @field_validator("pool")
    @classmethod
    def validate_pool(cls, v: str) -> str:
        return _validate_hex(v, 20, "pool")

    @field_validator("liquidity")
    @classmethod
    def validate_amount(cls, v: str) -> str:
        int(v)
        return v


class ConsolidateAddItem(BaseModel):
    pool: str
    amount0: str
    amount1: str
    tick_lower: int
    tick_upper: int

    @field_validator("pool")
    @classmethod
    def validate_pool(cls, v: str) -> str:
        return _validate_hex(v, 20, "pool")

    @field_validator("amount0", "amount1")
    @classmethod
    def validate_amount(cls, v: str, info: Any) -> str:
        int(v)
        return v


class BatchConsolidateAction(BaseModel):
    type: Literal["BatchConsolidate"] = "BatchConsolidate"
    removes: list[ConsolidateRemoveItem]
    adds: list[ConsolidateAddItem]


class SetDynamicFeeAction(BaseModel):
    type: Literal["SetDynamicFee"] = "SetDynamicFee"
    pool: str
    new_fee_ppm: int = Field(ge=500, le=10000)

    @field_validator("pool")
    @classmethod
    def validate_pool(cls, v: str) -> str:
        return _validate_hex(v, 20, "pool")


class CrossProtocolHedgeAction(BaseModel):
    type: Literal["CrossProtocolHedge"] = "CrossProtocolHedge"
    aave_borrow_asset: str
    aave_borrow_amount: str
    uniswap_pool: str
    uniswap_token_in: str
    uniswap_token_out: str
    uniswap_amount_in: str

    @field_validator(
        "aave_borrow_asset", "uniswap_pool",
        "uniswap_token_in", "uniswap_token_out",
    )
    @classmethod
    def validate_address(cls, v: str, info: Any) -> str:
        return _validate_hex(v, 20, info.field_name)

    @field_validator("aave_borrow_amount", "uniswap_amount_in")
    @classmethod
    def validate_amount(cls, v: str, info: Any) -> str:
        int(v)
        return v


class KillSwitchAction(BaseModel):
    type: Literal["KillSwitch"] = "KillSwitch"
    reason: str


# Discriminated union of all action types
ActionModel = Union[
    SwapAction,
    AddLiquidityAction,
    RemoveLiquidityAction,
    BackrunAction,
    DeltaHedgeAction,
    MigrateLiquidityAction,
    BatchConsolidateAction,
    SetDynamicFeeAction,
    CrossProtocolHedgeAction,
    KillSwitchAction,
]


# ---
#  AgentIntent wire model
# ---

class AgentIntentWire(BaseModel):
    """
    Wire-format AgentIntent matching INTERFACES_FOR_DEV3.md §3.1.

    Validates all field shapes and computes the keccak commitment.
    """
    agent_id: str
    epoch: int
    target_protocol: str = "Uniswap"
    action: ActionModel = Field(discriminator="type")
    priority: int = Field(ge=0, le=255)
    max_slippage_bps: int = Field(ge=0, le=10000)
    expected_profit_bps: int = Field(default=0, ge=0, le=10000)
    salt: str

    @field_validator("agent_id")
    @classmethod
    def validate_agent_id(cls, v: str) -> str:
        return _validate_hex(v, 20, "agent_id")

    @field_validator("salt")
    @classmethod
    def validate_salt(cls, v: str) -> str:
        return _validate_hex(v, 32, "salt")

    def compute_commitment(self) -> str:
        """
        Compute the keccak-256 commitment matching the Rust
        AgentIntent::compute_commitment encoding.

        Returns 0x-prefixed hex string.
        """
        return compute_commitment_hex(
            agent_id=self.agent_id,
            epoch=self.epoch,
            target_protocol=self.target_protocol,
            action=self.action.model_dump(),
            priority=self.priority,
            max_slippage_bps=self.max_slippage_bps,
            salt=self.salt,
        )

    def to_wire_json(self) -> dict:
        """Serialize to the JSON dict the orchestrator expects."""
        return self.model_dump(exclude_none=True)

    @classmethod
    def random_salt(cls) -> str:
        """Generate a random 32-byte salt as 0x-prefixed hex."""
        return "0x" + secrets.token_hex(32)
