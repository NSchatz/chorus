"""The example dashboard (`dashboard/chorus.yaml`): stock cards only (P10, Option A).

It is YAML a person copies into Home Assistant, so nothing runs it here. These
tests hold it to what can be held without a browser: it parses; every entity it
names is an enabled entity of the integration set up against the fake server;
every view, section, card, badge, tile feature and visibility condition is on an
explicit list of Home Assistant's own types; it has no resource, no action and
no URL. The checks are then run on bad examples, which they must name.
"""

from __future__ import annotations

from collections.abc import Iterator
import copy
from pathlib import Path
from typing import Any

from homeassistant.core import HomeAssistant
from homeassistant.helpers import entity_registry as er
from homeassistant.util.yaml import parse_yaml
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.chorus.const import DOMAIN

from .conftest import group_volume, room, saved_group, wait_for
from .fake_server import FakeChorusServer

DASHBOARD = Path(__file__).resolve().parents[1] / "dashboard" / "chorus.yaml"

# Home Assistant's own types, each as the 2026.9 documentation names it. A type
# that is not here fails the test: adding one is a decision, not a default.
VIEW_TYPES = {"sections"}
SECTION_TYPES = {"grid"}
CARD_TYPES = {"heading", "tile", "media-control"}
BADGE_TYPES = {"entity"}
# The tile feature, and the entity domain it is documented for.
FEATURE_DOMAINS = {
    "media-player-playback": "media_player",
    "media-player-volume-slider": "media_player",
    "media-player-source": "media_player",
    "numeric-input": "number",
    "toggle": "switch",
}
CONDITIONS = {"state", "numeric_state"}

# The key a list of typed things hangs under, and the types allowed there.
TYPED_LISTS = {
    "views": VIEW_TYPES,
    "sections": SECTION_TYPES,
    "cards": CARD_TYPES,
    "badges": BADGE_TYPES,
    "features": set(FEATURE_DOMAINS),
}
# Keys that load code or lead out of the dashboard. None is needed here.
FORBIDDEN_KEYS = {
    "resources",
    "url",
    "url_path",
    "navigation_path",
    "tap_action",
    "hold_action",
    "double_tap_action",
    "icon_tap_action",
    "strategy",
    "card_mod",
}
URL_MARKS = ("://", "/local/", "/hacsfiles/", "/api/", "www.", "custom:")


def load() -> dict[str, Any]:
    """Return the dashboard as Home Assistant's own YAML loader reads it."""
    config = parse_yaml(DASHBOARD.read_text(encoding="utf-8"))
    assert isinstance(config, dict)
    return config


def nodes(value: Any) -> Iterator[dict[str, Any]]:
    """Yield every mapping of the dashboard, the dashboard itself first."""
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from nodes(child)
    elif isinstance(value, list):
        for child in value:
            yield from nodes(child)


def strings(value: Any) -> Iterator[str]:
    """Yield every string value of the dashboard."""
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for child in value.values():
            yield from strings(child)
    elif isinstance(value, list):
        for child in value:
            yield from strings(child)


def type_problems(config: dict[str, Any]) -> list[str]:
    """Return every `type` that is not a stock type allowed where it stands."""
    problems = []
    reached = 0
    for node in nodes(config):
        for key, allowed in TYPED_LISTS.items():
            for item in node.get(key, []):
                reached += 1
                kind = item.get("type") if isinstance(item, dict) else None
                if kind not in allowed:
                    problems.append(
                        f"{key}: type {kind!r} is not an allowed stock type"
                    )
        problems.extend(
            f"visibility: condition {condition.get('condition')!r}"
            for condition in node.get("visibility", [])
            if condition.get("condition") not in CONDITIONS
        )
    # A `type` anywhere else (a nested card of a kind this walk does not know)
    # would escape the allowlist, so every one must have been reached.
    typed = sum(1 for node in nodes(config) if "type" in node)
    if typed != reached:
        problems.append(f"{typed} mappings carry a type and {reached} were checked")
    return problems


def link_problems(config: dict[str, Any], text: str) -> list[str]:
    """Return every resource, action and URL of the dashboard."""
    problems = [
        f"key {key!r}"
        for node in nodes(config)
        for key in node
        if key in FORBIDDEN_KEYS
    ]
    problems += [
        f"string {value!r}"
        for value in strings(config)
        if value.startswith("/") or any(mark in value for mark in URL_MARKS)
    ]
    # The comments too: a URL in a comment is one edit from being a resource.
    problems += [f"text contains {mark!r}" for mark in URL_MARKS if mark in text]
    return problems


def entities(config: dict[str, Any]) -> set[str]:
    """Return every entity id the dashboard names, in cards and in conditions."""
    found: set[str] = set()
    for node in nodes(config):
        if "entity" in node:
            found.add(node["entity"])
        found.update(node.get("entities", []))
    return found


def entity_problems(hass: HomeAssistant, config: dict[str, Any]) -> list[str]:
    """Return every entity that is not an enabled, present entity of chorus."""
    registry = er.async_get(hass)
    problems = []
    for entity_id in sorted(entities(config)):
        entry = registry.async_get(entity_id)
        if entry is None or entry.platform != DOMAIN:
            problems.append(f"{entity_id} is not an entity of the integration")
        elif entry.disabled_by is not None:
            problems.append(f"{entity_id} is disabled by default")
        elif hass.states.get(entity_id) is None:
            problems.append(f"{entity_id} has no state")
    for node in nodes(config):
        for feature in node.get("features", []):
            domain = FEATURE_DOMAINS.get(feature.get("type"))
            if domain is not None and node["entity"].split(".")[0] != domain:
                problems.append(f"{feature['type']} on {node['entity']}")
    return problems


def shown(hass: HomeAssistant, card: dict[str, Any]) -> bool:
    """Evaluate a card's visibility conditions as the frontend documents them."""
    for condition in card.get("visibility", []):
        state = hass.states.get(condition["entity"])
        value = None if state is None else state.state
        if condition["condition"] == "state":
            wanted = condition["state"]
            if value not in (wanted if isinstance(wanted, list) else [wanted]):
                return False
        else:
            try:
                number = float(value)
            except TypeError, ValueError:
                return False
            if not number > condition["above"]:
                return False
    return True


def cards_of(config: dict[str, Any], entity_id: str) -> list[dict[str, Any]]:
    return [
        card
        for node in nodes(config)
        for card in node.get("cards", [])
        if card.get("entity") == entity_id
    ]


# --- the dashboard ------------------------------------------------------------------


def test_dashboard_yaml_parses_into_one_sections_view() -> None:
    config = load()
    assert set(config) == {"title", "views"}
    (view,) = config["views"]
    assert view["type"] == "sections"
    assert len(view["sections"]) > 2
    assert all(section["cards"] for section in view["sections"])


async def test_dashboard_every_entity_exists_in_the_integration(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    config = load()
    assert entity_problems(hass, config) == []
    named = entities(config)
    assert len(named) > 20
    # And the other way round: every room and saved group of the house has its
    # player on the dashboard, and every room its group volume.
    registry = er.async_get(hass)
    players = {
        entry.entity_id
        for entry in registry.entities.values()
        if entry.platform == DOMAIN and entry.domain == "media_player"
    }
    assert len(players) == 5
    assert players <= named
    for zone in ("living", "kitchen", "study", "bedroom"):
        assert group_volume(hass, zone) in named, zone


def test_dashboard_every_type_is_an_allowed_stock_type() -> None:
    config = load()
    assert type_problems(config) == []
    used = {node["type"] for node in nodes(config) if "type" in node}
    # What the dashboard is made of (P10, Option A), so a card dropped by an
    # edit is seen too.
    assert used >= {"sections", "grid", "heading", "tile", "media-control"}
    assert used >= set(FEATURE_DOMAINS)
    assert not any(str(kind).startswith("custom:") for kind in used)


def test_dashboard_has_no_resource_no_action_and_no_url() -> None:
    assert link_problems(load(), DASHBOARD.read_text(encoding="utf-8")) == []


async def test_dashboard_group_volume_tile_shows_only_while_the_room_is_grouped(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    config = load()
    tiles = {}
    for zone in ("living", "kitchen", "study", "bedroom"):
        entity_id = group_volume(hass, zone)
        (tile,) = cards_of(config, entity_id)
        # The condition is a numeric test on the number itself, so the tile is
        # hidden while the number is unavailable or missing.
        assert tile["visibility"] == [
            {"condition": "numeric_state", "entity": entity_id, "above": -1}
        ]
        assert tile["features"] == [{"type": "numeric-input", "style": "slider"}]
        tiles[zone] = tile
    # The rich state: living and kitchen in Downstairs, study and bedroom live.
    assert all(shown(hass, tile) for tile in tiles.values())

    await hass.services.async_call(
        "media_player", "unjoin", {"entity_id": room(hass, "bedroom")}, blocking=True
    )
    bedroom = group_volume(hass, "bedroom")
    await wait_for(lambda: hass.states.get(bedroom).state == "unavailable")
    assert {zone: shown(hass, tile) for zone, tile in tiles.items()} == {
        "living": True,
        "kitchen": True,
        "study": False,
        "bedroom": False,
    }


async def test_dashboard_room_and_group_tiles_carry_playback_volume_and_source(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    config = load()
    players = [room(hass, z) for z in ("living", "kitchen", "study", "bedroom")]
    players.append(saved_group(hass, "downstairs"))
    for entity_id in players:
        (tile,) = [c for c in cards_of(config, entity_id) if c["type"] == "tile"]
        assert [feature["type"] for feature in tile["features"]] == [
            "media-player-playback",
            "media-player-volume-slider",
            "media-player-source",
        ], entity_id
        assert "visibility" not in tile, entity_id
    # Now playing: a media control card per room, hidden without a record (the
    # rich state has none: every player is `on`).
    for entity_id in players[:4]:
        (card,) = [
            c for c in cards_of(config, entity_id) if c["type"] == "media-control"
        ]
        assert card["visibility"][0]["state"] == ["playing", "paused", "buffering"]
        assert hass.states.get(entity_id).state == "on"
        assert not shown(hass, card)


# --- the checks, on bad examples ----------------------------------------------------


async def test_dashboard_checks_name_a_custom_card_a_missing_entity_and_a_url(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    good = load()
    cards = good["views"][0]["sections"][0]["cards"]

    bad = copy.deepcopy(good)
    bad["views"][0]["sections"][0]["cards"].append(
        {"type": "custom:chorus-rooms-card", "entity": cards[1]["entity"]}
    )
    assert type_problems(bad) == [
        "cards: type 'custom:chorus-rooms-card' is not an allowed stock type"
    ]

    bad = copy.deepcopy(good)
    bad["views"][0]["sections"][0]["cards"][1]["features"].append(
        {"type": "custom:chorus-group-volume"}
    )
    assert len(type_problems(bad)) == 1

    # A card nested where the walk does not look is still counted.
    bad = copy.deepcopy(good)
    bad["views"][0]["sections"][0]["cards"][1]["card"] = {"type": "iframe"}
    assert type_problems(bad) == [
        f"{sum(1 for n in nodes(bad) if 'type' in n)} mappings carry a type and "
        f"{sum(1 for n in nodes(good) if 'type' in n)} were checked"
    ]

    bad = copy.deepcopy(good)
    bad["views"][0]["sections"][0]["cards"][1]["entity"] = "media_player.upstairs"
    assert entity_problems(hass, bad) == [
        "media_player.upstairs is not an entity of the integration"
    ]

    # An entity of another integration, a disabled one, and a feature on the
    # wrong kind of entity.
    hass.states.async_set("media_player.other_brand", "idle")
    bad = copy.deepcopy(good)
    bad["views"][0]["sections"][0]["cards"][1]["entity"] = "media_player.other_brand"
    assert len(entity_problems(hass, bad)) == 1
    visualizer = er.async_get(hass).async_get_entity_id(
        "sensor", DOMAIN, f"{setup.unique_id}:room:living:visualizer"
    )
    assert visualizer is not None
    bad = copy.deepcopy(good)
    bad["views"][0]["sections"][0]["cards"].append(
        {"type": "tile", "entity": visualizer}
    )
    assert entity_problems(hass, bad) == [f"{visualizer} is disabled by default"]
    bad = copy.deepcopy(good)
    bad["views"][0]["sections"][0]["cards"][1]["entity"] = group_volume(hass, "study")
    assert len(entity_problems(hass, bad)) == 3

    bad = copy.deepcopy(good)
    bad["resources"] = [{"url": "/local/cards/chorus/card.js", "type": "module"}]
    assert link_problems(bad, "") == [
        "key 'resources'",
        "key 'url'",
        "string '/local/cards/chorus/card.js'",
    ]
    bad = copy.deepcopy(good)
    bad["views"][0]["sections"][0]["cards"][1]["tap_action"] = {
        "action": "url",
        "url_path": "https://chorus.example",
    }
    assert link_problems(bad, "# see https://chorus.example") == [
        "key 'tap_action'",
        "key 'url_path'",
        "string 'https://chorus.example'",
        "text contains '://'",
    ]
