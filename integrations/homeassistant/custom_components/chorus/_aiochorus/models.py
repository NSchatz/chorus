"""What the server says: the state message and the server's identity.

A reader takes the members it knows by name and ignores the rest
(``docs/control-plane.md``, "The state message"), so a later catalog field
never breaks this parser.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import json
import re
from typing import Any

from .errors import ChorusProtocolError

_SERVER_ID = re.compile(r"^[a-z0-9-]{1,64}$")

SOURCE_STREAM = "stream"
SOURCE_NONE = "none"
LINE_IN_PREFIX = "line-in:"
SOLOIST_PREFIX = "soloist:"


def _obj(value: Any, what: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ChorusProtocolError(f"{what} is not a JSON object")
    return value


def _str(obj: dict[str, Any], key: str, what: str) -> str:
    value = obj.get(key)
    if not isinstance(value, str):
        raise ChorusProtocolError(f"{what} has no string '{key}'")
    return value


def _opt_str(obj: dict[str, Any], key: str) -> str | None:
    value = obj.get(key)
    return value if isinstance(value, str) else None


def _number(obj: dict[str, Any], key: str, what: str) -> float:
    value = obj.get(key)
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ChorusProtocolError(f"{what} has no number '{key}'")
    return float(value)


def _str_tuple(obj: dict[str, Any], key: str) -> tuple[str, ...]:
    value = obj.get(key)
    if not isinstance(value, list):
        return ()
    return tuple(item for item in value if isinstance(item, str))


def _objects(obj: dict[str, Any], key: str) -> list[dict[str, Any]]:
    value = obj.get(key)
    if not isinstance(value, list):
        return []
    return [item for item in value if isinstance(item, dict)]


def loads(text: str | bytes, what: str) -> dict[str, Any]:
    """Read one JSON object, or say what was not one."""
    try:
        value = json.loads(text)
    except (ValueError, UnicodeDecodeError) as err:
        raise ChorusProtocolError(f"{what} is not JSON") from err
    return _obj(value, what)


@dataclass(frozen=True, slots=True)
class ServerInfo:
    """``GET /api/server``: who the server is and what it speaks."""

    id: str
    software: str
    catalogs: tuple[int, ...]
    announce_origins: tuple[str, ...]

    @classmethod
    def parse(cls, text: str | bytes) -> ServerInfo:
        """Read the ``server`` message."""
        obj = loads(text, "the server message")
        if obj.get("t") != "server":
            raise ChorusProtocolError("the answer is not a server message")
        server_id = _str(obj, "id", "the server message")
        if not _SERVER_ID.fullmatch(server_id):
            raise ChorusProtocolError("the server's id is not an identifier")
        catalogs = obj.get("catalogs")
        if not isinstance(catalogs, list):
            raise ChorusProtocolError("the server message has no 'catalogs'")
        return cls(
            id=server_id,
            software=_str(obj, "software", "the server message"),
            catalogs=tuple(
                c for c in catalogs if isinstance(c, int) and not isinstance(c, bool)
            ),
            announce_origins=_str_tuple(obj, "announce_origins"),
        )


@dataclass(frozen=True, slots=True)
class NowPlaying:
    """A group's now-playing record."""

    title: str | None
    artist: str | None
    album: str | None
    art_url: str | None
    duration_ms: int | None
    state: str
    via: str

    @classmethod
    def from_obj(cls, obj: dict[str, Any]) -> NowPlaying:
        """Read a ``now_playing`` object."""
        duration = obj.get("duration_ms")
        return cls(
            title=_opt_str(obj, "title"),
            artist=_opt_str(obj, "artist"),
            album=_opt_str(obj, "album"),
            art_url=_opt_str(obj, "art_url"),
            duration_ms=(
                duration
                if isinstance(duration, int) and not isinstance(duration, bool)
                else None
            ),
            state=_opt_str(obj, "state") or "playing",
            via=_opt_str(obj, "via") or "",
        )


def _whole(obj: dict[str, Any], key: str, default: int) -> int:
    value = obj.get(key)
    if isinstance(value, bool) or not isinstance(value, int):
        return default
    return value


def _flag(obj: dict[str, Any], key: str, default: bool) -> bool:
    value = obj.get(key)
    return value if isinstance(value, bool) else default


@dataclass(frozen=True, slots=True)
class Sound:
    """A room's sound: tone in whole dB, loudness, night mode, speech enhancement."""

    bass: int
    treble: int
    loudness: bool
    night: bool
    speech: bool

    @classmethod
    def from_obj(cls, obj: dict[str, Any]) -> Sound:
        """Read a room's ``sound`` object; an absent member is its default."""
        return cls(
            bass=_whole(obj, "bass", 0),
            treble=_whole(obj, "treble", 0),
            loudness=_flag(obj, "loudness", True),
            night=_flag(obj, "night", False),
            speech=_flag(obj, "speech", False),
        )


@dataclass(frozen=True, slots=True)
class Zone:
    """A room."""

    id: str
    name: str
    group: str
    volume: float
    muted: bool
    endpoints: tuple[str, ...]
    present: tuple[str, ...]
    limit: float
    effective_limit: float
    transport: str | None
    # None when the server said nothing of the room's sound.
    sound: Sound | None = None
    # Whether an active quiet-hours window caps the room; on unless switched off.
    quiet_enabled: bool = True

    @classmethod
    def from_obj(cls, obj: dict[str, Any]) -> Zone:
        """Read one element of ``zones``."""
        what = "a zone"
        volume = _number(obj, "volume", what)
        sound = obj.get("sound")
        return cls(
            id=_str(obj, "id", what),
            name=_str(obj, "name", what),
            group=_str(obj, "group", what),
            volume=volume,
            muted=obj.get("muted") is True,
            endpoints=_str_tuple(obj, "endpoints"),
            present=_str_tuple(obj, "present"),
            limit=_number(obj, "limit", what) if "limit" in obj else 1.0,
            effective_limit=(
                _number(obj, "effective_limit", what)
                if "effective_limit" in obj
                else 1.0
            ),
            transport=_opt_str(obj, "transport"),
            sound=Sound.from_obj(sound) if isinstance(sound, dict) else None,
            quiet_enabled=_flag(obj, "quiet_enabled", True),
        )


@dataclass(frozen=True, slots=True)
class Group:
    """A formed group: the unit a stream is served to."""

    id: str
    kind: str
    zones: tuple[str, ...]
    volume: float
    source: str
    now_playing: NowPlaying | None

    @classmethod
    def from_obj(cls, obj: dict[str, Any]) -> Group:
        """Read one element of ``groups``."""
        what = "a group"
        record = obj.get("now_playing")
        return cls(
            id=_str(obj, "id", what),
            kind=_str(obj, "kind", what),
            zones=_str_tuple(obj, "zones"),
            volume=_number(obj, "volume", what),
            source=_str(obj, "source", what),
            now_playing=(
                NowPlaying.from_obj(record) if isinstance(record, dict) else None
            ),
        )


@dataclass(frozen=True, slots=True)
class SavedGroup:
    """A saved group's definition, listed whether or not it is active."""

    id: str
    name: str
    zones: tuple[str, ...]
    active: bool

    @classmethod
    def from_obj(cls, obj: dict[str, Any]) -> SavedGroup:
        """Read one element of ``saved_groups``."""
        what = "a saved group"
        return cls(
            id=_str(obj, "id", what),
            name=_str(obj, "name", what),
            zones=_str_tuple(obj, "zones"),
            active=obj.get("active") is True,
        )


@dataclass(frozen=True, slots=True)
class AutoplayRule:
    """An autoplay rule: an input that starts playing in a room or a saved group."""

    input: str
    target: str
    enabled: bool
    # Both are written only when false (docs/control-plane.md, "The TV path").
    stop_on_standby: bool = True
    low_latency: bool = True

    @classmethod
    def from_obj(cls, obj: dict[str, Any]) -> AutoplayRule:
        """Read one element of ``autoplay``."""
        what = "an autoplay rule"
        return cls(
            input=_str(obj, "input", what),
            target=_str(obj, "target", what),
            enabled=obj.get("enabled") is True,
            stop_on_standby=_flag(obj, "stop_on_standby", True),
            low_latency=_flag(obj, "low_latency", True),
        )


@dataclass(frozen=True, slots=True)
class InputLabel:
    """A person's name for an input and what is wired to it."""

    input: str
    name: str
    role: str

    @classmethod
    def from_obj(cls, obj: dict[str, Any]) -> InputLabel:
        """Read one element of ``input_labels``."""
        what = "an input label"
        return cls(
            input=_str(obj, "input", what),
            name=_str(obj, "name", what),
            role=_opt_str(obj, "role") or "line-in",
        )


@dataclass(frozen=True, slots=True)
class State:
    """The v2 state message: a complete snapshot."""

    serial: int
    zones: tuple[Zone, ...]
    groups: tuple[Group, ...]
    saved_groups: tuple[SavedGroup, ...]
    inputs: tuple[str, ...]
    input_labels: tuple[InputLabel, ...]
    counts: tuple[tuple[str, int], ...] = field(default=())
    autoplay: tuple[AutoplayRule, ...] = field(default=())

    @classmethod
    def parse(cls, text: str | bytes) -> State:
        """Read a state message."""
        obj = loads(text, "the state message")
        if obj.get("t") != "state":
            raise ChorusProtocolError("the message is not a state message")
        if obj.get("v") != 2:
            raise ChorusProtocolError("the state message is not catalog version 2")
        serial = obj.get("serial")
        if isinstance(serial, bool) or not isinstance(serial, int):
            raise ChorusProtocolError("the state message has no whole 'serial'")
        return cls(
            serial=serial,
            zones=tuple(Zone.from_obj(z) for z in _objects(obj, "zones")),
            groups=tuple(Group.from_obj(g) for g in _objects(obj, "groups")),
            saved_groups=tuple(
                SavedGroup.from_obj(g) for g in _objects(obj, "saved_groups")
            ),
            inputs=_str_tuple(obj, "inputs"),
            input_labels=tuple(
                InputLabel.from_obj(label) for label in _objects(obj, "input_labels")
            ),
            counts=tuple(
                (key, len(value))
                for key, value in sorted(obj.items())
                if isinstance(value, list)
            ),
            autoplay=tuple(
                AutoplayRule.from_obj(rule) for rule in _objects(obj, "autoplay")
            ),
        )

    def zone(self, zone_id: str) -> Zone | None:
        """Return the room with this id."""
        return next((z for z in self.zones if z.id == zone_id), None)

    def group(self, group_id: str) -> Group | None:
        """Return the formed group with this id."""
        return next((g for g in self.groups if g.id == group_id), None)

    def group_of(self, zone_id: str) -> Group | None:
        """Return the formed group a room plays in."""
        zone = self.zone(zone_id)
        return None if zone is None else self.group(zone.group)

    def saved_group(self, group_id: str) -> SavedGroup | None:
        """Return the saved definition with this id."""
        return next((g for g in self.saved_groups if g.id == group_id), None)

    def autoplay_rule(self, input_id: str, target: str) -> AutoplayRule | None:
        """Return the autoplay rule of an input, while it has this target."""
        return next(
            (
                rule
                for rule in self.autoplay
                if rule.input == input_id and rule.target == target
            ),
            None,
        )

    def input_name(self, input_id: str) -> str | None:
        """Return the label a person gave an input, if any."""
        return next(
            (label.name for label in self.input_labels if label.input == input_id),
            None,
        )
