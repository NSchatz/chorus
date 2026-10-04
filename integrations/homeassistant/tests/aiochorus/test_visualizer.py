"""The reader of `GET /api/visualizer?zone=<room>`, against the fake server on loopback.

The frame is the repository's shared vector (`fixtures/visualizer/http-frame.json`
and its `.fields`), read from the repository and never copied: the server's
encoder is held to the same bytes (`crates/server/src/lights.rs`).
"""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator
import json
from typing import Any

import aiohttp
import pytest

from custom_components.chorus._aiochorus import (
    ChorusClient,
    ChorusError,
    ChorusProtocolError,
    VisualizerFrame,
)

from ..conftest import wait_for
from ..fake_server import REPO, FakeChorusServer

VECTORS = REPO / "fixtures" / "visualizer"
FRAME = (VECTORS / "http-frame.json").read_bytes().removesuffix(b"\n")


@pytest.fixture
async def session(socket_enabled: None) -> AsyncIterator[aiohttp.ClientSession]:
    async with aiohttp.ClientSession() as client_session:
        yield client_session


@pytest.fixture
def client(session: aiohttp.ClientSession, server: FakeChorusServer) -> ChorusClient:
    # The shared frame is the den's: the fake's kitchen is the den here.
    server.state_bytes = server.state_bytes.replace(b'"kitchen"', b'"den"')
    return ChorusClient(session, "127.0.0.1", server.port)


def fields() -> dict[str, str]:
    """Read the vector's canonical input: one `key = value` per line."""
    out: dict[str, str] = {}
    for line in (VECTORS / "http-frame.fields").read_text().splitlines():
        if line.lstrip().startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        out[key.strip()] = value.strip()
    return out


def changed(**members: Any) -> bytes:
    return json.dumps(json.loads(FRAME) | members, separators=(",", ":")).encode()


def test_visualizer_frame_reads_the_shared_vector() -> None:
    """Every member a lamp needs is read, as the vector's `.fields` file says."""
    want = fields()
    assert want["message_type"] == "visualizer"
    assert VisualizerFrame.parse(FRAME) == VisualizerFrame(
        zone=want["zone"],
        lead_ms=int(want["lead_ms"]),
        peak=int(want["peak"]),
        beat=int(want["beat"]),
        red=int(want["red"]),
        green=int(want["green"]),
        blue=int(want["blue"]),
        brightness=int(want["brightness"]),
        transition_ms=int(want["transition_ms"]),
    )
    # The stream's own bytes are that message on one `data:` line.
    assert (VECTORS / "http-frame.sse").read_bytes() == b"data: " + FRAME + b"\n\n"


def test_visualizer_frame_silent_is_no_level_and_no_beat() -> None:
    assert not VisualizerFrame.parse(FRAME).silent
    assert not VisualizerFrame.parse(changed(peak=0)).silent
    assert not VisualizerFrame.parse(changed(beat=0)).silent
    # A frame the room has heard already (a late one) still parses.
    assert VisualizerFrame.parse(changed(peak=0, beat=0, lead_ms=-12)).silent


@pytest.mark.parametrize(
    "message",
    [
        b"[]",
        b"not json",
        changed(t="state"),
        changed(v=1),
        changed(zone=7),
        changed(lead_ms="85"),
        changed(lead_ms=True),
        changed(peak=256),
        changed(peak=-1),
        changed(beat=1.5),
        changed(red=None),
        changed(green="96"),
        changed(blue=True),
        changed(brightness=300),
        changed(transition_ms=-1),
        changed(transition_ms=0.5),
    ],
)
def test_visualizer_frame_malformed_is_a_protocol_error(message: bytes) -> None:
    with pytest.raises(ChorusProtocolError):
        VisualizerFrame.parse(message)


async def test_visualizer_stream_delivers_the_rooms_frames_and_no_other(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    frames: list[VisualizerFrame] = []
    opened: list[None] = []
    lost: list[ChorusError] = []
    task = asyncio.create_task(
        client.visualizer("den").run(
            frames.append, lost.append, lambda: opened.append(None)
        )
    )
    try:
        await wait_for(lambda: server.visualizer_subscribers == ["den"])
        await wait_for(lambda: opened == [None])
        # The stream opens with a comment and no frame.
        assert frames == []
        server.frame(changed(zone="living"))
        server.frame(FRAME)
        await wait_for(lambda: len(frames) == 1)
        assert frames == [VisualizerFrame.parse(FRAME)]
        assert lost == []
    finally:
        task.cancel()
    await wait_for(lambda: server.visualizer_subscribers == [])


async def test_visualizer_stream_reconnects_and_reports_each_loss(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    frames: list[VisualizerFrame] = []
    opened: list[None] = []
    lost: list[ChorusError] = []

    async def no_wait(_seconds: float) -> None:
        await asyncio.sleep(0.01)

    task = asyncio.create_task(
        client.visualizer("den", sleep=no_wait).run(
            frames.append, lost.append, lambda: opened.append(None)
        )
    )
    try:
        await wait_for(lambda: len(opened) == 1)
        server.drop_visualizer_streams()
        await wait_for(lambda: len(lost) == 1 and len(opened) == 2)
        assert "closed the visualizer stream" in str(lost[0])
        # A frame that is not a frame ends the stream too, and it comes back.
        await wait_for(lambda: server.visualizer_subscribers == ["den"])
        server._light_streams[0][1].put_nowait(b'{"v":2,"t":"state"}')
        await wait_for(lambda: len(lost) == 2 and len(opened) == 3)
        assert isinstance(lost[1], ChorusProtocolError)
        # And so does another room's frame on this room's stream.
        await wait_for(lambda: server.visualizer_subscribers == ["den"])
        server._light_streams[0][1].put_nowait(changed(zone="living"))
        await wait_for(lambda: len(lost) == 3 and len(opened) == 4)
        assert "'living' on the stream of 'den'" in str(lost[2])
        await wait_for(lambda: server.visualizer_subscribers == ["den"])
        server.frame(FRAME)
        await wait_for(lambda: len(frames) == 1)
    finally:
        task.cancel()


@pytest.mark.parametrize(("zone", "status"), [("attic", 404)])
async def test_visualizer_stream_of_a_room_the_server_lacks_is_a_loss(
    client: ChorusClient, server: FakeChorusServer, zone: str, status: int
) -> None:
    lost: list[ChorusError] = []
    opened: list[None] = []

    async def no_wait(_seconds: float) -> None:
        await asyncio.sleep(0.01)

    task = asyncio.create_task(
        client.visualizer(zone, sleep=no_wait).run(
            lambda _frame: None, lost.append, lambda: opened.append(None)
        )
    )
    try:
        await wait_for(lambda: len(lost) >= 1)
        assert f"answered {status}" in str(lost[0])
        assert opened == []
        assert server.visualizer_subscribers == []
    finally:
        task.cancel()


async def test_visualizer_stream_probes_a_silent_stream_for_life(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    """A quiet room sends nothing: silence is checked once with the state."""
    lost: list[ChorusError] = []
    task = asyncio.create_task(
        client.visualizer("den", idle_probe=0.05).run(lambda _frame: None, lost.append)
    )
    try:
        await wait_for(lambda: server.requests.count("GET /api/state") >= 2)
        assert lost == []
        assert server.visualizer_subscribers == ["den"]
    finally:
        task.cancel()
