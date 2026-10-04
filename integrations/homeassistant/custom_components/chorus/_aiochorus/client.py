"""The HTTP client and the event-stream subscriber.

``docs/control-plane.md``, "How the messages travel": ``GET /api/state`` once,
``GET /api/events`` as server-sent events with one full state per change, and
``POST /api/command`` with ``Content-Type: application/json`` and no ``Origin``
header. ``GET /api/controller-events`` is a second stream of the same kind, one
``controller_event`` per button press the server accepted and nothing kept. Every response is ``Connection: close``. ``GET /metrics`` is the
speakers' telemetry (``docs/telemetry.md``), read on request.
"""

from __future__ import annotations

import asyncio
from collections.abc import Awaitable, Callable
import random as _random

import aiohttp
from yarl import URL

from . import commands
from .errors import (
    ChorusCommandError,
    ChorusConnectionError,
    ChorusError,
    ChorusProtocolError,
    ChorusRefusedError,
    ChorusUnsupportedError,
)
from .metrics import Metrics
from .models import ControllerEvent, ServerInfo, State, loads
from .sse import MAX_EVENT_BYTES, SSEParser

REQUEST_TIMEOUT = 10.0
# With no change in the house the stream is silent, so silence alone says
# nothing about the server. After this long with no bytes the subscriber asks
# for the state once; an answer keeps the stream, no answer ends it.
IDLE_PROBE_SECONDS = 45.0
BACKOFF_FIRST = 1.0
BACKOFF_FACTOR = 2.0
BACKOFF_MAX = 60.0
_CHUNK = 16 * 1024

_JSON = {"Content-Type": "application/json"}


class ChorusClient:
    """One chorus server's control plane."""

    def __init__(self, session: aiohttp.ClientSession, host: str, port: int) -> None:
        """Take the session to use (injected, never created here)."""
        self.session = session
        self._base = URL.build(scheme="http", host=host, port=port)
        self.host = host
        self.port = port

    def url(self, path: str) -> URL:
        """Return the URL of one route."""
        return self._base.with_path(path)

    async def _get(self, path: str) -> tuple[int, bytes]:
        try:
            async with self.session.get(
                self.url(path), timeout=aiohttp.ClientTimeout(total=REQUEST_TIMEOUT)
            ) as resp:
                return resp.status, await _read_bounded(resp)
        except (aiohttp.ClientError, TimeoutError, OSError) as err:
            raise ChorusConnectionError(
                f"no answer from {self._base}: {err or type(err).__name__}"
            ) from err

    async def server(self) -> ServerInfo:
        """Ask the server who it is (``GET /api/server``)."""
        status, body = await self._get("/api/server")
        if status == 404:
            raise ChorusUnsupportedError(
                "the server does not say who it is (no /api/server): it is older "
                "than this client needs"
            )
        if status != 200:
            raise ChorusProtocolError(f"GET /api/server answered {status}")
        return ServerInfo.parse(body)

    async def state(self) -> State:
        """Read the state once (``GET /api/state``)."""
        status, body = await self._get("/api/state")
        if status != 200:
            raise ChorusProtocolError(f"GET /api/state answered {status}")
        return State.parse(body)

    async def metrics(self) -> Metrics:
        """Read the speakers' telemetry once (``GET /metrics``, ``docs/telemetry.md``).

        The caller decides how often: the server renders the text on every
        request and keeps no history.
        """
        status, body = await self._get("/metrics")
        if status != 200:
            raise ChorusProtocolError(f"GET /metrics answered {status}")
        return Metrics.parse(body)

    async def command(self, message: bytes) -> State:
        """Send one control message and return the state it resulted in."""
        try:
            async with self.session.post(
                self.url("/api/command"),
                data=message,
                headers=_JSON,
                timeout=aiohttp.ClientTimeout(total=REQUEST_TIMEOUT),
            ) as resp:
                status = resp.status
                body = await _read_bounded(resp)
        except (aiohttp.ClientError, TimeoutError, OSError) as err:
            raise ChorusConnectionError(
                f"no answer from {self._base}: {err or type(err).__name__}"
            ) from err
        if status == 200:
            return State.parse(body)
        if status == 400:
            obj = loads(body, "the error message")
            field = obj.get("field")
            detail = obj.get("detail")
            if obj.get("t") != "error" or not isinstance(field, str):
                raise ChorusProtocolError("a 400 answer is not an error message")
            raise ChorusCommandError(field, detail if isinstance(detail, str) else "")
        if status == 426:
            obj = loads(body, "the refused message")
            offered = obj.get("offered")
            implemented = obj.get("implemented")
            detail = obj.get("detail")
            raise ChorusRefusedError(
                detail if isinstance(detail, str) else "",
                offered if isinstance(offered, int) else None,
                tuple(v for v in implemented if isinstance(v, int))
                if isinstance(implemented, list)
                else (),
            )
        raise ChorusProtocolError(f"POST /api/command answered {status}")

    async def set_volume(self, zone: str, thousandths: int) -> State:
        """Set a room's volume."""
        return await self.command(commands.volume(zone, thousandths))

    async def set_mute(self, zone: str, muted: bool) -> State:
        """Mute or unmute a room."""
        return await self.command(commands.mute(zone, muted))

    async def step_volume(self, zone: str, step: int) -> State:
        """Move a room's volume."""
        return await self.command(commands.volume_step(zone, step))

    async def set_group_volume(self, group: str, thousandths: int) -> State:
        """Set a formed group's volume."""
        return await self.command(commands.group_volume(group, thousandths))

    async def step_group_volume(self, group: str, step: int) -> State:
        """Move a formed group's volume."""
        return await self.command(commands.group_volume_step(group, step))

    async def join(self, zone: str, target: str) -> State:
        """Make a room play in the target's group."""
        return await self.command(commands.join(zone, target))

    async def take(self, target: str, source: str | None = None) -> State:
        """Take the room."""
        return await self.command(commands.take(target, source))

    async def playback(self, target: str, action: str) -> State:
        """Pause, resume or skip a Spotify receiver."""
        return await self.command(commands.playback(target, action))

    async def announce(
        self, target: str, url: str, thousandths: int | None = None
    ) -> State:
        """Play an announcement in a room or a saved group."""
        return await self.command(commands.announce(target, url, thousandths))

    def events(
        self,
        *,
        idle_probe: float = IDLE_PROBE_SECONDS,
        sleep: Callable[[float], Awaitable[None]] = asyncio.sleep,
        rand: Callable[[], float] = _random.random,
    ) -> EventStream:
        """Return a subscriber to this server's event stream."""
        return EventStream(self, idle_probe=idle_probe, sleep=sleep, rand=rand)

    def controller_events(
        self,
        *,
        idle_probe: float = IDLE_PROBE_SECONDS,
        sleep: Callable[[float], Awaitable[None]] = asyncio.sleep,
        rand: Callable[[], float] = _random.random,
    ) -> ControllerEventStream:
        """Return a subscriber to this server's button presses."""
        return ControllerEventStream(
            self, idle_probe=idle_probe, sleep=sleep, rand=rand
        )


async def _read_bounded(resp: aiohttp.ClientResponse) -> bytes:
    body = bytearray()
    async for chunk in resp.content.iter_chunked(_CHUNK):
        body.extend(chunk)
        if len(body) > MAX_EVENT_BYTES:
            raise ChorusProtocolError(
                f"an answer is larger than {MAX_EVENT_BYTES} bytes"
            )
    return bytes(body)


class Backoff:
    """Exponential reconnect delays with jitter: half to all of each step."""

    def __init__(self, rand: Callable[[], float]) -> None:
        """Start at the first step."""
        self._rand = rand
        self._attempt = 0

    def reset(self) -> None:
        """Go back to the first step after a connection that worked."""
        self._attempt = 0

    def next(self) -> float:
        """Return the next delay in seconds."""
        step = min(BACKOFF_MAX, BACKOFF_FIRST * BACKOFF_FACTOR**self._attempt)
        self._attempt += 1
        return step * (0.5 + 0.5 * self._rand())


class _Subscriber:
    """One server-sent event stream: read, and reconnect with backoff and jitter.

    The line splitter and its bound on one event (``sse.py``), the idle probe
    and the reconnect rule are the same for every stream of the control plane.
    """

    _path: str
    _what: str

    def __init__(
        self,
        client: ChorusClient,
        *,
        idle_probe: float,
        sleep: Callable[[float], Awaitable[None]],
        rand: Callable[[], float],
    ) -> None:
        """Hold the client and the clock to wait on."""
        self._client = client
        self._idle_probe = idle_probe
        self._sleep = sleep
        self._backoff = Backoff(rand)

    async def _run(
        self,
        on_open: Callable[[], None],
        on_event: Callable[[str], None],
        on_idle: Callable[[], Awaitable[None]],
        on_disconnect: Callable[[ChorusError], None],
    ) -> None:
        while True:
            try:
                await self._once(on_open, on_event, on_idle)
            except ChorusError as err:
                on_disconnect(err)
            await self._sleep(self._backoff.next())

    async def _once(
        self,
        on_open: Callable[[], None],
        on_event: Callable[[str], None],
        on_idle: Callable[[], Awaitable[None]],
    ) -> None:
        client = self._client
        parser = SSEParser()
        try:
            async with client.session.get(
                client.url(self._path),
                headers={"Accept": "text/event-stream"},
                timeout=aiohttp.ClientTimeout(total=None, sock_connect=REQUEST_TIMEOUT),
            ) as resp:
                if resp.status != 200:
                    raise ChorusConnectionError(
                        f"GET {self._path} answered {resp.status}"
                    )
                on_open()
                while True:
                    try:
                        async with asyncio.timeout(self._idle_probe):
                            chunk = await resp.content.readany()
                    except TimeoutError:
                        # Silence: ask once. A failure raises and ends the stream.
                        await on_idle()
                        continue
                    if not chunk:
                        raise ChorusConnectionError(
                            f"the server closed the {self._what}"
                        )
                    for event in parser.feed(chunk):
                        on_event(event)
        except (aiohttp.ClientError, TimeoutError, OSError) as err:
            raise ChorusConnectionError(
                f"the {self._what} from {client.url('/')} was lost: "
                f"{err or type(err).__name__}"
            ) from err


class EventStream(_Subscriber):
    """The subscriber to the state: every change, as the complete state."""

    _path = "/api/events"
    _what = "event stream"

    async def run(
        self,
        on_state: Callable[[State], None],
        on_disconnect: Callable[[ChorusError], None],
    ) -> None:
        """Deliver every state, for ever; say when the stream is lost.

        Returns only by cancellation.
        """

        def on_event(event: str) -> None:
            on_state(State.parse(event))
            # The stream opens with the state: one delivered is a stream that
            # worked, and the next loss waits the first step again.
            self._backoff.reset()

        async def on_idle() -> None:
            on_state(await self._client.state())

        await self._run(lambda: None, on_event, on_idle, on_disconnect)


class ControllerEventStream(_Subscriber):
    """The subscriber to button presses: each one the server accepted, once.

    The server keeps no press: a stream opens with a comment line and no
    message, so one that is opened again after a loss is sent nothing that
    happened before it, and this reader keeps nothing to send again either. A
    press accepted while no stream is attached is never delivered.
    """

    _path = "/api/controller-events"
    _what = "controller event stream"

    async def run(
        self,
        on_event: Callable[[ControllerEvent], None],
        on_disconnect: Callable[[ChorusError], None],
        on_connect: Callable[[], None] = lambda: None,
    ) -> None:
        """Deliver every press, for ever; say when the stream is open and lost.

        Returns only by cancellation.
        """

        def on_open() -> None:
            # A stream of presses opens empty, so the answer itself is what
            # says it worked: the next loss waits the first step again.
            self._backoff.reset()
            on_connect()

        async def on_idle() -> None:
            # No press is no news; the state is asked for as a sign of life
            # and its answer is not used.
            await self._client.state()

        await self._run(
            on_open,
            lambda event: on_event(ControllerEvent.parse(event)),
            on_idle,
            on_disconnect,
        )
