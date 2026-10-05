"""The integration against the real chorus-server on loopback.

Skipped by name unless `CHORUS_SERVER_BIN` points at a built `chorus-server`:

    CHORUS_SERVER_BIN=/path/to/chorus-server make ha-test HA_TEST_ARGS="-k live_server"

The server is started with three rooms and no audio device (the tone source,
both sockets on a port of its own choosing on 127.0.0.1), the integration is
set up through its config flow, and join, unjoin, volume, group volume and
select_source go through Home Assistant service calls; every assertion is on
what Home Assistant's entities then show, which is what the real server said.

The announce part serves a generated WAV from a loopback port that stands in
for Home Assistant's own address, starts the server with that port as its one
`--announce-origin` (and `--players 1 --media-allow-loopback`, as
`crates/server/tests/announce.rs` does). The server mixes the clip over what the
room plays (ADR 0175), so the room's now-playing record does not change: the
test watches the room take the announcement's volume and get its own back, and
then reads the server's own state, which must say the announcement `finished`.
`CHORUS_LIVE_ANNOUNCE=0` leaves it out.
`CHORUS_SERVER_ARGS` appends arguments to the server's command line.
"""

from __future__ import annotations

from collections.abc import AsyncIterator, Iterator
import io
import math
import os
from pathlib import Path
import re
import shlex
import struct
import subprocess
import time
import wave

from aiohttp import ClientSession, web
from homeassistant.config_entries import SOURCE_USER
from homeassistant.const import CONF_HOST, CONF_PORT
from homeassistant.core import HomeAssistant
from homeassistant.core_config import async_process_ha_core_config
from homeassistant.data_entry_flow import FlowResultType
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers import entity_registry as er
import pytest

from custom_components.chorus.const import DOMAIN

from .conftest import wait_for

SERVER_BIN = os.environ.get("CHORUS_SERVER_BIN", "")
LIVE_ANNOUNCE = os.environ.get("CHORUS_LIVE_ANNOUNCE", "1") != "0"

pytestmark = pytest.mark.skipif(
    not SERVER_BIN,
    reason=(
        "CHORUS_SERVER_BIN is not set: the live test drives a built chorus-server "
        "(cargo build -p chorus-server, then CHORUS_SERVER_BIN=target/debug/chorus-server)"
    ),
)

ROOMS = ("kitchen", "den", "patio")
_LISTENING = re.compile(r"control listening on=\S*?:(\d+)")


def _wav(seconds: float = 1.5) -> bytes:
    """A short 440 Hz tone: 48 kHz, stereo, 16 bits."""
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as out:
        out.setnchannels(2)
        out.setsampwidth(2)
        out.setframerate(48000)
        for n in range(int(48000 * seconds)):
            sample = int(8000 * math.sin(2 * math.pi * 440 * n / 48000))
            out.writeframes(struct.pack("<hh", sample, sample))
    return buffer.getvalue()


@pytest.fixture
async def clip_origin(socket_enabled: None) -> AsyncIterator[str]:
    """A loopback origin serving one clip: what Home Assistant's URL would be."""
    clip = _wav()

    async def serve(request: web.Request) -> web.Response:
        return web.Response(body=clip, content_type="audio/wav")

    app = web.Application()
    app.router.add_get("/api/tts_proxy/clip.wav", serve)
    runner = web.AppRunner(app)
    await runner.setup()
    await web.TCPSite(runner, "127.0.0.1", 0).start()
    yield f"http://127.0.0.1:{runner.addresses[0][1]}"
    await runner.cleanup()


@pytest.fixture
def live_server(tmp_path: Path, clip_origin: str) -> Iterator[int]:
    """Start the real server; yield its control port; stop it."""
    log = tmp_path / "chorus-server.log"
    args = [
        SERVER_BIN,
        "--listen", "127.0.0.1:0",
        "--control-listen", "127.0.0.1:0",
        "--ephemeral-identity",
        "--allow-non-realtime",
        "--allow-unlocked-memory",
        "--source", "tone",
        "--serve-forever",
        "--slots", "4",
        "--players", "1",
        "--media-allow-loopback",
        "--announce-origin", clip_origin,
    ]  # fmt: skip
    for zone in ROOMS:
        args += ["--zone", zone]
    args += shlex.split(os.environ.get("CHORUS_SERVER_ARGS", ""))
    with log.open("wb") as out:
        process = subprocess.Popen(args, stdout=out, stderr=subprocess.STDOUT)  # noqa: S603
    try:
        deadline = time.monotonic() + 30
        port = 0
        while not port:
            match = _LISTENING.search(log.read_text(errors="replace"))
            if match:
                port = int(match.group(1))
            elif process.poll() is not None or time.monotonic() > deadline:
                pytest.fail(
                    "chorus-server did not say where its control plane listens:\n"
                    + log.read_text(errors="replace")[-4000:]
                )
            else:
                time.sleep(0.05)
        yield port
    finally:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def _entity(hass: HomeAssistant, platform: str, suffix: str) -> str:
    for entry in er.async_get(hass).entities.values():
        if (
            entry.platform == DOMAIN
            and entry.domain == platform
            and entry.unique_id.endswith(suffix)
        ):
            return entry.entity_id
    raise AssertionError(f"no {platform} entity with a unique id ending {suffix}")


async def test_live_server(
    hass: HomeAssistant, live_server: int, clip_origin: str
) -> None:
    result = await hass.config_entries.flow.async_init(
        DOMAIN,
        context={"source": SOURCE_USER},
        data={CONF_HOST: "127.0.0.1", CONF_PORT: live_server},
    )
    assert result["type"] is FlowResultType.CREATE_ENTRY, result
    entry = result["result"]
    await hass.async_block_till_done()
    await wait_for(lambda: len(hass.states.async_entity_ids("media_player")) == 3)
    kitchen, den, patio = (_entity(hass, "media_player", f":room:{r}") for r in ROOMS)

    def members(entity_id: str) -> list[str]:
        return list(hass.states.get(entity_id).attributes["group_members"])

    async def call(domain: str, service: str, entity_id: str, **data: object) -> None:
        await hass.services.async_call(
            domain, service, {"entity_id": entity_id, **data}, blocking=True
        )

    try:
        for entity_id in (kitchen, den, patio):
            assert members(entity_id) == [entity_id]

        # join: den and patio play in the kitchen's group.
        await call("media_player", "join", kitchen, group_members=[den, patio])
        await wait_for(lambda: set(members(kitchen)) == {kitchen, den, patio})
        assert members(den) == members(kitchen) == members(patio)

        # group volume, through the room's number: the server scales each room.
        number = _entity(hass, "number", ":room:kitchen:group_volume")
        await call("number", "set_value", number, value=40)
        await wait_for(lambda: hass.states.get(number).state == "40.0")
        for entity_id in (kitchen, den, patio):
            assert hass.states.get(entity_id).attributes["volume_level"] == 0.4

        # volume: one room, and the others keep theirs.
        await call("media_player", "volume_set", den, volume_level=0.25)
        await wait_for(lambda: hass.states.get(den).attributes["volume_level"] == 0.25)
        assert hass.states.get(patio).attributes["volume_level"] == 0.4

        # unjoin: den leaves for its own group; the other two stay together.
        await call("media_player", "unjoin", den)
        await wait_for(lambda: members(den) == [den])
        assert set(members(kitchen)) == {kitchen, patio}
        # ... and the last pair dissolves when one of them leaves.
        await call("media_player", "unjoin", patio)
        await wait_for(lambda: members(patio) == [patio])
        await wait_for(lambda: members(kitchen) == [kitchen])
        await wait_for(lambda: hass.states.get(number).state == "unavailable")

        # select_source: the room plays nothing, then the server's stream.
        await call("media_player", "turn_off", den)
        await wait_for(lambda: hass.states.get(den).state == "off")
        await call("media_player", "select_source", den, source="stream")
        await wait_for(
            lambda: hass.states.get(den).attributes.get("source") == "stream"
        )
        assert hass.states.get(den).state == "on"

        if LIVE_ANNOUNCE:
            # The clip is served from "Home Assistant's own address", which is
            # the server's one announce origin. It is mixed over what the room
            # plays (ADR 0175): the room takes the announcement's volume while
            # it plays and has its own back when it is over, and the server
            # says the clip played to its end.
            await async_process_ha_core_config(hass, {"internal_url": clip_origin})
            await call(
                "media_player", "play_media", kitchen,
                media_content_type="music",
                media_content_id=f"{clip_origin}/api/tts_proxy/clip.wav",
                announce=True,
                extra={"volume": 0.2},
            )  # fmt: skip
            await wait_for(
                lambda: hass.states.get(kitchen).attributes["volume_level"] == 0.2,
                timeout=20,
            )
            await wait_for(
                lambda: hass.states.get(kitchen).attributes["volume_level"] == 0.4,
                timeout=30,
            )
            async with (
                ClientSession() as session,
                session.get(f"http://127.0.0.1:{live_server}/api/state") as response,
            ):
                announced = (await response.json(content_type=None))["announcements"]
            assert [(a["rooms"], a["state"]) for a in announced] == [
                (["kitchen"], "finished")
            ], announced

            # An address that is Home Assistant's own but not on the server's
            # list passes the integration's check and is refused by the server.
            await async_process_ha_core_config(
                hass, {"internal_url": "http://127.0.0.1:9"}
            )
            with pytest.raises(HomeAssistantError) as caught:
                await call(
                    "media_player", "play_media", kitchen,
                    media_content_type="music",
                    media_content_id="http://127.0.0.1:9/api/tts_proxy/clip.wav",
                    announce=True,
                )  # fmt: skip
            assert caught.value.translation_key == "announce_origin_not_on_server"
    finally:
        assert await hass.config_entries.async_unload(entry.entry_id)
        await hass.async_block_till_done()
