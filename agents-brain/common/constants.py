"""
Shared addresses, token constants, and protocol config.

Values here must match mock_intents.rs on the Rust side to keep
commitment hashes consistent across languages.
"""

# Agent IDs — 20-byte placeholders matching Rust orchestrator mocks.
# In production these become the real ECDSA addresses from wallets.py.
AGENT_ALPHA   = "0x" + "a0" * 20
AGENT_BETA    = "0x" + "a1" * 20
AGENT_GAMMA   = "0x" + "a2" * 20
AGENT_DELTA   = "0x" + "a3" * 20
AGENT_EPSILON = "0x" + "a4" * 20

AGENT_LABELS = {
    AGENT_ALPHA:   "α",
    AGENT_BETA:    "β",
    AGENT_GAMMA:   "γ",
    AGENT_DELTA:   "δ",
    AGENT_EPSILON: "ε",
}

# --- Pool addresses (Uniswap V3 mainnet canonical, used in mocks) ---

POOL_USDC_WETH_005 = "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"
POOL_USDC_WETH_030 = "0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8"
POOL_USDC_WETH_060 = "0x7bea39867e426681f6a1127cff9e65bf638fb29e"

# Byte arrays for the commitment hash — these have to match the Rust
# side exactly or the keccak commitments diverge.
POOL_USDC_WETH_005_BYTES = bytes([
    0x88, 0xe6, 0xa0, 0xc2, 0xdd, 0xd2, 0x6f, 0xee, 0xb6, 0x4f,
    0x03, 0x9a, 0x2c, 0x41, 0x29, 0x6f, 0xcb, 0x3f, 0x56, 0x40,
])
POOL_USDC_WETH_030_BYTES = bytes([
    0x8a, 0xd5, 0x99, 0xc3, 0xa0, 0xff, 0x1d, 0xe0, 0x82, 0x01,
    0x1e, 0xfd, 0xdc, 0x58, 0xf1, 0x90, 0x8e, 0xb6, 0xe6, 0xd8,
])
POOL_USDC_WETH_060_BYTES = bytes([
    0x7b, 0xea, 0x39, 0x86, 0x7e, 0x42, 0x66, 0x81, 0xf6, 0xa1,
    0x12, 0x7c, 0xff, 0x9e, 0x65, 0xbf, 0x63, 0x8f, 0xb2, 0x9e,
])

# --- Token addresses ---

TOKEN_USDC = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
TOKEN_WETH = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"

TOKEN_USDC_BYTES = bytes([
    0xa0, 0xb8, 0x69, 0x91, 0xc6, 0x21, 0x8b, 0x36, 0xc1, 0xd1,
    0x9d, 0x4a, 0x2e, 0x9e, 0xb0, 0xce, 0x36, 0x06, 0xeb, 0x48,
])
TOKEN_WETH_BYTES = bytes([
    0xc0, 0x2a, 0xaa, 0x39, 0xb2, 0x23, 0xfe, 0x8d, 0x0a, 0x0e,
    0x5c, 0x4f, 0x27, 0xea, 0xd9, 0x08, 0x3c, 0x75, 0x6c, 0xc2,
])

# --- Protocol config ---

TARGET_PROTOCOL = "Uniswap"
EPOCH_DURATION_SECS = 12
WS_DEFAULT_URL = "ws://localhost:8765"

# Action discriminators — must match Action::discriminator() in
# crates/prism-types/src/lib.rs. Adding a new action type? Add it
# here AND in commitment.py's _ACTION_DISCRIMINATORS.
DISC_SWAP                 = 0x01
DISC_ADD_LIQUIDITY        = 0x02
DISC_REMOVE_LIQUIDITY     = 0x03
DISC_BACKRUN              = 0x04
DISC_DELTA_HEDGE          = 0x05
DISC_MIGRATE_LIQUIDITY    = 0x06
DISC_BATCH_CONSOLIDATE    = 0x07
DISC_SET_DYNAMIC_FEE      = 0x08
DISC_CROSS_PROTOCOL_HEDGE = 0x09
DISC_KILL_SWITCH          = 0xFF
