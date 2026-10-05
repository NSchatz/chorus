"""Diagnostics: an allowlist of what the server said, nothing that names a host.

Diagnostics files get pasted into issues. So this is built from named fields
rather than by removing some from the state: hosts, URLs (artwork, stored
sources, announce origins), key fingerprints, speaker and endpoint ids and
titles of what is playing never enter it.
"""

from __future__ import annotations

from typing import Any

from homeassistant.components.diagnostics import async_redact_data
from homeassistant.const import CONF_HOST
from homeassistant.core import HomeAssistant

from .announce import own_origins, server_origins
from .coordinator import ChorusConfigEntry

TO_REDACT = {CONF_HOST, "unique_id", "title"}


def _source_kind(source: str) -> str:
    # `line-in:<endpoint>/<input>` names an endpoint; the kind is enough.
    return source.partition(":")[0]


async def async_get_config_entry_diagnostics(
    hass: HomeAssistant, entry: ChorusConfigEntry
) -> dict[str, Any]:
    """Return diagnostics for a config entry."""
    coordinator = entry.runtime_data
    state = coordinator.data
    server = coordinator.server
    return {
        "entry": async_redact_data(entry.as_dict(), TO_REDACT),
        "server": {
            "software": server.software,
            "catalogs": list(server.catalogs),
            "announce_origins": len(server.announce_origins),
            # Whether the server would fetch an announcement from this Home
            # Assistant; the origins themselves are addresses and stay out.
            "announces_from_this_home_assistant": bool(
                own_origins(hass) & server_origins(coordinator)
            ),
        },
        "connected": coordinator.last_update_success,
        "state": {
            "serial": state.serial,
            "zones": [
                {
                    "id": zone.id,
                    "name": zone.name,
                    "group": zone.group,
                    "volume": zone.volume,
                    "muted": zone.muted,
                    "limit": zone.limit,
                    "effective_limit": zone.effective_limit,
                    "transport": zone.transport,
                    "endpoints": len(zone.endpoints),
                    "present": len(zone.present),
                }
                for zone in state.zones
            ],
            "groups": [
                {
                    "id": group.id,
                    "kind": group.kind,
                    "zones": list(group.zones),
                    "volume": group.volume,
                    "source": _source_kind(group.source),
                    "now_playing": (
                        None
                        if group.now_playing is None
                        else {
                            "state": group.now_playing.state,
                            "via": group.now_playing.via,
                            "has_art": group.now_playing.art_url is not None,
                        }
                    ),
                }
                for group in state.groups
            ],
            "saved_groups": [
                {
                    "id": saved.id,
                    "name": saved.name,
                    "zones": list(saved.zones),
                    "active": saved.active,
                }
                for saved in state.saved_groups
            ],
            "inputs": len(state.inputs),
            "input_labels": [label.role for label in state.input_labels],
            "counts": dict(state.counts),
        },
    }
