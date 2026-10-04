"""The reader of `GET /api/controller-events`, against the fake server on loopback.

The messages are the repository's shared vectors
(`fixtures/control/v2/controller_event*.json`), read through `shared` and
never copied: the server's encoder is held to the same bytes.
"""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator

import aiohttp
import pytest

from custom_components.chorus._aiochorus import (
    BUTTONS,
    LONG_PRESS,
    PRESS,
    ChorusClient,
    ChorusConnectionError,
    ChorusError,
    ChorusProtocolError,
    ControllerEvent,
)

from ..conftest import wait_for
from ..fake_server import SHARED_V2, FakeChorusServer, shared

VECTORS = ("controller_event", "controller_event-transport")


@pytest.fixture
async def session(socket_enabled: None) -> AsyncIterator[aiohttp.ClientSession]:
    async with aiohttp.ClientSession() as client_session:
        yield client_session


@pytest.fixture
def client(session: aiohttp.ClientSession, server: FakeChorusServer) -> ChorusClient:
    return ChorusClient(session, "127.0.0.1", server.port)


def fields(name: str) -> dict[str, str]:
    """Read a vector's canonical input: one `key = value` per line."""
    out: dict[str, str] = {}
    for line in (SHARED_V2 / f"{name}.fields").read_text().splitlines():
        if line.lstrip().startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        out[key.strip()] = value.strip()
    return out


def event(command: str, value: int = 0, target: str = "") -> ControllerEvent:
    return ControllerEvent("endpoint-a", "kitchen", command, value, target, "applied")


@pytest.mark.parametrize("name", VECTORS)
def test_controller_event_reads_the_shared_vector(name: str) -> None:
    """Every member of the shared vector is read, as its `.fields` file says."""
    want = fields(name)
    assert want["message_type"] == "controller_event"
    got = ControllerEvent.parse(shared(f"{name}.json"))
    assert got == ControllerEvent(
        endpoint=want["endpoint"],
        zone=want["zone"],
        command=want["command"],
        value=int(want["value"]),
        target=want["target"],
        outcome=want["outcome"],
    )


def test_controller_event_vectors_are_the_two_documented_presses() -> None:
    down = ControllerEvent.parse(shared("controller_event.json"))
    assert (down.endpoint, down.command, down.value) == (
        "endpoint-a",
        "volume_step",
        -5,
    )
    assert down.button() == ("volume_down", PRESS)
    toggle = ControllerEvent.parse(shared("controller_event-transport.json"))
    assert toggle.outcome == "waits-for-an-input"
    assert toggle.button() == ("play_pause", PRESS)


@pytest.mark.parametrize(
    ("command", "value", "want"),
    [
        ("toggle", 0, ("play_pause", PRESS)),
        ("join", 0, ("play_pause", LONG_PRESS)),
        ("leave", 0, ("play_pause", LONG_PRESS)),
        ("volume_step", 5, ("volume_up", PRESS)),
        ("volume_step", -5, ("volume_down", PRESS)),
        ("next", 0, ("next", PRESS)),
        ("previous", 0, ("previous", PRESS)),
        # What no button of a speaker sends.
        ("volume_step", 0, None),
        ("play", 0, None),
        ("pause", 0, None),
        ("volume_set", 40, None),
        ("mute_set", 1, None),
        ("a_later_command", 0, None),
    ],
)
def test_controller_event_names_its_button(
    command: str, value: int, want: tuple[str, str] | None
) -> None:
    got = event(command, value).button()
    assert got == want
    assert got is None or got[0] in BUTTONS


@pytest.mark.parametrize(
    "text",
    [
        b"not json",
        b"[]",
        shared("state-rich.json"),
        shared("controller_event.json").replace(b'"v":2', b'"v":3'),
        shared("controller_event.json").replace(b'"value":-5', b'"value":"-5"'),
        shared("controller_event.json").replace(b'"value":-5', b'"value":true'),
        shared("controller_event.json").replace(b'"endpoint":"endpoint-a",', b""),
        shared("controller_event.json").replace(b'"zone":"kitchen"', b'"zone":7'),
    ],
)
def test_controller_event_that_is_not_one_is_refused(text: bytes) -> None:
    with pytest.raises(ChorusProtocolError):
        ControllerEvent.parse(text)


def test_controller_event_ignores_a_member_it_does_not_know() -> None:
    text = shared("controller_event.json").replace(b"}", b',"later":[1]}')
    assert ControllerEvent.parse(text) == ControllerEvent.parse(
        shared("controller_event.json")
    )


async def test_controller_event_stream_delivers_each_press_once(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    got: list[ControllerEvent] = []
    opened: list[None] = []
    stream = client.controller_events()
    task = asyncio.create_task(
        stream.run(got.append, lambda err: None, lambda: opened.append(None))
    )
    await wait_for(lambda: server.press_subscribers == 1)
    await wait_for(lambda: len(opened) == 1)
    # The stream opens with a comment line and no message.
    await asyncio.sleep(0.05)
    assert got == []

    server.press(shared("controller_event.json"))
    await wait_for(lambda: len(got) == 1)
    server.press(shared("controller_event-transport.json"))
    await wait_for(lambda: len(got) == 2)
    await asyncio.sleep(0.05)
    assert [e.command for e in got] == ["volume_step", "toggle"]
    # The state stream was never opened by this reader.
    assert "GET /api/events" not in server.requests
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task


async def test_controller_event_stream_reconnect_replays_and_invents_nothing(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    got: list[ControllerEvent] = []
    lost: list[ChorusError] = []
    opened: list[None] = []
    stream = client.controller_events(sleep=lambda delay: asyncio.sleep(0.01))
    task = asyncio.create_task(
        stream.run(got.append, lost.append, lambda: opened.append(None))
    )
    await wait_for(lambda: len(opened) == 1 and server.press_subscribers == 1)
    server.press(shared("controller_event.json"))
    await wait_for(lambda: len(got) == 1)

    # The stream is lost: a press accepted meanwhile is sent to nobody.
    server.controller_events_status = 503
    server.drop_press_streams()
    await wait_for(lambda: len(lost) >= 2)
    assert "closed the controller event stream" in str(lost[0])
    assert "GET /api/controller-events answered 503" in str(lost[1])
    assert server.press_subscribers == 0
    server.press(shared("controller_event-transport.json"))

    # It comes back, several times over: nothing is delivered again.
    server.controller_events_status = 200
    await wait_for(lambda: len(opened) == 2 and server.press_subscribers == 1)
    server.drop_press_streams()
    await wait_for(lambda: len(opened) == 3 and server.press_subscribers == 1)
    await asyncio.sleep(0.05)
    assert len(got) == 1

    # And the next press is delivered once.
    server.press(shared("controller_event-transport.json"))
    await wait_for(lambda: len(got) == 2)
    await asyncio.sleep(0.05)
    assert [e.command for e in got] == ["volume_step", "toggle"]
    task.cancel()


async def test_controller_event_stream_backs_off_and_starts_over_once_open(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    delays: list[float] = []
    lost: list[ChorusError] = []
    opened: list[None] = []
    server.controller_events_status = 404

    async def fake_sleep(delay: float) -> None:
        delays.append(delay)
        if len(delays) == 4:
            server.controller_events_status = 200
        await asyncio.sleep(0)

    stream = client.controller_events(sleep=fake_sleep, rand=lambda: 1.0)
    task = asyncio.create_task(
        stream.run(lambda e: None, lost.append, lambda: opened.append(None))
    )
    await wait_for(lambda: len(opened) == 1)
    assert delays == [1, 2, 4, 8]
    assert "answered 404" in str(lost[0])

    # An open stream is one that worked, with no press at all: losing it
    # waits the first step again.
    server.controller_events_status = 404
    server.drop_press_streams()
    await wait_for(lambda: len(delays) >= 6)
    assert delays[4:6] == [1, 2]
    task.cancel()


async def test_controller_event_stream_silence_is_probed(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    got: list[ControllerEvent] = []
    lost: list[ChorusError] = []
    stream = client.controller_events(idle_probe=0.05)
    task = asyncio.create_task(stream.run(got.append, lost.append))
    await wait_for(lambda: server.requests.count("GET /api/state") >= 2)
    # A living server's answer keeps the stream and is no press.
    assert not lost
    assert got == []
    assert server.press_subscribers == 1
    server.state_status = 500
    await wait_for(lambda: len(lost) == 1)
    assert "answered 500" in str(lost[0])
    task.cancel()


async def test_controller_event_stream_message_of_another_kind_is_a_lost_stream(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    got: list[ControllerEvent] = []
    lost: list[ChorusError] = []
    task = asyncio.create_task(client.controller_events().run(got.append, lost.append))
    await wait_for(lambda: server.press_subscribers == 1)
    server.press(shared("state-rich.json"))
    await wait_for(lambda: len(lost) == 1)
    assert isinstance(lost[0], ChorusProtocolError)
    assert got == []
    task.cancel()


async def test_controller_event_stream_connection_refused_is_a_lost_stream(
    session: aiohttp.ClientSession, server: FakeChorusServer
) -> None:
    port = server.port
    await server.stop()
    lost: list[ChorusError] = []
    stream = ChorusClient(session, "127.0.0.1", port).controller_events()
    task = asyncio.create_task(stream.run(lambda e: None, lost.append))
    await wait_for(lambda: len(lost) == 1)
    assert isinstance(lost[0], ChorusConnectionError)
    assert "controller event stream" in str(lost[0])
    task.cancel()
