"""The config flow: a host and port (primary), zeroconf discovery, reconfigure."""

from __future__ import annotations

from typing import Any

from homeassistant.config_entries import ConfigFlow, ConfigFlowResult
from homeassistant.const import CONF_HOST, CONF_PORT
from homeassistant.helpers.aiohttp_client import async_get_clientsession
import homeassistant.helpers.config_validation as cv
from homeassistant.helpers.service_info.zeroconf import ZeroconfServiceInfo
import voluptuous as vol

from ._aiochorus import (
    ChorusClient,
    ChorusConnectionError,
    ChorusError,
    ChorusUnsupportedError,
    ServerInfo,
)
from .const import CATALOG_VERSION, DEFAULT_PORT, DOMAIN

_TITLE = "chorus"


def _schema(host: str | None, port: int) -> vol.Schema:
    return vol.Schema(
        {
            vol.Required(CONF_HOST, default=host)
            if host is not None
            else vol.Required(CONF_HOST): cv.string,
            vol.Required(CONF_PORT, default=port): cv.port,
        }
    )


class ChorusConfigFlow(ConfigFlow, domain=DOMAIN):
    """Set up one chorus server."""

    VERSION = 1

    _host: str
    _port: int

    async def _async_probe(self, host: str, port: int) -> ServerInfo:
        """Ask the server who it is and read its state once."""
        client = ChorusClient(async_get_clientsession(self.hass), host, port)
        server = await client.server()
        if CATALOG_VERSION not in server.catalogs:
            raise ChorusUnsupportedError(
                f"the server implements catalog versions {server.catalogs}"
            )
        await client.state()
        return server

    async def _async_try(
        self, host: str, port: int, errors: dict[str, str]
    ) -> ServerInfo | None:
        try:
            return await self._async_probe(host, port)
        except ChorusUnsupportedError:
            errors["base"] = "unsupported_catalog"
        except ChorusConnectionError:
            errors["base"] = "cannot_connect"
        except ChorusError:
            errors["base"] = "invalid_response"
        return None

    async def async_step_user(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        """Ask for the server's host and control port."""
        errors: dict[str, str] = {}
        if user_input is not None:
            host = user_input[CONF_HOST]
            port = user_input[CONF_PORT]
            server = await self._async_try(host, port, errors)
            if server is not None:
                await self.async_set_unique_id(server.id)
                self._abort_if_unique_id_configured()
                return self.async_create_entry(
                    title=_TITLE, data={CONF_HOST: host, CONF_PORT: port}
                )
        return self.async_show_form(
            step_id="user",
            data_schema=self.add_suggested_values_to_schema(
                _schema(None, DEFAULT_PORT), user_input
            ),
            errors=errors,
        )

    async def async_step_zeroconf(
        self, discovery_info: ZeroconfServiceInfo
    ) -> ConfigFlowResult:
        """Handle a `_chorus-ctl._tcp.local.` advertisement."""
        host = discovery_info.host
        port = discovery_info.port or DEFAULT_PORT
        updates = {CONF_HOST: host, CONF_PORT: port}
        if server_id := discovery_info.properties.get("id"):
            # A server already set up that moved: its entry follows it.
            await self.async_set_unique_id(server_id)
            self._abort_if_unique_id_configured(updates=updates)
        errors: dict[str, str] = {}
        server = await self._async_try(host, port, errors)
        if server is None:
            return self.async_abort(reason=errors["base"])
        await self.async_set_unique_id(server.id)
        self._abort_if_unique_id_configured(updates=updates)
        self._host = host
        self._port = port
        self.context["title_placeholders"] = {"host": host}
        return await self.async_step_zeroconf_confirm()

    async def async_step_zeroconf_confirm(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        """Ask before adding a discovered server."""
        if user_input is not None:
            return self.async_create_entry(
                title=_TITLE, data={CONF_HOST: self._host, CONF_PORT: self._port}
            )
        self._set_confirm_only()
        return self.async_show_form(
            step_id="zeroconf_confirm",
            description_placeholders={"host": self._host, "port": str(self._port)},
        )

    async def async_step_reconfigure(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        """Point an entry at the same server's new host or port."""
        entry = self._get_reconfigure_entry()
        errors: dict[str, str] = {}
        if user_input is not None:
            host = user_input[CONF_HOST]
            port = user_input[CONF_PORT]
            server = await self._async_try(host, port, errors)
            if server is not None:
                await self.async_set_unique_id(server.id)
                self._abort_if_unique_id_mismatch(reason="wrong_server")
                return self.async_update_reload_and_abort(
                    entry, data_updates={CONF_HOST: host, CONF_PORT: port}
                )
        return self.async_show_form(
            step_id="reconfigure",
            data_schema=self.add_suggested_values_to_schema(
                _schema(entry.data[CONF_HOST], entry.data[CONF_PORT]), user_input
            ),
            errors=errors,
        )
