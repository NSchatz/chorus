"""A fake chorus control plane on loopback, driven by the shared vectors.

It serves the repository's `fixtures/control/v2/` bytes (the state vectors and
the server's identity, read from the repository and never copied), records
every command it is sent byte for byte (with its headers), sends a button press
to the subscribers of `GET /api/controller-events` when a test makes one, sends
a visualizer frame to the subscribers of a room's `GET /api/visualizer` when a
test makes one, sends a wake word to the subscribers of `GET /api/voice-events`
when a test makes one, opens and ends voice runs and serves a run's microphone
audio on `GET /api/voice-audio` under the route's four rules, numbers every
announcement and lists it until a test says how it ended, and applies the
commands the integration sends to an in-memory model of the house so a test
can see Home Assistant's entities follow. The model is a test double written
from `docs/control-plane.md`, not the server: what the real server does is
held by `tests/test_live_server.py`.
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
import json
from pathlib import Path
import re
import secrets
from typing import Any

from aiohttp import web

REPO = Path(__file__).resolve().parents[3]
SHARED = REPO / "fixtures" / "control"
SHARED_V2 = SHARED / "v2"


_SERIAL = re.compile(rb'"serial": ?(\d+)')


def shared(name: str) -> bytes:
    """Read a shared vector (catalog v2 first, then v1) from the repository.

    A vector file ends with one newline, which is the file's and not the
    message's: a message has no trailing newline on the wire.
    """
    for directory in (SHARED_V2, SHARED):
        path = directory / name
        if path.is_file():
            return path.read_bytes().removesuffix(b"\n")
    raise FileNotFoundError(name)


def metrics_sample(disconnected: str | None = None) -> bytes:
    """Return the sample scrape, in the server's own format (`docs/telemetry.md`).

    `tests/aiochorus/test_metrics.py` holds its families, HELP and TYPE lines to
    `crates/server/src/metrics.rs`. With `disconnected`, that speaker's session
    has ended: the exporter keeps `connected` (0), `info` and `firmware_info`
    of it and nothing else.
    """
    text = (Path(__file__).parent / "fixtures" / "metrics-scrape.txt").read_text()
    if disconnected is None:
        return text.encode()
    kept = (
        "chorus_speaker_connected{",
        "chorus_speaker_info{",
        "chorus_speaker_firmware_info{",
    )
    lines = []
    for line in text.splitlines():
        if f'speaker="{disconnected}"' in line:
            if not line.startswith(kept):
                continue
            if line.startswith(kept[0]):
                line = line.removesuffix(" 1") + " 0"  # noqa: PLW2901
        lines.append(line)
    return ("\n".join(lines) + "\n").encode()


def _half_up(value: float) -> int:
    return int(value + 0.5)


@dataclass
class Received:
    """One request to `POST /api/command`."""

    body: bytes
    headers: dict[str, str]


# What a voice run's audio is, as the server's answer says it.
VOICE_AUDIO_FORMAT = "pcm_s16le; rate=16000; channels=1"


@dataclass
class VoiceRun:
    """One open voice run: its room, its identifier and what its reader is owed."""

    zone: str
    id: str
    queue: asyncio.Queue[bytes | None] = field(default_factory=asyncio.Queue)
    claimed: bool = False


@dataclass
class FakeChorusServer:
    """The fake: routes, a recorded command list and a small house model."""

    state_bytes: bytes
    server_bytes: bytes = field(default_factory=lambda: shared("server.json"))
    commands: list[Received] = field(default_factory=list)
    scripted: list[tuple[int, bytes]] = field(default_factory=list)
    requests: list[str] = field(default_factory=list)
    refuse_connections: bool = False
    # Speakers whose session is not from this host: the owner-at-bench guard
    # refuses a transfer to them (no test sets the owner's bench variable).
    remote_speakers: set[str] = field(default_factory=set)
    server_status: int = 200
    state_status: int = 200
    events_status: int = 200
    controller_events_status: int = 200
    visualizer_status: int = 200
    # `GET /metrics`: these bytes when set, else the three series the real
    # exporter has for a speaker that sent no telemetry (docs/telemetry.md).
    metrics_bytes: bytes | None = None
    metrics_status: int = 200
    voice_events_status: int = 200
    # `--voice-integration`: the one address a run's audio is served to. None:
    # the server was started without it, and opens no run.
    voice_integration: str | None = "127.0.0.1"
    # `--voice-run-limit-ms`.
    voice_run_limit_ms: int = 30000
    # Every run that ended, as (room, the reason the server's log would name).
    runs_ended: list[tuple[str, str]] = field(default_factory=list)
    # Every refusal of `GET /api/voice-audio`, by its reason.
    voice_refusals: list[str] = field(default_factory=list)
    # Every identifier a `voice_start` was answered with, in order.
    run_ids: list[str] = field(default_factory=list)
    # With this set an announcement is over as soon as it starts.
    announcements_end_at_once: str | None = None
    port: int = 0
    _streams: list[asyncio.Queue[bytes | None]] = field(default_factory=list)
    _press_streams: list[asyncio.Queue[bytes | None]] = field(default_factory=list)
    # One (room, queue) per open `GET /api/visualizer?zone=<room>`.
    _light_streams: list[tuple[str, asyncio.Queue[bytes | None]]] = field(
        default_factory=list
    )
    _wake_streams: list[asyncio.Queue[bytes | None]] = field(default_factory=list)
    # Room id to its open run.
    _runs: dict[str, VoiceRun] = field(default_factory=dict)
    # Room id to what its speaker has sent since the wake word it heard.
    _heard: dict[str, bytes] = field(default_factory=dict)
    _announcements: int = 0
    _runner: web.AppRunner | None = None
    _model: dict[str, Any] | None = None

    # --- life cycle ---------------------------------------------------------

    async def start(self, port: int = 0) -> None:
        app = web.Application()
        app.router.add_get("/api/server", self._server)
        app.router.add_get("/api/state", self._state)
        app.router.add_get("/api/events", self._events)
        app.router.add_get("/api/controller-events", self._controller_events)
        app.router.add_get("/api/visualizer", self._visualizer)
        app.router.add_get("/api/voice-events", self._voice_events)
        app.router.add_get("/api/voice-audio", self._voice_audio)
        app.router.add_post("/api/command", self._command)
        app.router.add_get("/metrics", self._metrics)
        # No access log: nobody reads it, and it cannot format a frozen clock.
        self._runner = web.AppRunner(app, shutdown_timeout=0.1, access_log=None)
        await self._runner.setup()
        site = web.TCPSite(self._runner, "127.0.0.1", port)
        await site.start()
        self.port = self._runner.addresses[0][1]

    async def stop(self) -> None:
        self.drop_streams()
        if self._runner is not None:
            await self._runner.cleanup()
            self._runner = None

    # --- what a test drives ---------------------------------------------------

    @property
    def subscribers(self) -> int:
        return len(self._streams)

    def set_state(self, state: bytes) -> None:
        """Replace the state and send it to every subscriber.

        A server's serial only grows while it runs, so the new state is given
        the next serial; every other byte of it is the vector's.
        """
        serial = int(_SERIAL.search(self.state_bytes).group(1)) + 1  # type: ignore[union-attr]
        state = _SERIAL.sub(b'"serial":%d' % serial, state, count=1)
        self.state_bytes = state
        self._model = None
        for queue in self._streams:
            queue.put_nowait(state)

    def drop_streams(self) -> None:
        """Close every stream of both kinds, as a server going away does."""
        for queue in self._streams:
            queue.put_nowait(None)
        self.drop_press_streams()
        self.drop_visualizer_streams()
        self.drop_wake_streams()
        for zone in list(self._runs):
            self._end_run(zone, "session-ended")

    @property
    def press_subscribers(self) -> int:
        return len(self._press_streams)

    def press(self, event: bytes) -> None:
        """Send one `controller_event` to every subscriber attached right now.

        As the server does (docs/control-plane.md), nothing is kept: a press
        made while nobody is subscribed is sent to nobody, and a stream opened
        afterwards starts empty.
        """
        for queue in self._press_streams:
            queue.put_nowait(event)

    def drop_press_streams(self) -> None:
        """Close every stream of button presses and leave the state's alone."""
        for queue in self._press_streams:
            queue.put_nowait(None)

    @property
    def visualizer_subscribers(self) -> list[str]:
        """The room of every visualizer stream open right now."""
        return sorted(zone for zone, _ in self._light_streams)

    def frame(self, frame: bytes) -> None:
        """Send one `visualizer` frame to the subscribers of the room it names.

        As the server does (docs/visualizer.md, "The HTTP stream"), nothing is
        kept for a stream opened later. Unlike the server this fake holds no
        rate cap and supersedes nothing: it sends every frame a test makes,
        however fast, which is what the entity's own cap is tested against.
        """
        zone = json.loads(frame)["zone"]
        for subscribed, queue in self._light_streams:
            if subscribed == zone:
                queue.put_nowait(frame)

    def drop_visualizer_streams(self) -> None:
        """Close every visualizer stream and leave the others alone."""
        for _, queue in self._light_streams:
            queue.put_nowait(None)

    @property
    def wake_subscribers(self) -> int:
        return len(self._wake_streams)

    def wake(self, event: bytes, audio: bytes = b"") -> None:
        """Send one `voice_wake` to every subscriber attached right now.

        As the server does, nothing is kept for a stream opened later. `audio`
        is what the room's speaker has sent since the detection: a run opened
        in the room afterwards starts with it. Unlike the server this fake
        does not look at the room's gate first: it sends every wake word a
        test makes, which is what the entity's own gate is tested against.
        """
        self._heard[json.loads(event)["zone"]] = audio
        for queue in self._wake_streams:
            queue.put_nowait(event)

    def drop_wake_streams(self) -> None:
        """Close every stream of wake words and leave the others alone."""
        for queue in self._wake_streams:
            queue.put_nowait(None)

    def open_run(self, zone: str) -> VoiceRun | None:
        """The room's open run, if it has one."""
        return self._runs.get(zone)

    def mic(self, zone: str, pcm: bytes) -> None:
        """Microphone audio arriving in a room: it goes to the room's open run."""
        self._runs[zone].queue.put_nowait(pcm)

    def end_run(self, zone: str, reason: str = "limit") -> None:
        """End a room's run as the server does on its own (its limit, say)."""
        self._end_run(zone, reason)

    def set_mic_muted(self, zone: str, muted: bool) -> None:
        """A speaker's hardware switch: the room's gate closes or goes live."""
        self._zone(zone)["mic_muted"] = muted
        if muted:
            self._end_run(zone, "muted")
        self.model["serial"] += 1
        self.set_model_state(self._encode())

    def end_announcement(
        self, number: int, state: str = "finished", reason: str | None = None
    ) -> None:
        """Say how an announcement ended; the state lists it as over."""
        listed = self.model.get("announcements", [])
        entry = next(a for a in listed if a["id"] == number)
        assert entry["state"] == "playing", entry
        entry["state"] = state
        if reason is not None:
            entry["reason"] = reason
        # The ones that are over are listed in the order they ended, after
        # the ones still playing; the last eight are kept.
        playing = [a for a in listed if a["state"] == "playing"]
        over = [a for a in listed if a["state"] != "playing" and a is not entry]
        self.model["announcements"] = [*playing, *[*over, entry][-8:]]
        self.model["serial"] += 1
        self.set_model_state(self._encode())

    def script(self, status: int, body: bytes) -> None:
        """Answer the next command with this instead of applying it."""
        self.scripted.append((status, body))

    @property
    def bodies(self) -> list[bytes]:
        return [received.body for received in self.commands]

    # --- routes -------------------------------------------------------------

    def _gone(self) -> None:
        if self.refuse_connections:
            raise web.HTTPServiceUnavailable(text="every worker is busy")

    async def _server(self, request: web.Request) -> web.Response:
        self.requests.append("GET /api/server")
        self._gone()
        if self.server_status != 200:
            return web.Response(status=self.server_status, text="no")
        return web.Response(
            body=self.server_bytes,
            content_type="application/json",
            headers={"Connection": "close"},
        )

    async def _state(self, request: web.Request) -> web.Response:
        self.requests.append("GET /api/state")
        self._gone()
        if self.state_status != 200:
            return web.Response(status=self.state_status, text="no")
        return web.Response(
            body=self.state_bytes,
            content_type="application/json",
            headers={"Connection": "close"},
        )

    async def _metrics(self, request: web.Request) -> web.Response:
        self.requests.append("GET /metrics")
        self._gone()
        if self.metrics_status != 200:
            return web.Response(status=self.metrics_status, text="no")
        body = self.metrics_bytes
        if body is None:
            body = self._bare_metrics()
        return web.Response(
            body=body,
            headers={
                "Content-Type": "text/plain; version=0.0.4; charset=utf-8",
                "Connection": "close",
            },
        )

    def _bare_metrics(self) -> bytes:
        """The scrape of a server whose speakers have reported no telemetry."""
        speakers = json.loads(self.state_bytes).get("speakers", [])
        lines = [
            'chorus_server_build_info{version="0.1.0"} 1',
            f"chorus_speakers {len(speakers)}",
        ]
        lines += [
            f'chorus_speaker_connected{{speaker="{s["id"]}"}} {int(s["present"])}'
            for s in speakers
        ]
        for speaker in speakers:
            running = speaker.get("firmware")
            version = running["version"] if running else speaker.get("software", "")
            if version:
                lines.append(
                    f'chorus_speaker_firmware_info{{speaker="{speaker["id"]}",'
                    f'version="{version}"}} 1'
                )
        return ("\n".join(lines) + "\n").encode()

    async def _events(self, request: web.Request) -> web.StreamResponse:
        self.requests.append("GET /api/events")
        self._gone()
        if self.events_status != 200:
            return web.Response(status=self.events_status, text="no")
        response = web.StreamResponse(
            headers={"Content-Type": "text/event-stream", "Connection": "close"}
        )
        await response.prepare(request)
        queue: asyncio.Queue[bytes | None] = asyncio.Queue()
        self._streams.append(queue)
        try:
            await response.write(b"data: " + self.state_bytes + b"\n\n")
            while True:
                try:
                    item = await asyncio.wait_for(queue.get(), 0.02)
                except TimeoutError:
                    # Notice a subscriber that went away, as the server does.
                    if request.transport is None or request.transport.is_closing():
                        break
                    continue
                if item is None:
                    break
                await response.write(b"data: " + item + b"\n\n")
        finally:
            self._streams.remove(queue)
        return response

    async def _controller_events(self, request: web.Request) -> web.StreamResponse:
        self.requests.append("GET /api/controller-events")
        self._gone()
        if self.controller_events_status != 200:
            return web.Response(status=self.controller_events_status, text="no")
        response = web.StreamResponse(
            headers={"Content-Type": "text/event-stream", "Connection": "close"}
        )
        await response.prepare(request)
        queue: asyncio.Queue[bytes | None] = asyncio.Queue()
        self._press_streams.append(queue)
        try:
            # One comment line and no message: a press is not a state.
            await response.write(b": controller events\n\n")
            while True:
                try:
                    item = await asyncio.wait_for(queue.get(), 0.02)
                except TimeoutError:
                    if request.transport is None or request.transport.is_closing():
                        break
                    continue
                if item is None:
                    break
                await response.write(b"data: " + item + b"\n\n")
        finally:
            self._press_streams.remove(queue)
        return response

    async def _visualizer(self, request: web.Request) -> web.StreamResponse:
        self.requests.append("GET /api/visualizer")
        self._gone()
        if self.visualizer_status != 200:
            return web.Response(status=self.visualizer_status, text="no")
        zone = request.query.get("zone")
        if zone is None:
            return web.Response(status=400, text="no zone")
        if zone not in {z["id"] for z in json.loads(self.state_bytes)["zones"]}:
            return web.Response(status=404, text="no such room")
        response = web.StreamResponse(
            headers={"Content-Type": "text/event-stream", "Connection": "close"}
        )
        await response.prepare(request)
        queue: asyncio.Queue[bytes | None] = asyncio.Queue()
        stream = (zone, queue)
        self._light_streams.append(stream)
        try:
            # One comment line and no frame until the room plays something.
            await response.write(b": visualizer\n\n")
            while True:
                try:
                    item = await asyncio.wait_for(queue.get(), 0.02)
                except TimeoutError:
                    if request.transport is None or request.transport.is_closing():
                        break
                    continue
                if item is None:
                    break
                await response.write(b"data: " + item + b"\n\n")
        finally:
            self._light_streams.remove(stream)
        return response

    async def _voice_events(self, request: web.Request) -> web.StreamResponse:
        self.requests.append("GET /api/voice-events")
        self._gone()
        if self.voice_events_status != 200:
            return web.Response(status=self.voice_events_status, text="no")
        response = web.StreamResponse(
            headers={"Content-Type": "text/event-stream", "Connection": "close"}
        )
        await response.prepare(request)
        queue: asyncio.Queue[bytes | None] = asyncio.Queue()
        self._wake_streams.append(queue)
        try:
            # One comment line and no message: a wake word is not a state.
            await response.write(b": voice events\n\n")
            while True:
                try:
                    item = await asyncio.wait_for(queue.get(), 0.02)
                except TimeoutError:
                    if request.transport is None or request.transport.is_closing():
                        break
                    continue
                if item is None:
                    break
                await response.write(b"data: " + item + b"\n\n")
        finally:
            self._wake_streams.remove(queue)
        return response

    def _refuse_voice(self, status: int, reason: str, detail: str) -> web.Response:
        self.voice_refusals.append(reason)
        return web.Response(
            status=status,
            body=json.dumps(
                {"v": 1, "t": "error", "field": "", "detail": f"{reason}: {detail}"},
                separators=(",", ":"),
            ).encode(),
            content_type="application/json",
        )

    async def _voice_audio(self, request: web.Request) -> web.StreamResponse:
        """One run's audio, under the route's four rules, in the server's order."""
        self.requests.append("GET /api/voice-audio")
        self._gone()
        if self.voice_integration is None or request.remote != self.voice_integration:
            return self._refuse_voice(
                403,
                "not-the-voice-integration",
                "a voice run's audio is served to the address this server was "
                "started with (--voice-integration) and to no other",
            )
        named = request.query.get("run", "")
        if not named:
            return self._refuse_voice(400, "no-run-named", "name the run")
        run = next((r for r in self._runs.values() if r.id == named), None)
        if run is None:
            return self._refuse_voice(
                404, "no-voice-run", "no voice run with that identifier is open"
            )
        if run.claimed:
            return self._refuse_voice(
                409, "voice-run-taken", "this voice run has its reader already"
            )
        run.claimed = True
        response = web.StreamResponse(
            headers={
                "Content-Type": "application/octet-stream",
                "X-Chorus-Audio-Format": VOICE_AUDIO_FORMAT,
                "Cache-Control": "no-store",
                "Connection": "close",
            }
        )
        await response.prepare(request)
        try:
            while True:
                try:
                    item = await asyncio.wait_for(run.queue.get(), 0.02)
                except TimeoutError:
                    if request.transport is None or request.transport.is_closing():
                        self._end_run(run.zone, "reader-gone", run)
                        break
                    continue
                if item is None:
                    break
                await response.write(item)
        except ConnectionError:
            self._end_run(run.zone, "reader-gone", run)
        return response

    def _end_run(self, zone: str, reason: str, only: VoiceRun | None = None) -> None:
        """End a room's run, if it has one (and it is `only`, when given)."""
        run = self._runs.get(zone)
        if run is None or (only is not None and run is not only):
            return
        del self._runs[zone]
        self.runs_ended.append((zone, reason))
        run.queue.put_nowait(None)

    def _voice_start(self, command: dict[str, Any]) -> bytes:
        """`voice_start`: a run, or one of the refusals by name."""
        name = command["zone"]
        zone = self._zone(name)
        if not zone.get("voice_enabled", False):
            raise Refusal(
                "zone",
                f"voice-disabled: room '{name}' has voice switched off, so nothing "
                "listens there; voice_enabled switches it on",
            )
        if zone.get("mic_muted", True):
            raise Refusal(
                "zone",
                f"mic-muted: no microphone in room '{name}' reports its gate live "
                "(the mute switch is the speaker's own, and no command opens it)",
            )
        if self.voice_integration is None:
            raise Refusal(
                "t",
                "no-voice-integration: this server was started without "
                "--voice-integration, so no address may read a voice run's audio "
                "and none is opened",
            )
        self._end_run(name, "superseded")
        run = VoiceRun(name, secrets.token_hex(16))
        # What the speaker has sent since the wake word, when one was heard.
        if heard := self._heard.pop(name, b""):
            run.queue.put_nowait(heard)
        self._runs[name] = run
        self.run_ids.append(run.id)
        return json.dumps(
            {
                "v": 2,
                "t": "voice_run",
                "zone": name,
                "run": run.id,
                "limit_ms": self.voice_run_limit_ms,
            },
            separators=(",", ":"),
        ).encode()

    async def _command(self, request: web.Request) -> web.Response:
        body = await request.read()
        self.requests.append("POST /api/command")
        self._gone()
        self.commands.append(Received(body, dict(request.headers)))
        if request.headers.get("Content-Type") != "application/json":
            return web.Response(status=415, text="application/json only")
        if "Origin" in request.headers:
            return web.Response(status=403, text="not this server's origin")
        if self.scripted:
            status, answer = self.scripted.pop(0)
            return web.Response(
                status=status, body=answer, content_type="application/json"
            )
        command = json.loads(body)
        answer: bytes | None = None
        try:
            if command["t"] == "voice_start":
                # Answered with a `voice_run` message, not a state.
                answer = self._voice_start(command)
            else:
                number = self._apply(command)
        except Refusal as refusal:
            return web.Response(
                status=400,
                body=json.dumps(
                    {
                        "v": 2,
                        "t": "error",
                        "field": refusal.field,
                        "detail": refusal.detail,
                    },
                    separators=(",", ":"),
                ).encode(),
                content_type="application/json",
            )
        if answer is not None:
            return web.Response(body=answer, content_type="application/json")
        state = self._encode()
        self.set_model_state(state)
        if number is not None:
            # An `announce` is answered with the state and one more member.
            state = state[:-1] + b',"announcement":%d}' % number
        return web.Response(body=state, content_type="application/json")

    def set_model_state(self, state: bytes) -> None:
        self.state_bytes = state
        for queue in self._streams:
            queue.put_nowait(state)

    # --- the house model ----------------------------------------------------

    @property
    def model(self) -> dict[str, Any]:
        if self._model is None:
            self._model = json.loads(self.state_bytes)
        return self._model

    def _encode(self) -> bytes:
        return json.dumps(
            self.model, separators=(",", ":"), ensure_ascii=False
        ).encode()

    def _zone(self, zone_id: str, field_name: str = "zone") -> dict[str, Any]:
        for zone in self.model["zones"]:
            if zone["id"] == zone_id:
                return zone
        raise Refusal(field_name, f"'{zone_id}' is not a room")

    def _saved(self, group_id: str) -> dict[str, Any] | None:
        return next(
            (g for g in self.model["saved_groups"] if g["id"] == group_id), None
        )

    def _sources(self) -> dict[str, str]:
        return {g["id"]: g["source"] for g in self.model["groups"]}

    def _regroup(self, sources: dict[str, str]) -> None:
        """Rebuild `groups` from each room's `group`, dissolving lone live groups."""
        model = self.model
        zones = model["zones"]
        saved_ids = {g["id"] for g in model["saved_groups"]}
        room_ids = {z["id"] for z in zones}
        members: dict[str, list[dict[str, Any]]] = {}
        for zone in zones:
            members.setdefault(zone["group"], []).append(zone)
        for group_id, rooms in list(members.items()):
            if (
                len(rooms) == 1
                and group_id not in saved_ids
                and group_id not in room_ids
            ):
                # A live group left with one room dissolves into the room's own.
                room = rooms[0]
                sources[room["id"]] = sources.get(group_id, "stream")
                room["group"] = room["id"]
        members = {}
        for zone in zones:
            members.setdefault(zone["group"], []).append(zone)
        groups = []
        for group_id, rooms in members.items():
            if group_id in saved_ids:
                kind = "saved"
            elif len(rooms) == 1 and rooms[0]["id"] == group_id:
                kind = "room"
            else:
                kind = "live"
            total = sum(_half_up(r["volume"] * 1000) for r in rooms)
            groups.append(
                {
                    "id": group_id,
                    "kind": kind,
                    "zones": [r["id"] for r in rooms],
                    "volume": _half_up(total / len(rooms)) / 1000,
                    "source": sources.get(group_id, "stream"),
                    "audio": rooms[0]["audio"],
                }
            )
        model["groups"] = groups
        for saved in model["saved_groups"]:
            saved["active"] = all(
                self._zone(z)["group"] == saved["id"] for z in saved["zones"]
            )
        model["serial"] += 1

    def _live_id(self) -> str:
        taken = {z["id"] for z in self.model["zones"]}
        taken |= {z["group"] for z in self.model["zones"]}
        taken |= {g["id"] for g in self.model["saved_groups"]}
        n = 1
        while f"live-{n}" in taken:
            n += 1
        return f"live-{n}"

    def _formed(self, group_id: str) -> list[dict[str, Any]]:
        rooms = [z for z in self.model["zones"] if z["group"] == group_id]
        if not rooms:
            raise Refusal("group", f"no room is in a group '{group_id}'")
        return rooms

    def _set_volume(self, zone: dict[str, Any], thousandths: int) -> None:
        cap = _half_up(zone["effective_limit"] * 1000)
        zone["volume"] = max(0, min(cap, thousandths)) / 1000

    def _group_volume(self, group_id: str, wanted: int) -> None:
        rooms = self._formed(group_id)
        wanted = max(0, min(1000, wanted))
        current = _half_up(
            sum(_half_up(r["volume"] * 1000) for r in rooms) / len(rooms)
        )
        for room in rooms:
            if current == 0:
                self._set_volume(room, wanted)
            else:
                self._set_volume(
                    room, _half_up(_half_up(room["volume"] * 1000) * wanted / current)
                )

    def _firmware_install(self, command: dict[str, Any]) -> None:
        """`firmware_install` for one speaker, with the server's refusals by name."""
        speaker_id, name = command["speaker"], command["image"]
        speaker = next(
            (s for s in self.model.get("speakers", []) if s["id"] == speaker_id), None
        )
        if speaker is None:
            raise Refusal("speaker", f"'{speaker_id}' is not an adopted speaker")
        images = self.model.get("firmware", {}).get("images", [])
        image = next((i for i in images if i["name"] == name), None)
        if image is None:
            raise Refusal("image", f"unknown-image: no staged image '{name}'")
        if image["verdict"] != "verified":
            raise Refusal(
                "image",
                f"image-not-verified: image '{name}' was refused "
                f"({image['reason']}) and is never offered",
            )
        running = speaker.get("firmware")
        if running is None:
            raise Refusal(
                "speaker", f"not-updatable: speaker '{speaker_id}' reported no version"
            )
        if not speaker["present"]:
            raise Refusal(
                "speaker",
                f"speaker-absent: speaker '{speaker_id}' has no session that "
                "takes updates",
            )
        if speaker_id in self.remote_speakers:
            raise Refusal(
                "speaker",
                f"owner-not-at-bench: speaker '{speaker_id}' is at 192.0.2.7, which "
                "is not this host; a transfer to a real device is the owner's "
                "action at the bench (docs/firmware-updates.md)",
            )
        if running["state"] in ("requested", "receiving", "verified", "pending_verify"):
            raise Refusal(
                "speaker",
                f"busy: speaker '{speaker_id}' has an install in progress "
                f"(its firmware state is {running['state']})",
            )
        if image["board"] != running["board"]:
            raise Refusal(
                "speaker", f"wrong-board: image '{name}' is for {image['board']}"
            )
        if image["version"] == running["version"] and not command.get("force"):
            raise Refusal(
                "speaker", f"already-running: speaker '{speaker_id}' runs it already"
            )
        running |= {
            "state": "requested",
            "reason": "none",
            "image": name,
            "image_version": image["version"],
            "received": 0,
            "size": image["size"],
        }
        self.model["serial"] += 1

    def _announce(self, command: dict[str, Any]) -> int:
        """`announce`: number it and list it as playing in the rooms that hear it."""
        target = command["target"]
        zones = self.model["zones"]
        saved = self._saved(target)
        if saved is not None:
            rooms = list(saved["zones"])
        elif any(z["id"] == target for z in zones):
            rooms = [target]
        else:
            rooms = [z["id"] for z in zones if z["group"] == target]
            if not rooms:
                raise Refusal(
                    "target",
                    f"'{target}' is not a room, a saved group or a formed group",
                )
        self._announcements += 1
        number = self._announcements
        self.model.setdefault("announcements", []).insert(
            0, {"id": number, "target": target, "rooms": rooms, "state": "playing"}
        )
        if self.announcements_end_at_once is not None:
            entry = self.model["announcements"].pop(0)
            entry["state"] = self.announcements_end_at_once
            self.model["announcements"] = [*self.model["announcements"], entry][-8:]
        return number

    def _apply(self, command: dict[str, Any]) -> int | None:
        """Apply a command; return the number of the announcement it started."""
        kind = command["t"]
        number: int | None = None
        if kind == "firmware_install":
            self._firmware_install(command)
            return None
        if kind == "voice_stop":
            # A room with no run is not an error; the state is as it stands.
            self._zone(command["zone"])
            self._end_run(command["zone"], "stopped")
            return None
        sources = self._sources()
        if kind == "volume":
            self._set_volume(
                self._zone(command["zone"]), _half_up(command["volume"] * 1000)
            )
        elif kind == "volume_step":
            zone = self._zone(command["zone"])
            self._set_volume(
                zone,
                max(0, min(1000, _half_up(zone["volume"] * 1000) + command["step"])),
            )
        elif kind == "mute":
            self._zone(command["zone"])["muted"] = command["muted"]
        elif kind == "group_volume":
            self._group_volume(command["group"], _half_up(command["volume"] * 1000))
        elif kind == "group_volume_step":
            rooms = self._formed(command["group"])
            current = _half_up(
                sum(_half_up(r["volume"] * 1000) for r in rooms) / len(rooms)
            )
            self._group_volume(command["group"], current + command["step"])
        elif kind == "join":
            zone = self._zone(command["zone"])
            target = command["target"]
            if target in {z["id"] for z in self.model["zones"]}:
                leader = self._zone(target)
                if (
                    leader["group"] == leader["id"]
                    and len(self._formed(leader["id"])) == 1
                ):
                    live = self._live_id()
                    sources[live] = sources.get(leader["id"], "stream")
                    leader["group"] = live
                zone["group"] = leader["group"]
            else:
                self._formed(target)
                zone["group"] = target
        elif kind == "take":
            target = command["target"]
            saved = self._saved(target)
            if saved is not None:
                for zone_id in saved["zones"]:
                    self._zone(zone_id)["group"] = target
                group_id = target
            elif target in {z["id"] for z in self.model["zones"]}:
                for other in self.model["zones"]:
                    if other["group"] == target and other["id"] != target:
                        other["group"] = other["id"]
                self._zone(target)["group"] = target
                group_id = target
            elif any(z["group"] == target for z in self.model["zones"]):
                group_id = target
            else:
                raise Refusal(
                    "target",
                    f"'{target}' is not a room, a saved group or a formed group",
                )
            if "source" in command:
                source = command["source"]
                if source.startswith("line-in:") and (
                    source.removeprefix("line-in:") not in self.model["inputs"]
                ):
                    raise Refusal("source", f"'{source}' is not offered")
                sources[group_id] = source
        elif kind == "playback":
            target = command["target"]
            zone_group = next(
                (z["group"] for z in self.model["zones"] if z["id"] == target),
                target,
            )
            if not sources.get(zone_group, "stream").startswith("soloist:"):
                raise Refusal(
                    "target", f"'{target}' plays something that is not a receiver"
                )
        elif kind == "announce":
            allowed = json.loads(self.server_bytes)["announce_origins"]
            if not any(command["url"].startswith(origin + "/") for origin in allowed):
                raise Refusal("url", "the origin is not one of the announce origins")
            number = self._announce(command)
        elif kind == "voice_enabled":
            zone = self._zone(command["zone"])
            zone["voice_enabled"] = command["enabled"]
            if not command["enabled"]:
                self._end_run(command["zone"], "voice-disabled")
        elif kind == "sound":
            sound = self._zone(command["zone"])["sound"]
            for key in ("bass", "treble"):
                if key in command and not -10 <= command[key] <= 10:
                    raise Refusal(
                        key,
                        f"the field '{key}' is {command[key]} and the catalog "
                        "declares a whole number from -10 to 10",
                    )
            for key in ("bass", "treble", "loudness", "night", "speech"):
                if key in command:
                    sound[key] = command[key]
        elif kind == "quiet_hours_enabled":
            zone = self._zone(command["zone"])
            zone["quiet_enabled"] = command["enabled"]
            # Off: no window caps the room. On inside a window: the cap applies
            # at once and the volume is pulled down to it.
            caps = [
                window["limit"]
                for window in zone["quiet"]
                if window["active"] and command["enabled"]
            ]
            zone["effective_limit"] = min([zone["limit"], *caps])
            zone["volume"] = min(zone["volume"], zone["effective_limit"])
        elif kind == "autoplay":
            endpoint, _, name = command["input"].partition("/")
            if not endpoint or not name:
                raise Refusal(
                    "input",
                    f"'{command['input']}' is not an input: '<endpoint>/<input>'",
                )
            target = command["target"]
            if self._saved(target) is None:
                self._zone(target, "target")
            rule = {k: v for k, v in command.items() if k not in ("v", "t")}
            rules = [r for r in self.model["autoplay"] if r["input"] != rule["input"]]
            self.model["autoplay"] = sorted([*rules, rule], key=lambda r: r["input"])
        else:
            raise Refusal("t", f"'{kind}' is not a command of this fake")
        self._regroup(sources)
        return number


class Refusal(Exception):
    """A command the model refuses, as the server's `error` message."""

    def __init__(self, field_name: str, detail: str) -> None:
        super().__init__(detail)
        self.field = field_name
        self.detail = detail
