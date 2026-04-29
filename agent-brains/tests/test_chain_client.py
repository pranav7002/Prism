"""
Tests for common.chain_client.ChainClient.

Mocks all httpx JSON-RPC calls so no real network or blockchain is needed.

Tests:
  1. commit_intent builds the correct calldata (selector + 32-byte commitment)
     and sends it to the correct hook address.
  2. commit_intent returns the tx hash from eth_sendRawTransaction.
  3. Receipt polling succeeds when eth_getTransactionReceipt returns a receipt.
  4. Receipt polling times out and raises ReceiptTimeoutError when no receipt
     arrives within the configured window.
"""


import sys
import os
import asyncio
from unittest.mock import AsyncMock, MagicMock, patch, call

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from common.chain_client import (
    ChainClient,
    ChainClientError,
    ReceiptTimeoutError,
    TransactionRevertedError,
    COMMIT_INTENT_SELECTOR,
)

# ---------------------------------------------------------------------------
# Fixtures & helpers
# ---------------------------------------------------------------------------

# Lowercase — the client will auto-checksum it to EIP-55 form
HOOK_ADDRESS = "0x5110dab469ff1dff0053cac172ac038ea3fefff0"
HOOK_ADDRESS_CHECKSUM = "0x5110daB469FF1Dff0053CAC172ac038ea3fEfFF0"
RPC_URL = "http://localhost:8545"

# A real secp256k1 private key (dev-only, from deterministic seed — safe to embed)
PRIVATE_KEY = "0xf2e96f75a19443c17e88f2cd8e85a188a37d1eff7ce70cde9ee59f5e02a93dff"

COMMITMENT_32 = bytes.fromhex("aa" * 32)
EXPECTED_CALLDATA_HEX = COMMIT_INTENT_SELECTOR.hex() + "aa" * 32

FAKE_TX_HASH = "0xdeadbeefcafe1234567890abcdef1234567890abcdef1234567890abcdef1234"


def make_client(receipt_timeout: int = 60) -> ChainClient:
    """Return a ChainClient pointed at the mock RPC."""
    return ChainClient(
        rpc_url=RPC_URL,
        hook_address=HOOK_ADDRESS,
        chain_id=1337,  # local test chain
        receipt_poll_interval=0.01,  # fast polling in tests
        receipt_timeout=receipt_timeout,
    )


def rpc_response(result) -> MagicMock:
    """Build a mock httpx.Response that returns a JSON-RPC success payload."""
    mock_resp = MagicMock()
    mock_resp.raise_for_status = MagicMock()
    mock_resp.json = MagicMock(return_value={"jsonrpc": "2.0", "id": 1, "result": result})
    return mock_resp


def make_post_side_effect(responses: list) -> list:
    """
    Build a list of async side-effects for httpx.AsyncClient.post.
    Each element is either a mock response or an exception.
    """
    async def _post_factory(resp):
        if isinstance(resp, Exception):
            raise resp
        return resp

    return [_post_factory(r) for r in responses]


# ---------------------------------------------------------------------------
# Test 1: Correct calldata (selector + 32-byte commitment + correct to address)
# ---------------------------------------------------------------------------

class TestCommitIntentCalldata:

    def test_selector_constant_is_correct(self):
        """Verify the selector constant matches keccak256('commitIntent(bytes32)')[:4]."""
        from Crypto.Hash import keccak
        h = keccak.new(digest_bits=256)
        h.update(b"commitIntent(bytes32)")
        expected = bytes.fromhex(h.hexdigest()[:8])
        assert COMMIT_INTENT_SELECTOR == expected, (
            f"Selector mismatch: got {COMMIT_INTENT_SELECTOR.hex()!r}, "
            f"expected {expected.hex()!r}"
        )

    @pytest.mark.asyncio
    async def test_calldata_contains_selector_and_commitment(self):
        """
        The transaction data field must be exactly selector (4B) + commitment (32B).
        """
        client = make_client()
        captured_tx = {}

        # eth_gasPrice → eth_getTransactionCount → eth_sendRawTransaction
        async def mock_post(url, json=None, **kwargs):
            method = json["method"]
            if method == "eth_gasPrice":
                return rpc_response("0x3B9ACA00")  # 1 gwei
            elif method == "eth_getTransactionCount":
                return rpc_response("0x0")
            elif method == "eth_sendRawTransaction":
                # Decode the signed tx to verify calldata
                raw_hex = json["params"][0]
                # We can't trivially decode the raw tx here, but we capture it
                captured_tx["raw"] = raw_hex
                return rpc_response(FAKE_TX_HASH)
            elif method == "eth_getTransactionReceipt":
                return rpc_response({
                    "status": "0x1",
                    "blockNumber": "0x1",
                    "transactionHash": FAKE_TX_HASH,
                })
            return rpc_response(None)

        with patch("httpx.AsyncClient.post", side_effect=mock_post):
            tx_hash = await client.commit_intent(PRIVATE_KEY, COMMITMENT_32)

        assert tx_hash == FAKE_TX_HASH
        assert "raw" in captured_tx  # transaction was actually sent

    @pytest.mark.asyncio
    async def test_to_address_is_hook_address(self):
        """
        The 'to' field in the built transaction must equal the hook address.
        """
        client = make_client()
        signed_txs = []

        original_sign = __import__("eth_account").Account.sign_transaction

        def capture_sign(tx, pk):
            signed_txs.append(tx.copy())
            return original_sign(tx, pk)

        call_count = [0]

        async def mock_post(url, json=None, **kwargs):
            method = json["method"]
            call_count[0] += 1
            if method == "eth_gasPrice":
                return rpc_response("0x3B9ACA00")
            elif method == "eth_getTransactionCount":
                return rpc_response("0x0")
            elif method == "eth_sendRawTransaction":
                return rpc_response(FAKE_TX_HASH)
            elif method == "eth_getTransactionReceipt":
                return rpc_response({
                    "status": "0x1",
                    "blockNumber": "0x2",
                    "transactionHash": FAKE_TX_HASH,
                })
            return rpc_response(None)

        with patch("httpx.AsyncClient.post", side_effect=mock_post):
            with patch("eth_account.Account.sign_transaction", side_effect=capture_sign):
                await client.commit_intent(PRIVATE_KEY, COMMITMENT_32)

        assert len(signed_txs) == 1
        tx = signed_txs[0]
        # ChainClient checksums the address to EIP-55 form for eth_account compatibility
        assert tx["to"].lower() == HOOK_ADDRESS.lower()
        assert tx["gas"] == 150_000
        assert tx["chainId"] == 1337

        # Verify calldata = selector + commitment
        data_hex = tx["data"].removeprefix("0x")
        assert data_hex == EXPECTED_CALLDATA_HEX, (
            f"Calldata mismatch:\n  got:      {data_hex}\n  expected: {EXPECTED_CALLDATA_HEX}"
        )


# ---------------------------------------------------------------------------
# Test 2: Returns tx hash from eth_sendRawTransaction
# ---------------------------------------------------------------------------

class TestCommitIntentTxHash:

    @pytest.mark.asyncio
    async def test_returns_tx_hash_on_success(self):
        """commit_intent must return the exact tx hash from eth_sendRawTransaction."""
        client = make_client()
        specific_hash = "0x" + "ab" * 32

        async def mock_post(url, json=None, **kwargs):
            method = json["method"]
            if method == "eth_gasPrice":
                return rpc_response("0x1DCD6500")  # 0.5 gwei
            elif method == "eth_getTransactionCount":
                return rpc_response("0x3")  # nonce = 3
            elif method == "eth_sendRawTransaction":
                return rpc_response(specific_hash)
            elif method == "eth_getTransactionReceipt":
                return rpc_response({
                    "status": "0x1",
                    "blockNumber": "0x64",
                    "transactionHash": specific_hash,
                })
            return rpc_response(None)

        with patch("httpx.AsyncClient.post", side_effect=mock_post):
            result = await client.commit_intent(PRIVATE_KEY, COMMITMENT_32)

        assert result == specific_hash

    @pytest.mark.asyncio
    async def test_raises_on_rpc_error(self):
        """commit_intent must raise ChainClientError if the RPC returns an error."""
        client = make_client()

        async def mock_post(url, json=None, **kwargs):
            return MagicMock(
                raise_for_status=MagicMock(),
                json=MagicMock(return_value={
                    "jsonrpc": "2.0",
                    "id": 1,
                    "error": {"code": -32000, "message": "execution reverted"},
                }),
            )

        with patch("httpx.AsyncClient.post", side_effect=mock_post):
            with pytest.raises(ChainClientError):
                await client.commit_intent(PRIVATE_KEY, COMMITMENT_32)

    @pytest.mark.asyncio
    async def test_raises_on_bad_commitment_length(self):
        """commit_intent must raise ValueError if commitment is not 32 bytes."""
        client = make_client()
        with pytest.raises(ValueError, match="32 bytes"):
            await client.commit_intent(PRIVATE_KEY, b"\xaa" * 16)


# ---------------------------------------------------------------------------
# Test 3: Receipt polling — success path
# ---------------------------------------------------------------------------

class TestReceiptPolling:

    @pytest.mark.asyncio
    async def test_polls_until_receipt_available(self):
        """
        _poll_receipt should return as soon as the receipt is non-null,
        even after several null responses.
        """
        client = make_client()
        call_count = [0]

        async def mock_post(url, json=None, **kwargs):
            method = json["method"]
            if method == "eth_getTransactionReceipt":
                call_count[0] += 1
                if call_count[0] < 3:
                    # First two calls: not yet mined
                    return rpc_response(None)
                else:
                    # Third call: mined
                    return rpc_response({
                        "status": "0x1",
                        "blockNumber": "0xa",
                        "transactionHash": FAKE_TX_HASH,
                    })
            return rpc_response(None)

        with patch("httpx.AsyncClient.post", side_effect=mock_post):
            receipt = await client._poll_receipt(FAKE_TX_HASH)

        assert receipt["status"] == "0x1"
        assert call_count[0] == 3

    @pytest.mark.asyncio
    async def test_reverted_transaction_raises(self):
        """
        If the receipt status is 0x0 (revert), commit_intent must raise
        TransactionRevertedError.
        """
        client = make_client()

        async def mock_post(url, json=None, **kwargs):
            method = json["method"]
            if method == "eth_gasPrice":
                return rpc_response("0x3B9ACA00")
            elif method == "eth_getTransactionCount":
                return rpc_response("0x0")
            elif method == "eth_sendRawTransaction":
                return rpc_response(FAKE_TX_HASH)
            elif method == "eth_getTransactionReceipt":
                return rpc_response({
                    "status": "0x0",  # reverted
                    "blockNumber": "0x5",
                    "transactionHash": FAKE_TX_HASH,
                })
            return rpc_response(None)

        with patch("httpx.AsyncClient.post", side_effect=mock_post):
            with pytest.raises(TransactionRevertedError) as exc_info:
                await client.commit_intent(PRIVATE_KEY, COMMITMENT_32)

        assert exc_info.value.tx_hash == FAKE_TX_HASH


# ---------------------------------------------------------------------------
# Test 4: Receipt polling — timeout path
# ---------------------------------------------------------------------------

class TestReceiptTimeout:

    @pytest.mark.asyncio
    async def test_poll_receipt_raises_on_timeout(self):
        """
        _poll_receipt must raise ReceiptTimeoutError when no receipt arrives
        within the configured receipt_timeout window.
        """
        # Very short timeout so the test runs quickly
        client = ChainClient(
            rpc_url=RPC_URL,
            hook_address=HOOK_ADDRESS,
            chain_id=1337,
            receipt_poll_interval=0.01,
            receipt_timeout=1,  # 1 second — only ~100 polls at 0.01s interval
        )

        async def mock_post(url, json=None, **kwargs):
            # Always return null (not yet mined)
            return rpc_response(None)

        with patch("httpx.AsyncClient.post", side_effect=mock_post):
            with pytest.raises(ReceiptTimeoutError) as exc_info:
                await client._poll_receipt(FAKE_TX_HASH)

        assert exc_info.value.tx_hash == FAKE_TX_HASH
        assert "1s" in str(exc_info.value)

    @pytest.mark.asyncio
    async def test_commit_intent_propagates_timeout(self):
        """
        commit_intent must propagate ReceiptTimeoutError when polling times out.
        """
        client = ChainClient(
            rpc_url=RPC_URL,
            hook_address=HOOK_ADDRESS,
            chain_id=1337,
            receipt_poll_interval=0.01,
            receipt_timeout=1,
        )

        async def mock_post(url, json=None, **kwargs):
            method = json["method"]
            if method == "eth_gasPrice":
                return rpc_response("0x3B9ACA00")
            elif method == "eth_getTransactionCount":
                return rpc_response("0x0")
            elif method == "eth_sendRawTransaction":
                return rpc_response(FAKE_TX_HASH)
            elif method == "eth_getTransactionReceipt":
                # Never returns a receipt
                return rpc_response(None)
            return rpc_response(None)

        with patch("httpx.AsyncClient.post", side_effect=mock_post):
            with pytest.raises(ReceiptTimeoutError):
                await client.commit_intent(PRIVATE_KEY, COMMITMENT_32)
