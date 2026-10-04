"""Diagnostics: what is in the file, and that nothing in it names a host."""

from __future__ import annotations

import json

from homeassistant.core import HomeAssistant
from pytest_homeassistant_custom_component.common import MockConfigEntry
from syrupy.assertion import SnapshotAssertion
from syrupy.filters import props

from custom_components.chorus.diagnostics import async_get_config_entry_diagnostics

from .conftest import wait_for
from .fake_server import FakeChorusServer, shared


async def test_diagnostics_snapshot(
    hass: HomeAssistant, setup: MockConfigEntry, snapshot: SnapshotAssertion
) -> None:
    diagnostics = await async_get_config_entry_diagnostics(hass, setup)
    assert diagnostics == snapshot(
        exclude=props("created_at", "modified_at", "entry_id", "port")
    )
    assert diagnostics["entry"]["data"]["host"] == "**REDACTED**"
    assert diagnostics["entry"]["unique_id"] == "**REDACTED**"
    assert "127.0.0.1" not in json.dumps(diagnostics)


async def test_diagnostics_hold_no_host_url_key_or_stored_value(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    """Every vector with something to leak, read back through the diagnostics."""
    for vector in (
        "state-playing.json",
        "state-inputs.json",
        "state-speakers.json",
        "state-firmware.json",
        "state-soloist.json",
    ):
        server.set_state(shared(vector))
        raw = json.loads(shared(vector))
        await wait_for(
            lambda raw=raw: (
                [z.id for z in setup.runtime_data.data.zones]
                == [z["id"] for z in raw["zones"]]
            )
        )
        text = json.dumps(await async_get_config_entry_diagnostics(hass, setup))
        leaks = ["127.0.0.1", "ha.example", "http://", "https://", "endpoint-"]
        leaks += [s["value"] for s in raw.get("stored_sources", [])]
        for speaker in raw.get("speakers", []):
            leaks += [speaker["id"], speaker["key"]]
        for change in raw.get("key_changes", []):
            leaks += [change["pinned"], change["offered"]]
        for group in raw["groups"]:
            record = group.get("now_playing") or {}
            leaks += [v for v in (record.get("title"), record.get("art_url")) if v]
        for leak in leaks:
            assert leak not in text, (vector, leak)
