"""Fixtures shared by the integration's tests."""

from __future__ import annotations

from collections.abc import AsyncIterator

from homeassistant.const import CONF_HOST, CONF_PORT
from homeassistant.core import HomeAssistant
import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.chorus.const import DOMAIN

from .fake_server import FakeChorusServer, shared

SERVER_ID = "chorus-server-0123456789abcdef"


@pytest.fixture(autouse=True)
def _enable_custom_integrations(enable_custom_integrations: None) -> None:
    """Let Home Assistant load `custom_components/chorus`."""


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


def _entity_id(hass: HomeAssistant, platform: str, unique_id: str) -> str:
    from homeassistant.helpers import entity_registry as er  # noqa: PLC0415

    entity_id = er.async_get(hass).async_get_entity_id(platform, DOMAIN, unique_id)
    assert entity_id is not None, unique_id
    return entity_id
