"""A fake chorus control plane on loopback, driven by the shared vectors.

It serves the repository's `fixtures/control/v2/` bytes (the state vectors and
the server's identity, read from the repository and never copied), records
every command it is sent byte for byte (with its headers), and applies the
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


def _half_up(value: float) -> int:
    return int(value + 0.5)


@dataclass
class Received:
    """One request to `POST /api/command`."""

    body: bytes
    headers: dict[str, str]


@dataclass
class FakeChorusServer:
    """The fake: routes, a recorded command list and a small house model."""

    state_bytes: bytes
    server_bytes: bytes = field(default_factory=lambda: shared("server.json"))
    commands: list[Received] = field(default_factory=list)
    scripted: list[tuple[int, bytes]] = field(default_factory=list)
    requests: list[str] = field(default_factory=list)
    refuse_connections: bool = False
    server_status: int = 200
    state_status: int = 200
    events_status: int = 200
    port: int = 0
    _streams: list[asyncio.Queue[bytes | None]] = field(default_factory=list)
    _runner: web.AppRunner | None = None
    _model: dict[str, Any] | None = None

    # --- life cycle ---------------------------------------------------------

    async def start(self, port: int = 0) -> None:
        app = web.Application()
        app.router.add_get("/api/server", self._server)
        app.router.add_get("/api/state", self._state)
        app.router.add_get("/api/events", self._events)
        app.router.add_post("/api/command", self._command)
        self._runner = web.AppRunner(app, shutdown_timeout=0.1)
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
        """Close every event stream, as a server going away does."""
        for queue in self._streams:
            queue.put_nowait(None)

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
        try:
            self._apply(json.loads(body))
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
        state = self._encode()
        self.set_model_state(state)
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

    def _apply(self, command: dict[str, Any]) -> None:
        kind = command["t"]
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
        else:
            raise Refusal("t", f"'{kind}' is not a command of this fake")
        self._regroup(sources)


class Refusal(Exception):
    """A command the model refuses, as the server's `error` message."""

    def __init__(self, field_name: str, detail: str) -> None:
        super().__init__(detail)
        self.field = field_name
        self.detail = detail
