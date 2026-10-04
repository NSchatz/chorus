"""Reading the state and server messages, from the shared vectors."""

from __future__ import annotations

import json

import pytest

from custom_components.chorus._aiochorus import (
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
