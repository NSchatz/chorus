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
from collections.abc import AsyncIterator, Callable, Iterator
import json
import logging
from types import SimpleNamespace
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
from homeassistant.components.intent import async_device_supports_timers
from homeassistant.components.intent.timers import TIMER_DATA, TimerManager
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

from custom_components.chorus.assist_satellite import (
    TIMER_SOUND_TIMES,
    voice_issue_id,
)
from custom_components.chorus.const import DOMAIN

from .conftest import SERVER_ID, room, wait_for
from .fake_server import FakeChorusServer, shared

HA_URL = "http://ha.example:8123"
REPLY = "/api/tts_proxy/reply.mp3"
PROMPT = "/api/tts_proxy/prompt.mp3"
CHIME = "/api/assist_satellite/static/preannounce.mp3"
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
    spoken answer as a media URL of Home Assistant. `continues` says, run by
    run, whether the answer asks for more (`continue_conversation`); a run
    that is asked to end at speech-to-text ends there, as a question's does.
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
        self.continues: list[bool] = []

    async def __call__(self, hass: HomeAssistant, **kwargs: Any) -> None:
        self.calls.append(kwargs)
        emit: Callable[[PipelineEvent], None] = kwargs["event_callback"]
        emit(PipelineEvent(PipelineEventType.RUN_START))
        emit(PipelineEvent(PipelineEventType.STT_START))
        got = 0
        async for chunk in kwargs["stt_stream"]:
            self.audio += chunk
            got += len(chunk)
            if got >= self.want:
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
        if kwargs["end_stage"] is PipelineStage.STT:
            emit(PipelineEvent(PipelineEventType.RUN_END))
            return
        more = self.continues.pop(0) if self.continues else False
        emit(PipelineEvent(PipelineEventType.INTENT_START))
        emit(
            PipelineEvent(
                PipelineEventType.INTENT_END,
                {
                    "intent_output": {
                        "conversation_id": kwargs["conversation_id"],
                        "continue_conversation": more,
                    }
                },
            )
        )
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
        | AssistSatelliteEntityFeature.START_CONVERSATION
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


def the_entity(hass: HomeAssistant) -> AssistSatelliteEntity:
    found = hass.data[DATA_COMPONENT].get_entity(satellite(hass))
    assert found is not None
    return found


def choice(*ids: str) -> AssistSatelliteConfiguration:
    """What Home Assistant hands a satellite when a person chose wake words."""
    return AssistSatelliteConfiguration(
        available_wake_words=[], active_wake_words=list(ids), max_active_wake_words=1
    )


async def test_the_configuration_lists_the_servers_wake_words_and_sends_the_choice(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    entity = the_entity(hass)
    config = entity.async_get_configuration()
    assert [(w.id, w.wake_word) for w in config.available_wake_words] == [
        ("okay_nabu", "Okay Nabu")
    ]
    # A room that never chose listens for every one the server runs.
    assert config.active_wake_words == ["okay_nabu"]
    assert config.max_active_wake_words == 1

    # The choice goes to the server, as the catalog spells it.
    await entity.async_set_configuration(choice("okay_nabu"))
    assert server.bodies == [shared("voice_wake_words.json")]
    assert entity.async_get_configuration().active_wake_words == ["okay_nabu"]

    # A choice of none is a choice: the room then answers to no wake word.
    await entity.async_set_configuration(choice())
    assert sent(server, "voice_wake_words")[1] == {
        "v": 2,
        "t": "voice_wake_words",
        "zone": "kitchen",
        "wake_words": [],
    }
    config = entity.async_get_configuration()
    assert config.active_wake_words == []
    assert [w.id for w in config.available_wake_words] == ["okay_nabu"]

    # A wake word the server does not run is refused, in words.
    with pytest.raises(HomeAssistantError) as caught:
        await entity.async_set_configuration(choice("hey_jarvis"))
    assert caught.value.translation_key == "unknown_wake_word"
    assert "hey_jarvis" in caught.value.translation_placeholders["detail"]
    assert entity.async_get_configuration().active_wake_words == []


async def test_the_wake_words_are_chosen_through_home_assistants_own_command(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    hass_ws_client: Any,
) -> None:
    """The websocket command the voice settings of Home Assistant use."""
    client = await hass_ws_client(hass)
    await client.send_json_auto_id(
        {"type": "assist_satellite/get_configuration", "entity_id": satellite(hass)}
    )
    answer = await client.receive_json()
    assert answer["success"]
    assert answer["result"]["active_wake_words"] == ["okay_nabu"]
    await client.send_json_auto_id(
        {
            "type": "assist_satellite/set_wake_words",
            "entity_id": satellite(hass),
            "wake_word_ids": ["okay_nabu"],
        }
    )
    assert (await client.receive_json())["success"]
    assert server.bodies == [shared("voice_wake_words.json")]


# --- continue conversation -------------------------------------------------------


async def test_a_reply_that_asks_for_more_starts_a_second_run_without_a_wake_word(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
) -> None:
    pipeline.continues = [True, False]
    await listening(server)
    server.wake(WAKE, audio=HEARD + SPOKEN)
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    # The room listens again only once the question has been heard.
    await settled(hass)
    assert len(sent(server, "voice_start")) == 1
    assert len(pipeline.calls) == 1
    server.end_announcement(1)

    # A second run, opened by the satellite: the server sent no wake word.
    await wait_for(lambda: len(sent(server, "voice_start")) == 2)
    await wait_for(lambda: server.open_run("kitchen").claimed)
    server.mic("kitchen", HEARD + SPOKEN)
    await wait_for(lambda: len(pipeline.calls) == 2)
    first, second = pipeline.calls
    assert first["wake_word_phrase"] == "Okay Nabu"
    assert second["wake_word_phrase"] is None
    assert second["start_stage"] is PipelineStage.STT
    # It is the same conversation.
    assert second["conversation_id"] == first["conversation_id"]

    # Its answer asks for nothing more: the room is idle after it.
    await wait_for(lambda: len(sent(server, "announce")) == 2)
    server.end_announcement(2)
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "idle")
    await settled(hass)
    assert len(sent(server, "voice_start")) == 2
    assert len(sent(server, "voice_stop")) == 2
    assert len(set(server.run_ids)) == 2


@pytest.mark.parametrize("why", ["muted", "not played", "no reply"])
async def test_a_conversation_is_not_continued_where_nobody_was_asked_or_listens(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    why: str,
) -> None:
    pipeline.continues = [True]
    if why == "no reply":
        pipeline.reply = None
    await listening(server)
    server.wake(WAKE, audio=HEARD + SPOKEN)
    if why == "no reply":
        await wait_for(lambda: len(sent(server, "voice_stop")) == 1)
    else:
        await wait_for(lambda: len(sent(server, "announce")) == 1)
        if why == "muted":
            server.set_mic_muted("kitchen", True)
            await wait_for(lambda: hass.states.get(sensor(hass)).state == "on")
            server.end_announcement(1)
        else:
            server.end_announcement(1, "failed", "http status 404")
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "idle")
    await settled(hass)
    assert len(sent(server, "voice_start")) == 1
    assert len(pipeline.calls) == 1


# --- start conversation and ask question ---------------------------------------------


@pytest.fixture
def an_agent_that_converses() -> Iterator[None]:
    """A pipeline whose conversation agent is not Home Assistant's built-in one.

    Home Assistant refuses `start_conversation` for the built-in agent, which
    cannot hold a conversation, before it calls the satellite at all.
    """
    with patch(
        "homeassistant.components.assist_satellite.entity.async_get_pipeline",
        return_value=SimpleNamespace(conversation_engine="conversation.an_agent"),
    ):
        yield


async def call(hass: HomeAssistant, action: str, **data: Any) -> Any:
    return await hass.services.async_call(
        "assist_satellite",
        action,
        {"entity_id": satellite(hass), **data},
        blocking=True,
        return_response=action == "ask_question",
    )


def start_conversation(hass: HomeAssistant, **data: Any) -> asyncio.Task[Any]:
    return hass.async_create_task(
        call(hass, "start_conversation", start_media_id=HA_URL + PROMPT, **data)
    )


def ask_question(hass: HomeAssistant, **data: Any) -> asyncio.Task[Any]:
    return hass.async_create_task(
        call(hass, "ask_question", question_media_id=HA_URL + PROMPT, **data)
    )


async def test_start_conversation_plays_its_prompt_then_opens_a_run(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    an_agent_that_converses: None,
) -> None:
    await listening(server)
    task = start_conversation(hass)
    # The chime, then the prompt, each waited for; nothing listens meanwhile.
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    assert sent(server, "announce")[0]["url"] == HA_URL + CHIME
    server.end_announcement(1)
    await wait_for(lambda: len(sent(server, "announce")) == 2)
    assert sent(server, "announce")[1] == {
        "v": 2,
        "t": "announce",
        "target": "kitchen",
        "url": HA_URL + PROMPT,
    }
    await settled(hass)
    assert sent(server, "voice_start") == []
    assert not task.done()
    assert hass.states.get(satellite(hass)).state == "responding"
    # A wake word said over the prompt starts no run of its own.
    server.wake(WAKE)
    await settled(hass)
    assert sent(server, "voice_start") == []

    # The prompt is over: the run opens and the action returns.
    server.end_announcement(2)
    await task
    assert sent(server, "voice_start") == [json.loads(shared("voice_start.json"))]
    await wait_for(lambda: server.open_run("kitchen").claimed)
    server.mic("kitchen", HEARD + SPOKEN)
    await wait_for(lambda: len(pipeline.calls) == 1)
    (heard,) = pipeline.calls
    assert heard["wake_word_phrase"] is None
    assert heard["start_stage"] is PipelineStage.STT
    assert heard["end_stage"] is PipelineStage.TTS
    await wait_for(lambda: pipeline.audio == HEARD + SPOKEN)

    # What is answered is played like any reply, and the run was stopped.
    await wait_for(lambda: len(sent(server, "announce")) == 3)
    assert sent(server, "announce")[2]["url"] == HA_URL + REPLY
    server.end_announcement(3)
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "idle")
    assert len(sent(server, "voice_stop")) == 1
    assert server.open_run("kitchen") is None


async def test_ask_question_plays_its_prompt_then_opens_a_run_and_returns_the_answer(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
) -> None:
    await listening(server)
    task = ask_question(hass, preannounce=False)
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    assert sent(server, "announce")[0]["url"] == HA_URL + PROMPT
    await settled(hass)
    assert sent(server, "voice_start") == []
    server.end_announcement(1)

    await wait_for(lambda: len(sent(server, "voice_start")) == 1)
    await wait_for(lambda: server.open_run("kitchen").claimed)
    server.mic("kitchen", HEARD + SPOKEN)
    assert await task == {"id": None, "sentence": "turn on the light", "slots": {}}
    # The pipeline was asked for the words alone: nothing is said back.
    (heard,) = pipeline.calls
    assert heard["wake_word_phrase"] is None
    assert heard["start_stage"] is PipelineStage.STT
    assert heard["end_stage"] is PipelineStage.STT
    await wait_for(lambda: len(sent(server, "voice_stop")) == 1)
    await wait_for(lambda: hass.states.get(satellite(hass)).state == "idle")
    assert len(sent(server, "announce")) == 1


@pytest.mark.parametrize("action", ["start_conversation", "ask_question"])
@pytest.mark.parametrize("gate", ["voice-disabled", "mic-muted"])
async def test_while_muted_or_voice_disabled_a_prompt_plays_and_nothing_listens(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    an_agent_that_converses: None,
    action: str,
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

    start = start_conversation if action == "start_conversation" else ask_question
    task = start(hass, preannounce=False)
    # The prompt plays: it uses the speaker, not the microphone.
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    assert sent(server, "announce")[0]["url"] == HA_URL + PROMPT
    await settled(hass)
    assert not task.done()
    server.end_announcement(1)

    # Then it ends without listening, and the caller is told why.
    with pytest.raises(HomeAssistantError) as caught:
        await task
    assert caught.value.translation_domain == DOMAIN
    assert caught.value.translation_key == (
        "not_listening_voice_disabled"
        if gate == "voice-disabled"
        else "not_listening_mic_muted"
    )
    assert caught.value.translation_placeholders["room"] == "kitchen"
    await settled(hass)
    assert [c["t"] for c in map(json.loads, server.bodies)] == ["announce"]
    assert pipeline.calls == []
    assert server.requests.count("GET /api/voice-audio") == 0
    assert hass.states.get(satellite(hass)).state == "idle"


@pytest.mark.parametrize(
    ("refusal", "key"),
    [
        ("error-voice-start-disabled.json", "not_listening_voice_disabled"),
        ("error-voice-start-muted.json", "not_listening_mic_muted"),
        ("error-unknown-zone.json", "not_listening"),
    ],
)
async def test_a_prompt_whose_run_the_server_refuses_ends_in_the_same_error(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
    refusal: str,
    key: str,
) -> None:
    """The gate closed after the last state: the server's refusal says so."""
    await listening(server)
    task = ask_question(hass, preannounce=False)
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    server.script(400, shared(refusal))
    server.end_announcement(1)
    with pytest.raises(HomeAssistantError) as caught:
        await task
    assert caught.value.translation_key == key
    assert pipeline.calls == []


async def test_a_prompt_whose_run_has_no_audio_ends_in_an_error_and_stops_the_run(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
) -> None:
    server.voice_integration = "192.0.2.10"
    await listening(server)
    task = ask_question(hass, preannounce=False)
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    server.end_announcement(1)
    with pytest.raises(HomeAssistantError) as caught:
        await task
    assert caught.value.translation_key == "not_listening"
    assert (
        "not-the-voice-integration" in caught.value.translation_placeholders["reason"]
    )
    assert pipeline.calls == []
    assert len(sent(server, "voice_stop")) == 1
    assert server.open_run("kitchen") is None


async def test_a_prompt_that_cannot_be_played_opens_no_run(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
) -> None:
    await listening(server)
    task = ask_question(hass, preannounce=False)
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    server.end_announcement(1, "failed", "http status 404")
    with pytest.raises(HomeAssistantError) as caught:
        await task
    assert caught.value.translation_key == "announcement_failed"
    await settled(hass)
    assert sent(server, "voice_start") == []
    # The next wake word is the room's again.
    server.wake(WAKE, audio=HEARD + SPOKEN)
    await wait_for(lambda: len(pipeline.calls) == 1)


async def test_a_question_whose_pipeline_breaks_has_no_answer_and_does_not_hang(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
) -> None:
    await listening(server)
    task = ask_question(hass, preannounce=False)
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    with patch.object(
        the_entity(hass),
        "async_accept_pipeline_from_satellite",
        side_effect=RuntimeError("Pipeline entity not found"),
    ):
        server.end_announcement(1)
        with pytest.raises(HomeAssistantError, match="No answer"):
            await task
    await wait_for(lambda: len(sent(server, "voice_stop")) == 1)
    assert hass.states.get(satellite(hass)).state == "idle"


async def test_a_prompt_ends_the_run_in_progress_and_then_opens_its_own(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    pipeline: FakePipeline,
) -> None:
    pipeline.want = 10**9
    await listening(server)
    server.wake(WAKE, audio=HEARD)
    await wait_for(lambda: bool(pipeline.audio))
    task = ask_question(hass, preannounce=False)
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    await wait_for(lambda: server.open_run("kitchen") is None)
    pipeline.want = len(SPOKEN)
    server.end_announcement(1)
    await wait_for(lambda: len(sent(server, "voice_start")) == 2)
    await wait_for(lambda: server.open_run("kitchen").claimed)
    server.mic("kitchen", SPOKEN)
    assert (await task)["sentence"] == "turn on the light"
    assert len(pipeline.calls) == 2


# --- timers --------------------------------------------------------------------------


def device_id(hass: HomeAssistant) -> str:
    found = er.async_get(hass).async_get(satellite(hass)).device_id
    assert found is not None
    return found


def timers(hass: HomeAssistant) -> TimerManager:
    return hass.data[TIMER_DATA]


async def test_a_timer_that_finishes_plays_a_sound_in_the_room(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    # The room's device takes timers: Home Assistant's own handler has it.
    assert async_device_supports_timers(hass, device_id(hass))
    # The living room has no satellite and takes none.
    living = er.async_get(hass).async_get(room(hass, "living")).device_id
    assert not async_device_supports_timers(hass, living)

    timers(hass).start_timer(device_id(hass), None, None, 1, "en")
    # Started, and running: nothing sounds.
    await settled(hass)
    assert server.bodies == []

    # It finishes: the sound is played in the room, through `announce`, each
    # time once the one before is over.
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    assert sent(server, "announce") == [
        {"v": 2, "t": "announce", "target": "kitchen", "url": HA_URL + CHIME}
    ]
    for number in range(1, TIMER_SOUND_TIMES):
        await settled(hass)
        assert len(sent(server, "announce")) == number
        server.end_announcement(number)
        await wait_for(lambda n=number: len(sent(server, "announce")) == n + 1)
    server.end_announcement(TIMER_SOUND_TIMES)
    await settled(hass)
    assert [c["url"] for c in sent(server, "announce")] == [
        HA_URL + CHIME
    ] * TIMER_SOUND_TIMES
    # A timer listens to nobody: no run was opened.
    assert sent(server, "voice_start") == []


async def test_a_timer_sounds_while_the_microphone_is_muted_and_two_ring_once(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    server.set_mic_muted("kitchen", True)
    await wait_for(lambda: hass.states.get(sensor(hass)).state == "on")
    timers(hass).start_timer(device_id(hass), None, None, 1, "en", name="tea")
    timers(hass).start_timer(device_id(hass), None, None, 1, "en", name="eggs")
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    await settled(hass, turns=60)
    assert len(sent(server, "announce")) == 1
    for number in range(1, TIMER_SOUND_TIMES + 1):
        await wait_for(lambda n=number: len(sent(server, "announce")) == n)
        server.end_announcement(number)
    await settled(hass)
    assert len(sent(server, "announce")) == TIMER_SOUND_TIMES


async def test_a_cancelled_timer_makes_no_sound_and_a_sound_that_fails_is_logged(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    caplog: pytest.LogCaptureFixture,
) -> None:
    timer = timers(hass).start_timer(device_id(hass), None, None, 30, "en")
    timers(hass).cancel_timer(timer)
    await settled(hass)
    assert server.bodies == []
    assert "Timer" in caplog.text

    timers(hass).start_timer(device_id(hass), None, None, 1, "en")
    await wait_for(lambda: len(sent(server, "announce")) == 1)
    server.end_announcement(1, "failed", "http status 404")
    await wait_for(lambda: "The timer sound was not played in kitchen" in caplog.text)
    await settled(hass)
    assert len(sent(server, "announce")) == 1


async def test_unloading_takes_the_timer_handler_with_it(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    device = device_id(hass)
    assert await hass.config_entries.async_unload(setup.entry_id)
    await hass.async_block_till_done()
    assert not async_device_supports_timers(hass, device)
