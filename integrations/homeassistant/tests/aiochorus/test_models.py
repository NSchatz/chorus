"""Reading the state and server messages, from the shared vectors."""

from __future__ import annotations

import json

import pytest

from custom_components.chorus._aiochorus import (
    ChorusCommandError,
    ChorusProtocolError,
    ServerInfo,
    State,
)

from ..fake_server import SHARED_V2, shared


@pytest.mark.parametrize(
    "path", sorted(SHARED_V2.glob("state-*.json")), ids=lambda p: p.name
)
def test_every_shared_state_vector_parses(path) -> None:
    raw = json.loads(path.read_bytes())
    state = State.parse(path.read_bytes())
    assert state.serial == raw["serial"]
    assert [z.id for z in state.zones] == [z["id"] for z in raw["zones"]]
    assert [g.id for g in state.groups] == [g["id"] for g in raw["groups"]]
    assert [g.id for g in state.saved_groups] == [g["id"] for g in raw["saved_groups"]]
    assert list(state.inputs) == raw["inputs"]
    for zone in state.zones:
        assert state.group_of(zone.id) is not None


def test_the_rich_state() -> None:
    state = State.parse(shared("state-rich.json"))
    living = state.zone("living")
    assert living is not None
    assert (living.name, living.group, living.volume, living.muted) == (
        "Living Room",
        "downstairs",
        0.857,
        False,
    )
    assert living.present == ("endpoint-a", "endpoint-b", "endpoint-c")
    kitchen = state.zone("kitchen")
    assert kitchen is not None
    assert (kitchen.limit, kitchen.effective_limit) == (0.4, 0.4)
    group = state.group_of("bedroom")
    assert group is not None
    assert (group.id, group.kind, group.zones, group.volume, group.source) == (
        "live-1",
        "live",
        ("study", "bedroom"),
        0.6,
        "stream",
    )
    saved = state.saved_group("downstairs")
    assert saved is not None
    assert (saved.name, saved.zones, saved.active) == (
        "Downstairs",
        ("living", "kitchen"),
        True,
    )
    assert state.zone("attic") is None
    assert state.group_of("attic") is None
    assert state.saved_group("attic") is None
    assert dict(state.counts)["alarms"] == 1


def test_now_playing_and_labels() -> None:
    state = State.parse(shared("state-playing.json"))
    group = state.group("downstairs")
    assert group is not None
    record = group.now_playing
    assert record is not None
    assert (record.title, record.artist, record.album) == (
        "Morning Light",
        "The Example Quartet",
        "First Takes",
    )
    assert (record.duration_ms, record.state, record.via) == (215000, "playing", "upnp")
    study = state.group("study")
    assert study is not None
    assert study.now_playing is not None
    assert study.now_playing.duration_ms is None
    assert study.now_playing.state == "paused"
    hall = state.group("hall")
    assert hall is not None
    assert hall.now_playing is None

    inputs = State.parse(shared("state-inputs.json"))
    assert inputs.input_name("endpoint-c/line-1") == "Kitchen streamer"
    assert inputs.input_name("endpoint-z/line-9") is None
    assert inputs.input_labels[0].role == "streamer"


def test_the_server_message() -> None:
    info = ServerInfo.parse(shared("server.json"))
    assert info == ServerInfo(
        id="chorus-server-0123456789abcdef",
        software="chorus-server 0.1.0",
        catalogs=(1, 2),
        announce_origins=("http://ha.example:8123",),
    )


@pytest.mark.parametrize(
    "text",
    [
        b"not json",
        b"[]",
        b'{"v":2,"t":"state","serial":0}',
        b'{"v":2,"t":"server","id":"UPPER","software":"1","catalogs":[2]}',
        b'{"v":2,"t":"server","id":"ok","software":1,"catalogs":[2]}',
        b'{"v":2,"t":"server","id":"ok","software":"1"}',
        b'{"v":2,"t":"server","software":"1","catalogs":[2]}',
    ],
)
def test_what_is_not_a_server_message_is_refused(text: bytes) -> None:
    with pytest.raises(ChorusProtocolError):
        ServerInfo.parse(text)


@pytest.mark.parametrize(
    "text",
    [
        b"\xff",
        b'{"v":2,"t":"error","field":"","detail":""}',
        shared("state.json"),  # the v1 shape is not what this client asks for
        b'{"v":2,"t":"state","serial":true,"zones":[]}',
        b'{"v":2,"t":"state","serial":1,"zones":[{"id":"a"}]}',
        b'{"v":2,"t":"state","serial":1,"zones":[{"id":"a","name":"a","group":"a","volume":"loud"}]}',
    ],
)
def test_what_is_not_a_v2_state_is_refused(text: bytes) -> None:
    with pytest.raises(ChorusProtocolError):
        State.parse(text)


def test_members_a_later_catalog_adds_are_ignored() -> None:
    state = State.parse(
        b'{"v":2,"t":"state","serial":3,"new":{"x":1},"zones":[{"id":"a","name":"A",'
        b'"group":"a","volume":0.5,"later":true}],"groups":[{"id":"a","kind":"room",'
        b'"zones":["a"],"volume":0.5,"source":"stream","audio":"x","now_playing":'
        b'{"title":null,"duration_ms":true}}],"saved_groups":"?","inputs":[1,"e/i"]}'
    )
    zone = state.zone("a")
    assert zone is not None
    assert (zone.limit, zone.effective_limit, zone.transport) == (1.0, 1.0, None)
    assert state.inputs == ("e/i",)
    assert state.saved_groups == ()
    group = state.group("a")
    assert group is not None
    assert group.now_playing is not None
    assert (group.now_playing.state, group.now_playing.duration_ms) == ("playing", None)


def test_sound_controls_in_the_rich_state() -> None:
    state = State.parse(shared("state-rich.json"))
    living = state.zone("living")
    assert living is not None
    assert living.sound is not None
    sound = living.sound
    assert (sound.bass, sound.treble) == (3, -2)
    assert (sound.loudness, sound.night, sound.speech) == (True, False, True)
    bedroom = state.zone("bedroom")
    assert bedroom is not None
    assert bedroom.quiet_enabled is True
    line, tv = state.autoplay
    assert (line.input, line.target, line.enabled) == (
        "endpoint-c/line-1",
        "living",
        True,
    )
    assert (line.stop_on_standby, line.low_latency) == (True, True)
    assert (tv.input, tv.stop_on_standby, tv.low_latency) == ("hub/tv", False, True)
    assert state.autoplay_rule("hub/tv", "living") is tv
    assert state.autoplay_rule("hub/tv", "kitchen") is None


def test_sound_controls_quiet_hours_switched_off() -> None:
    state = State.parse(shared("state-quiet-disabled.json"))
    bedroom = state.zone("bedroom")
    assert bedroom is not None
    assert bedroom.quiet_enabled is False


def test_sound_controls_absent_members_are_their_defaults() -> None:
    state = State.parse(
        b'{"v":2,"t":"state","serial":1,"zones":[{"id":"a","name":"a","group":"a",'
        b'"volume":0.5,"sound":{"bass":true,"night":1}},{"id":"b","name":"b",'
        b'"group":"b","volume":0.5}],"groups":[],"saved_groups":[],"inputs":[]}'
    )
    first, second = state.zones
    assert first.sound is not None
    assert (first.sound.bass, first.sound.treble) == (0, 0)
    assert (first.sound.loudness, first.sound.night, first.sound.speech) == (
        True,
        False,
        False,
    )
    assert second.sound is None
    assert second.quiet_enabled is True
    assert state.autoplay == ()


def test_firmware_members_of_the_shared_state() -> None:
    state = State.parse(shared("state-firmware.json"))
    assert [s.id for s in state.speakers] == [
        "chorus-0123456789ab",
        "chorus-ba9876543210",
    ]
    first = state.speaker("chorus-0123456789ab")
    assert first is not None
    assert first.name == "Speaker 89ab"
    assert first.room is None
    assert first.present
    running = first.firmware
    assert running is not None
    assert (running.version, running.board, running.slot) == (
        "1.0.0",
        "brick-s3-wired",
        0,
    )
    assert (running.state, running.reason, running.busy) == ("requested", "none", True)
    assert (running.image, running.image_version) == ("brick-2-0-0", "2.0.0")
    assert (running.received, running.size) == (0, 1536000)
    back = state.speaker("chorus-ba9876543210")
    assert back is not None
    assert back.firmware is not None
    assert (back.firmware.state, back.firmware.reason) == (
        "rolled_back",
        "not_confirmed",
    )
    assert not back.firmware.busy
    assert back.firmware.image is None
    assert [(i.name, i.verified) for i in state.firmware_images] == [
        ("brick-2-0-0", True),
        ("brick-tampered", False),
        ("compact-2-0-0", True),
        ("brick-1-0-0", True),
    ]
    # The verified image for the board with another version; never the refused
    # one, the other board's, or the version it runs.
    offer = state.firmware_offer("chorus-0123456789ab")
    assert offer is not None
    assert offer.name == "brick-2-0-0"
    assert state.firmware_offer("nobody") is None


def test_firmware_absent_members_and_the_offer_among_several() -> None:
    # No speakers and no firmware directory: nothing, and nothing offered.
    assert State.parse(shared("state-rich.json")).speakers == ()
    assert State.parse(shared("state-rich.json")).firmware_images == ()
    # A speaker that has not reported has no firmware.
    plain = State.parse(shared("state-speakers.json"))
    assert [s.firmware for s in plain.speakers] == [None, None]
    assert plain.firmware_offer("chorus-0123456789ab") is None

    raw = json.loads(shared("state-firmware.json"))
    brick = raw["firmware"]["images"][0]
    raw["firmware"]["images"] += [
        brick | {"name": "brick-2-10-0", "version": "2.10.0"},
        brick | {"name": "brick-2-9-0", "version": "2.9.0"},
        brick | {"name": "a-brick-2-10-0", "version": "2.10.0"},
        brick | {"name": "brick-9-0-0", "version": "9.0.0", "verdict": "refused"},
        brick | {"name": "brick-8-0-0", "version": "8.0.0", "verdict": "staged"},
    ]
    del raw["firmware"]["images"][-1]["verdict"]
    state = State.parse(json.dumps(raw))
    offer = state.firmware_offer("chorus-0123456789ab")
    # Digits compare as numbers; of two images of one version, the first name.
    assert offer is not None
    assert (offer.name, offer.version) == ("a-brick-2-10-0", "2.10.0")
    # An older verified image is never offered: going back is the server's
    # command to send, not this client's to suggest.
    raw["speakers"][0]["firmware"]["version"] = "2.10.0"
    assert State.parse(json.dumps(raw)).firmware_offer("chorus-0123456789ab") is None
    raw["speakers"][0]["firmware"]["version"] = "1.0.0"
    # The server's word wins: no update available, nothing offered.
    raw["speakers"][0]["firmware"]["update_available"] = False
    assert State.parse(json.dumps(raw)).firmware_offer("chorus-0123456789ab") is None


def test_a_refusal_name_is_the_start_of_the_detail() -> None:
    busy = json.loads(shared("error-firmware-install-busy.json"))
    assert ChorusCommandError(busy["field"], busy["detail"]).name == "busy"
    refused = json.loads(shared("error-firmware-install-not-verified.json"))
    assert (
        ChorusCommandError(refused["field"], refused["detail"]).name
        == "image-not-verified"
    )
    assert ChorusCommandError("zone", "'den' is not a room").name is None
    assert ChorusCommandError("url", "http://x.example: not allowed").name is None
    assert ChorusCommandError("t", ": nothing").name is None
