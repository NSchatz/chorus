"""Fixtures shared by the integration's tests."""

from __future__ import annotations

from collections.abc import AsyncIterator

from homeassistant.const import CONF_HOST, CONF_PORT
from homeassistant.core import HomeAssistant
import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry
from pytest_homeassistant_custom_component.syrupy import HomeAssistantSnapshotExtension
from syrupy.assertion import SnapshotAssertion

from custom_components.chorus.const import DOMAIN

from .fake_server import FakeChorusServer, shared

SERVER_ID = "chorus-server-0123456789abcdef"


@pytest.fixture(autouse=True)
def _enable_custom_integrations(enable_custom_integrations: None) -> None:
    """Let Home Assistant load `custom_components/chorus`."""


@pytest.fixture
def snapshot(snapshot: SnapshotAssertion) -> SnapshotAssertion:
    """Read snapshots the Home Assistant way, from `tests/snapshots`.

    The harness and syrupy each register a `snapshot` fixture, and which one a
    test gets depends on the order pytest loads the two plugins in, which is the
    order the environment lists their metadata: not the same on every machine.
    A fixture in conftest.py is preferred over both, on every machine.
    """
    return snapshot.use_extension(HomeAssistantSnapshotExtension)


@pytest.fixture
async def server(socket_enabled: None) -> AsyncIterator[FakeChorusServer]:
    """A fake chorus server on loopback, serving the shared rich state."""
    fake = FakeChorusServer(shared("state-rich.json"))
    await fake.start()
    yield fake
    await fake.stop()


@pytest.fixture
def entry(hass: HomeAssistant, server: FakeChorusServer) -> MockConfigEntry:
    """A config entry pointing at the fake server."""
    config_entry = MockConfigEntry(
        domain=DOMAIN,
        title="chorus",
        unique_id=SERVER_ID,
        data={CONF_HOST: "127.0.0.1", CONF_PORT: server.port},
    )
    config_entry.add_to_hass(hass)
    return config_entry


@pytest.fixture
async def setup(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> MockConfigEntry:
    """The integration set up against the fake server, its stream attached."""
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    await wait_for(lambda: server.subscribers == 1)
    return entry


async def wait_for(condition, timeout: float = 5.0) -> None:  # noqa: ASYNC109
    """Wait, on the real loop, until a condition holds."""
    import asyncio  # noqa: PLC0415

    async with asyncio.timeout(timeout):
        while not condition():
            await asyncio.sleep(0.005)


def room(hass: HomeAssistant, zone_id: str, server_id: str = SERVER_ID) -> str:
    """Return the entity id of a room's media player."""
    return _entity_id(hass, "media_player", f"{server_id}:room:{zone_id}")


def saved_group(hass: HomeAssistant, group_id: str, server_id: str = SERVER_ID) -> str:
    """Return the entity id of a saved group's media player."""
    return _entity_id(hass, "media_player", f"{server_id}:group:{group_id}")


def group_volume(hass: HomeAssistant, zone_id: str, server_id: str = SERVER_ID) -> str:
    """Return the entity id of a room's group-volume number."""
    return _entity_id(hass, "number", f"{server_id}:room:{zone_id}:group_volume")


def room_entity(
    hass: HomeAssistant,
    platform: str,
    zone_id: str,
    key: str,
    server_id: str = SERVER_ID,
) -> str:
    """Return the entity id of one of a room's controls (`bass`, `night`, ...)."""
    return _entity_id(hass, platform, f"{server_id}:room:{zone_id}:{key}")


def autoplay_switch(
    hass: HomeAssistant, input_id: str, target: str, server_id: str = SERVER_ID
) -> str:
    """Return the entity id of an autoplay rule's switch."""
    return _entity_id(hass, "switch", f"{server_id}:autoplay:{input_id}:{target}")


def speaker_firmware(
    hass: HomeAssistant, speaker_id: str, server_id: str = SERVER_ID
) -> str:
    """Return the entity id of a speaker's firmware update entity."""
    return _entity_id(hass, "update", f"{server_id}:speaker:{speaker_id}:firmware")


def speaker_button(
    hass: HomeAssistant, speaker_id: str, button: str, server_id: str = SERVER_ID
) -> str:
    """Return the entity id of one of a speaker's button event entities."""
    return _entity_id(
        hass, "event", f"{server_id}:speaker:{speaker_id}:button:{button}"
    )


def only(vector: bytes, *members: str) -> bytes:
    """Return a shared command vector cut down to `v`, `t` and these members.

    The catalog's partial commands carry only the fields they change, in the
    order the full vector has them, so the bytes one control sends are the
    vector's with the other fields left out.
    """
    import json  # noqa: PLC0415

    fields = json.loads(vector)
    kept = {k: v for k, v in fields.items() if k in ("v", "t", *members)}
    assert set(kept) == {"v", "t", *members}, members
    return json.dumps(kept, separators=(",", ":"), ensure_ascii=False).encode()


def _entity_id(hass: HomeAssistant, platform: str, unique_id: str) -> str:
    from homeassistant.helpers import entity_registry as er  # noqa: PLC0415

    entity_id = er.async_get(hass).async_get_entity_id(platform, DOMAIN, unique_id)
    assert entity_id is not None, unique_id
    return entity_id
