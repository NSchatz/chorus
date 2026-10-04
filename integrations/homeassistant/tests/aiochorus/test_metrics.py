"""The reader of `GET /metrics`, against a sample in the server's own format."""

from __future__ import annotations

from collections.abc import AsyncIterator
import re

import aiohttp
import pytest

from custom_components.chorus._aiochorus import (
    ChorusClient,
    ChorusConnectionError,
    ChorusProtocolError,
    Metrics,
)

from ..fake_server import REPO, FakeChorusServer, metrics_sample

WIFI = "chorus-0123456789ab"
WIRED = "chorus-ba9876543210"


@pytest.fixture
async def session(socket_enabled: None) -> AsyncIterator[aiohttp.ClientSession]:
    async with aiohttp.ClientSession() as client_session:
        yield client_session


@pytest.fixture
def client(session: aiohttp.ClientSession, server: FakeChorusServer) -> ChorusClient:
    return ChorusClient(session, "127.0.0.1", server.port)


def test_metrics_sample_is_in_the_servers_own_format() -> None:
    """Every family of the sample is one the exporter writes, with its words.

    The sample is not a scrape of a running server (that needs the built
    server and a C endpoint: `crates/server/tests/metrics_scrape.rs`), so this
    holds it to the exporter's source: the same families, in the same order,
    each with the HELP text and the TYPE `crates/server/src/metrics.rs` gives it.
    """
    source = (REPO / "crates" / "server" / "src" / "metrics.rs").read_text()
    render = source.split("pub fn render(")[1].split("\n}\n")[0]
    families = re.findall(
        r'"(chorus_[a-z_]+)",\s*"(gauge|counter)",\s*"([^"]*)"', render
    )
    assert len(families) == 16
    expected = [
        line
        for name, kind, text in families
        for line in (f"# HELP {name} {text}", f"# TYPE {name} {kind}")
    ]
    sample = metrics_sample().decode()
    assert [line for line in sample.splitlines() if line.startswith("#")] == expected
    assert sample.endswith("\n")
    for line in sample.splitlines():
        if not line.startswith("#"):
            assert line.split("{")[0].split(" ")[0] in {f[0] for f in families}


def test_metrics_values_parsed_from_the_servers_text() -> None:
    metrics = Metrics.parse(metrics_sample())
    assert set(metrics.speakers) == {WIFI, WIRED}

    wifi = metrics.speaker(WIFI)
    assert wifi is not None
    assert wifi.connected
    assert wifi.sync_error_seconds == pytest.approx(18.5e-6)
    assert wifi.buffer_fill_seconds == pytest.approx(0.251333)
    assert wifi.rate_correction_ratio == pytest.approx(-3.1e-6)
    assert wifi.resyncs == 2
    assert wifi.underruns == 1
    assert wifi.link == "wifi"
    assert wifi.rssi_dbm == -58
    assert wifi.temperature_celsius == 41.5
    assert wifi.firmware_version == "1.0.0"
    assert wifi.telemetry_age_seconds == pytest.approx(0.412)

    # Unknown is omitted, never zero: a wired speaker has no signal strength,
    # and an endpoint with no sensor has no temperature.
    wired = metrics.speaker(WIRED)
    assert wired is not None
    assert wired.sync_error_seconds == pytest.approx(-42e-6)
    assert wired.buffer_fill_seconds == pytest.approx(0.12)
    assert wired.rate_correction_ratio == pytest.approx(1.25e-6)
    assert wired.resyncs == 0
    assert wired.link == "wired"
    assert wired.rssi_dbm is None
    assert wired.temperature_celsius is None
    assert wired.firmware_version == "chorus-client 0.1.0"

    assert metrics.speaker("chorus-000000000000") is None


def test_metrics_a_disconnected_speaker_keeps_three_series() -> None:
    metrics = Metrics.parse(metrics_sample(disconnected=WIRED))
    wired = metrics.speaker(WIRED)
    assert wired is not None
    assert not wired.connected
    assert wired.firmware_version == "chorus-client 0.1.0"
    assert wired.link is None
    assert wired.sync_error_seconds is None
    assert wired.resyncs is None
    assert metrics.speakers[WIFI].connected


def test_metrics_label_values_are_unescaped() -> None:
    # Backslash, double quote and line feed are the three escapes; a comma, a
    # brace and a space inside a value are the value's.
    text = (
        "chorus_speakers 1\n"
        'chorus_speaker_info{speaker="s",name="a \\"b\\", {c} \\\\ d\\ne",room=""} 1\n'
        'chorus_speaker_firmware_info{speaker="s",version="1.0 \\"rc\\", x} 2"} 1\n'
        'chorus_speaker_link_info{speaker="s",link="carrier-pigeon"} 1\n'
        'chorus_speaker_connected{speaker="s"} 1\n'
        'some_other_program_total{job="x"} 7\n'
    )
    speaker = Metrics.parse(text.encode()).speaker("s")
    assert speaker is not None
    assert speaker.firmware_version == '1.0 "rc", x} 2'
    # A link this client does not know is unknown, never passed through.
    assert speaker.link == "unknown"
    assert speaker.connected


@pytest.mark.parametrize(
    "body",
    [
        b"",
        b"<html><body>every worker is busy</body></html>\n",
        b"\xff\xfe not text",
        # No `chorus_speakers`: some other exporter answered.
        b'process_cpu_seconds_total 1.5\nup{job="x"} 1\n',
        b"chorus_speakers 1\nchorus_speaker_connected 1\n",
        b'chorus_speakers 1\nchorus_speaker_connected{speaker="s"}\n',
        b'chorus_speakers 1\nchorus_speaker_connected{speaker="s"}1\n',
        b'chorus_speakers 1\nchorus_speaker_rssi_dbm{speaker="s"} strong\n',
        b'chorus_speakers 1\nchorus_speaker_rssi_dbm{speaker="s"} NaN\n',
        b'chorus_speakers 1\nchorus_speaker_resyncs_total{speaker="s"} 1.5\n',
        b'chorus_speakers 1\nchorus_speaker_resyncs_total{speaker="s"} -1\n',
        b'chorus_speakers 1\nchorus_speaker_connected{speaker="s\n',
        b"chorus_speakers 1\nchorus_speaker_connected{speaker} 1\n",
        b'chorus_speakers 1\nchorus_speaker_info{speaker="s",name="a\\tb"} 1\n',
        b"chorus_speakers 1\nchorus_speaker_connected\n",
        b'chorus_speakers 1\n{speaker="s"} 1\n',
        # Cut off in the middle of a line, as a connection that dropped is.
        metrics_sample()[:-30],
    ],
)
def test_metrics_malformed_text_is_refused(body: bytes) -> None:
    with pytest.raises(ChorusProtocolError):
        Metrics.parse(body)


async def test_metrics_client_reads_the_route(
    client: ChorusClient, server: FakeChorusServer
) -> None:
    server.metrics_bytes = metrics_sample()
    metrics = await client.metrics()
    assert metrics.speakers[WIFI].link == "wifi"
    assert server.requests == ["GET /metrics"]

    server.metrics_status = 404
    with pytest.raises(ChorusProtocolError, match="GET /metrics answered 404"):
        await client.metrics()

    await server.stop()
    with pytest.raises(ChorusConnectionError):
        await client.metrics()
