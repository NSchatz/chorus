"""aiochorus: an async client for the chorus control plane (HTTP and server-sent events).

Vendored inside the Home Assistant integration; this directory is its one source
location. It imports nothing from Home Assistant and takes an injected
``aiohttp.ClientSession``. The contract it speaks is ``docs/control-plane.md``.
"""

from .client import ChorusClient, EventStream
from .commands import (
    announce,
    autoplay,
    encode_volume,
    firmware_install,
    group_volume,
    group_volume_step,
    join,
    mute,
    playback,
    quiet_hours_enabled,
    sound,
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
from .metrics import Metrics, SpeakerMetrics
from .models import (
    AutoplayRule,
    FirmwareImage,
    Group,
    InputLabel,
    NowPlaying,
    SavedGroup,
    ServerInfo,
    Sound,
    Speaker,
    SpeakerFirmware,
    State,
    Zone,
)
from .sse import SSEParser

__all__ = [
    "AutoplayRule",
    "ChorusClient",
    "ChorusCommandError",
    "ChorusConnectionError",
    "ChorusError",
    "ChorusProtocolError",
    "ChorusRefusedError",
    "ChorusUnsupportedError",
    "EventStream",
    "FirmwareImage",
    "Group",
    "InputLabel",
    "Metrics",
    "NowPlaying",
    "SSEParser",
    "SavedGroup",
    "ServerInfo",
    "Sound",
    "Speaker",
    "SpeakerFirmware",
    "SpeakerMetrics",
    "State",
    "Zone",
    "announce",
    "autoplay",
    "encode_volume",
    "firmware_install",
    "group_volume",
    "group_volume_step",
    "join",
    "mute",
    "playback",
    "quiet_hours_enabled",
    "sound",
    "take",
    "volume",
    "volume_from_level",
    "volume_step",
]
