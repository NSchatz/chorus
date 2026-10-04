"""A speaker's buttons as event entities, fired once per press the server accepted.

The presses are the repository's shared vectors
(`fixtures/control/v2/controller_event*.json`), sent by the fake server to the
subscribers of `GET /api/controller-events` as the real server sends them:
once, to whoever is attached, and never again.
"""

from __future__ import annotations

import asyncio
import json
import logging
from typing import Any

from homeassistant.const import EVENT_STATE_CHANGED, STATE_UNAVAILABLE, STATE_UNKNOWN
from homeassistant.core import Event, HomeAssistant, callback
from homeassistant.helpers import device_registry as dr, entity_registry as er
import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.chorus._aiochorus import BUTTONS
from custom_components.chorus.const import DOMAIN

from .conftest import SERVER_ID, room, speaker_button, wait_for
from .fake_server import FakeChorusServer, shared

# The endpoint the shared vectors name, and the shared state's second speaker.
FIRST = "endpoint-a"
SECOND = "chorus-ba9876543210"
COORDINATOR_LOG = "custom_components.chorus.coordinator"


def house() -> dict[str, Any]:
    """The shared speakers state, its two speakers ones with buttons.

    The first is given the id the shared `controller_event` vectors name, so a
    vector is sent as it is; both declare the controller role.
    """
    raw = json.loads(shared("state-speakers.json"))
    first, second = raw["speakers"]
    for zone in raw["zones"]:
        zone["endpoints"] = [
            FIRST if e == first["id"] else e for e in zone["endpoints"]
        ]
    first |= {"id": FIRST, "roles": ["player", "controller"]}
    second |= {"present": True, "room": "living", "roles": ["player", "controller"]}
    return raw


def encode(raw: dict[str, Any]) -> bytes:
    return json.dumps(raw, separators=(",", ":"), ensure_ascii=False).encode()


def press(speaker: str = FIRST, vector: str = "controller_event.json", **changed: Any):
    """A shared vector, as it is or from another speaker or with another command."""
    text = shared(vector)
    if speaker == FIRST and not changed:
        return text
    return encode(json.loads(text) | {"endpoint": speaker} | changed)


async def start(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    raw: dict[str, Any] | None = None,
) -> None:
    """Set the entry up with both streams attached."""
    server.state_bytes = encode(house() if raw is None else raw)
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    await wait_for(lambda: server.subscribers == 1 and server.press_subscribers == 1)
    await wait_for(lambda: entry.runtime_data.presses_connected)


def record(hass: HomeAssistant) -> list[tuple[str, str, str | None]]:
    """Collect every state change of an event entity: (entity, state, event type)."""
    changes: list[tuple[str, str, str | None]] = []

    @callback
    def changed(event: Event) -> None:
        new = event.data["new_state"]
        if new is not None and new.entity_id.startswith("event."):
            changes.append((new.entity_id, new.state, new.attributes.get("event_type")))

    hass.bus.async_listen(EVENT_STATE_CHANGED, changed)
    return changes


def fired(
    changes: list[tuple[str, str, str | None]], last: dict[str, str] | None = None
) -> list[tuple[str, str | None]]:
    """The presses among the changes: a new time, not a change of availability.

    An event entity's state is the time of its last event, so coming back from
    unavailable with the time it had (`last`, for the ones that had one before
    the record began) is no press.
    """
    seen: dict[str, str] = dict(last or {})
    out = []
    for entity_id, state, event_type in changes:
        if state in (STATE_UNAVAILABLE, STATE_UNKNOWN):
            continue
        if seen.get(entity_id) != state:
            out.append((entity_id, event_type))
        seen[entity_id] = state
    return out


async def settle(hass: HomeAssistant) -> None:
    """Give a press that should not arrive the time to arrive."""
    await asyncio.sleep(0.1)
    await hass.async_block_till_done()


async def test_button_entities_are_five_per_speaker_with_buttons_on_its_device(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    raw = house()
    # A speaker without buttons (a two-way, a subwoofer) declares no controller role.
    raw["speakers"][1]["roles"] = ["player"]
    await start(hass, entry, server, raw)
    registry = er.async_get(hass)
    entries = [
        e
        for e in er.async_entries_for_config_entry(registry, entry.entry_id)
        if e.domain == "event"
    ]
    assert {e.unique_id for e in entries} == {
        f"{SERVER_ID}:speaker:{FIRST}:button:{button}" for button in BUTTONS
    }
    device = dr.async_get(hass).async_get_device_by_identifier(
        (DOMAIN, f"{SERVER_ID}:speaker:{FIRST}"), config_entry_id=entry.entry_id
    )
    assert device is not None
    assert {e.device_id for e in entries} == {device.id}
    assert sorted(e.entity_id for e in entries) == [
        "event.kitchen_kitchen_left_next_button",
        "event.kitchen_kitchen_left_play_pause_button",
        "event.kitchen_kitchen_left_previous_button",
        "event.kitchen_kitchen_left_volume_down_button",
        "event.kitchen_kitchen_left_volume_up_button",
    ]
    for button in BUTTONS:
        state = hass.states.get(speaker_button(hass, FIRST, button))
        assert state.state == STATE_UNKNOWN
        assert state.attributes["device_class"] == "button"
        assert state.attributes["event_types"] == (
            ["press", "long_press"] if button == "play_pause" else ["press"]
        )

    # The other speaker says it has buttons: its five entities appear at once.
    server.set_state(encode(house()))
    await wait_for(
        lambda: (
            registry.async_get_entity_id(
                "event", DOMAIN, f"{SERVER_ID}:speaker:{SECOND}:button:next"
            )
            is not None
        )
    )
    # It goes away and its hello's roles with it: the entities stay, once.
    raw = house()
    raw["speakers"][1] |= {"present": False, "roles": []}
    server.set_state(encode(raw))
    await wait_for(lambda: not entry.runtime_data.data.speakers[1].present)
    server.set_state(encode(house()))
    await wait_for(lambda: entry.runtime_data.data.speakers[1].present)
    await hass.async_block_till_done()
    events = [
        e
        for e in er.async_entries_for_config_entry(registry, entry.entry_id)
        if e.domain == "event"
    ]
    assert len(events) == 10

    # A forgotten speaker's entities go with its device.
    raw = house()
    del raw["speakers"][1]
    server.set_state(encode(raw))
    await wait_for(
        lambda: (
            registry.async_get_entity_id(
                "event", DOMAIN, f"{SERVER_ID}:speaker:{SECOND}:button:next"
            )
            is None
        )
    )


async def test_button_one_press_fires_exactly_one_event(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    await start(hass, entry, server)
    changes = record(hass)
    down = speaker_button(hass, FIRST, "volume_down")
    play = speaker_button(hass, FIRST, "play_pause")

    # The shared vector as it is: a volume-down press on the first speaker.
    server.press(press())
    await wait_for(lambda: hass.states.get(down).state != STATE_UNKNOWN)
    await settle(hass)
    assert fired(changes) == [(down, "press")]
    assert len(changes) == 1
    attributes = hass.states.get(down).attributes
    assert attributes["event_type"] == "press"
    assert attributes["command"] == "volume_step"
    assert attributes["value"] == -5
    assert attributes["target"] == ""
    assert attributes["room"] == "kitchen"
    assert attributes["outcome"] == "applied"
    # No other button of that speaker and no button of the other one fired.
    for speaker in (FIRST, SECOND):
        for button in BUTTONS:
            if (speaker, button) != (FIRST, "volume_down"):
                entity_id = speaker_button(hass, speaker, button)
                assert hass.states.get(entity_id).state == STATE_UNKNOWN

    # The transport vector: a play/pause press, which changes no room.
    server.press(press(vector="controller_event-transport.json"))
    await wait_for(lambda: hass.states.get(play).state != STATE_UNKNOWN)
    await settle(hass)
    assert fired(changes) == [(down, "press"), (play, "press")]
    assert hass.states.get(play).attributes["command"] == "toggle"
    assert hass.states.get(play).attributes["outcome"] == "waits-for-an-input"
    # The integration only listened: it sent the server nothing.
    assert server.bodies == []


@pytest.mark.parametrize(
    ("changed", "button", "event_type"),
    [
        ({"command": "volume_step", "value": 5}, "volume_up", "press"),
        ({"command": "volume_step", "value": -5}, "volume_down", "press"),
        ({"command": "toggle", "value": 0}, "play_pause", "press"),
        ({"command": "next", "value": 0}, "next", "press"),
        ({"command": "previous", "value": 0}, "previous", "press"),
        ({"command": "leave", "value": 0}, "play_pause", "long_press"),
        (
            {"command": "join", "value": 0, "target": "downstairs"},
            "play_pause",
            "long_press",
        ),
    ],
)
async def test_button_each_command_fires_its_button_with_the_documented_event_type(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    changed: dict[str, Any],
    button: str,
    event_type: str,
) -> None:
    await start(hass, entry, server)
    changes = record(hass)
    entity_id = speaker_button(hass, FIRST, button)
    server.press(press(**changed))
    await wait_for(lambda: hass.states.get(entity_id).state != STATE_UNKNOWN)
    await settle(hass)
    assert fired(changes) == [(entity_id, event_type)]
    state = hass.states.get(entity_id)
    assert state.attributes["event_type"] == event_type
    assert state.attributes["command"] == changed["command"]
    assert state.attributes["target"] == changed.get("target", "")


async def test_button_two_speakers_do_not_cross(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    await start(hass, entry, server)
    changes = record(hass)
    first = speaker_button(hass, FIRST, "volume_down")
    second = speaker_button(hass, SECOND, "volume_down")
    assert first != second

    server.press(press(SECOND, zone="living"))
    await wait_for(lambda: hass.states.get(second).state != STATE_UNKNOWN)
    await settle(hass)
    assert fired(changes) == [(second, "press")]
    assert hass.states.get(second).attributes["room"] == "living"
    assert hass.states.get(first).state == STATE_UNKNOWN

    server.press(press(FIRST))
    await wait_for(lambda: hass.states.get(first).state != STATE_UNKNOWN)
    await settle(hass)
    assert fired(changes) == [(second, "press"), (first, "press")]
    assert hass.states.get(first).attributes["room"] == "kitchen"


async def test_button_stream_drop_and_reconnect_fires_nothing(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "custom_components.chorus._aiochorus.client.BACKOFF_FIRST", 0.01
    )
    await start(hass, entry, server)
    down = speaker_button(hass, FIRST, "volume_down")
    play = speaker_button(hass, FIRST, "play_pause")
    server.press(press())
    await wait_for(lambda: hass.states.get(down).state != STATE_UNKNOWN)
    before = hass.states.get(down)
    changes = record(hass)

    # The stream of presses alone is lost and comes back.
    opened = server.requests.count("GET /api/controller-events")
    server.controller_events_status = 503
    server.drop_press_streams()
    await wait_for(lambda: hass.states.get(down).state == STATE_UNAVAILABLE)
    # A press the server accepts meanwhile reaches nobody, and is not kept.
    assert server.press_subscribers == 0
    server.press(press(vector="controller_event-transport.json"))
    server.controller_events_status = 200
    await wait_for(lambda: hass.states.get(down).state != STATE_UNAVAILABLE)
    await wait_for(lambda: server.press_subscribers == 1)
    assert server.requests.count("GET /api/controller-events") > opened

    # Then the whole server goes away and comes back.
    server.refuse_connections = True
    server.drop_streams()
    await wait_for(lambda: hass.states.get(down).state == STATE_UNAVAILABLE)
    server.refuse_connections = False
    await wait_for(lambda: server.subscribers == 1 and server.press_subscribers == 1)
    await wait_for(lambda: hass.states.get(down).state != STATE_UNAVAILABLE)

    # And the entry is reloaded.
    await hass.config_entries.async_reload(entry.entry_id)
    await hass.async_block_till_done()
    await wait_for(lambda: server.subscribers == 1 and server.press_subscribers == 1)
    await wait_for(lambda: hass.states.get(down).state != STATE_UNAVAILABLE)
    await settle(hass)

    # Through all of it nothing fired: the one press is still the last one,
    # at its own time, and the press made while detached never arrived.
    assert fired(changes, {down: before.state}) == []
    after = hass.states.get(down)
    assert after.state == before.state
    assert after.attributes == before.attributes
    assert hass.states.get(play).state == STATE_UNKNOWN

    # The next press is one event.
    server.press(press(vector="controller_event-transport.json"))
    await wait_for(lambda: hass.states.get(play).state != STATE_UNKNOWN)
    await settle(hass)
    assert fired(changes, {down: before.state}) == [(play, "press")]


async def test_button_unknown_button_or_speaker_is_ignored_with_one_log_line(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    caplog: pytest.LogCaptureFixture,
) -> None:
    await start(hass, entry, server)
    changes = record(hass)
    caplog.set_level(logging.DEBUG, logger=COORDINATOR_LOG)

    def ignored() -> list[str]:
        return [
            r.getMessage()
            for r in caplog.records
            if r.name == COORDINATOR_LOG and "Ignored" in r.getMessage()
        ]

    # A speaker this server has not adopted (or a wall remote).
    server.press(press("wall-remote-1"))
    await wait_for(lambda: len(ignored()) == 1)
    await settle(hass)
    assert len(ignored()) == 1
    assert "wall-remote-1" in ignored()[0]

    # A command no button of a speaker sends, from a speaker that has buttons.
    server.press(press(command="mute_set", value=1))
    await wait_for(lambda: len(ignored()) == 2)
    await settle(hass)
    assert len(ignored()) == 2
    assert "mute_set" in ignored()[1]
    assert FIRST in ignored()[1]

    # A command name of a later server.
    server.press(press(command="shuffle"))
    await wait_for(lambda: len(ignored()) == 3)
    await settle(hass)
    assert len(ignored()) == 3

    # Nothing fired and the stream is still the same one.
    assert changes == []
    assert server.requests.count("GET /api/controller-events") == 1
    for speaker in (FIRST, SECOND):
        for button in BUTTONS:
            entity_id = speaker_button(hass, speaker, button)
            assert hass.states.get(entity_id).state == STATE_UNKNOWN

    # A known press still fires afterwards.
    down = speaker_button(hass, FIRST, "volume_down")
    server.press(press())
    await wait_for(lambda: hass.states.get(down).state != STATE_UNKNOWN)
    assert fired(changes) == [(down, "press")]
    assert len(ignored()) == 3


async def test_button_server_without_the_route_leaves_the_rest_working(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    caplog: pytest.LogCaptureFixture,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "custom_components.chorus._aiochorus.client.BACKOFF_FIRST", 0.01
    )
    caplog.set_level(logging.INFO, logger=COORDINATOR_LOG)
    server.controller_events_status = 404
    server.state_bytes = encode(house())
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    await wait_for(lambda: server.subscribers == 1)
    await wait_for(lambda: server.requests.count("GET /api/controller-events") >= 3)

    def lines(word: str) -> list[str]:
        return [
            r.getMessage()
            for r in caplog.records
            if r.name == COORDINATOR_LOG
            and "button presses" in r.getMessage()
            and word in r.getMessage()
        ]

    # The buttons are unavailable, said once; the rooms are not.
    down = speaker_button(hass, FIRST, "volume_down")
    assert hass.states.get(down).state == STATE_UNAVAILABLE
    assert hass.states.get(room(hass, "kitchen")).state != STATE_UNAVAILABLE
    assert len(lines("unavailable")) == 1
    assert "answered 404" in lines("unavailable")[0]

    # The route appears (the server was updated): available, said once.
    server.controller_events_status = 200
    await wait_for(lambda: hass.states.get(down).state == STATE_UNKNOWN)
    assert len(lines("are back")) == 1
