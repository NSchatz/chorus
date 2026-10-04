"""What ``GET /metrics`` says about each speaker (``docs/telemetry.md``).

The server answers a Prometheus scrape with text exposition format 0.0.4. This
reads the per-speaker series of that text and nothing else: it is not a general
Prometheus parser. The rules it relies on are the exporter's own ("The rules of
the text"): one sample a line, ``name{label="value",...} number``, a label value
escaping backslash, double quote and line feed, no timestamps, and a value the
speaker does not know left out, never sent as zero.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import math

from .errors import ChorusProtocolError

# The one unlabelled series every scrape of a chorus server has: a text without
# it is not the exporter's (a proxy's page, another program on the port).
_MARKER = "chorus_speakers"

LINKS = ("wired", "wifi", "unknown")

# The series whose value is a number of the speaker, by the field it fills.
_NUMBERS = {
    "chorus_speaker_sync_error_seconds": "sync_error_seconds",
    "chorus_speaker_buffer_fill_seconds": "buffer_fill_seconds",
    "chorus_speaker_rate_correction_ratio": "rate_correction_ratio",
    "chorus_speaker_rssi_dbm": "rssi_dbm",
    "chorus_speaker_temperature_celsius": "temperature_celsius",
    "chorus_speaker_telemetry_age_seconds": "telemetry_age_seconds",
}
# The counters: whole numbers.
_COUNTS = {
    "chorus_speaker_resyncs_total": "resyncs",
    "chorus_speaker_underruns_total": "underruns",
}


@dataclass(slots=True)
class SpeakerMetrics:
    """One speaker's part of a scrape. ``None`` is "the scrape does not say"."""

    id: str
    connected: bool = False
    sync_error_seconds: float | None = None
    buffer_fill_seconds: float | None = None
    rate_correction_ratio: float | None = None
    resyncs: int | None = None
    underruns: int | None = None
    link: str | None = None
    rssi_dbm: float | None = None
    temperature_celsius: float | None = None
    firmware_version: str | None = None
    telemetry_age_seconds: float | None = None


@dataclass(slots=True)
class Metrics:
    """One scrape: the adopted speakers it names, by id."""

    speakers: dict[str, SpeakerMetrics] = field(default_factory=dict)

    def speaker(self, speaker_id: str) -> SpeakerMetrics | None:
        """Return what the scrape says about one speaker."""
        return self.speakers.get(speaker_id)

    @classmethod
    def parse(cls, body: bytes) -> Metrics:
        """Read a scrape; anything that is not the exporter's text is refused."""
        try:
            text = body.decode("utf-8")
        except UnicodeDecodeError as err:
            raise ChorusProtocolError("the metrics text is not UTF-8") from err
        metrics = cls()
        marked = False
        for raw in text.split("\n"):
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            name, labels, value = _sample(line)
            if name == _MARKER:
                marked = True
                continue
            if not name.startswith("chorus_speaker_"):
                continue
            speaker_id = labels.get("speaker")
            if not speaker_id:
                raise ChorusProtocolError(
                    f"the metrics sample '{name}' names no speaker"
                )
            speaker = metrics.speakers.get(speaker_id)
            if speaker is None:
                speaker = metrics.speakers[speaker_id] = SpeakerMetrics(speaker_id)
            _keep(speaker, name, labels, value)
        if not marked:
            raise ChorusProtocolError(
                f"the metrics text has no '{_MARKER}' sample: it is not a chorus "
                "server's"
            )
        return metrics


def _keep(
    speaker: SpeakerMetrics, name: str, labels: dict[str, str], value: float
) -> None:
    """Keep one sample of a speaker; a series this client does not use is skipped."""
    if name == "chorus_speaker_connected":
        speaker.connected = value == 1
    elif name == "chorus_speaker_link_info":
        link = labels.get("link")
        speaker.link = link if link in LINKS else "unknown"
    elif name == "chorus_speaker_firmware_info":
        speaker.firmware_version = labels.get("version") or None
    elif name in _NUMBERS:
        setattr(speaker, _NUMBERS[name], value)
    elif name in _COUNTS:
        if value < 0 or value != int(value):
            raise ChorusProtocolError(f"the counter '{name}' is not a whole number")
        setattr(speaker, _COUNTS[name], int(value))


def _sample(line: str) -> tuple[str, dict[str, str], float]:
    """Split one sample line into its name, its labels and its value."""
    brace = line.find("{")
    space = line.find(" ")
    labels: dict[str, str] = {}
    if brace != -1 and (space == -1 or brace < space):
        name = line[:brace]
        labels, end = _labels(line, brace + 1)
        rest = line[end:]
    elif space != -1:
        name, rest = line[:space], line[space:]
    else:
        raise ChorusProtocolError("a metrics line is not a sample")
    if not name or not all(c.isalnum() or c in "_:" for c in name):
        raise ChorusProtocolError("a metrics line does not start with a name")
    if not rest.startswith(" "):
        raise ChorusProtocolError(f"the metrics sample '{name}' has no value")
    try:
        value = float(rest.strip())
    except ValueError as err:
        raise ChorusProtocolError(
            f"the metrics sample '{name}' has a value that is not a number"
        ) from err
    if not math.isfinite(value):
        raise ChorusProtocolError(f"the metrics sample '{name}' is not finite")
    return name, labels, value


def _labels(line: str, at: int) -> tuple[dict[str, str], int]:
    """Read ``label="value",...}`` from ``at``; return them and where they end."""
    labels: dict[str, str] = {}
    size = len(line)
    while True:
        if at < size and line[at] == "}":
            return labels, at + 1
        eq = line.find('="', at)
        if eq == -1:
            raise ChorusProtocolError("a metrics label has no value")
        label = line[at:eq]
        at = eq + 2
        value: list[str] = []
        while True:
            if at >= size:
                raise ChorusProtocolError("a metrics label value does not end")
            char = line[at]
            if char == "\\":
                escaped = line[at + 1 : at + 2]
                if escaped == "n":
                    value.append("\n")
                elif escaped in ("\\", '"'):
                    value.append(escaped)
                else:
                    raise ChorusProtocolError("a metrics label has an unknown escape")
                at += 2
                continue
            at += 1
            if char == '"':
                break
            value.append(char)
        labels[label] = "".join(value)
        if at < size and line[at] == ",":
            at += 1
