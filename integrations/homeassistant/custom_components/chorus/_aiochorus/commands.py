"""The control commands this client sends, in the catalog's canonical encoding.

``docs/control-plane.md``, "The canonical encoding": no insignificant
whitespace, members in the order the catalog's tables give them, a volume with
exactly three fractional digits, whole numbers bare. A command is written at
the lowest catalog version that declares it, so ``volume`` and ``mute`` are
``"v":1`` and everything else here is ``"v":2``.
"""

from __future__ import annotations

import json
import math

_PLAYBACK_ACTIONS = frozenset({"pause", "resume", "next", "previous"})


def _string(value: str) -> str:
    # json.dumps with ensure_ascii off escapes the quote, the backslash and the
    # C0 controls (short forms where they exist, \u00xx otherwise), leaves "/"
    # alone and writes every other character as itself: the catalog's rule.
    return json.dumps(value, ensure_ascii=False)


def volume_from_level(level: float) -> int:
    """Turn a 0..1 level into thousandths, rounded half up and held to 0..1000."""
    if math.isnan(level):
        raise ValueError("a volume is a number from 0 to 1")
    return max(0, min(1000, math.floor(level * 1000 + 0.5)))


def encode_volume(thousandths: int) -> str:
    """Write a volume with exactly three fractional digits: 500 is ``0.500``."""
    if not 0 <= thousandths <= 1000:
        raise ValueError("a volume is 0.000 to 1.000")
    return f"{thousandths // 1000}.{thousandths % 1000:03d}"


def _step(step: int) -> int:
    if not -1000 <= step <= 1000:
        raise ValueError("a step is -1000 to 1000 thousandths")
    return step


def volume(zone: str, thousandths: int) -> bytes:
    """Set a room's volume (catalog v1)."""
    return (
        f'{{"v":1,"t":"volume","zone":{_string(zone)},'
        f'"volume":{encode_volume(thousandths)}}}'
    ).encode()


def mute(zone: str, muted: bool) -> bytes:
    """Mute or unmute a room (catalog v1)."""
    return (
        f'{{"v":1,"t":"mute","zone":{_string(zone)},'
        f'"muted":{"true" if muted else "false"}}}'
    ).encode()


def volume_step(zone: str, step: int) -> bytes:
    """Move a room's volume by signed thousandths."""
    return (
        f'{{"v":2,"t":"volume_step","zone":{_string(zone)},"step":{_step(step)}}}'
    ).encode()


def group_volume(group: str, thousandths: int) -> bytes:
    """Set a formed group's volume, scaling its rooms."""
    return (
        f'{{"v":2,"t":"group_volume","group":{_string(group)},'
        f'"volume":{encode_volume(thousandths)}}}'
    ).encode()


def group_volume_step(group: str, step: int) -> bytes:
    """Move a formed group's volume by signed thousandths."""
    return (
        f'{{"v":2,"t":"group_volume_step","group":{_string(group)},'
        f'"step":{_step(step)}}}'
    ).encode()


def join(zone: str, target: str) -> bytes:
    """Make a room play in the target's group."""
    return (
        f'{{"v":2,"t":"join","zone":{_string(zone)},"target":{_string(target)}}}'
    ).encode()


def take(target: str, source: str | None = None) -> bytes:
    """Take the room: the target's rooms move into the target's group."""
    if source is None:
        return f'{{"v":2,"t":"take","target":{_string(target)}}}'.encode()
    return (
        f'{{"v":2,"t":"take","target":{_string(target)},"source":{_string(source)}}}'
    ).encode()


def playback(target: str, action: str) -> bytes:
    """Pause, resume or skip the Spotify receiver the target's group plays."""
    if action not in _PLAYBACK_ACTIONS:
        raise ValueError(f"'{action}' is not a playback action")
    return (
        f'{{"v":2,"t":"playback","target":{_string(target)},'
        f'"action":{_string(action)}}}'
    ).encode()


def announce(target: str, url: str, thousandths: int | None = None) -> bytes:
    """Play an announcement fetched from ``url`` in a room or a saved group."""
    head = f'{{"v":2,"t":"announce","target":{_string(target)},"url":{_string(url)}'
    if thousandths is None:
        return (head + "}").encode()
    return (head + f',"volume":{encode_volume(thousandths)}}}').encode()
