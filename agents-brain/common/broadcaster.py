"""
WebSocket intent broadcaster — publishes AgentIntentWire payloads
to the Rust orchestrator at ws://localhost:8765.

Protocol:
  - Connects to the orchestrator's WebSocket endpoint
  - Sends JSON-serialized AgentIntentWire per agent per epoch
  - Receives WsEvent confirmations (EpochStart, IntentsReceived, etc.)
  - Supports reconnection with exponential backoff

Usage:
    broadcaster = IntentBroadcaster("ws://localhost:8765")
    await broadcaster.connect()
    await broadcaster.send_intent(intent)
    await broadcaster.close()
"""


import asyncio
import json
import logging
import time
from dataclasses import dataclass, field
from typing import Callable, Awaitable, Optional

import websockets
from websockets.legacy.client import WebSocketClientProtocol

from .schemas import AgentIntentWire
from .constants import WS_DEFAULT_URL

logger = logging.getLogger(__name__)


@dataclass
class BroadcasterConfig:
    """Configuration for the WebSocket intent broadcaster."""
    ws_url: str = WS_DEFAULT_URL
    reconnect_attempts: int = 5
    reconnect_base_delay: float = 1.0   # seconds
    reconnect_max_delay: float = 30.0
    send_timeout: float = 5.0
    ping_interval: float = 20.0
    ping_timeout: float = 10.0


@dataclass
class BroadcastResult:
    """Result of a single broadcast attempt."""
    success: bool
    agent_id: str
    epoch: int
    commitment: str
    error: str | None = None
    latency_ms: float = 0.0


class IntentBroadcaster:
    """
    Publishes agent intents to the PRISM orchestrator over WebSocket.

    Sends each AgentIntentWire as a JSON text message. The orchestrator
    is expected to be running at the configured ws_url.
    """

    def __init__(self, config: BroadcasterConfig | None = None):
        self.config = config or BroadcasterConfig()
        self._ws: WebSocketClientProtocol | None = None
        self._connected = False
        self._listener_task: asyncio.Task | None = None
        self._event_callbacks: list[Callable[[dict], Awaitable[None]]] = []
        self._sent_count = 0

    async def connect(self) -> bool:
        """
        Establish WebSocket connection with retry logic.

        Returns True if connection succeeded, False if all attempts failed.
        """
        delay = self.config.reconnect_base_delay

        for attempt in range(1, self.config.reconnect_attempts + 1):
            try:
                logger.info(
                    f"Connecting to orchestrator at {self.config.ws_url} "
                    f"(attempt {attempt}/{self.config.reconnect_attempts})"
                )
                self._ws = await websockets.connect(
                    self.config.ws_url,
                    ping_interval=self.config.ping_interval,
                    ping_timeout=self.config.ping_timeout,
                )
                self._connected = True
                logger.info("✓ Connected to orchestrator")

                # Start background listener for orchestrator events
                self._listener_task = asyncio.create_task(
                    self._listen_events()
                )
                return True

            except (ConnectionRefusedError, OSError, websockets.exceptions.WebSocketException) as e:
                logger.warning(
                    f"Connection attempt {attempt} failed: {e}"
                )
                if attempt < self.config.reconnect_attempts:
                    logger.info(f"Retrying in {delay:.1f}s...")
                    await asyncio.sleep(delay)
                    delay = min(delay * 2, self.config.reconnect_max_delay)

        logger.error("All connection attempts failed")
        self._connected = False
        return False

    async def _listen_events(self):
        """Background task: listen for orchestrator events."""
        if not self._ws:
            return

        try:
            async for message in self._ws:
                try:
                    event = json.loads(message)
                    event_type = event.get("type", "unknown")
                    logger.debug(f"Orchestrator event: {event_type}")

                    for cb in self._event_callbacks:
                        await cb(event)

                except json.JSONDecodeError:
                    logger.warning(f"Non-JSON message from orchestrator: {message[:100]}")
        except websockets.exceptions.ConnectionClosed as e:
            logger.warning(f"Orchestrator connection closed: {e}")
            self._connected = False
        except Exception as e:
            logger.error(f"Listener error: {e}")
            self._connected = False

    def on_event(self, callback: Callable[[dict], Awaitable[None]]):
        """Register a callback for orchestrator events."""
        self._event_callbacks.append(callback)

    async def send_intent(self, intent: AgentIntentWire) -> BroadcastResult:
        """
        Send a single AgentIntentWire to the orchestrator.

        Returns a BroadcastResult with success status, commitment, and latency.
        """
        commitment = intent.compute_commitment()

        if not self._connected or not self._ws:
            return BroadcastResult(
                success=False,
                agent_id=intent.agent_id,
                epoch=intent.epoch,
                commitment=commitment,
                error="Not connected to orchestrator",
            )

        wire_json = intent.to_wire_json()
        payload = json.dumps({
            "type": "SubmitIntent",
            "intent": wire_json,
            "commitment": commitment,
        })

        start = time.monotonic()
        try:
            await asyncio.wait_for(
                self._ws.send(payload),
                timeout=self.config.send_timeout,
            )
            latency = (time.monotonic() - start) * 1000
            self._sent_count += 1

            logger.info(
                f"→ Sent {intent.agent_id[:10]}... epoch={intent.epoch} "
                f"action={intent.action.type} ({latency:.1f}ms)"
            )

            return BroadcastResult(
                success=True,
                agent_id=intent.agent_id,
                epoch=intent.epoch,
                commitment=commitment,
                latency_ms=latency,
            )
        except asyncio.TimeoutError:
            return BroadcastResult(
                success=False,
                agent_id=intent.agent_id,
                epoch=intent.epoch,
                commitment=commitment,
                error=f"Send timed out after {self.config.send_timeout}s",
            )
        except Exception as e:
            return BroadcastResult(
                success=False,
                agent_id=intent.agent_id,
                epoch=intent.epoch,
                commitment=commitment,
                error=str(e),
            )

    async def send_epoch_intents(
        self, intents: list[AgentIntentWire]
    ) -> list[BroadcastResult]:
        """
        Send all 5 agent intents for one epoch.

        Sends sequentially (order matters for the solver).
        Returns list of BroadcastResults.
        """
        results = []
        for intent in intents:
            result = await self.send_intent(intent)
            results.append(result)
        return results

    async def close(self):
        """Gracefully close the WebSocket connection."""
        if self._listener_task:
            self._listener_task.cancel()
            try:
                await self._listener_task
            except asyncio.CancelledError:
                pass

        if self._ws:
            await self._ws.close()
            self._ws = None

        self._connected = False
        logger.info(f"Broadcaster closed (sent {self._sent_count} intents)")

    @property
    def is_connected(self) -> bool:
        return self._connected

    @property
    def sent_count(self) -> int:
        return self._sent_count


class OfflineBroadcaster:
    """
    Offline broadcaster for testing — logs intents to stdout/file
    instead of sending over WebSocket.

    Drop-in replacement for IntentBroadcaster when the orchestrator
    is not running.
    """

    def __init__(self, output_file: str | None = None):
        self._output_file = output_file
        self._intents: list[dict] = []
        self._connected = True
        self._sent_count = 0

    async def connect(self) -> bool:
        logger.info("OfflineBroadcaster: simulating connection (no WS)")
        return True

    async def send_intent(self, intent: AgentIntentWire) -> BroadcastResult:
        commitment = intent.compute_commitment()
        wire = intent.to_wire_json()
        wire["commitment"] = commitment

        self._intents.append(wire)
        self._sent_count += 1

        if self._output_file:
            with open(self._output_file, "a") as f:
                f.write(json.dumps(wire) + "\n")

        logger.info(
            f"[offline] {intent.agent_id[:10]}... "
            f"epoch={intent.epoch} action={intent.action.type}"
        )

        return BroadcastResult(
            success=True,
            agent_id=intent.agent_id,
            epoch=intent.epoch,
            commitment=commitment,
            latency_ms=0.0,
        )

    async def send_epoch_intents(
        self, intents: list[AgentIntentWire]
    ) -> list[BroadcastResult]:
        return [await self.send_intent(i) for i in intents]

    async def close(self):
        logger.info(f"OfflineBroadcaster: {self._sent_count} intents logged")

    @property
    def is_connected(self) -> bool:
        return self._connected

    @property
    def sent_count(self) -> int:
        return self._sent_count

    @property
    def intents(self) -> list[dict]:
        return self._intents


def get_broadcaster(
    ws_url: str = WS_DEFAULT_URL,
    offline: bool = False,
    output_file: str | None = None,
) -> IntentBroadcaster | OfflineBroadcaster:
    """Factory: returns live or offline broadcaster."""
    if offline:
        return OfflineBroadcaster(output_file)
    config = BroadcasterConfig(ws_url=ws_url)
    return IntentBroadcaster(config)
