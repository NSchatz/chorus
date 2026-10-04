"""The config flow: user, zeroconf, reconfigure, duplicates and failures."""

from __future__ import annotations

from ipaddress import ip_address

from homeassistant.config_entries import SOURCE_USER, SOURCE_ZEROCONF
from homeassistant.const import CONF_HOST, CONF_PORT
from homeassistant.core import HomeAssistant
from homeassistant.data_entry_flow import FlowResultType
from homeassistant.helpers.service_info.zeroconf import ZeroconfServiceInfo
import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.chorus.const import DOMAIN

from .conftest import SERVER_ID, wait_for
from .fake_server import FakeChorusServer

OTHER = (
    b'{"v":2,"t":"server","id":"another-server","software":"0.18.0",'
    b'"catalogs":[1,2],"announce_origins":[]}'
)
CATALOG_1 = b'{"v":1,"t":"server","id":"old-server","software":"0.9.0","catalogs":[1]}'


def discovery(port: int, properties: dict[str, str]) -> ZeroconfServiceInfo:
    return ZeroconfServiceInfo(
        ip_address=ip_address("127.0.0.1"),
        ip_addresses=[ip_address("127.0.0.1")],
        hostname="chorus.local.",
        name="chorus._chorus-ctl._tcp.local.",
        port=port,
        properties=properties,
        type="_chorus-ctl._tcp.local.",
    )


async def test_user_flow(hass: HomeAssistant, server: FakeChorusServer) -> None:
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": SOURCE_USER}
    )
    assert result["type"] is FlowResultType.FORM
    assert result["step_id"] == "user"
    assert result["errors"] == {}
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {CONF_HOST: "127.0.0.1", CONF_PORT: server.port}
    )
    assert result["type"] is FlowResultType.CREATE_ENTRY
    assert result["title"] == "chorus"
    assert result["data"] == {CONF_HOST: "127.0.0.1", CONF_PORT: server.port}
    assert result["result"].unique_id == SERVER_ID
    # Tested before it was configured: who the server is, then its state.
    assert server.requests[:2] == ["GET /api/server", "GET /api/state"]
    await hass.async_block_till_done()


@pytest.mark.parametrize(
    ("fault", "error"),
    [
        ("down", "cannot_connect"),
        ("catalog-1", "unsupported_catalog"),
        ("no-server-route", "unsupported_catalog"),
        ("not-chorus", "invalid_response"),
        ("bad-state", "invalid_response"),
    ],
)
async def test_user_flow_errors_then_recovers(
    hass: HomeAssistant, server: FakeChorusServer, fault: str, error: str
) -> None:
    good = server.server_bytes
    port = server.port
    if fault == "down":
        await server.stop()
    elif fault == "catalog-1":
        server.server_bytes = CATALOG_1
    elif fault == "no-server-route":
        server.server_status = 404
    elif fault == "not-chorus":
        server.server_bytes = b"<html>a router's page</html>"
    else:
        server.state_status = 500
    result = await hass.config_entries.flow.async_init(
        DOMAIN,
        context={"source": SOURCE_USER},
        data={CONF_HOST: "127.0.0.1", CONF_PORT: port},
    )
    assert result["type"] is FlowResultType.FORM
    assert result["errors"] == {"base": error}

    # The fault is fixed and the same flow finishes.
    if fault == "down":
        await server.start(port)
    server.server_bytes = good
    server.server_status = 200
    server.state_status = 200
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {CONF_HOST: "127.0.0.1", CONF_PORT: port}
    )
    assert result["type"] is FlowResultType.CREATE_ENTRY
    await hass.async_block_till_done()


async def test_user_flow_aborts_on_a_server_already_set_up(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    result = await hass.config_entries.flow.async_init(
        DOMAIN,
        context={"source": SOURCE_USER},
        data={CONF_HOST: "127.0.0.1", CONF_PORT: server.port},
    )
    assert result["type"] is FlowResultType.ABORT
    assert result["reason"] == "already_configured"
    assert setup.data[CONF_HOST] == "127.0.0.1"


@pytest.mark.parametrize("txt", [{"v": "2", "id": SERVER_ID}, {"v": "2"}])
async def test_zeroconf_flow(
    hass: HomeAssistant, server: FakeChorusServer, txt: dict[str, str]
) -> None:
    """With the id in the TXT record, or fetched from the server without it."""
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": SOURCE_ZEROCONF}, data=discovery(server.port, txt)
    )
    assert result["type"] is FlowResultType.FORM
    assert result["step_id"] == "zeroconf_confirm"
    assert result["description_placeholders"] == {
        "host": "127.0.0.1",
        "port": str(server.port),
    }
    result = await hass.config_entries.flow.async_configure(result["flow_id"], {})
    assert result["type"] is FlowResultType.CREATE_ENTRY
    assert result["data"] == {CONF_HOST: "127.0.0.1", CONF_PORT: server.port}
    assert result["result"].unique_id == SERVER_ID
    await hass.async_block_till_done()


@pytest.mark.parametrize("txt", [{"v": "2", "id": SERVER_ID}, {"v": "2"}])
async def test_zeroconf_updates_the_address_of_an_existing_entry(
    hass: HomeAssistant, server: FakeChorusServer, txt: dict[str, str]
) -> None:
    """The server moved: its entry follows it instead of a second one appearing."""
    entry = MockConfigEntry(
        domain=DOMAIN,
        unique_id=SERVER_ID,
        data={CONF_HOST: "192.0.2.10", CONF_PORT: 4020},
    )
    entry.add_to_hass(hass)
    result = await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": SOURCE_ZEROCONF}, data=discovery(server.port, txt)
    )
    assert result["type"] is FlowResultType.ABORT
    assert result["reason"] == "already_configured"
    assert entry.data == {CONF_HOST: "127.0.0.1", CONF_PORT: server.port}
    assert len(hass.config_entries.async_entries(DOMAIN)) == 1
    await hass.async_block_till_done()


@pytest.mark.parametrize(
    ("fault", "reason"),
    [("down", "cannot_connect"), ("catalog-1", "unsupported_catalog")],
)
async def test_zeroconf_aborts_on_a_server_it_cannot_use(
    hass: HomeAssistant, server: FakeChorusServer, fault: str, reason: str
) -> None:
    port = server.port
    if fault == "down":
        await server.stop()
    else:
        server.server_bytes = CATALOG_1
    result = await hass.config_entries.flow.async_init(
        DOMAIN,
        context={"source": SOURCE_ZEROCONF},
        data=discovery(port, {"v": "1", "id": "old-server"}),
    )
    assert result["type"] is FlowResultType.ABORT
    assert result["reason"] == reason


async def test_reconfigure(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    """The same server answers on another port: the entry is pointed at it."""
    moved = FakeChorusServer(server.state_bytes)
    await moved.start()
    try:
        result = await setup.start_reconfigure_flow(hass)
        assert result["type"] is FlowResultType.FORM
        assert result["step_id"] == "reconfigure"
        result = await hass.config_entries.flow.async_configure(
            result["flow_id"], {CONF_HOST: "127.0.0.1", CONF_PORT: moved.port}
        )
        assert result["type"] is FlowResultType.ABORT
        assert result["reason"] == "reconfigure_successful"
        assert setup.data == {CONF_HOST: "127.0.0.1", CONF_PORT: moved.port}
        await hass.async_block_till_done()
        # The entry was reloaded against the new address.
        await wait_for(lambda: moved.subscribers == 1)
        assert await hass.config_entries.async_unload(setup.entry_id)
        await wait_for(lambda: moved.subscribers == 0)
    finally:
        await moved.stop()


async def test_reconfigure_errors_and_refuses_another_server(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    result = await setup.start_reconfigure_flow(hass)
    server.state_status = 500
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {CONF_HOST: "127.0.0.1", CONF_PORT: server.port}
    )
    assert result["type"] is FlowResultType.FORM
    assert result["errors"] == {"base": "invalid_response"}

    server.state_status = 200
    server.server_bytes = OTHER
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {CONF_HOST: "127.0.0.1", CONF_PORT: server.port}
    )
    assert result["type"] is FlowResultType.ABORT
    assert result["reason"] == "wrong_server"
    assert setup.unique_id == SERVER_ID
