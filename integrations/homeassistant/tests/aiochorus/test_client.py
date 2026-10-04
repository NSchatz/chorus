"""The HTTP client and the event stream, against the fake server on loopback."""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator
import json

import aiohttp
import pytest

from custom_components.chorus._aiochorus import (
    ChorusClient,
    ChorusCommandError,
    ChorusConnectionError,
    ChorusError,
    ChorusProtocolError,
    ChorusRefusedError,
    ChorusUnsupportedError,
    State,
)
from custom_components.chorus._aiochorus.client import Backoff

from ..conftest import wait_for
from ..fake_server import FakeChorusServer, shared


@pytest.fixture
async def session(socket_enabled: None) -> AsyncIterator[aiohttp.ClientSession]:
    async with aiohttp.ClientSession() as client_session:
        yield client_session


@pytest.fixture
def client(session: aiohttp.ClientSession, server: FakeChorusServer) -> ChorusClient:
    return ChorusClient(session, "127.0.0.1", server.port)


def big_state() -> bytes:
    """A valid state message larger than 64 KiB: a house of 150 rooms."""
    raw = json.loads(shared("state-rich.json"))
    zone = raw["zones"][1]
    raw["zones"] = [
        zone | {"id": f"room-{n}", "group": f"room-{n}"} for n in range(150)
    ]
    raw["groups"] = [
        raw["groups"][1] | {"id": f"room-{n}", "kind": "room", "zones": [f"room-{n}"]}
        for n in range(150)
    ]
    raw["saved_groups"] = []
    data = json.dumps(raw, separators=(",", ":")).encode()
    assert len(data) > 64 * 1024
    return data


async def test_server_and_state(client: ChorusClient, server: FakeChorusServer) -> None:
    info = await client.server()
    assert info.id == "chorus-server-0123456789abcdef"
    assert info.catalogs == (1, 2)
    state = await client.state()
    assert state == State.parse(shared("state-rich.json"))


async def test_a_command_is_posted_as_json_with_no_origin(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    state = await client.set_volume("living", 500)
    assert server.bodies == [b'{"v":1,"t":"volume","zone":"living","volume":0.500}']
    headers = server.commands[0].headers
    assert headers["Content-Type"] == "application/json"
    assert "Origin" not in headers
    zone = state.zone("living")
    assert zone is not None
    assert zone.volume == 0.5


async def test_every_helper_sends_its_command(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    await client.set_mute("living", True)
    await client.step_volume("living", -25)
    await client.set_group_volume("downstairs", 400)
    await client.step_group_volume("downstairs", 50)
    await client.join("study", "living")
    await client.take("study")
    await client.take("study", "none")
    await client.announce("study", "http://ha.example:8123/a.mp3", 250)
    server.script(200, shared("state-soloist.json"))
    await client.playback("kitchen", "pause")
    assert server.bodies == [
        b'{"v":1,"t":"mute","zone":"living","muted":true}',
        b'{"v":2,"t":"volume_step","zone":"living","step":-25}',
        b'{"v":2,"t":"group_volume","group":"downstairs","volume":0.400}',
        b'{"v":2,"t":"group_volume_step","group":"downstairs","step":50}',
        b'{"v":2,"t":"join","zone":"study","target":"living"}',
        b'{"v":2,"t":"take","target":"study"}',
        b'{"v":2,"t":"take","target":"study","source":"none"}',
        b'{"v":2,"t":"announce","target":"study","url":"http://ha.example:8123/a.mp3","volume":0.250}',
        b'{"v":2,"t":"playback","target":"kitchen","action":"pause"}',
    ]


async def test_an_error_names_its_field(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    server.script(400, shared("error-take-unknown-target.json"))
    with pytest.raises(ChorusCommandError) as caught:
        await client.take("attic")
    assert caught.value.field == "target"
    assert "attic" in caught.value.detail

    server.script(400, shared("error-announce-origin.json"))
    with pytest.raises(ChorusCommandError) as caught:
        await client.announce("kitchen", "http://elsewhere.example:8123/a.mp3")
    assert caught.value.field == "url"

    server.script(400, shared("error-malformed.json"))
    with pytest.raises(ChorusCommandError) as caught:
        await client.command(b"{")
    assert caught.value.field == ""


async def test_refused_and_other_answers(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    server.script(426, shared("refused-unknown-version.json"))
    with pytest.raises(ChorusRefusedError) as caught:
        await client.take("kitchen")
    assert caught.value.offered == 9
    assert caught.value.implemented == (1, 2)
    assert isinstance(caught.value, ChorusUnsupportedError)

    server.script(426, b'{"v":1,"t":"refused","field":"v","offered":null}')
    with pytest.raises(ChorusRefusedError) as caught:
        await client.take("kitchen")
    assert (caught.value.offered, caught.value.implemented) == (None, ())

    server.script(400, b'{"v":2,"t":"state"}')
    with pytest.raises(ChorusProtocolError, match="not an error message"):
        await client.take("kitchen")
    server.script(400, b'{"v":2,"t":"error","field":"x"}')
    with pytest.raises(ChorusCommandError) as command_error:
        await client.take("kitchen")
    assert command_error.value.detail == ""
    server.script(503, b"busy")
    with pytest.raises(ChorusProtocolError, match="answered 503"):
        await client.take("kitchen")


async def test_get_failures(client: ChorusClient, server: FakeChorusServer) -> None:
    server.server_status = 404
    with pytest.raises(ChorusUnsupportedError, match="no /api/server"):
        await client.server()
    server.server_status = 500
    with pytest.raises(ChorusProtocolError, match="answered 500"):
        await client.server()
    server.state_status = 503
    with pytest.raises(ChorusProtocolError, match="answered 503"):
        await client.state()


async def test_nothing_listening_is_a_connection_error(
    session: aiohttp.ClientSession, server: FakeChorusServer
) -> None:
    port = server.port
    await server.stop()
    gone = ChorusClient(session, "127.0.0.1", port)
    with pytest.raises(ChorusConnectionError):
        await gone.state()
    with pytest.raises(ChorusConnectionError):
        await gone.take("kitchen")


async def test_an_answer_past_the_bound_is_refused(
    client: ChorusClient, server: FakeChorusServer, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(
        "custom_components.chorus._aiochorus.client.MAX_EVENT_BYTES", 100
    )
    with pytest.raises(ChorusProtocolError, match="larger than 100 bytes"):
        await client.state()


async def test_a_state_larger_than_64_kib(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    """Neither the one-shot read nor the stream depends on a line limit."""
    big = big_state()
    server.state_bytes = big
    assert len((await client.state()).zones) == 150

    states: list[State] = []
    task = asyncio.create_task(client.events().run(states.append, lambda err: None))
    await wait_for(lambda: len(states) == 1)
    server.set_state(shared("state-rich.json"))
    server.set_state(big)
    await wait_for(lambda: len(states) == 3)
    task.cancel()
    assert [len(s.zones) for s in states] == [150, 4, 150]


def test_backoff_doubles_to_a_ceiling_with_jitter() -> None:
    full = Backoff(lambda: 1.0)
    assert [full.next() for _ in range(9)] == [1, 2, 4, 8, 16, 32, 60, 60, 60]
    half = Backoff(lambda: 0.0)
    assert [half.next() for _ in range(8)] == [0.5, 1, 2, 4, 8, 16, 30, 30]
    full.reset()
    assert full.next() == 1


async def test_reconnect_backs_off_on_a_fake_clock(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    """Delays grow while the server refuses, and start over once it answered."""
    delays: list[float] = []
    states: list[State] = []
    lost: list[ChorusError] = []
    server.events_status = 503

    async def fake_sleep(delay: float) -> None:
        delays.append(delay)
        if len(delays) == 4:
            server.events_status = 200
        await asyncio.sleep(0)

    stream = client.events(sleep=fake_sleep, rand=lambda: 1.0)
    task = asyncio.create_task(stream.run(states.append, lost.append))
    await wait_for(lambda: len(states) == 1)
    assert delays == [1, 2, 4, 8]
    assert len(lost) == 4
    assert "answered 503" in str(lost[0])

    # The stream that worked reset the delays: losing it waits the first step.
    server.events_status = 503
    server.drop_streams()
    await wait_for(lambda: len(delays) >= 6)
    assert delays[4:6] == [1, 2]
    assert "closed the event stream" in str(lost[4])
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task


async def test_silence_is_probed_and_a_dead_server_ends_the_stream(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    states: list[State] = []
    lost: list[ChorusError] = []
    stream = client.events(idle_probe=0.05)
    task = asyncio.create_task(stream.run(states.append, lost.append))
    await wait_for(lambda: len(states) >= 2)
    assert server.requests.count("GET /api/state") >= 1
    assert not lost
    server.state_status = 500
    await wait_for(lambda: len(lost) == 1)
    assert "answered 500" in str(lost[0])
    task.cancel()


async def test_a_stream_that_is_not_a_state_is_a_lost_stream(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    server.state_bytes = b"not a state"
    lost: list[ChorusError] = []
    task = asyncio.create_task(client.events().run(lambda s: None, lost.append))
    await wait_for(lambda: len(lost) == 1)
    assert isinstance(lost[0], ChorusProtocolError)
    task.cancel()


async def test_a_connection_refused_is_a_lost_stream(
    session: aiohttp.ClientSession, server: FakeChorusServer
) -> None:
    port = server.port
    await server.stop()
    lost: list[ChorusError] = []
    stream = ChorusClient(session, "127.0.0.1", port).events()
    task = asyncio.create_task(stream.run(lambda s: None, lost.append))
    await wait_for(lambda: len(lost) == 1)
    assert isinstance(lost[0], ChorusConnectionError)
    assert "was lost" in str(lost[0])
    task.cancel()
