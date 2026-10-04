"""The per-room group volume: the group volume of whatever group the room is in."""

from __future__ import annotations

from unittest.mock import patch

from homeassistant.core import HomeAssistant
from homeassistant.exceptions import HomeAssistantError
import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from .conftest import group_volume, room, wait_for
from .fake_server import FakeChorusServer


async def test_group_volume_of_a_live_group(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    study = group_volume(hass, "study")
    state = hass.states.get(study)
    assert state is not None
    assert state.state == "60.0"
    assert state.name == "study Group volume"
    assert state.attributes["unit_of_measurement"] == "%"

    await hass.services.async_call(
        "number", "set_value", {"entity_id": study, "value": 30}, blocking=True
    )
    # The room's formed group is live-1: that is the group the command names.
    assert server.bodies == [
        b'{"v":2,"t":"group_volume","group":"live-1","volume":0.300}'
    ]
    await wait_for(lambda: hass.states.get(study).state == "30.0")
    assert hass.states.get(group_volume(hass, "bedroom")).state == "30.0"
    # study was 1.000 and bedroom 0.200 (its quiet-hours cap): scaled by a half.
    assert hass.states.get(room(hass, "study")).attributes["volume_level"] == 0.5
    assert hass.states.get(room(hass, "bedroom")).attributes["volume_level"] == 0.1


async def test_group_volume_of_a_saved_group_from_a_room(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    await hass.services.async_call(
        "number",
        "set_value",
        {"entity_id": group_volume(hass, "kitchen"), "value": 45},
        blocking=True,
    )
    assert server.bodies == [
        b'{"v":2,"t":"group_volume","group":"downstairs","volume":0.450}'
    ]


async def test_unavailable_while_the_room_is_alone(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    bedroom = group_volume(hass, "bedroom")
    assert hass.states.get(bedroom).state == "60.0"
    await hass.services.async_call(
        "media_player", "unjoin", {"entity_id": room(hass, "bedroom")}, blocking=True
    )
    await wait_for(lambda: hass.states.get(bedroom).state == "unavailable")
    assert hass.states.get(group_volume(hass, "study")).state == "unavailable"
    assert hass.states.get(group_volume(hass, "kitchen")).state == "60.0"


async def test_a_room_the_server_no_longer_has(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    entity = hass.data["number"].get_entity(group_volume(hass, "study"))
    with (
        patch.object(type(entity), "formed_group", None),
        pytest.raises(HomeAssistantError) as caught,
    ):
        await entity.async_set_native_value(10)
    assert caught.value.translation_key == "room_gone"
    assert server.bodies == []
