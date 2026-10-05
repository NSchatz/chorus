"""The HTTP client and the event-stream subscriber.

``docs/control-plane.md``, "How the messages travel": ``GET /api/state`` once,
``GET /api/events`` as server-sent events with one full state per change, and
``POST /api/command`` with ``Content-Type: application/json`` and no ``Origin``
header. ``GET /api/controller-events`` is a second stream of the same kind, one
``controller_event`` per button press the server accepted and nothing kept.
``GET /api/visualizer?zone=<room>`` is a third: one room's visualizer frames,
at most ten a second, the latest only (``docs/visualizer.md``, "The HTTP
stream"). Every response is ``Connection: close``. ``GET /metrics`` is the
speakers' telemetry (``docs/telemetry.md``), read on request.

Voice ("Voice: the wake word, the run and its audio"): ``GET
/api/voice-events`` is a fourth stream, one ``voice_wake`` per wake word and
nothing kept; ``voice_start`` is the one command answered with a ``voice_run``
and not a state; ``GET /api/voice-audio?run=<identifier>`` is that run's
microphone audio, raw PCM until the server closes the connection.
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
    ChorusVoiceRouteError,
)
from .metrics import Metrics
from .models import (
    ControllerEvent,
    ServerInfo,
    State,
    VisualizerFrame,
    VoiceRun,
    VoiceWake,
    loads,
)
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

# What a voice run's audio is (docs/control-plane.md, "3. The audio"): the
# value of the answer's ``X-Chorus-Audio-Format`` header, and the bytes of one
# sample.
VOICE_AUDIO_FORMAT = "pcm_s16le; rate=16000; channels=1"
VOICE_SAMPLE_BYTES = 2
# A live microphone sends without pause, so a run's audio that is silent on
# the socket for this long is a connection that died. ASSUMED, not measured:
# twice the 5 s after which the server itself drops a reader that takes nothing.
VOICE_AUDIO_IDLE_SECONDS = 10.0
# The server ends a run at its ``limit_ms`` and closes the connection. This
# reader stops on its own this long after the limit, on a monotonic clock, so
# a server that did not close cannot hold a run open. ASSUMED margin.
VOICE_RUN_GRACE_SECONDS = 2.0

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

    async def _post(self, message: bytes) -> bytes:
        """Send one control message; return the body of a ``200`` answer."""
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
            return body
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

    async def command(self, message: bytes) -> State:
        """Send one control message and return the state it resulted in."""
        return State.parse(await self._post(message))

    async def voice_start(self, zone: str) -> VoiceRun:
        """Open a voice run in a room; return its identifier and its limit.

        A refusal is a ``ChorusCommandError`` whose ``name`` is the reason:
        ``voice-disabled``, ``mic-muted`` or ``no-voice-integration``.
        """
        return VoiceRun.parse(await self._post(commands.voice_start(zone)))

    async def voice_stop(self, zone: str) -> State:
        """End a room's voice run; a room with none is not an error."""
        return await self.command(commands.voice_stop(zone))

    async def set_voice_enabled(self, zone: str, enabled: bool) -> State:
        """Switch a room's voice path on or off."""
        return await self.command(commands.voice_enabled(zone, enabled))

    async def voice_audio(self, run: VoiceRun) -> VoiceAudio:
        """Open a voice run's audio (``GET /api/voice-audio?run=<identifier>``).

        Returns once the server has answered ``200``: the reader is then the
        run's one reader. A refusal is a ``ChorusVoiceRouteError`` naming the
        reason. No message this raises carries the run's identifier.
        """
        try:
            resp = await self.session.get(
                self.url("/api/voice-audio").with_query(run=run.run),
                timeout=aiohttp.ClientTimeout(
                    total=None,
                    sock_connect=REQUEST_TIMEOUT,
                    sock_read=VOICE_AUDIO_IDLE_SECONDS,
                ),
            )
        except (aiohttp.ClientError, TimeoutError, OSError) as err:
            # The error's own text may quote the request's address, which
            # holds the identifier: only its kind is passed on.
            raise ChorusConnectionError(
                f"no answer from {self._base} for the voice run's audio: "
                f"{type(err).__name__}"
            ) from None
        if resp.status != 200:
            try:
                body = await _read_bounded(resp)
            except aiohttp.ClientError, TimeoutError, OSError:
                body = b""
            finally:
                resp.close()
            detail = ""
            try:
                said = loads(body, "the refusal").get("detail")
            except ChorusProtocolError:
                said = None
            if isinstance(said, str):
                detail = said
            reason, colon, _ = detail.partition(": ")
            raise ChorusVoiceRouteError(resp.status, reason if colon else "", detail)
        said_format = resp.headers.get("X-Chorus-Audio-Format")
        if said_format != VOICE_AUDIO_FORMAT:
            resp.close()
            raise ChorusProtocolError(
                f"the voice run's audio is '{said_format}', not '{VOICE_AUDIO_FORMAT}'"
            )
        loop = asyncio.get_running_loop()
        return VoiceAudio(
            resp, loop.time() + run.limit_ms / 1000 + VOICE_RUN_GRACE_SECONDS
        )

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

    def voice_events(
        self,
        *,
        idle_probe: float = IDLE_PROBE_SECONDS,
        sleep: Callable[[float], Awaitable[None]] = asyncio.sleep,
        rand: Callable[[], float] = _random.random,
    ) -> VoiceEventStream:
        """Return a subscriber to this server's wake words."""
        return VoiceEventStream(self, idle_probe=idle_probe, sleep=sleep, rand=rand)

    def visualizer(
        self,
        zone: str,
        *,
        idle_probe: float = IDLE_PROBE_SECONDS,
        sleep: Callable[[float], Awaitable[None]] = asyncio.sleep,
        rand: Callable[[], float] = _random.random,
    ) -> VisualizerStream:
        """Return a subscriber to one room's visualizer stream."""
        return VisualizerStream(
            self, zone, idle_probe=idle_probe, sleep=sleep, rand=rand
        )


class VoiceAudio:
    """One open voice run's audio: raw 16 kHz mono 16-bit PCM, as it arrives.

    An async iterator of chunks, each a whole number of samples. It ends when
    the server closes the connection (the run ended), when the connection is
    lost or silent for too long (``error`` then says so), when the run's limit
    has passed on this side's monotonic clock, or when it is closed. Nothing
    is kept: a chunk handed on is gone from here.
    """

    def __init__(self, resp: aiohttp.ClientResponse, deadline: float) -> None:
        """Take the open answer and the loop time past which nothing is read."""
        self._resp = resp
        self._deadline = deadline
        self._carry = b""
        self._closed = False
        self.error: ChorusConnectionError | None = None

    def __aiter__(self) -> VoiceAudio:
        """Return the iterator: this object."""
        return self

    async def __anext__(self) -> bytes:
        """Return the next whole samples; stop when the audio has ended."""
        while not self._closed:
            left = self._deadline - asyncio.get_running_loop().time()
            if left <= 0:
                break
            try:
                async with asyncio.timeout(left):
                    chunk = await self._resp.content.readany()
            except TimeoutError, aiohttp.ClientError, OSError:
                if asyncio.get_running_loop().time() < self._deadline:
                    self.error = ChorusConnectionError(
                        "the voice run's audio was lost before the run ended"
                    )
                break
            if not chunk:
                break
            data = self._carry + chunk
            whole = len(data) - len(data) % VOICE_SAMPLE_BYTES
            self._carry = data[whole:]
            if whole:
                return data[:whole]
        self.close()
        raise StopAsyncIteration

    def close(self) -> None:
        """Close the connection; the server then ends the run (``reader-gone``)."""
        self._closed = True
        self._carry = b""
        self._resp.close()


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

    def _url(self) -> URL:
        """Return the address of the stream."""
        return self._client.url(self._path)

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
                self._url(),
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


class VoiceEventStream(_Subscriber):
    """The subscriber to wake words: each one a voice room's microphone heard, once.

    Like a press, a wake word is an event and not a state: the stream opens
    with a comment line and no message, nothing is kept for a stream opened
    later, and one heard while no stream is attached is never delivered.
    """

    _path = "/api/voice-events"
    _what = "voice event stream"

    async def run(
        self,
        on_wake: Callable[[VoiceWake], None],
        on_disconnect: Callable[[ChorusError], None],
        on_connect: Callable[[], None] = lambda: None,
    ) -> None:
        """Deliver every wake word, for ever; say when the stream is open and lost.

        Returns only by cancellation.
        """

        def on_open() -> None:
            self._backoff.reset()
            on_connect()

        async def on_idle() -> None:
            # No wake word is no news; the state is asked for as a sign of
            # life and its answer is not used.
            await self._client.state()

        await self._run(
            on_open,
            lambda event: on_wake(VoiceWake.parse(event)),
            on_idle,
            on_disconnect,
        )


class VisualizerStream(_Subscriber):
    """The subscriber to one room's visualizer stream: the latest frame, as sent.

    The server keeps one frame per room and sends a subscriber the latest, at
    most ten a second (``docs/visualizer.md``, "The rate cap", "The drop
    rule"); this reader hands each frame on as it arrives and keeps none. A
    stream opens with a comment line and no frame, and stays empty while the
    room plays nothing.
    """

    _path = "/api/visualizer"
    _what = "visualizer stream"

    def __init__(
        self,
        client: ChorusClient,
        zone: str,
        *,
        idle_probe: float,
        sleep: Callable[[float], Awaitable[None]],
        rand: Callable[[], float],
    ) -> None:
        """Hold the room whose stream this is."""
        super().__init__(client, idle_probe=idle_probe, sleep=sleep, rand=rand)
        self._zone = zone

    def _url(self) -> URL:
        return self._client.url(self._path).with_query(zone=self._zone)

    async def run(
        self,
        on_frame: Callable[[VisualizerFrame], None],
        on_disconnect: Callable[[ChorusError], None],
        on_connect: Callable[[], None] = lambda: None,
    ) -> None:
        """Deliver every frame, for ever; say when the stream is open and lost.

        Returns only by cancellation.
        """

        def on_open() -> None:
            # The stream opens empty and stays so while the room is quiet, so
            # the answer itself says it worked.
            self._backoff.reset()
            on_connect()

        def on_event(event: str) -> None:
            frame = VisualizerFrame.parse(event)
            if frame.zone != self._zone:
                raise ChorusProtocolError(
                    f"a visualizer frame for '{frame.zone}' on the stream of "
                    f"'{self._zone}'"
                )
            on_frame(frame)

        async def on_idle() -> None:
            # A quiet room sends nothing; the state is asked for as a sign of
            # life and its answer is not used.
            await self._client.state()

        await self._run(on_open, on_event, on_idle, on_disconnect)
