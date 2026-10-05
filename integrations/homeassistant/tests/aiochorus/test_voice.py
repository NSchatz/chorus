"""The client's voice half, against the shared vectors and the fake server.

`docs/control-plane.md`, "Voice: `voice_enabled` and `mic_muted`" and "Voice:
the wake word, the run and its audio": the three commands' bytes, the state's
voice members, the `voice_wake` and `voice_run` messages, the stream of wake
words and the run's audio route with each of its refusals.
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
    ChorusCommandError,
    ChorusConnectionError,
    ChorusError,
    ChorusProtocolError,
    ChorusVoiceRouteError,
    State,
    VoiceRun,
    VoiceWake,
    WakeWord,
    client as client_module,
    commands,
)

from ..conftest import wait_for
from ..fake_server import FakeChorusServer, shared

RUN = "0123456789abcdef0123456789abcdef"


def live_house() -> bytes:
    """The shared voice vector with the kitchen's microphone live."""
    state = json.loads(shared("state-voice.json"))
    state["zones"][0]["mic_muted"] = False
    return json.dumps(state, separators=(",", ":")).encode()


@pytest.fixture
async def server(socket_enabled: None) -> AsyncIterator[FakeChorusServer]:
    fake = FakeChorusServer(live_house())
    await fake.start()
    yield fake
    await fake.stop()


@pytest.fixture
async def session(socket_enabled: None) -> AsyncIterator[aiohttp.ClientSession]:
    async with aiohttp.ClientSession() as client_session:
        yield client_session


@pytest.fixture
def client(session: aiohttp.ClientSession, server: FakeChorusServer) -> ChorusClient:
    return ChorusClient(session, "127.0.0.1", server.port)


# --- the bytes and the messages --------------------------------------------------


def test_the_voice_commands_bytes_equal_the_shared_vectors() -> None:
    assert commands.voice_enabled("kitchen", True) == shared("voice_enabled.json")
    assert commands.voice_start("kitchen") == shared("voice_start.json")
    assert commands.voice_stop("kitchen") == shared("voice_stop.json")
    assert commands.voice_wake_words("kitchen", ["okay_nabu"]) == shared(
        "voice_wake_words.json"
    )
    assert commands.voice_wake_words("kitchen", []).endswith(b'"wake_words":[]}')
    assert commands.voice_wake_words("kitchen", ("a", "b")).endswith(
        b'"wake_words":["a","b"]}'
    )
    assert commands.voice_enabled("kitchen", False).endswith(b'"enabled":false}')


def test_the_state_says_which_rooms_listen_and_which_words_are_heard() -> None:
    state = State.parse(shared("state-voice.json"))
    kitchen = state.zone("kitchen")
    assert kitchen is not None
    assert (kitchen.voice_enabled, kitchen.mic_muted) == (True, True)
    assert state.wake_words == (WakeWord("okay_nabu", "Okay Nabu"),)
    # No speaker of the vector declares the voice role: no room has a microphone.
    assert state.voice_rooms() == frozenset()
    # A server that says nothing of voice reads as off and muted.
    old = State.parse(shared("state-rich.json"))
    assert all(not z.voice_enabled and z.mic_muted for z in old.zones)
    assert old.wake_words == ()
    assert old.announcements == ()
    assert old.announcement is None


def test_a_room_has_a_microphone_while_a_speaker_adopted_into_it_has_the_role() -> None:
    state = json.loads(shared("state-speakers.json"))
    assert State.parse(json.dumps(state)).voice_rooms() == frozenset()
    in_kitchen, unplaced = state["speakers"]
    in_kitchen["roles"] = ["player", "voice"]
    # One with the role and no room, and one in a room the server no longer
    # has, make no voice room.
    unplaced["roles"] = ["voice"]
    assert State.parse(json.dumps(state)).voice_rooms() == frozenset({"kitchen"})
    unplaced["room"] = "attic"
    assert State.parse(json.dumps(state)).voice_rooms() == frozenset({"kitchen"})
    # Away is still adopted: the room keeps its microphone.
    in_kitchen["present"] = False
    assert State.parse(json.dumps(state)).voice_rooms() == frozenset({"kitchen"})


def test_the_state_lists_announcements_and_an_answer_numbers_one() -> None:
    state = json.loads(shared("state-voice.json"))
    state["announcements"] = [
        {"id": 7, "target": "kitchen", "rooms": ["kitchen"], "state": "playing"},
        {
            "id": 6,
            "target": "kitchen",
            "rooms": ["kitchen"],
            "state": "failed",
            "reason": "http status 404",
        },
    ]
    plain = State.parse(json.dumps(state))
    playing = plain.announcement_numbered(7)
    failed = plain.announcement_numbered(6)
    assert playing is not None
    assert failed is not None
    assert playing.playing
    assert playing.rooms == ("kitchen",)
    assert (failed.playing, failed.state, failed.reason) == (
        False,
        "failed",
        "http status 404",
    )
    assert plain.announcement_numbered(5) is None
    # The number is the answer's and no part of the house's state.
    answer = State.parse(json.dumps(state | {"announcement": 7}))
    assert answer.announcement == 7
    assert answer == plain
    assert State.parse(json.dumps(state | {"announcement": True})).announcement is None
    state["announcements"][0]["id"] = "seven"
    with pytest.raises(ChorusProtocolError, match="whole 'id'"):
        State.parse(json.dumps(state))


def test_voice_wake_reads_the_shared_vector() -> None:
    assert VoiceWake.parse(shared("voice_wake.json")) == VoiceWake(
        "kitchen", "Okay Nabu"
    )
    with pytest.raises(ChorusProtocolError, match="not a voice wake event"):
        VoiceWake.parse(shared("voice_run.json"))
    with pytest.raises(ChorusProtocolError, match="catalog version 2"):
        VoiceWake.parse(b'{"v":1,"t":"voice_wake","zone":"kitchen","phrase":"x"}')


def test_voice_run_reads_the_shared_vector_and_keeps_its_identifier_out_of_repr() -> (
    None
):
    run = VoiceRun.parse(shared("voice_run.json"))
    assert (run.zone, run.run, run.limit_ms) == ("kitchen", RUN, 30000)
    assert RUN not in repr(run)
    assert RUN not in str(run)


@pytest.mark.parametrize(
    ("change", "words"),
    [
        ({"t": "state"}, "not a voice run message"),
        ({"v": 1}, "catalog version 2"),
        ({"run": "0123"}, "32-digit hexadecimal"),
        ({"run": RUN.upper()}, "32-digit hexadecimal"),
        ({"limit_ms": 0}, "limit_ms"),
        ({"limit_ms": True}, "limit_ms"),
        ({"limit_ms": "30000"}, "limit_ms"),
    ],
)
def test_a_voice_run_that_is_not_one_is_refused(
    change: dict[str, Any], words: str
) -> None:
    message = json.loads(shared("voice_run.json")) | change
    with pytest.raises(ChorusProtocolError, match=words) as caught:
        VoiceRun.parse(json.dumps(message))
    assert RUN not in str(caught.value)


# --- the commands, against the fake ----------------------------------------------


async def test_voice_start_is_answered_with_a_run_and_voice_stop_with_the_state(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    run = await client.voice_start("kitchen")
    assert server.bodies == [shared("voice_start.json")]
    assert (run.zone, run.limit_ms) == ("kitchen", 30000)
    assert server.open_run("kitchen") is not None
    assert run.run == server.open_run("kitchen").id

    # Another start supersedes the room's run, under a new identifier.
    second = await client.voice_start("kitchen")
    assert second.run != run.run
    assert server.runs_ended == [("kitchen", "superseded")]

    state = await client.voice_stop("kitchen")
    assert server.bodies[-1] == shared("voice_stop.json")
    assert state.zone("kitchen") is not None
    assert server.open_run("kitchen") is None
    # A room with no run is not an error.
    await client.voice_stop("kitchen")
    assert server.runs_ended == [("kitchen", "superseded"), ("kitchen", "stopped")]


async def test_voice_start_is_refused_by_name(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    server.voice_integration = None
    with pytest.raises(ChorusCommandError) as caught:
        await client.voice_start("kitchen")
    assert (caught.value.field, caught.value.name) == ("t", "no-voice-integration")

    server.set_mic_muted("kitchen", True)
    with pytest.raises(ChorusCommandError) as caught:
        await client.voice_start("kitchen")
    assert (caught.value.field, caught.value.name) == ("zone", "mic-muted")
    assert (
        caught.value.detail
        == json.loads(shared("error-voice-start-muted.json"))["detail"]
    )

    state = await client.set_voice_enabled("kitchen", False)
    assert state.zone("kitchen").voice_enabled is False
    with pytest.raises(ChorusCommandError) as caught:
        await client.voice_start("kitchen")
    assert (caught.value.field, caught.value.name) == ("zone", "voice-disabled")
    assert (
        caught.value.detail
        == json.loads(shared("error-voice-start-disabled.json"))["detail"]
    )

    with pytest.raises(ChorusCommandError) as caught:
        await client.voice_start("attic")
    assert (caught.value.field, caught.value.name) == ("zone", None)
    assert server.run_ids == []


async def test_switching_voice_off_or_muting_ends_the_rooms_run(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    await client.voice_start("kitchen")
    await client.set_voice_enabled("kitchen", False)
    assert server.bodies[-1] == shared("voice_enabled.json").replace(b"true", b"false")
    await client.set_voice_enabled("kitchen", True)
    assert server.bodies[-1] == shared("voice_enabled.json")
    await client.voice_start("kitchen")
    server.set_mic_muted("kitchen", True)
    assert server.runs_ended == [("kitchen", "voice-disabled"), ("kitchen", "muted")]


# --- the run's audio -----------------------------------------------------------


async def test_a_runs_audio_is_read_as_whole_samples_until_the_server_closes(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    run = await client.voice_start("kitchen")
    audio = await client.voice_audio(run)
    assert server.open_run("kitchen").claimed
    got: list[bytes] = []

    async def read() -> None:
        async for chunk in audio:
            got.append(chunk)  # noqa: PERF401 (read as it arrives, one by one)

    task = asyncio.ensure_future(read())
    # Three bytes, then five: a sample is cut in two on the wire.
    server.mic("kitchen", b"\x01\x02\x03")
    await wait_for(lambda: got == [b"\x01\x02"])
    server.mic("kitchen", b"\x04\x05\x06\x07\x08")
    await wait_for(lambda: b"".join(got) == b"\x01\x02\x03\x04\x05\x06\x07\x08")
    assert all(len(chunk) % 2 == 0 for chunk in got)
    server.end_run("kitchen", "limit")
    await task
    assert audio.error is None
    # Ended is ended: asking again gives nothing.
    assert [chunk async for chunk in audio] == []


async def test_a_reader_that_closes_ends_the_run(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    run = await client.voice_start("kitchen")
    audio = await client.voice_audio(run)
    audio.close()
    await wait_for(lambda: server.runs_ended == [("kitchen", "reader-gone")])
    assert [chunk async for chunk in audio] == []


async def test_a_runs_audio_stops_at_the_runs_limit_on_this_side(
    client: ChorusClient,
    server: FakeChorusServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A server that does not close at the limit cannot hold the reader."""
    monkeypatch.setattr(client_module, "VOICE_RUN_GRACE_SECONDS", 0.05)
    server.voice_run_limit_ms = 50
    run = await client.voice_start("kitchen")
    audio = await client.voice_audio(run)
    server.mic("kitchen", b"\x01\x02")
    assert [chunk async for chunk in audio] == [b"\x01\x02"]
    # The limit, not a lost connection.
    assert audio.error is None
    await wait_for(lambda: server.runs_ended == [("kitchen", "reader-gone")])


async def test_a_runs_audio_that_goes_silent_is_a_lost_connection(
    client: ChorusClient,
    server: FakeChorusServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(client_module, "VOICE_AUDIO_IDLE_SECONDS", 0.05)
    run = await client.voice_start("kitchen")
    audio = await client.voice_audio(run)
    assert [chunk async for chunk in audio] == []
    assert isinstance(audio.error, ChorusConnectionError)
    assert run.run not in str(audio.error)


@pytest.mark.parametrize(
    ("case", "status", "reason"),
    [
        ("elsewhere", 403, "not-the-voice-integration"),
        ("no-integration", 403, "not-the-voice-integration"),
        ("unnamed", 400, "no-run-named"),
        ("unknown", 404, "no-voice-run"),
        ("ended", 404, "no-voice-run"),
        ("taken", 409, "voice-run-taken"),
    ],
)
async def test_each_refusal_of_the_audio_route_is_named_and_never_names_the_run(
    client: ChorusClient, server: FakeChorusServer, case: str, status: int, reason: str
) -> None:
    run = await client.voice_start("kitchen")
    if case == "elsewhere":
        server.voice_integration = "192.0.2.10"
    elif case == "no-integration":
        server.voice_integration = None
    elif case == "unnamed":
        run = VoiceRun("kitchen", "", 30000)
    elif case == "unknown":
        run = VoiceRun("kitchen", "f" * 32, 30000)
    elif case == "ended":
        await client.voice_stop("kitchen")
    elif case == "taken":
        await client.voice_audio(run)
    with pytest.raises(ChorusVoiceRouteError) as caught:
        await client.voice_audio(run)
    assert (caught.value.status, caught.value.reason) == (status, reason)
    assert caught.value.detail.startswith(f"{reason}: ")
    assert server.voice_refusals == [reason]
    if run.run:
        assert run.run not in str(caught.value)


async def test_an_answer_that_is_not_the_runs_audio_is_refused(
    client: ChorusClient,
    server: FakeChorusServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    run = await client.voice_start("kitchen")
    monkeypatch.setattr(
        "tests.fake_server.VOICE_AUDIO_FORMAT", "pcm_s16le; rate=48000; channels=2"
    )
    with pytest.raises(ChorusProtocolError, match="rate=48000"):
        await client.voice_audio(run)


async def test_a_refusal_with_no_words_and_a_server_that_is_gone(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    run = await client.voice_start("kitchen")
    server.refuse_connections = True
    with pytest.raises(ChorusVoiceRouteError) as caught:
        await client.voice_audio(run)
    assert (caught.value.status, caught.value.reason, caught.value.detail) == (
        503,
        "",
        "",
    )
    await server.stop()
    with pytest.raises(ChorusConnectionError) as gone:
        await client.voice_audio(run)
    assert run.run not in str(gone.value)


# --- the stream of wake words ---------------------------------------------------


async def test_the_wake_stream_delivers_each_wake_word_and_keeps_none(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    heard: list[VoiceWake] = []
    lost: list[ChorusError] = []
    opened: list[bool] = []
    # One made before anybody listens is sent to nobody.
    server.wake(shared("voice_wake.json"))
    stream = client.voice_events(idle_probe=0.05)
    task = asyncio.ensure_future(
        stream.run(heard.append, lost.append, lambda: opened.append(True))
    )
    try:
        await wait_for(lambda: server.wake_subscribers == 1)
        await wait_for(lambda: opened == [True])
        assert heard == []
        server.wake(shared("voice_wake.json"))
        await wait_for(lambda: heard == [VoiceWake("kitchen", "Okay Nabu")])
        # Silence is probed with the state, and its answer is not an event.
        before = server.requests.count("GET /api/state")
        await wait_for(lambda: server.requests.count("GET /api/state") > before)
        assert len(heard) == 1

        # A stream that is lost is opened again, and starts empty.
        server.drop_wake_streams()
        await wait_for(lambda: len(lost) == 1)
        await wait_for(lambda: len(opened) == 2, timeout=10)
        assert len(heard) == 1
    finally:
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task


# --- a room's choice of wake words ------------------------------------------------


def test_a_room_carries_its_choice_of_wake_words_only_once_it_chose() -> None:
    state = State.parse(shared("state-wake-words.json"))
    kitchen, bedroom = state.zone("kitchen"), state.zone("bedroom")
    assert kitchen is not None
    assert bedroom is not None
    assert kitchen.wake_words == ("okay_nabu",)
    # A room that never chose listens for every one the server runs.
    assert bedroom.wake_words is None
    # A choice of none is a choice.
    raw = json.loads(shared("state-wake-words.json"))
    raw["zones"][0]["wake_words"] = []
    assert State.parse(json.dumps(raw)).zones[0].wake_words == ()


async def test_a_rooms_wake_words_are_set_and_an_unknown_one_is_refused_by_name(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    state = await client.set_voice_wake_words("kitchen", ["okay_nabu"])
    assert server.bodies == [shared("voice_wake_words.json")]
    kitchen = state.zone("kitchen")
    assert kitchen is not None
    assert kitchen.wake_words == ("okay_nabu",)
    with pytest.raises(ChorusCommandError) as caught:
        await client.set_voice_wake_words("kitchen", ["hey_jarvis"])
    assert (caught.value.field, caught.value.name) == (
        "wake_words",
        "unknown-wake-word",
    )
    assert (
        caught.value.detail
        == json.loads(shared("error-voice-wake-words-unknown.json"))["detail"]
    )
