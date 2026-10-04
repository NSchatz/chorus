"""A speaker's diagnostic sensors, fed by `GET /metrics` at a bounded rate."""

from __future__ import annotations

import asyncio
from collections.abc import Callable, Iterator
from datetime import timedelta
import json
from typing import Any
from unittest.mock import PropertyMock, patch

from freezegun.api import FrozenDateTimeFactory, real_monotonic
from homeassistant.config_entries import ConfigEntryState
from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.helpers import entity_registry as er
import pytest
from pytest_homeassistant_custom_component.common import (
    MockConfigEntry,
    async_fire_time_changed,
)

from custom_components.chorus._aiochorus import ChorusClient
from custom_components.chorus.const import (
    DOMAIN,
    METRICS_MIN_GAP_SECONDS,
    METRICS_SCAN_INTERVAL,
)
from custom_components.chorus.sensor import SENSORS

from .conftest import SERVER_ID, room
from .fake_server import FakeChorusServer, metrics_sample, shared

WIFI = "chorus-0123456789ab"
WIRED = "chorus-ba9876543210"
SPEAKERS = (WIFI, WIRED)
KEYS = {description.key for description in SENSORS}
# The named set that starts enabled: the two that almost never change.
ENABLED_BY_DEFAULT = {"link", "firmware_version"}
INTERVAL = METRICS_SCAN_INTERVAL.total_seconds()


async def wait_for(condition: Callable[[], bool]) -> None:
    """Wait until a condition holds, without a timer.

    Under the `freezer` fixture the event loop's clock stands still between
    ticks, so a sleep never ends: this yields to the loop (which still serves
    the fake server's sockets) and bounds the wait on the real clock.
    """
    deadline = real_monotonic() + 5.0
    while not condition():
        assert real_monotonic() < deadline, "the condition did not come to hold"
        await asyncio.sleep(0)


@pytest.fixture
def every_sensor_enabled() -> Iterator[None]:
    """Register every entity enabled, as an owner who enabled them all has."""
    with patch(
        "homeassistant.helpers.entity.Entity.entity_registry_enabled_default",
        new_callable=PropertyMock(return_value=True),
    ):
        yield


def unique_id(speaker_id: str, key: str) -> str:
    return f"{SERVER_ID}:speaker:{speaker_id}:{key}"


def sensor(hass: HomeAssistant, speaker_id: str, key: str) -> str:
    entity_id = er.async_get(hass).async_get_entity_id(
        "sensor", DOMAIN, unique_id(speaker_id, key)
    )
    assert entity_id is not None, (speaker_id, key)
    return entity_id


def value(hass: HomeAssistant, speaker_id: str, key: str) -> str:
    state = hass.states.get(sensor(hass, speaker_id, key))
    assert state is not None, (speaker_id, key)
    return state.state


def scrapes(server: FakeChorusServer) -> int:
    return server.requests.count("GET /metrics")


async def start(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    raw: dict[str, Any] | None = None,
) -> None:
    """Set the entry up against a server with two speakers and the sample scrape."""
    if raw is not None:
        server.state_bytes = json.dumps(raw, separators=(",", ":")).encode()
    else:
        server.state_bytes = shared("state-firmware.json")
    if server.metrics_bytes is None:
        server.metrics_bytes = metrics_sample()
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    await wait_for(lambda: server.subscribers == 1)


class SimulatedTime:
    """Simulated time for a test whose integration talks to a real socket.

    A tick fires every loop timer that came due, the timeout of a request
    still on its way among them. So a tick is followed by `settle`, which
    waits (on the real clock) until the client has no request in flight: the
    scheduled scrape and the event stream's liveness probe run as background
    tasks, which `async_block_till_done` does not wait for.
    """

    def __init__(
        self,
        hass: HomeAssistant,
        freezer: FrozenDateTimeFactory,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        self.hass = hass
        self.freezer = freezer
        self.in_flight = 0
        get = ChorusClient._get
        clock = self

        async def counted(client: ChorusClient, path: str) -> tuple[int, bytes]:
            clock.in_flight += 1
            try:
                return await get(client, path)
            finally:
                clock.in_flight -= 1

        monkeypatch.setattr(ChorusClient, "_get", counted)

    async def settle(self) -> None:
        for _ in range(5):
            await asyncio.sleep(0)
        await wait_for(lambda: self.in_flight == 0)
        for _ in range(5):
            await asyncio.sleep(0)
        await self.hass.async_block_till_done()

    async def advance(self, seconds: float) -> None:
        """Move simulated time on and let what it fires finish."""
        self.freezer.tick(timedelta(seconds=seconds))
        async_fire_time_changed(self.hass)
        await self.settle()


@pytest.fixture
def clock(
    hass: HomeAssistant, freezer: FrozenDateTimeFactory, monkeypatch: pytest.MonkeyPatch
) -> SimulatedTime:
    return SimulatedTime(hass, freezer, monkeypatch)


@pytest.mark.usefixtures("every_sensor_enabled")
async def test_sensor_values_parsed_from_the_servers_text(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    await start(hass, entry, server)
    await wait_for(lambda: value(hass, WIFI, "link") == "wifi")

    # The speaker on Wi-Fi: every series of the sample, in the sensor's unit.
    assert float(value(hass, WIFI, "sync_error")) == 18.5  # 0.000018500 s
    assert float(value(hass, WIFI, "buffer_fill")) == 251.333  # 0.251333 s
    assert float(value(hass, WIFI, "rate_correction")) == -3.1  # -0.000003100
    assert value(hass, WIFI, "resyncs") == "2"
    assert value(hass, WIFI, "rssi") == "-58"
    assert float(value(hass, WIFI, "temperature")) == 41.5
    assert value(hass, WIFI, "firmware_version") == "1.0.0"

    # The wired speaker: what it knows, and unknown (never zero) for what the
    # scrape leaves out.
    assert float(value(hass, WIRED, "sync_error")) == -42.0
    assert float(value(hass, WIRED, "buffer_fill")) == 120.0
    assert float(value(hass, WIRED, "rate_correction")) == 1.25
    assert value(hass, WIRED, "resyncs") == "0"
    assert value(hass, WIRED, "link") == "wired"
    assert value(hass, WIRED, "rssi") == "unknown"
    assert value(hass, WIRED, "temperature") == "unknown"
    assert value(hass, WIRED, "firmware_version") == "chorus-client 0.1.0"

    units = {
        key: hass.states.get(sensor(hass, WIFI, key)).attributes.get(
            "unit_of_measurement"
        )
        for key in KEYS
    }
    assert units == {
        "sync_error": "μs",
        "buffer_fill": "ms",
        "rate_correction": "ppm",
        "resyncs": None,
        "link": None,
        "rssi": "dBm",
        "temperature": "°C",
        "firmware_version": None,
    }
    names = {
        hass.states.get(sensor(hass, WIFI, key)).name.removeprefix("Speaker 89ab ")
        for key in KEYS
    }
    assert names == {
        "Sync error",
        "Buffer fill",
        "Rate correction",
        "Resyncs",
        "Link",
        "Signal strength",
        "Temperature",
        "Firmware version",
    }


async def test_sensor_registry_every_one_diagnostic_and_most_disabled_by_default(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    await start(hass, entry, server)
    registry = er.async_get(hass)
    assert sorted(description.key for description in SENSORS) == [
        "buffer_fill",
        "firmware_version",
        "link",
        "rate_correction",
        "resyncs",
        "rssi",
        "sync_error",
        "temperature",
    ]
    ours = [
        e
        for e in er.async_entries_for_config_entry(registry, entry.entry_id)
        if e.domain == "sensor"
    ]
    assert {e.unique_id for e in ours} == {
        unique_id(speaker, key) for speaker in SPEAKERS for key in KEYS
    }
    for registered in ours:
        key = registered.unique_id.rsplit(":", 1)[1]
        assert registered.entity_category is EntityCategory.DIAGNOSTIC, key
        if key in ENABLED_BY_DEFAULT:
            assert registered.disabled_by is None, key
            assert hass.states.get(registered.entity_id) is not None, key
        else:
            assert registered.disabled_by is er.RegistryEntryDisabler.INTEGRATION, key
            # A disabled entity has no state: nothing of it reaches the recorder.
            assert hass.states.get(registered.entity_id) is None, key
        # Each one hangs on its speaker's device.
        device_id = registered.device_id
        assert device_id is not None
    await wait_for(lambda: value(hass, WIFI, "link") == "wifi")
    assert value(hass, WIRED, "firmware_version") == "chorus-client 0.1.0"


async def test_sensor_scrapes_never_exceed_the_documented_rate(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    clock: SimulatedTime,
) -> None:
    assert INTERVAL == 60
    assert METRICS_MIN_GAP_SECONDS == 55
    await start(hass, entry, server)
    # One scrape when the first enabled sensor is added, for its first value.
    await wait_for(lambda: scrapes(server) == 1)
    await wait_for(lambda: value(hass, WIFI, "link") == "wifi")
    metrics = entry.runtime_data.metrics

    elapsed = 0
    step = 5
    for _ in range(10 * 60 // step):
        await clock.advance(step)
        elapsed += step
        # Something asks for a refresh far more often than the interval (the
        # `homeassistant.update_entity` action does exactly this).
        await metrics.async_refresh()
        await metrics.async_request_refresh()
        await clock.settle()
        # Never more than the one at the start and one per whole minimum gap.
        assert scrapes(server) <= 1 + elapsed // METRICS_MIN_GAP_SECONDS, elapsed
    # Ten simulated minutes: the first scrape and at most one a minute after,
    # and the polling did go on.
    assert 1 + 10 * 60 // INTERVAL - 1 <= scrapes(server) <= 1 + 10 * 60 // 55
    assert value(hass, WIFI, "link") == "wifi"


async def test_sensor_no_scrape_while_no_diagnostic_entity_is_enabled(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    clock: SimulatedTime,
) -> None:
    # The owner disabled the few that start enabled.
    registry = er.async_get(hass)
    for speaker in SPEAKERS:
        for key in ENABLED_BY_DEFAULT:
            registry.async_get_or_create(
                "sensor",
                DOMAIN,
                unique_id(speaker, key),
                config_entry=entry,
                disabled_by=er.RegistryEntryDisabler.USER,
            )
    await start(hass, entry, server)
    assert not [
        e
        for e in er.async_entries_for_config_entry(registry, entry.entry_id)
        if e.domain == "sensor" and e.disabled_by is None
    ]
    for _ in range(20):
        await clock.advance(30)
    assert scrapes(server) == 0
    assert "GET /metrics" not in server.requests
    # The media players were served all along.
    assert hass.states.get(room(hass, "kitchen")).state != "unavailable"

    # One sensor enabled (Home Assistant reloads the entry for it): now, and
    # only now, the server is asked.
    registry.async_update_entity(sensor(hass, WIFI, "rssi"), disabled_by=None)
    await hass.config_entries.async_reload(entry.entry_id)
    await hass.async_block_till_done()
    await wait_for(lambda: scrapes(server) == 1)
    await wait_for(lambda: value(hass, WIFI, "rssi") == "-58")
    for _ in range(4):
        await clock.advance(30)
    assert scrapes(server) == 3


@pytest.mark.parametrize(
    ("status", "body"),
    [
        (500, None),
        (404, None),
        (200, b"<html><body>every worker is busy</body></html>\n"),
        (200, b""),
        (200, b"\xff\xfe"),
        (200, metrics_sample()[:-30]),
        (200, metrics_sample().replace(b" -58\n", b" strong\n")),
    ],
)
async def test_sensor_failed_or_malformed_scrape_leaves_the_media_players_alone(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    clock: SimulatedTime,
    caplog: pytest.LogCaptureFixture,
    status: int,
    body: bytes | None,
) -> None:
    await start(hass, entry, server)
    await wait_for(lambda: value(hass, WIFI, "link") == "wifi")
    kitchen = room(hass, "kitchen")
    players = {
        entity_id: hass.states.get(entity_id).state
        for entity_id in hass.states.async_entity_ids("media_player")
    }
    assert players
    assert "unavailable" not in players.values()

    server.metrics_status = status
    if body is not None:
        server.metrics_bytes = body
    before = scrapes(server)
    for _ in range(3):
        await clock.advance(INTERVAL)
    assert scrapes(server) == before + 3

    # The diagnostic sensors are unavailable.
    for speaker in SPEAKERS:
        for key in ENABLED_BY_DEFAULT:
            assert value(hass, speaker, key) == "unavailable", (speaker, key)
    # It is logged once, not at every scrape.
    assert caplog.text.count("Error fetching chorus speaker diagnostics data") == 1
    # The media players, the entry and the state's coordinator are untouched.
    assert entry.state is ConfigEntryState.LOADED
    assert entry.runtime_data.last_update_success
    assert {
        entity_id: hass.states.get(entity_id).state
        for entity_id in hass.states.async_entity_ids("media_player")
    } == players
    assert server.subscribers == 1
    await hass.services.async_call(
        "media_player",
        "volume_set",
        {"entity_id": kitchen, "volume_level": 0.25},
        blocking=True,
    )
    assert json.loads(server.bodies[-1])["t"] == "volume"

    # The next good scrape brings them back.
    server.metrics_status = 200
    server.metrics_bytes = metrics_sample()
    await clock.advance(INTERVAL)
    assert value(hass, WIFI, "link") == "wifi"
    assert value(hass, WIRED, "firmware_version") == "chorus-client 0.1.0"


@pytest.mark.usefixtures("every_sensor_enabled")
async def test_sensor_a_disconnected_speaker_keeps_only_its_firmware_version(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    clock: SimulatedTime,
) -> None:
    await start(hass, entry, server)
    await wait_for(lambda: value(hass, WIRED, "link") == "wired")

    server.metrics_bytes = metrics_sample(disconnected=WIRED)
    await clock.advance(INTERVAL)
    for key in KEYS - {"firmware_version"}:
        assert value(hass, WIRED, key) == "unavailable", key
    assert value(hass, WIRED, "firmware_version") == "chorus-client 0.1.0"
    # The other speaker is as it was.
    assert value(hass, WIFI, "link") == "wifi"
    assert value(hass, WIFI, "rssi") == "-58"


async def test_sensor_speakers_adopted_and_forgotten(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    both = json.loads(shared("state-firmware.json"))
    one = json.loads(shared("state-firmware.json"))
    one["speakers"] = one["speakers"][:1]
    registry = er.async_get(hass)

    def registered() -> set[str]:
        return {
            e.unique_id
            for e in er.async_entries_for_config_entry(registry, entry.entry_id)
            if e.domain == "sensor"
        }

    await start(hass, entry, server, one)
    assert registered() == {unique_id(WIFI, key) for key in KEYS}

    # A second speaker is adopted: its sensors appear with it.
    server.set_state(json.dumps(both, separators=(",", ":")).encode())
    await wait_for(lambda: len(registered()) == 2 * len(KEYS))
    await wait_for(lambda: value(hass, WIRED, "link") in ("wired", "unavailable"))

    # It is forgotten: its device goes, and its sensors with it.
    server.set_state(json.dumps(one, separators=(",", ":")).encode())
    await wait_for(lambda: registered() == {unique_id(WIFI, key) for key in KEYS})

    # No speaker at all (the shared rich state): no sensor, and no scrape.
    await hass.config_entries.async_unload(entry.entry_id)
    await hass.async_block_till_done()
