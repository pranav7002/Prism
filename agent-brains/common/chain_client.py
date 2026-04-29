"""
On-chain commitment client for PrismHook.commitIntent(bytes32).

Sends commitment hashes from each agent to the deployed PrismHook
contract on Unichain Sepolia (chainId 1301) using raw JSON-RPC calls.

Protocol:
  - Fetches gasPrice and nonce via eth_gasPrice / eth_getTransactionCount
  - Builds and signs a transaction with eth_account
  - Sends via eth_sendRawTransaction
  - Polls eth_getTransactionReceipt until confirmed or timeout

Usage:
    client = ChainClient(
        rpc_url="https://sepolia.unichain.org",
        hook_address="0x5110dab469ff1dff0053cac172ac038ea3fefff0",
    )
    tx_hash = await client.commit_intent(private_key_hex, commitment_bytes)
"""


import asyncio
import logging
from typing import Any

import httpx
from eth_account import Account
from eth_utils import to_checksum_address

logger = logging.getLogger(__name__)

# keccak256("commitIntent(bytes32)")[:4] — verified via PyCryptodome
COMMIT_INTENT_SELECTOR: bytes = bytes.fromhex("5f193909")


class ChainClientError(Exception):
    """Base error for ChainClient failures."""


class InsufficientFundsError(ChainClientError):
    """Raised when the sender lacks ETH for gas."""


class TransactionRevertedError(ChainClientError):
    """Raised when the transaction was mined but reverted."""

    def __init__(self, tx_hash: str, revert_data: str = ""):
        self.tx_hash = tx_hash
        self.revert_data = revert_data
        super().__init__(f"Transaction {tx_hash} reverted: {revert_data or 'no data'}")


class ReceiptTimeoutError(ChainClientError):
    """Raised when the receipt was not found within the polling window."""

    def __init__(self, tx_hash: str, timeout_secs: int):
        self.tx_hash = tx_hash
        super().__init__(
            f"Receipt for {tx_hash} not found after {timeout_secs}s"
        )


class ChainClient:
    """
    Minimal Ethereum JSON-RPC client for submitting PrismHook commitments.

    Uses only ``httpx`` and ``eth_account`` — no web3.py dependency.

    Args:
        rpc_url:      Full HTTP(S) JSON-RPC endpoint URL.
        hook_address: Deployed PrismHook contract address (0x-prefixed).
        chain_id:     EVM chain ID (default 1301 = Unichain Sepolia).
        receipt_poll_interval: Seconds between receipt polls (default 1.0).
        receipt_timeout:       Max seconds to wait for receipt (default 60).
    """

    def __init__(
        self,
        rpc_url: str,
        hook_address: str,
        chain_id: int = 1301,
        receipt_poll_interval: float = 1.0,
        receipt_timeout: int = 60,
    ) -> None:
        self.rpc_url = rpc_url
        # EIP-55 checksummed address is required by eth_account for the `to` field
        self.hook_address = to_checksum_address(hook_address)
        self.chain_id = chain_id
        self.receipt_poll_interval = receipt_poll_interval
        self.receipt_timeout = receipt_timeout

    def __repr__(self) -> str:
        return (
            f"ChainClient(rpc_url={self.rpc_url!r}, "
            f"hook_address={self.hook_address!r}, "
            f"chain_id={self.chain_id})"
        )

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    async def commit_intent(
        self, private_key: str, commitment: bytes
    ) -> str:
        """
        Call ``PrismHook.commitIntent(bytes32)`` on-chain and return the tx hash.

        The method builds, signs, and broadcasts a raw transaction, then
        waits for the receipt to confirm success.

        Args:
            private_key: 0x-prefixed hex private key of the registered agent EOA.
            commitment:  32-byte commitment hash to submit.

        Returns:
            The 0x-prefixed transaction hash (confirmed in a block).

        Raises:
            ChainClientError:      Bad RPC response or network error.
            InsufficientFundsError: Sender has insufficient ETH for gas.
            TransactionRevertedError: Transaction was mined but reverted.
            ReceiptTimeoutError:   Receipt not found within timeout window.
            ValueError:            commitment is not exactly 32 bytes.
        """
        if len(commitment) != 32:
            raise ValueError(
                f"commitment must be 32 bytes, got {len(commitment)}"
            )

        acct = Account.from_key(private_key)
        sender: str = acct.address

        logger.info(
            f"Committing intent on-chain: sender={sender} "
            f"commitment=0x{commitment.hex()[:16]}..."
        )

        async with httpx.AsyncClient(timeout=30.0) as client:
            gas_price = await self._get_gas_price(client)
            nonce = await self._get_nonce(client, sender)

            # ABI-encode: selector (4B) + commitment (32B, already 32 bytes)
            calldata = COMMIT_INTENT_SELECTOR + commitment

            tx: dict[str, Any] = {
                "to": self.hook_address,
                "data": "0x" + calldata.hex(),
                "gas": 150_000,
                "gasPrice": gas_price,
                "nonce": nonce,
                "chainId": self.chain_id,
                "value": 0,
            }

            signed = Account.sign_transaction(tx, private_key)
            raw_hex = "0x" + signed.raw_transaction.hex()

            tx_hash = await self._send_raw_transaction(client, raw_hex)
            logger.info(f"Transaction broadcast: {tx_hash}")

        # Poll for receipt outside the shared client context (may take time)
        receipt = await self._poll_receipt(tx_hash)
        if receipt.get("status") == "0x0":
            revert_data = receipt.get("revertReason", "")
            raise TransactionRevertedError(tx_hash, revert_data)

        logger.info(
            f"Commitment confirmed: tx={tx_hash} "
            f"block={receipt.get('blockNumber')}"
        )
        return tx_hash

    # ------------------------------------------------------------------
    # JSON-RPC helpers
    # ------------------------------------------------------------------

    async def _rpc(
        self,
        client: httpx.AsyncClient,
        method: str,
        params: list[Any],
    ) -> Any:
        """
        Execute a single JSON-RPC call and return the ``result`` field.

        Raises:
            ChainClientError: On HTTP error, JSON parse failure, or RPC error
                              response from the node.
        """
        payload = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        }
        try:
            resp = await client.post(self.rpc_url, json=payload)
            resp.raise_for_status()
        except httpx.HTTPStatusError as exc:
            raise ChainClientError(
                f"RPC HTTP error {exc.response.status_code} for {method}: {exc}"
            ) from exc
        except httpx.RequestError as exc:
            raise ChainClientError(
                f"RPC request error for {method}: {exc}"
            ) from exc

        try:
            data = resp.json()
        except Exception as exc:
            raise ChainClientError(
                f"Failed to parse JSON-RPC response for {method}: {exc}"
            ) from exc

        if "error" in data:
            error = data["error"]
            msg = error.get("message", str(error))
            # Detect insufficient funds from common node error messages
            if "insufficient funds" in msg.lower():
                raise InsufficientFundsError(
                    f"Insufficient funds for {method}: {msg}"
                )
            raise ChainClientError(
                f"JSON-RPC error in {method}: {msg}"
            )

        return data["result"]

    async def _get_gas_price(self, client: httpx.AsyncClient) -> int:
        """Fetch the current gas price in wei via eth_gasPrice."""
        result = await self._rpc(client, "eth_gasPrice", [])
        gas_price = int(result, 16)
        logger.debug(f"Gas price: {gas_price} wei")
        return gas_price

    async def _get_nonce(self, client: httpx.AsyncClient, address: str) -> int:
        """Fetch the next nonce for address via eth_getTransactionCount."""
        result = await self._rpc(
            client, "eth_getTransactionCount", [address, "latest"]
        )
        nonce = int(result, 16)
        logger.debug(f"Nonce for {address}: {nonce}")
        return nonce

    async def _send_raw_transaction(
        self, client: httpx.AsyncClient, raw_tx_hex: str
    ) -> str:
        """
        Broadcast a signed transaction via eth_sendRawTransaction.

        Returns:
            The transaction hash (0x-prefixed hex string).
        """
        result = await self._rpc(
            client, "eth_sendRawTransaction", [raw_tx_hex]
        )
        # result is the tx hash
        return result

    async def _poll_receipt(self, tx_hash: str) -> dict[str, Any]:
        """
        Poll eth_getTransactionReceipt until mined or timeout.

        Args:
            tx_hash: 0x-prefixed transaction hash to poll.

        Returns:
            The receipt dict from the node.

        Raises:
            ReceiptTimeoutError: If no receipt is found within receipt_timeout seconds.
        """
        elapsed = 0.0
        async with httpx.AsyncClient(timeout=30.0) as client:
            while elapsed < self.receipt_timeout:
                result = await self._rpc(
                    client, "eth_getTransactionReceipt", [tx_hash]
                )
                if result is not None:
                    logger.debug(
                        f"Receipt found for {tx_hash} after {elapsed:.1f}s"
                    )
                    return result

                await asyncio.sleep(self.receipt_poll_interval)
                elapsed += self.receipt_poll_interval

        raise ReceiptTimeoutError(tx_hash, self.receipt_timeout)
