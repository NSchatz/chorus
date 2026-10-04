"""aiochorus: an async client for the chorus control plane (HTTP and server-sent events).

Vendored inside the Home Assistant integration; this directory is its one source
location. It imports nothing from Home Assistant and takes an injected
``aiohttp.ClientSession``. The contract it speaks is ``docs/control-plane.md``.
"""

from .client import ChorusClient, EventStream
from .commands import (
    announce,
    encode_volume,
    group_volume,
    group_volume_step,
    join,
    mute,
    playback,
    take,
    volume,
    volume_from_level,
    volume_step,
)
from .errors import (
    ChorusCommandError,
    ChorusConnectionError,
    ChorusError,
    ChorusProtocolError,
    ChorusRefusedError,
    ChorusUnsupportedError,
)
from .models import (
    Group,
    InputLabel,
    NowPlaying,
    SavedGroup,
    ServerInfo,
    State,
    Zone,
)
from .sse import SSEParser

__all__ = [
    "ChorusClient",
    "ChorusCommandError",
    "ChorusConnectionError",
    "ChorusError",
    "ChorusProtocolError",
    "ChorusRefusedError",
    "ChorusUnsupportedError",
    "EventStream",
    "Group",
    "InputLabel",
    "NowPlaying",
    "SSEParser",
    "SavedGroup",
    "ServerInfo",
    "State",
    "Zone",
    "announce",
    "encode_volume",
    "group_volume",
    "group_volume_step",
    "join",
    "mute",
    "playback",
    "take",
    "volume",
    "volume_from_level",
    "volume_step",
]
