"""Setup, unload, the server going away and coming back, repairs, devices."""

from __future__ import annotations

import json
import logging

from homeassistant.config_entries import ConfigEntryState
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers import (
    device_registry as dr,
    entity_registry as er,
    issue_registry as ir,
)
import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.chorus.const import DOMAIN

from .conftest import SERVER_ID, group_volume, room, saved_group, wait_for
from .fake_server import FakeChorusServer, shared

LOGGER = "custom_components.chorus.coordinator"


def device(hass: HomeAssistant, entry: MockConfigEntry, identifier: str):
    return dr.async_get(hass).async_get_device_by_identifier(
        (DOMAIN, identifier), config_entry_id=entry.entry_id
    )


def chorus_lines(caplog: pytest.LogCaptureFixture, word: str) -> list[str]:
    return [
        record.getMessage()
        for record in caplog.records
        if record.name == LOGGER and word in record.getMessage()
    ]


async def test_setup_and_unload(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    assert setup.state is ConfigEntryState.LOADED
    # One subscriber per config entry, and the setup asked who the server is.
    assert server.subscribers == 1
    assert server.requests[:2] == ["GET /api/server", "GET /api/state"]
    assert len(hass.states.async_entity_ids("media_player")) == 5
    assert len(hass.states.async_entity_ids("number")) == 4

    assert await hass.config_entries.async_unload(setup.entry_id)
    await hass.async_block_till_done()
    assert setup.state is ConfigEntryState.NOT_LOADED
    # The event stream ends with the entry.
    await wait_for(lambda: server.subscribers == 0)
    assert hass.states.get(room(hass, "kitchen")).state == "unavailable"


async def test_devices(hass: HomeAssistant, setup: MockConfigEntry) -> None:
    devices = dr.async_get(hass)
    hub = device(hass, setup, SERVER_ID)
    assert hub is not None
    assert hub.entry_type is dr.DeviceEntryType.SERVICE
    assert hub.sw_version == "0.18.0"
    living = device(hass, setup, f"{SERVER_ID}:room:living")
    assert living is not None
    assert living.name == "Living Room"
    assert living.via_device_id == hub.id
    # The room's name is only suggested as the area: Home Assistant decides.
    assert living.suggested_area == "Living Room"
    group = device(hass, setup, f"{SERVER_ID}:group:downstairs")
    assert group is not None
    assert group.name == "Downstairs"
    assert group.via_device_id == hub.id
    assert group.suggested_area is None
    assert len(dr.async_entries_for_config_entry(devices, setup.entry_id)) == 6
    entities = er.async_get(hass)
    assert entities.async_get(room(hass, "living")).device_id == living.id
    assert entities.async_get(group_volume(hass, "living")).device_id == living.id


async def test_not_ready_when_the_server_does_not_answer(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    server.refuse_connections = True
    assert not await hass.config_entries.async_setup(entry.entry_id)
    assert entry.state is ConfigEntryState.SETUP_RETRY

    server.refuse_connections = False
    server.state_status = 500
    await hass.config_entries.async_reload(entry.entry_id)
    assert entry.state is ConfigEntryState.SETUP_RETRY
    assert not ir.async_get(hass).issues


@pytest.mark.parametrize(
    "answer",
    [
        b'{"v":1,"t":"server","id":"chorus-test-0001","software":"0.9.0","catalogs":[1]}',
        None,
    ],
    ids=["catalog-1-only", "no-server-route"],
)
async def test_a_server_without_catalog_2_is_a_repair_issue(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    answer: bytes | None,
) -> None:
    good = server.server_bytes
    if answer is None:
        server.server_status = 404
    else:
        server.server_bytes = answer
    assert not await hass.config_entries.async_setup(entry.entry_id)
    assert entry.state is ConfigEntryState.SETUP_ERROR
    issue = ir.async_get(hass).async_get_issue(
        DOMAIN, f"unsupported_catalog_{entry.entry_id}"
    )
    assert issue is not None
    assert issue.translation_key == "unsupported_catalog"
    assert issue.severity is ir.IssueSeverity.ERROR
    assert not issue.is_fixable
    assert server.subscribers == 0

    # The server is updated: the entry loads and the issue goes.
    server.server_status = 200
    server.server_bytes = good
    await hass.config_entries.async_reload(entry.entry_id)
    await hass.async_block_till_done()
    assert entry.state is ConfigEntryState.LOADED
    assert not ir.async_get(hass).issues


async def test_a_refused_command_raises_the_repair_issue(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    server.script(426, shared("refused-unknown-version.json"))
    with pytest.raises(HomeAssistantError) as caught:
        await hass.services.async_call(
            "media_player",
            "turn_on",
            {"entity_id": room(hass, "kitchen")},
            blocking=True,
        )
    assert caught.value.translation_key == "unsupported_catalog"
    assert ir.async_get(hass).async_get_issue(
        DOMAIN, f"unsupported_catalog_{setup.entry_id}"
    )


async def test_unavailable_and_back_with_one_log_line_each(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    caplog: pytest.LogCaptureFixture,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "custom_components.chorus._aiochorus.client.BACKOFF_FIRST", 0.01
    )
    caplog.set_level(logging.INFO, logger=LOGGER)
    kitchen = room(hass, "kitchen")
    assert hass.states.get(kitchen).state == "on"

    # The server goes away: every attempt to reach it fails for a while.
    server.refuse_connections = True
    server.drop_streams()
    await wait_for(lambda: hass.states.get(kitchen).state == "unavailable")
    assert hass.states.get(saved_group(hass, "downstairs")).state == "unavailable"
    assert hass.states.get(group_volume(hass, "kitchen")).state == "unavailable"
    await wait_for(lambda: server.requests.count("GET /api/events") >= 5)
    assert len(chorus_lines(caplog, "is unavailable")) == 1
    assert chorus_lines(caplog, "is back") == []

    # A command while it is away is a translated error, not a new log line.
    with pytest.raises(HomeAssistantError) as caught:
        await setup.runtime_data.async_command(b'{"v":2,"t":"take","target":"kitchen"}')
    assert caught.value.translation_key == "cannot_connect"

    server.refuse_connections = False
    await wait_for(lambda: hass.states.get(kitchen).state == "on")
    assert len(chorus_lines(caplog, "is back")) == 1
    assert len(chorus_lines(caplog, "is unavailable")) == 1
    assert server.subscribers == 1


async def test_a_command_that_works_while_the_stream_is_down_waits_for_the_stream(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "custom_components.chorus._aiochorus.client.BACKOFF_FIRST", 0.01
    )
    kitchen = room(hass, "kitchen")
    server.events_status = 503
    server.drop_streams()
    await wait_for(lambda: hass.states.get(kitchen).state == "unavailable")
    await setup.runtime_data.async_command(b'{"v":2,"t":"take","target":"kitchen"}')
    assert hass.states.get(kitchen).state == "unavailable"
    server.events_status = 200
    await wait_for(lambda: hass.states.get(kitchen).state == "on")


async def test_an_overtaken_state_is_ignored(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    coordinator = setup.runtime_data
    current = coordinator.data
    older = json.loads(shared("state-rich.json"))
    older["serial"] = current.serial - 1
    older["zones"][0]["volume"] = 0.1
    from custom_components.chorus._aiochorus import State  # noqa: PLC0415

    coordinator.handle_state(State.parse(json.dumps(older)))
    assert coordinator.data is current
    coordinator.handle_state(State.parse(shared("state-rich.json")))
    assert coordinator.data is current


async def test_rooms_and_saved_groups_appear_and_disappear_while_running(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    devices = dr.async_get(hass)
    entities = er.async_get(hass)
    living = room(hass, "living")
    downstairs = saved_group(hass, "downstairs")

    # Another house: kitchen, study and bedroom stay; living and the saved
    # group are gone (stale devices), and nothing new appears.
    server.set_state(shared("state-inputs.json"))
    await wait_for(lambda: hass.states.get(living) is None)
    await hass.async_block_till_done()
    assert hass.states.get(downstairs) is None
    assert entities.async_get(living) is None
    assert entities.async_get(downstairs) is None
    assert not device(hass, setup, f"{SERVER_ID}:room:living")
    assert not device(hass, setup, f"{SERVER_ID}:group:downstairs")
    assert len(dr.async_entries_for_config_entry(devices, setup.entry_id)) == 4
    assert hass.states.get(room(hass, "kitchen")).state == "playing"

    # A saved group is created and a room is renamed while running, and a room
    # comes back: new devices and entities (dynamic devices), the name follows.
    raw = json.loads(shared("state-rich.json"))
    raw["zones"][1]["name"] = "Kitchen and bar"
    raw["saved_groups"].append(
        {
            "id": "upstairs",
            "name": "Upstairs",
            "zones": ["study", "bedroom"],
            "active": False,
        }
    )
    server.set_state(json.dumps(raw).encode())
    await wait_for(lambda: len(hass.states.async_entity_ids("media_player")) == 6)
    await hass.async_block_till_done()
    assert hass.states.get(saved_group(hass, "upstairs")).state == "off"
    assert hass.states.get(saved_group(hass, "upstairs")).name == "Upstairs"
    assert hass.states.get(room(hass, "living")).state == "on"
    assert hass.states.get(group_volume(hass, "living")).state == "60.0"
    kitchen = device(hass, setup, f"{SERVER_ID}:room:kitchen")
    assert kitchen is not None
    assert kitchen.name == "Kitchen and bar"
    assert len(dr.async_entries_for_config_entry(devices, setup.entry_id)) == 7
