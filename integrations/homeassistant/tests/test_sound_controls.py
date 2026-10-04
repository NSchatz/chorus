"""The sound controls of a room (K83): tone, loudness, night mode, speech
enhancement, the input select, the autoplay switches and the quiet-hours switch.

For each kind: its state from the fake server, the exact bytes of the command
it sends (held to `fixtures/control/v2/`), a refusal as a translated error,
and unavailability while the event stream is lost.
"""

from __future__ import annotations

import json
from typing import Any
from unittest.mock import patch

from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import HomeAssistantError, ServiceValidationError
from homeassistant.helpers import device_registry as dr, entity_registry as er
import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.chorus.const import DOMAIN

from .conftest import SERVER_ID, autoplay_switch, only, room, room_entity, wait_for
from .fake_server import FakeChorusServer, shared

LINE_1 = "endpoint-c/line-1"


def push(server: FakeChorusServer) -> None:
    """Send the fake's edited house model to every subscriber."""
    server.model["serial"] += 1
    server.set_model_state(server._encode())


def controls(hass: HomeAssistant) -> dict[str, str]:
    """Return one entity of every kind of sound control."""
    return {
        "bass": room_entity(hass, "number", "living", "bass"),
        "treble": room_entity(hass, "number", "living", "treble"),
        "loudness": room_entity(hass, "switch", "living", "loudness"),
        "night": room_entity(hass, "switch", "living", "night"),
        "speech": room_entity(hass, "switch", "living", "speech"),
        "input": room_entity(hass, "select", "living", "input"),
        "autoplay": autoplay_switch(hass, LINE_1, "living"),
        "quiet_hours": room_entity(hass, "switch", "bedroom", "quiet_hours"),
    }


async def call(
    hass: HomeAssistant, domain: str, service: str, entity_id: str, **data: Any
) -> None:
    await hass.services.async_call(
        domain, service, {"entity_id": entity_id, **data}, blocking=True
    )


# --- state --------------------------------------------------------------------


async def test_sound_controls_state_from_the_server(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    ids = controls(hass)
    states = {key: hass.states.get(entity_id) for key, entity_id in ids.items()}
    assert {key: state.state for key, state in states.items()} == {
        "bass": "3",
        "treble": "-2",
        "loudness": "on",
        "night": "off",
        "speech": "on",
        "input": LINE_1,
        "autoplay": "on",
        "quiet_hours": "on",
    }
    assert {key: state.name for key, state in states.items()} == {
        "bass": "Living Room Bass",
        "treble": "Living Room Treble",
        "loudness": "Living Room Loudness",
        "night": "Living Room Night mode",
        "speech": "Living Room Speech enhancement",
        "input": "Living Room Input",
        "autoplay": f"Living Room Autoplay {LINE_1}",
        "quiet_hours": "bedroom Quiet hours",
    }
    bass = states["bass"].attributes
    assert (bass["min"], bass["max"], bass["step"]) == (-10, 10, 1)
    assert bass["unit_of_measurement"] == "dB"
    assert states["input"].attributes["options"] == ["stream", LINE_1]
    # A room at its defaults, and a room playing the server's stream.
    assert hass.states.get(room_entity(hass, "number", "kitchen", "bass")).state == "0"
    assert (
        hass.states.get(room_entity(hass, "switch", "kitchen", "loudness")).state
        == "on"
    )
    assert (
        hass.states.get(room_entity(hass, "select", "study", "input")).state == "stream"
    )
    # The second rule of the same room, the TV's, is a switch of its own.
    assert hass.states.get(autoplay_switch(hass, "hub/tv", "living")).state == "on"

    # Every control is on its room's device; all but the input are settings.
    registry = er.async_get(hass)
    living = dr.async_get(hass).async_get_device_by_identifier(
        (DOMAIN, f"{SERVER_ID}:room:living"), setup.entry_id
    )
    for key, entity_id in ids.items():
        registered = registry.async_get(entity_id)
        if key != "quiet_hours":
            assert registered.device_id == living.id, key
        assert registered.entity_category == (
            None if key == "input" else EntityCategory.CONFIG
        ), key


async def test_sound_controls_quiet_hours_switched_off_on_the_server(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    bedroom = room_entity(hass, "switch", "bedroom", "quiet_hours")
    server.set_state(shared("state-quiet-disabled.json"))
    await wait_for(lambda: hass.states.get(bedroom).state == "off")


async def test_sound_controls_input_select_shows_a_label(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    kitchen = room_entity(hass, "select", "kitchen", "input")
    server.set_state(shared("state-inputs.json"))
    await wait_for(lambda: hass.states.get(kitchen).state == "Kitchen streamer")
    assert hass.states.get(kitchen).attributes["options"] == [
        "stream",
        "Kitchen streamer",
    ]
    await call(hass, "select", "select_option", kitchen, option="Kitchen streamer")
    assert server.bodies == [
        b'{"v":2,"t":"take","target":"kitchen","source":"line-in:endpoint-c/line-1"}'
    ]


async def test_sound_controls_input_select_of_a_source_that_is_no_input(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    study = room_entity(hass, "select", "study", "input")
    await call(hass, "media_player", "turn_off", room(hass, "study"))
    await wait_for(lambda: hass.states.get(study).state == "unknown")


async def test_sound_controls_without_a_sound_in_the_state_are_unavailable(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    ids = controls(hass)
    del server.model["zones"][0]["sound"]
    push(server)
    await wait_for(lambda: hass.states.get(ids["bass"]).state == "unavailable")
    for key in ("treble", "loudness", "night", "speech"):
        assert hass.states.get(ids[key]).state == "unavailable", key
    # What does not come from `sound` stays.
    assert hass.states.get(ids["input"]).state == LINE_1
    assert hass.states.get(ids["autoplay"]).state == "on"


# --- the bytes sent -----------------------------------------------------------


async def test_sound_controls_tone_bytes(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    ids = controls(hass)
    vector = shared("sound.json")
    await call(hass, "number", "set_value", ids["bass"], value=3)
    await call(hass, "number", "set_value", ids["treble"], value=-2)
    # Each control sends the vector's command with its own field alone.
    assert server.bodies == [
        only(vector, "zone", "bass"),
        only(vector, "zone", "treble"),
    ]
    assert server.bodies[0] == b'{"v":2,"t":"sound","zone":"living","bass":3}'

    kitchen = room_entity(hass, "number", "kitchen", "treble")
    await call(hass, "number", "set_value", kitchen, value=-10)
    assert server.bodies[-1] == b'{"v":2,"t":"sound","zone":"kitchen","treble":-10}'
    await wait_for(lambda: hass.states.get(kitchen).state == "-10")
    # The room's other settings are untouched.
    assert hass.states.get(room_entity(hass, "number", "kitchen", "bass")).state == "0"


async def test_sound_controls_switch_bytes(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    ids = controls(hass)
    vector = shared("sound.json")
    await call(hass, "switch", "turn_off", ids["loudness"])
    await call(hass, "switch", "turn_on", ids["night"])
    await call(hass, "switch", "turn_on", ids["speech"])
    assert server.bodies == [
        only(vector, "zone", "loudness"),
        only(vector, "zone", "night"),
        only(vector, "zone", "speech"),
    ]
    assert server.bodies[0] == b'{"v":2,"t":"sound","zone":"living","loudness":false}'
    await wait_for(lambda: hass.states.get(ids["night"]).state == "on")
    assert hass.states.get(ids["loudness"]).state == "off"
    assert hass.states.get(ids["speech"]).state == "on"
    assert hass.states.get(ids["bass"]).state == "3"

    # The catalog's own partial vector: night mode on in the kitchen.
    kitchen = room_entity(hass, "switch", "kitchen", "night")
    await call(hass, "switch", "turn_on", kitchen)
    assert server.bodies[-1] == shared("sound-partial.json")
    await call(hass, "switch", "turn_off", kitchen)
    assert server.bodies[-1] == b'{"v":2,"t":"sound","zone":"kitchen","night":false}'
    await call(hass, "switch", "turn_off", ids["speech"])
    await call(hass, "switch", "turn_on", ids["loudness"])
    assert server.bodies[-2:] == [
        b'{"v":2,"t":"sound","zone":"living","speech":false}',
        b'{"v":2,"t":"sound","zone":"living","loudness":true}',
    ]


async def test_sound_controls_input_select_bytes(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    study = room_entity(hass, "select", "study", "input")
    await call(hass, "select", "select_option", study, option=LINE_1)
    # The catalog's `take` with a source, for this room instead of the vector's
    # saved group.
    assert server.bodies == [
        shared("take-source.json").replace(b'"downstairs"', b'"study"')
    ]
    await wait_for(lambda: hass.states.get(study).state == LINE_1)
    assert hass.states.get(room(hass, "study")).attributes["source"] == LINE_1

    await call(hass, "select", "select_option", study, option="stream")
    assert server.bodies[-1] == b'{"v":2,"t":"take","target":"study","source":"stream"}'
    await wait_for(lambda: hass.states.get(study).state == "stream")

    # Home Assistant refuses what is not an option before anything is sent, and
    # so does the entity when an input went away under it.
    with pytest.raises(ServiceValidationError):
        await call(hass, "select", "select_option", study, option="radio")
    entity = hass.data["select"].get_entity(study)
    with pytest.raises(ServiceValidationError) as caught:
        await entity.async_select_option("radio")
    assert caught.value.translation_key == "unknown_source"
    assert len(server.bodies) == 2


async def test_sound_controls_autoplay_bytes(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    line = autoplay_switch(hass, LINE_1, "living")
    tv = autoplay_switch(hass, "hub/tv", "living")
    await call(hass, "switch", "turn_on", line)
    assert server.bodies == [shared("autoplay.json")]

    await call(hass, "switch", "turn_off", line)
    assert server.bodies[-1] == only(
        shared("autoplay.json"), "input", "target", "enabled"
    ).replace(b"true", b"false")
    await wait_for(lambda: hass.states.get(line).state == "off")
    assert hass.states.get(tv).state == "on"

    # The command replaces the rule, so the TV rule's own field is sent again.
    await call(hass, "switch", "turn_off", tv)
    assert server.bodies[-1] == (
        b'{"v":2,"t":"autoplay","input":"hub/tv","target":"living",'
        b'"enabled":false,"stop_on_standby":false}'
    )
    await wait_for(lambda: hass.states.get(tv).state == "off")
    assert server.model["autoplay"][1] == {
        "input": "hub/tv",
        "target": "living",
        "enabled": False,
        "stop_on_standby": False,
    }


async def test_sound_controls_quiet_hours_bytes(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    bedroom = room_entity(hass, "switch", "bedroom", "quiet_hours")
    await call(hass, "switch", "turn_off", bedroom)
    assert server.bodies == [shared("quiet_hours_enabled.json")]
    await wait_for(lambda: hass.states.get(bedroom).state == "off")

    await call(hass, "switch", "turn_on", bedroom)
    assert server.bodies[-1] == shared("quiet_hours_enabled.json").replace(
        b"false", b"true"
    )
    await wait_for(lambda: hass.states.get(bedroom).state == "on")


# --- autoplay rules come and go -----------------------------------------------


async def test_sound_controls_autoplay_switches_follow_the_rules(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    registry = er.async_get(hass)
    line = autoplay_switch(hass, LINE_1, "living")
    tv = autoplay_switch(hass, "hub/tv", "living")

    # A rule given a saved group as its target is another switch, on the saved
    # group's device; a rule for nothing the server has gets none; a rule that
    # is gone takes its switch with it.
    server.model["autoplay"] = [
        {"input": LINE_1, "target": "downstairs", "enabled": False},
        {"input": "hub/aux", "target": "attic", "enabled": True},
    ]
    push(server)
    await wait_for(lambda: registry.async_get(line) is None)
    assert registry.async_get(tv) is None
    assert hass.states.get(line) is None
    moved = autoplay_switch(hass, LINE_1, "downstairs")
    assert hass.states.get(moved).state == "off"
    saved = dr.async_get(hass).async_get_device_by_identifier(
        (DOMAIN, f"{SERVER_ID}:group:downstairs"), setup.entry_id
    )
    assert registry.async_get(moved).device_id == saved.id
    assert (
        registry.async_get_entity_id(
            "switch", DOMAIN, f"{SERVER_ID}:autoplay:hub/aux:attic"
        )
        is None
    )

    await call(hass, "switch", "turn_on", moved)
    assert server.bodies == [
        b'{"v":2,"t":"autoplay","input":"endpoint-c/line-1","target":"downstairs",'
        b'"enabled":true}'
    ]


async def test_sound_controls_autoplay_switch_of_a_rule_that_just_went(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    entity = hass.data["switch"].get_entity(autoplay_switch(hass, LINE_1, "living"))
    with (
        patch.object(type(entity), "rule", None),
        pytest.raises(HomeAssistantError) as caught,
    ):
        await entity.async_turn_off()
    assert caught.value.translation_key == "autoplay_gone"
    assert server.bodies == []


async def test_sound_controls_autoplay_switch_left_from_before_a_restart_is_removed(
    hass: HomeAssistant, server: FakeChorusServer, entry: MockConfigEntry
) -> None:
    registry = er.async_get(hass)
    stale = registry.async_get_or_create(
        "switch",
        DOMAIN,
        f"{SERVER_ID}:autoplay:hub/phono:living",
        config_entry=entry,
    )
    kept = registry.async_get_or_create(
        "switch",
        DOMAIN,
        f"{SERVER_ID}:autoplay:{LINE_1}:living",
        config_entry=entry,
    )
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    assert registry.async_get(stale.entity_id) is None
    assert hass.states.get(kept.entity_id).state == "on"


async def test_sound_controls_autoplay_switch_is_named_for_the_inputs_label(
    hass: HomeAssistant, server: FakeChorusServer, entry: MockConfigEntry
) -> None:
    server.model["input_labels"] = [
        {"input": LINE_1, "name": "Record player", "role": "line-in"}
    ]
    server.state_bytes = server._encode()
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    line = hass.states.get(autoplay_switch(hass, LINE_1, "living"))
    assert line.name == "Living Room Autoplay Record player"
    # An input nobody named is shown by its id.
    assert (
        hass.states.get(autoplay_switch(hass, "hub/tv", "living")).name
        == "Living Room Autoplay hub/tv"
    )


# --- refusals -----------------------------------------------------------------


@pytest.mark.parametrize(
    ("control", "domain", "service", "data", "vector", "key"),
    [
        (
            "bass",
            "number",
            "set_value",
            {"value": 10},
            "error-sound-bass-out-of-range.json",
            "refused_sound",
        ),
        (
            "treble",
            "number",
            "set_value",
            {"value": 1},
            "error-sound-treble-not-whole.json",
            "refused_sound",
        ),
        (
            "loudness",
            "switch",
            "turn_off",
            {},
            "error-sound-loudness-not-bool.json",
            "refused_sound",
        ),
        (
            "night",
            "switch",
            "turn_on",
            {},
            "error-sound-unknown-zone.json",
            "refused_zone",
        ),
        (
            "speech",
            "switch",
            "turn_off",
            {},
            "error-sound-unknown-zone.json",
            "refused_zone",
        ),
        (
            "input",
            "select",
            "select_option",
            {"option": "stream"},
            "error-source.json",
            "refused_source",
        ),
        (
            "autoplay",
            "switch",
            "turn_off",
            {},
            "error-autoplay-input.json",
            "refused_input",
        ),
        (
            "quiet_hours",
            "switch",
            "turn_off",
            {},
            "error-sound-unknown-zone.json",
            "refused_zone",
        ),
    ],
)
async def test_sound_controls_refusal_is_a_translated_error(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    control: str,
    domain: str,
    service: str,
    data: dict[str, Any],
    vector: str,
    key: str,
) -> None:
    entity_id = controls(hass)[control]
    before = hass.states.get(entity_id).state
    refusal = shared(vector)
    server.script(400, refusal)
    with pytest.raises(HomeAssistantError) as caught:
        await call(hass, domain, service, entity_id, **data)
    assert caught.value.translation_domain == DOMAIN
    assert caught.value.translation_key == key
    # The server's own words reach the person.
    said = json.loads(refusal)
    assert caught.value.translation_placeholders == {
        "field": said["field"],
        "detail": said["detail"],
    }
    assert len(server.bodies) == 1
    assert hass.states.get(entity_id).state == before


async def test_sound_controls_refusal_by_the_house_model(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    # Not scripted: the fake's own model refuses a tone the catalog does not
    # have, a room it does not have and an input that is not one.
    coordinator = setup.runtime_data
    for message, key in (
        (b'{"v":2,"t":"sound","zone":"living","bass":11}', "refused_sound"),
        (b'{"v":2,"t":"sound","zone":"attic","night":true}', "refused_zone"),
        (
            b'{"v":2,"t":"quiet_hours_enabled","zone":"attic","enabled":true}',
            "refused_zone",
        ),
        (
            b'{"v":2,"t":"autoplay","input":"hub","target":"living","enabled":true}',
            "refused_input",
        ),
        (
            b'{"v":2,"t":"autoplay","input":"hub/tv","target":"attic","enabled":true}',
            "refused_target",
        ),
    ):
        with pytest.raises(HomeAssistantError) as caught:
            await coordinator.async_command(message)
        assert caught.value.translation_key == key, message


# --- unavailability -----------------------------------------------------------


async def test_sound_controls_are_unavailable_while_the_event_stream_is_lost(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "custom_components.chorus._aiochorus.client.BACKOFF_FIRST", 0.01
    )
    ids = controls(hass)
    before = {key: hass.states.get(entity_id).state for key, entity_id in ids.items()}
    assert "unavailable" not in before.values()

    server.refuse_connections = True
    server.drop_streams()
    await wait_for(
        lambda: all(
            hass.states.get(entity_id).state == "unavailable"
            for entity_id in ids.values()
        )
    )

    server.refuse_connections = False
    await wait_for(
        lambda: (
            {key: hass.states.get(entity_id).state for key, entity_id in ids.items()}
            == before
        )
    )
