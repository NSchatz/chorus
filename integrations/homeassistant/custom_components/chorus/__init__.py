"""The chorus integration: multiroom audio rooms and saved groups as media players."""

from __future__ import annotations

from homeassistant.const import CONF_HOST, CONF_PORT, Platform
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryError, ConfigEntryNotReady
from homeassistant.helpers import device_registry as dr, issue_registry as ir
from homeassistant.helpers.aiohttp_client import async_get_clientsession

from ._aiochorus import ChorusClient, ChorusError, ChorusUnsupportedError
from .const import CATALOG_VERSION, DOMAIN
from .coordinator import (
    ChorusConfigEntry,
    ChorusCoordinator,
    async_create_unsupported_issue,
    unsupported_issue_id,
)

PLATFORMS: list[Platform] = [
    Platform.MEDIA_PLAYER,
    Platform.NUMBER,
    Platform.SELECT,
    Platform.SWITCH,
    Platform.UPDATE,
]


async def async_setup_entry(hass: HomeAssistant, entry: ChorusConfigEntry) -> bool:
    """Set up one chorus server."""
    client = ChorusClient(
        async_get_clientsession(hass), entry.data[CONF_HOST], entry.data[CONF_PORT]
    )
    try:
        server = await client.server()
        if CATALOG_VERSION not in server.catalogs:
            raise ChorusUnsupportedError(  # noqa: TRY301
                f"the server implements catalog versions {server.catalogs}"
            )
    except ChorusUnsupportedError as err:
        async_create_unsupported_issue(hass, entry)
        raise ConfigEntryError(
            translation_domain=DOMAIN, translation_key="unsupported_catalog"
        ) from err
    except ChorusError as err:
        raise ConfigEntryNotReady(
            translation_domain=DOMAIN,
            translation_key="cannot_connect",
            translation_placeholders={"error": str(err)},
        ) from err
    ir.async_delete_issue(hass, DOMAIN, unsupported_issue_id(entry))

    coordinator = ChorusCoordinator(hass, entry, client, server)
    await coordinator.async_config_entry_first_refresh()
    entry.runtime_data = coordinator

    coordinator.server_device_id = (
        dr.async_get(hass)
        .async_get_or_create(
            config_entry_id=entry.entry_id,
            identifiers={(DOMAIN, server.id)},
            manufacturer="chorus",
            model="chorus-server",
            name=entry.title,
            sw_version=server.software,
            entry_type=dr.DeviceEntryType.SERVICE,
        )
        .id
    )
    coordinator.sync_devices(coordinator.data)

    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    entry.async_create_background_task(
        hass, coordinator.async_listen(), f"{DOMAIN} event stream {entry.entry_id}"
    )
    return True


async def async_unload_entry(hass: HomeAssistant, entry: ChorusConfigEntry) -> bool:
    """Unload one chorus server; the event stream's task ends with the entry."""
    return await hass.config_entries.async_unload_platforms(entry, PLATFORMS)
