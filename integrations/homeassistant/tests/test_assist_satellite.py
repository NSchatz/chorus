"""A voice room as an Assist satellite, with its switch and its sensor.

The fake chorus server emits the wake word, opens the run and serves its
microphone audio as `docs/control-plane.md` documents them; the Assist pipeline
is faked at the one function Home Assistant's satellite base class calls
(`async_pipeline_from_audio_stream`), so everything between the server's event
and the reply's `announce` is the integration's own code and Home Assistant's.

The house is the shared speakers vector with one change: the kitchen's speaker
declares the voice role, the kitchen has voice switched on and its microphone
live, and the server lists its wake-word model. The living room has no
microphone.
"""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator, Callable
import json
import logging
from typing import Any
from unittest.mock import patch

from homeassistant.components import stt
from homeassistant.components.assist_pipeline import (
    PipelineEvent,
    PipelineEventType,
    PipelineStage,
)
from homeassistant.components.assist_satellite import (
    AssistSatelliteConfiguration,
    AssistSatelliteEntity,
    AssistSatelliteEntityFeature,
)
from homeassistant.components.assist_satellite.const import DATA_COMPONENT
from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.core_config import async_process_ha_core_config
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers import (
    device_registry as dr,
    entity_registry as er,
    issue_registry as ir,
)
import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.chorus.assist_satellite import voice_issue_id
from custom_components.chorus.const import DOMAIN

from .conftest import SERVER_ID, room, wait_for
from .fake_server import FakeChorusServer, shared

HA_URL = "http://ha.example:8123"
REPLY = "/api/tts_proxy/reply.mp3"
SPEAKER = "chorus-0123456789ab"
WAKE = shared("voice_wake.json")
# Two stretches of "microphone audio": what the speaker sent between the wake
# word and the run, and what it sends once the run is open. Each is a whole
# number of 16-bit samples and a marker a test can find.
HEARD = b"\x01\x02" * 400
SPOKEN = b"\x03\x04" * 800


def voice_house(*, voice: bool = True, enabled: bool = True, muted: bool = False):
    """The shared speakers vector, with the kitchen made a voice room."""
    state = json.loads(shared("state-speakers.json"))
    for zone in state["zones"]:
        if zone["id"] == "kitchen":
            zone["voice_enabled"] = enabled
            zone["mic_muted"] = muted
    for speaker in state["speakers"]:
        if speaker["id"] == SPEAKER and voice:
            speaker["roles"] = [*speaker["roles"], "voice"]
    state["wake_words"] = [{"id": "okay_nabu", "phrase": "Okay Nabu"}]
    return json.dumps(state, separators=(",", ":")).encode()


@pytest.fixture
async def server(socket_enabled: None) -> AsyncIterator[FakeChorusServer]:
    """The fake chorus server, serving the house with a voice room."""
    fake = FakeChorusServer(voice_house())
    await fake.start()
    yield fake
    await fake.stop()


@pytest.fixture(autouse=True)
async def ha_url(hass: HomeAssistant) -> None:
    """Home Assistant's own address is the server's one announce origin."""
    await async_process_ha_core_config(hass, {"internal_url": HA_URL})


@pytest.fixture(autouse=True)
def debug_log(caplog: pytest.LogCaptureFixture) -> None:
    caplog.set_level(logging.DEBUG, logger="custom_components.chorus")


class FakePipeline:
    """Home Assistant's Assist pipeline, faked where the satellite calls it.

    It takes the audio the satellite hands it until it has `want` bytes, as
    speech-to-text would until the speaker stops talking, and then sends the
    events a pipeline sends: the transcript, the intent and, with `reply`, a
    spoken answer as a media URL of Home Assistant.
    """

    def __init__(
        self,
        want: int = len(HEARD) + len(SPOKEN),
        reply: str | None = REPLY,
        mode: str = "ok",
    ) -> None:
        self.want = want
        self.reply = reply
        self.mode = mode
        self.calls: list[dict[str, Any]] = []
        self.audio = b""
        self.hold: asyncio.Event | None = None

    async def __call__(self, hass: HomeAssistant, **kwargs: Any) -> None:
        self.calls.append(kwargs)
        emit: Callable[[PipelineEvent], None] = kwargs["event_callback"]
        emit(PipelineEvent(PipelineEventType.RUN_START))
        emit(PipelineEvent(PipelineEventType.STT_START))
        async for chunk in kwargs["stt_stream"]:
            self.audio += chunk
            if len(self.audio) >= self.want:
                break
        if self.hold is not None:
            await self.hold.wait()
        if self.mode == "raise":
            raise RuntimeError("the pipeline broke")
        if self.mode == "error":
            emit(
                PipelineEvent(
                    PipelineEventType.ERROR,
                    {"code": "stt-no-text-recognized", "message": "nothing heard"},
                )
            )
            emit(PipelineEvent(PipelineEventType.RUN_END))
            return
        emit(
            PipelineEvent(
                PipelineEventType.STT_END, {"stt_output": {"text": "turn on the light"}}
            )
        )
        emit(PipelineEvent(PipelineEventType.INTENT_START))
        emit(PipelineEvent(PipelineEventType.INTENT_END))
        if self.mode == "silent-reply":
            # It said it would speak and gave nothing to play.
            emit(PipelineEvent(PipelineEventType.TTS_START))
        elif self.reply is not None:
            emit(PipelineEvent(PipelineEventType.TTS_START))
            emit(
                PipelineEvent(
                    PipelineEventType.TTS_END,
                    {"tts_output": {"url": self.reply, "mime_type": "audio/mpeg"}},
                )
            )
        emit(PipelineEvent(PipelineEventType.RUN_END))


@pytest.fixture
def pipeline() -> AsyncIterator[FakePipeline]:
    fake = FakePipeline()
    with patch(
        "homeassistant.components.assist_satellite.entity.async_pipeline_from_audio_stream",
        new=fake,
    ):
        yield fake


def entity_id(hass: HomeAssistant, platform: str, zone: str, key: str) -> str | None:
    return er.async_get(hass).async_get_entity_id(
        platform, DOMAIN, f"{SERVER_ID}:room:{zone}:{key}"
    )


def satellite(hass: HomeAssistant) -> str:
    found = entity_id(hass, "assist_satellite", "kitchen", "assist_satellite")
    assert found is not None
    return found


def switch(hass: HomeAssistant) -> str:
    found = entity_id(hass, "switch", "kitchen", "voice_enabled")
    assert found is not None
    return found


def sensor(hass: HomeAssistant) -> str:
    found = entity_id(hass, "binary_sensor", "kitchen", "mic_muted")
    assert found is not None
    return found


def sent(server: FakeChorusServer, kind: str) -> list[dict[str, Any]]:
    """Every command of one type the server was sent, in order."""
    return [c for c in map(json.loads, server.bodies) if c["t"] == kind]


async def listening(server: FakeChorusServer) -> None:
    """Wait until the integration's stream of wake words is attached."""
    await wait_for(lambda: server.wake_subscribers == 1)


async def announce(hass: HomeAssistant, **data: Any) -> None:
    await hass.services.async_call(
        "assist_satellite",
        "announce",
        {"entity_id": satellite(hass), **data},
        blocking=True,
    )


async def settled(hass: HomeAssistant, turns: int = 20) -> None:
    """Let the loop and the fake server's sockets run for a moment.

    Not `async_block_till_done`: that would wait for an announcement a test
    is holding open.
    """
    for _ in range(turns):
        await asyncio.sleep(0.005)


# --- the finish line: wake, stream, reply ---------------------------------------


async def test_a_wake_word_runs_a_pipeline_on_the_room_audio_and_the_reply_is_announced(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    caplog: pytest.LogCaptureFixture,
) -> None:
    await listening(server)
    assert hass.states.get(satellite(hass)).state == "idle"

    # The server heard the wake word in the kitchen; the speaker went on
    # sending while Home Assistant answered.
    server.wake(WAKE, audio=HEARD)
    await wait_for(lambda: server.open_run("kitchen") is not None)
    assert server.bodies[0] == shared("voice_start.json")
    await wait_for(lambda: server.open_run("kitchen").claimed)
    server.mic("kitchen", SPOKEN[:333])  # an odd cut: a sample split in two
    server.mic("kitchen", SPOKEN[333:])

    # The reply's media URL reaches the server's `announce` for that room.
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    assert sent(server, "announce") == [
        {"v": 2, "t": "announce", "target": "kitchen", "url": HA_URL + REPLY}
    ]

    # The faked pipeline received the streamed audio, whole and in order, and
    # the wake word phrase, and started at speech-to-text.
    (call,) = pipeline.calls
    assert pipeline.audio == HEARD + SPOKEN
    assert call["wake_word_phrase"] == "Okay Nabu"
    assert call["start_stage"] is PipelineStage.STT
    assert call["end_stage"] is PipelineStage.TTS
    metadata = call["stt_metadata"]
    assert metadata.sample_rate is stt.AudioSampleRates.SAMPLERATE_16000
    assert metadata.bit_rate is stt.AudioBitRates.BITRATE_16
    assert metadata.channel is stt.AudioChannels.CHANNEL_MONO
    assert metadata.codec is stt.AudioCodecs.PCM
    # It ran as the room's satellite, on the room's device.
    registered = er.async_get(hass).async_get(satellite(hass))
    assert call["satellite_id"] == satellite(hass)
    assert call["device_id"] == registered.device_id
    device = dr.async_get(hass).async_get(registered.device_id)
    assert device.identifiers == {(DOMAIN, f"{SERVER_ID}:room:kitchen")}
    assert er.async_get(hass).async_get(room(hass, "kitchen")).device_id == device.id

    # The satellite is speaking until the server says the clip is over.
    await settled(hass)
    assert hass.states.get(satellite(hass)).state == "responding"
    server.end_announcement(1)
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "idle")

    # The run was stopped, and its microphone audio went to one reader.
    assert sent(server, "voice_stop") == [json.loads(shared("voice_stop.json"))]
    assert server.open_run("kitchen") is None
    assert server.requests.count("GET /api/voice-audio") == 1
    assert server.voice_refusals == []
    # The run's identifier is in the answer to `voice_start` and nowhere else.
    (run_id,) = server.run_ids
    assert run_id not in caplog.text
    assert not ir.async_get(hass).async_get_issue(DOMAIN, voice_issue_id(setup))


async def test_a_second_wake_word_during_a_run_starts_nothing(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    caplog: pytest.LogCaptureFixture,
) -> None:
    await listening(server)
    server.wake(WAKE, audio=HEARD)
    await wait_for(lambda: server.open_run("kitchen") is not None)
    server.wake(WAKE)
    await wait_for(lambda: "a voice run is in progress" in caplog.text)
    server.mic("kitchen", SPOKEN)
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    server.end_announcement(1)
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "idle")
    assert len(sent(server, "voice_start")) == 1
    assert len(pipeline.calls) == 1

    # Once it is over the next wake word starts a run of its own.
    server.wake(WAKE, audio=HEARD + SPOKEN)
    await wait_for(lambda: len(pipeline.calls) == 2)
    await wait_for(lambda: len(sent(server, "announce")) == 2)
    server.end_announcement(2)
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "idle")
    assert len(set(server.run_ids)) == 2


# --- muted or voice disabled: no pipeline, and announce still plays ---------------


@pytest.mark.parametrize("gate", ["voice-disabled", "mic-muted"])
async def test_while_muted_or_voice_disabled_no_pipeline_starts_and_announce_plays(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    caplog: pytest.LogCaptureFixture,
    gate: str,
) -> None:
    await listening(server)
    if gate == "voice-disabled":
        await hass.services.async_call(
            "switch", "turn_off", {"entity_id": switch(hass)}, blocking=True
        )
        await wait_for(lambda: hass.states.get(switch(hass)).state == "off")
    else:
        server.set_mic_muted("kitchen", True)
        await wait_for(lambda: hass.states.get(sensor(hass)).state == "on")
    server.commands.clear()

    server.wake(WAKE, audio=HEARD)
    await wait_for(lambda: "Ignored a wake word in kitchen" in caplog.text)
    await settled(hass)
    # Nothing was asked of the server and no pipeline started.
    assert server.bodies == []
    assert pipeline.calls == []
    assert server.requests.count("GET /api/voice-audio") == 0
    assert hass.states.get(satellite(hass)).state == "idle"

    # An announcement uses the speaker, not the microphone: it still plays.
    task = hass.async_create_task(
        announce(hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3", preannounce=False)
    )
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    assert sent(server, "announce")[0]["target"] == "kitchen"
    server.end_announcement(1)
    await task
    assert pipeline.calls == []


@pytest.mark.parametrize(
    "refusal", ["error-voice-start-disabled.json", "error-voice-start-muted.json"]
)
async def test_a_run_the_server_refuses_at_the_gate_starts_no_pipeline(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    caplog: pytest.LogCaptureFixture,
    refusal: str,
) -> None:
    """The gate closed between the last state and the wake word: the server says so."""
    await listening(server)
    server.script(400, shared(refusal))
    server.wake(WAKE)
    await wait_for(lambda: "No voice run in kitchen" in caplog.text)
    await settled(hass)
    assert server.bodies == [shared("voice_start.json")]
    assert pipeline.calls == []
    assert server.requests.count("GET /api/voice-audio") == 0
    assert not ir.async_get(hass).async_get_issue(DOMAIN, voice_issue_id(setup))


# --- async_announce returns only after playback ----------------------------------


async def test_announce_returns_only_after_the_server_reports_the_clip_finished(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    task = hass.async_create_task(
        announce(hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3", preannounce=False)
    )
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    assert server.bodies == [shared("announce.json")]

    # The server answered the command and the clip is playing: not done yet.
    await settled(hass, turns=40)
    assert not task.done()
    assert hass.states.get(satellite(hass)).state == "responding"
    # A change of the house that is not the clip's end does not end it either.
    await hass.services.async_call(
        "media_player",
        "volume_set",
        {"entity_id": room(hass, "kitchen"), "volume_level": 0.5},
        blocking=True,
    )
    await settled(hass)
    assert not task.done()

    server.end_announcement(1)
    await task
    assert hass.states.get(satellite(hass)).state == "idle"


async def test_announce_plays_the_chime_first_and_waits_for_each(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    task = hass.async_create_task(
        announce(hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3")
    )
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    chime = f"{HA_URL}/api/assist_satellite/static/preannounce.mp3"
    assert sent(server, "announce")[0]["url"] == chime
    await settled(hass)
    # The clip is not sent while the chime plays.
    assert len(sent(server, "announce")) == 1
    server.end_announcement(1)
    await wait_for(lambda: len(sent(server, "announce")) == 2)
    assert sent(server, "announce")[1]["url"] == f"{HA_URL}/api/tts_proxy/abc.mp3"
    await settled(hass)
    assert not task.done()
    server.end_announcement(2)
    await task


async def test_a_chime_that_fails_does_not_stop_the_announcement(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    task = hass.async_create_task(
        announce(hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3")
    )
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    server.end_announcement(1, "failed", "http status 404")
    await wait_for(lambda: len(sent(server, "announce")) == 2)
    server.end_announcement(2)
    await task


async def test_an_announcement_that_fails_is_a_translated_error(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    task = hass.async_create_task(
        announce(hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3", preannounce=False)
    )
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    server.end_announcement(1, "failed", "http status 404")
    with pytest.raises(HomeAssistantError) as caught:
        await task
    assert caught.value.translation_key == "announcement_failed"
    assert caught.value.translation_placeholders == {"reason": "http status 404"}
    assert hass.states.get(satellite(hass)).state == "idle"


async def test_an_announcement_that_is_displaced_is_over(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    task = hass.async_create_task(
        announce(hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3", preannounce=False)
    )
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    server.end_announcement(1, "displaced", "an alarm rang in kitchen")
    await task


async def test_an_announcement_over_before_its_answer_returns_at_once(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    server.announcements_end_at_once = "finished"
    await announce(hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3", preannounce=False)
    assert len(sent(server, "announce")) == 1


async def test_an_answer_without_a_number_is_not_waited_on(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    """A server that numbers no announcement gives nothing to wait for."""
    server.script(200, server.state_bytes)
    await announce(hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3", preannounce=False)
    assert len(sent(server, "announce")) == 1


async def test_an_announcement_the_server_no_longer_lists_is_over(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    """It dropped off the list of the last eight, or the server restarted."""
    task = hass.async_create_task(
        announce(hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3", preannounce=False)
    )
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    await settled(hass)
    assert not task.done()
    del server.model["announcements"]
    server.model["serial"] += 1
    server.set_model_state(server._encode())
    await task


async def test_an_announcement_never_reported_over_is_a_translated_error(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    with (
        patch("custom_components.chorus.coordinator.ANNOUNCEMENT_WAIT_SECONDS", 0.05),
        pytest.raises(HomeAssistantError) as caught,
    ):
        await announce(
            hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3", preannounce=False
        )
    assert caught.value.translation_key == "announcement_timeout"
    assert hass.states.get(satellite(hass)).state == "idle"


async def test_an_announcement_waits_through_a_lost_event_stream(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    task = hass.async_create_task(
        announce(hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3", preannounce=False)
    )
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    server.drop_streams()
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "unavailable")
    assert not task.done()
    server.end_announcement(1)
    await task


async def test_an_announcement_from_elsewhere_is_refused_before_it_is_sent(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    with pytest.raises(HomeAssistantError) as caught:
        await announce(
            hass, media_id="http://elsewhere.example/clip.mp3", preannounce=False
        )
    assert caught.value.translation_key == "announce_origin"
    assert server.bodies == []


# --- which rooms have one -----------------------------------------------------


async def test_a_room_without_a_microphone_gets_no_satellite_entity(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    # The kitchen has a speaker that declared the voice role.
    assert satellite(hass)
    assert switch(hass)
    assert sensor(hass)
    # The living room has no speaker at all: no satellite, switch or sensor.
    assert entity_id(hass, "assist_satellite", "living", "assist_satellite") is None
    assert entity_id(hass, "switch", "living", "voice_enabled") is None
    assert entity_id(hass, "binary_sensor", "living", "mic_muted") is None
    assert hass.states.async_entity_ids("assist_satellite") == [satellite(hass)]
    # It still has what every room has.
    assert room(hass, "living")


async def test_a_speaker_without_the_voice_role_makes_no_voice_room(
    hass: HomeAssistant, server: FakeChorusServer, entry: MockConfigEntry
) -> None:
    server.state_bytes = voice_house(voice=False)
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    assert hass.states.async_entity_ids("assist_satellite") == []
    assert entity_id(hass, "switch", "kitchen", "voice_enabled") is None
    assert entity_id(hass, "binary_sensor", "kitchen", "mic_muted") is None
    # With no voice room the stream of wake words is never opened.
    assert "GET /api/voice-events" not in server.requests

    # The speaker is replaced by one with a microphone: the room gets them.
    server.set_state(voice_house())
    await wait_for(lambda: len(hass.states.async_entity_ids("assist_satellite")) == 1)
    assert switch(hass)
    assert sensor(hass)
    await listening(server)

    # And it loses them with its last microphone, in the registry as well.
    server.set_state(voice_house(voice=False))
    await wait_for(lambda: hass.states.async_entity_ids("assist_satellite") == [])
    assert entity_id(hass, "assist_satellite", "kitchen", "assist_satellite") is None
    assert entity_id(hass, "switch", "kitchen", "voice_enabled") is None
    assert entity_id(hass, "binary_sensor", "kitchen", "mic_muted") is None


async def test_the_satellite_and_its_two_entities(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    registry = er.async_get(hass)
    state = hass.states.get(satellite(hass))
    assert state.attributes["supported_features"] == (
        AssistSatelliteEntityFeature.ANNOUNCE
    )
    assert state.attributes["friendly_name"] == "kitchen Assist satellite"
    assert hass.states.get(switch(hass)).attributes["friendly_name"] == (
        "kitchen Voice enabled"
    )
    assert hass.states.get(sensor(hass)).attributes["friendly_name"] == (
        "kitchen Mic muted"
    )
    assert registry.async_get(switch(hass)).entity_category is EntityCategory.CONFIG
    assert registry.async_get(sensor(hass)).entity_category is EntityCategory.DIAGNOSTIC
    devices = {
        registry.async_get(entity).device_id
        for entity in (
            satellite(hass),
            switch(hass),
            sensor(hass),
            room(hass, "kitchen"),
        )
    }
    assert len(devices) == 1


# --- the run is stopped on pipeline end and on error ------------------------------


@pytest.mark.parametrize("mode", ["ok", "error", "raise", "silent-reply"])
async def test_the_run_is_stopped_on_pipeline_end_and_on_error(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    caplog: pytest.LogCaptureFixture,
    mode: str,
) -> None:
    pipeline.mode = mode
    pipeline.reply = None
    await listening(server)
    server.wake(WAKE, audio=HEARD + SPOKEN)
    await wait_for(lambda: len(sent(server, "voice_stop")) == 1)
    await wait_for(lambda: server.open_run("kitchen") is None)
    assert server.runs_ended[0][0] == "kitchen"
    assert sent(server, "voice_stop") == [json.loads(shared("voice_stop.json"))]
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "idle")
    await settled(hass)
    # Stopped once, and nothing was said.
    assert len(sent(server, "voice_stop")) == 1
    assert sent(server, "announce") == []
    assert ("The Assist pipeline failed in kitchen" in caplog.text) == (mode == "raise")

    # The satellite takes the next wake word.
    pipeline.mode = "ok"
    server.wake(WAKE, audio=HEARD + SPOKEN)
    await wait_for(lambda: len(pipeline.calls) == 2)
    await wait_for(lambda: len(sent(server, "voice_stop")) == 2)


async def test_a_run_the_server_ends_at_its_limit_ends_the_pipeline(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
) -> None:
    pipeline.want = 10**9  # speech-to-text never has enough
    pipeline.reply = None
    await listening(server)
    server.wake(WAKE, audio=HEARD)
    await wait_for(lambda: bool(pipeline.audio))
    server.end_run("kitchen", "limit")
    # The server closed the audio: the stream ends and so does the pipeline.
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "idle")
    await wait_for(lambda: len(sent(server, "voice_stop")) == 1)
    assert pipeline.audio == HEARD
    assert server.runs_ended == [("kitchen", "limit")]


async def test_a_reply_that_cannot_be_played_leaves_the_satellite_idle(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    caplog: pytest.LogCaptureFixture,
) -> None:
    await listening(server)
    server.wake(WAKE, audio=HEARD + SPOKEN)
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    server.end_announcement(1, "failed", "http status 404")
    await wait_for(lambda: "The reply was not played in kitchen" in caplog.text)
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "idle")


async def test_unloading_during_a_run_stops_it(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
) -> None:
    pipeline.want = 10**9
    await listening(server)
    server.wake(WAKE, audio=HEARD)
    await wait_for(lambda: bool(pipeline.audio))
    assert await hass.config_entries.async_unload(setup.entry_id)
    await hass.async_block_till_done()
    await wait_for(lambda: server.open_run("kitchen") is None)
    assert len(sent(server, "voice_stop")) == 1
    await wait_for(lambda: server.wake_subscribers == 0)


async def test_an_announcement_cancels_the_run_in_progress(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
) -> None:
    pipeline.want = 10**9
    await listening(server)
    server.wake(WAKE, audio=HEARD)
    await wait_for(lambda: bool(pipeline.audio))
    task = hass.async_create_task(
        announce(hass, media_id=f"{HA_URL}/api/tts_proxy/abc.mp3", preannounce=False)
    )
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    await wait_for(lambda: server.open_run("kitchen") is None)
    assert len(sent(server, "voice_stop")) == 1
    server.end_announcement(1)
    await task


# --- the server's side of the microphone route -----------------------------------


async def test_a_server_without_a_voice_integration_is_a_repair_issue(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
) -> None:
    issues = ir.async_get(hass)
    server.voice_integration = None
    await listening(server)
    server.wake(WAKE)
    await wait_for(
        lambda: issues.async_get_issue(DOMAIN, voice_issue_id(setup)) is not None
    )
    issue = issues.async_get_issue(DOMAIN, voice_issue_id(setup))
    assert issue.translation_key == "no_voice_integration"
    assert issue.severity is ir.IssueSeverity.WARNING
    assert pipeline.calls == []
    assert server.requests.count("GET /api/voice-audio") == 0

    # Started with this Home Assistant's address, it serves the run, and the
    # issue goes.
    server.voice_integration = "127.0.0.1"
    server.wake(WAKE, audio=HEARD + SPOKEN)
    await wait_for(lambda: len(pipeline.calls) == 1)
    assert issues.async_get_issue(DOMAIN, voice_issue_id(setup)) is None
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    server.end_announcement(1)
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "idle")


async def test_a_server_that_serves_another_address_is_a_repair_issue(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
) -> None:
    issues = ir.async_get(hass)
    server.voice_integration = "192.0.2.10"
    await listening(server)
    server.wake(WAKE)
    await wait_for(
        lambda: issues.async_get_issue(DOMAIN, voice_issue_id(setup)) is not None
    )
    issue = issues.async_get_issue(DOMAIN, voice_issue_id(setup))
    assert issue.translation_key == "not_the_voice_integration"
    assert server.voice_refusals == ["not-the-voice-integration"]
    assert pipeline.calls == []
    # The run it had opened is not left open until its limit.
    await wait_for(lambda: len(sent(server, "voice_stop")) == 1)
    assert server.open_run("kitchen") is None


@pytest.mark.parametrize(
    ("status", "body", "words"),
    [
        (400, shared("error-unknown-zone.json"), "No voice run in kitchen"),
        (500, b"no", "No voice run in kitchen"),
    ],
)
async def test_a_run_the_server_cannot_open_is_logged(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    caplog: pytest.LogCaptureFixture,
    status: int,
    body: bytes,
    words: str,
) -> None:
    await listening(server)
    server.script(status, body)
    server.wake(WAKE)
    await wait_for(
        lambda: any(
            words in r.getMessage() and r.levelno == logging.WARNING
            for r in caplog.records
        )
    )
    assert pipeline.calls == []


async def test_a_run_whose_audio_cannot_be_read_is_stopped(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    caplog: pytest.LogCaptureFixture,
) -> None:
    """Another reader took the run first: this one reads nothing and stops it."""
    await listening(server)
    original = server._voice_start

    def taken(command: dict[str, Any]) -> bytes:
        answer = original(command)
        server.open_run(command["zone"]).claimed = True
        return answer

    server._voice_start = taken  # type: ignore[method-assign]
    server.wake(WAKE)
    await wait_for(lambda: "has no audio" in caplog.text)
    assert server.voice_refusals == ["voice-run-taken"]
    assert pipeline.calls == []
    await wait_for(lambda: len(sent(server, "voice_stop")) == 1)


async def test_wake_words_lost_and_back_are_each_logged_once(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    caplog: pytest.LogCaptureFixture,
) -> None:
    await listening(server)
    server.voice_events_status = 503
    server.drop_wake_streams()
    await wait_for(
        lambda: server.requests.count("GET /api/voice-events") >= 3, timeout=20
    )
    server.voice_events_status = 200
    await wait_for(lambda: server.wake_subscribers == 1, timeout=20)
    await settled(hass)
    assert caplog.text.count("The wake words of the chorus server") == 2
    assert caplog.text.count("are unavailable") == 1
    assert caplog.text.count("are back") == 1
    # The satellite stays available: an announcement needs no wake word.
    assert hass.states.get(satellite(hass)).state == "idle"


async def test_a_wake_word_in_a_room_without_a_satellite_is_ignored(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    caplog: pytest.LogCaptureFixture,
) -> None:
    await listening(server)
    server.wake(WAKE.replace(b"kitchen", b"living"))
    await wait_for(lambda: "Ignored a wake word heard in living" in caplog.text)
    assert server.bodies == []
    assert pipeline.calls == []


# --- the switch and the sensor -----------------------------------------------------


async def test_the_voice_enabled_switch_sends_the_catalog_command(
    hass: HomeAssistant, server: FakeChorusServer, entry: MockConfigEntry
) -> None:
    server.state_bytes = voice_house(enabled=False)
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    assert hass.states.get(switch(hass)).state == "off"

    await hass.services.async_call(
        "switch", "turn_on", {"entity_id": switch(hass)}, blocking=True
    )
    assert server.bodies == [shared("voice_enabled.json")]
    await wait_for(lambda: hass.states.get(switch(hass)).state == "on")
    # Switching voice on opens no microphone: the sensor is the speaker's.
    assert hass.states.get(sensor(hass)).state == "off"

    await hass.services.async_call(
        "switch", "turn_off", {"entity_id": switch(hass)}, blocking=True
    )
    assert server.bodies[1] == shared("voice_enabled.json").replace(b"true", b"false")
    await wait_for(lambda: hass.states.get(switch(hass)).state == "off")


async def test_a_refused_voice_switch_is_a_translated_error(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    server.script(400, shared("error-unknown-zone.json"))
    with pytest.raises(HomeAssistantError) as caught:
        await hass.services.async_call(
            "switch", "turn_off", {"entity_id": switch(hass)}, blocking=True
        )
    assert caught.value.translation_key == "refused_zone"


async def test_the_mic_muted_sensor_follows_the_speaker_and_has_no_action(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    assert hass.states.get(sensor(hass)).state == "off"
    server.set_mic_muted("kitchen", True)
    await wait_for(lambda: hass.states.get(sensor(hass)).state == "on")
    server.set_mic_muted("kitchen", False)
    await wait_for(lambda: hass.states.get(sensor(hass)).state == "off")
    # Read-only: Home Assistant has no action that would set a binary sensor,
    # and the integration sent the server nothing.
    assert hass.services.async_services().get("binary_sensor", {}) == {}
    assert server.bodies == []


async def test_the_entities_are_unavailable_while_the_server_is(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    server.refuse_connections = True
    server.drop_streams()
    for entity in (satellite(hass), switch(hass), sensor(hass)):
        await wait_for(lambda e=entity: hass.states.get(e).state == "unavailable")


# --- the configuration Home Assistant asks a satellite for ---------------------


async def test_the_configuration_lists_the_servers_wake_words_and_is_fixed(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    entity: AssistSatelliteEntity = hass.data[DATA_COMPONENT].get_entity(
        satellite(hass)
    )
    config = entity.async_get_configuration()
    assert [(w.id, w.wake_word) for w in config.available_wake_words] == [
        ("okay_nabu", "Okay Nabu")
    ]
    assert config.active_wake_words == ["okay_nabu"]
    with pytest.raises(HomeAssistantError) as caught:
        await entity.async_set_configuration(
            AssistSatelliteConfiguration(
                available_wake_words=[], active_wake_words=[], max_active_wake_words=0
            )
        )
    assert caught.value.translation_key == "wake_words_fixed"
