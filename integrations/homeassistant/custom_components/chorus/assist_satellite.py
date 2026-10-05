"""A room that has a microphone, as a Home Assistant Assist satellite.

`docs/decisions/0176-the-home-assistant-voice-satellite.md`; proposal P8,
Option A. The wake word is heard on the chorus server, so a run here starts at
speech-to-text with the phrase the server's model heard:

1. the server says a wake word was heard in the room (`voice_wake`);
2. this entity opens a voice run there (`voice_start`) and is answered with
   the run's identifier, which only it holds;
3. it reads the run's microphone audio (`GET /api/voice-audio`) and hands it
   to Home Assistant's Assist pipeline;
4. the reply is a clip Home Assistant serves, which the server plays in the
   room through its announcement mixer (`announce`), and the entity says it is
   done speaking only when the server says the clip is over.

The microphone's audio is never kept here: each chunk goes from the server's
answer to the pipeline and is gone. While the room's voice is switched off or
its microphone is muted nothing is asked for and no pipeline starts; an
announcement still plays, since it uses the speaker and not the microphone.

Beyond that run (`docs/decisions/0178-a-rooms-choice-of-wake-words.md`):

- a reply that asks for more (the pipeline's `continue_conversation`) is
  followed, once it has been played, by a second run with no wake word;
- `start_conversation` and `ask_question` play their prompt and then open a
  run, or, where the room does not listen, play it and raise an error that
  says why;
- a timer of Home Assistant's that finishes on the room's device plays a
  sound in the room;
- the wake words the room listens for are chosen from the server's own.
"""

from __future__ import annotations

import asyncio
from collections.abc import Coroutine
from dataclasses import dataclass
import logging
from typing import Any

from homeassistant.components.assist_pipeline import (
    PipelineEvent,
    PipelineEventType,
    PipelineStage,
)
from homeassistant.components.assist_satellite import (
    AssistSatelliteAnnouncement,
    AssistSatelliteConfiguration,
    AssistSatelliteEntity,
    AssistSatelliteEntityFeature,
    AssistSatelliteWakeWord,
)
from homeassistant.components.assist_satellite.const import PREANNOUNCE_URL
from homeassistant.components.intent import (
    TimerEventType,
    TimerInfo,
    async_register_timer_handler,
)
from homeassistant.core import HomeAssistant, callback
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers import issue_registry as ir
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from ._aiochorus import (
    ChorusCommandError,
    ChorusError,
    ChorusVoiceRouteError,
    VoiceAudio,
    VoiceRun,
    VoiceWake,
    Zone,
    commands,
)
from ._aiochorus.models import ANNOUNCEMENT_FAILED
from .announce import async_announce
from .const import DOMAIN
from .coordinator import ChorusConfigEntry, ChorusCoordinator
from .entity import (
    ChorusRoomEntity,
    async_setup_voice_room_entities,
    voice_room_unique_id,
)

_LOGGER = logging.getLogger(__name__)

# An announcement holds its call until the clip has played, so the rooms are
# not made to wait for one another.
PARALLEL_UPDATES = 0

# The key of a voice room's satellite in its unique id.
SATELLITE_KEY = "assist_satellite"

# The refusals of `voice_start` that mean "nothing listens in this room now":
# the room's own gate, in software or at the speaker's switch.
_GATE_REFUSALS = frozenset({"voice-disabled", "mic-muted"})
# The server was started without the address it serves a run's audio to.
_NO_VOICE_INTEGRATION = "no-voice-integration"
# The server serves a run's audio to another address than this Home Assistant's.
_NOT_THE_VOICE_INTEGRATION = "not-the-voice-integration"

# What a finished timer sounds like in the room: the chime Home Assistant
# itself serves for announcements, this many times. It is the one clip every
# Home Assistant has at an address the server may fetch from; a sound of
# chorus's own would need a route of the integration's that asks for no
# credentials, and it has none.
TIMER_SOUND = PREANNOUNCE_URL
TIMER_SOUND_TIMES = 3


@dataclass(frozen=True, slots=True)
class _NotListening:
    """Why no run was opened in the room: the error's key and its reason."""

    key: str
    reason: str = ""


_VOICE_DISABLED = _NotListening("not_listening_voice_disabled")
_MIC_MUTED = _NotListening("not_listening_mic_muted")
_GATES = {"voice-disabled": _VOICE_DISABLED, "mic-muted": _MIC_MUTED}


def voice_issue_id(entry: ChorusConfigEntry) -> str:
    """Return the id of the repair issue for a server that serves no voice run here."""
    return f"voice_integration_{entry.entry_id}"


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ChorusConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    """Add a satellite per room that has a microphone, as they appear."""
    async_setup_voice_room_entities(
        entry,
        async_add_entities,
        domain="assist_satellite",
        key=SATELLITE_KEY,
        make=ChorusAssistSatellite,
    )


class ChorusAssistSatellite(ChorusRoomEntity, AssistSatelliteEntity):
    """The voice satellite of one room, on the room's device."""

    _attr_translation_key = "assist_satellite"
    _attr_supported_features = (
        AssistSatelliteEntityFeature.ANNOUNCE
        | AssistSatelliteEntityFeature.START_CONVERSATION
    )

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone) -> None:
        """Name the entity for the room."""
        super().__init__(coordinator, zone)
        self._attr_unique_id = voice_room_unique_id(
            coordinator.server.id, zone.id, SATELLITE_KEY
        )
        # From the wake word to the end of the pipeline it started.
        self._wake_task: asyncio.Task[None] | None = None
        # The reply being played in the room.
        self._reply_task: asyncio.Task[None] | None = None
        # Whether the server has a run open that this entity started and has
        # not yet asked it to end.
        self._run_open = False
        # Whether the pipeline said it will speak, and whether it then gave a
        # clip to play.
        self._reply_expected = False
        self._reply_started = False
        # Whether the pipeline's answer asks for more: once the reply has been
        # played the room listens again, with no wake word.
        self._continue = False
        # Whether a prompt of `start_conversation` or `ask_question` is being
        # played, after which this entity opens a run itself.
        self._prompting = False
        # Why the last run was not opened, for the caller that has to say so.
        self._not_listening: _NotListening | None = None
        # The sound of a finished timer being played in the room.
        self._timer_task: asyncio.Task[None] | None = None
        self._removed = False

    async def async_added_to_hass(self) -> None:
        """Take the room's wake words and its device's timers from now on."""
        await super().async_added_to_hass()
        self.async_on_remove(
            self.coordinator.async_add_wake_listener(self._zone_id, self._handle_wake)
        )
        # Registering the handler is what tells Home Assistant that a timer
        # may be set by voice on this device.
        if self.registry_entry is not None and self.registry_entry.device_id:
            self.async_on_remove(
                async_register_timer_handler(
                    self.hass, self.registry_entry.device_id, self._handle_timer
                )
            )

    async def async_will_remove_from_hass(self) -> None:
        """End what is running: the pipeline, the reply, the server's run."""
        self._removed = True
        for task in (self._wake_task, self._reply_task, self._timer_task):
            if task is not None and not task.done():
                task.cancel()
        await self._async_stop_run()
        await super().async_will_remove_from_hass()

    # --- configuration ------------------------------------------------------

    @callback
    def async_get_configuration(self) -> AssistSatelliteConfiguration:
        """Return the server's wake words and the ones this room listens for."""
        words = self.coordinator.data.wake_words
        zone = self.zone
        # A room that never chose listens for every one the server runs.
        chosen = zone.wake_words if zone is not None else None
        return AssistSatelliteConfiguration(
            available_wake_words=[
                AssistSatelliteWakeWord(
                    id=word.id, wake_word=word.phrase, trained_languages=[]
                )
                for word in words
            ],
            active_wake_words=[
                word.id for word in words if chosen is None or word.id in chosen
            ],
            # A room may listen for all of them at once. (Home Assistant
            # refuses a choice longer than this, so it is never 0 here while
            # there is a word to choose.)
            max_active_wake_words=len(words),
        )

    async def async_set_configuration(
        self, config: AssistSatelliteConfiguration
    ) -> None:
        """Have the room listen for the chosen wake words and no other."""
        await self.coordinator.async_command(
            commands.voice_wake_words(self._zone_id, config.active_wake_words),
            refusals={"unknown-wake-word": "unknown_wake_word"},
        )

    # --- the wake word, the run and the pipeline -----------------------------

    @callback
    def _handle_wake(self, wake: VoiceWake) -> None:
        """Start a pipeline for a wake word heard in the room, if the room listens."""
        if self._busy:
            _LOGGER.debug(
                "Ignored a wake word in %s: a voice run is in progress", self._zone_id
            )
            return
        zone = self.zone
        if zone is None or not self.available:
            return
        if self._gate() is not None:
            # The gate is the server's and the speaker's; this is the same
            # answer the server would give, without asking it.
            _LOGGER.debug(
                "Ignored a wake word in %s: voice is %s and the microphone is %s",
                self._zone_id,
                "on" if zone.voice_enabled else "off",
                "muted" if zone.mic_muted else "live",
            )
            return
        self._wake_task = self._async_background(
            self._async_run(wake.phrase), "voice run"
        )

    @property
    def _busy(self) -> bool:
        """Whether a run is in progress here, or a prompt that ends in one."""
        return self._prompting or (
            self._wake_task is not None and not self._wake_task.done()
        )

    def _gate(self) -> _NotListening | None:
        """Why the room does not listen, as the last state has it; None if it does."""
        zone = self.zone
        if zone is None or not zone.voice_enabled:
            return _VOICE_DISABLED
        if zone.mic_muted:
            return _MIC_MUTED
        return None

    async def _async_run(self, phrase: str | None) -> None:
        """Open a run, feed its audio to a pipeline, and end the run.

        `phrase` is the wake word that was heard, or None for a run nobody
        woke: the second half of a conversation the pipeline continues.
        """
        audio = await self._async_open()
        if audio is not None:
            await self._async_listen(audio, phrase)

    async def _async_open(self) -> VoiceAudio | None:
        """Open a run in the room and become its one reader.

        None when there is none to read, with the reason in `_not_listening`
        and the run, if the server opened one, ended.
        """
        self._not_listening = None
        run = await self._async_open_run()
        if run is None:
            return None
        self._run_open = True
        audio: VoiceAudio | None = None
        try:
            audio = await self._async_open_audio(run)
        finally:
            if audio is None:
                await self._async_stop_run()
        if audio is not None:
            ir.async_delete_issue(self.hass, DOMAIN, self._voice_issue_id)
        return audio

    async def _async_listen(
        self,
        audio: VoiceAudio,
        phrase: str | None,
        question: asyncio.Future[str | None] | None = None,
    ) -> None:
        """Feed an open run's audio to a pipeline, and end the run.

        `question` is what an `ask_question` waits on, when this run is the
        one that listens for its answer.
        """
        try:
            self._reply_expected = False
            self._reply_started = False
            self._continue = False
            try:
                await self.async_accept_pipeline_from_satellite(
                    audio,
                    start_stage=PipelineStage.STT,
                    wake_word_phrase=phrase,
                )
            except Exception:
                # Whatever a pipeline of Home Assistant's raises, the room's
                # microphone is closed (below) and the satellite waits for the
                # next wake word; the failure is the log's.
                _LOGGER.exception("The Assist pipeline failed in %s", self._zone_id)
                if not self._removed:
                    self.tts_response_finished()
            finally:
                audio.close()
                if audio.error is not None:
                    _LOGGER.debug("In %s: %s", self._zone_id, audio.error)
        finally:
            await self._async_stop_run()
            # A question that is still waiting when its pipeline is over has
            # no answer: say so, or the action that asked would wait for ever
            # (a pipeline that ended before its first event sends none).
            if question is not None and not question.done():
                question.set_result(None)

    async def _async_open_run(self) -> VoiceRun | None:
        """Ask the server for a run in the room; None when it opens none."""
        try:
            return await self.coordinator.client.voice_start(self._zone_id)
        except ChorusCommandError as err:
            self._not_listening = _GATES.get(
                err.name or "", _NotListening("not_listening", err.detail)
            )
            if err.name in _GATE_REFUSALS:
                _LOGGER.debug("No voice run in %s: %s", self._zone_id, err.detail)
            elif err.name == _NO_VOICE_INTEGRATION:
                self._async_create_voice_issue("no_voice_integration")
            else:
                _LOGGER.warning("No voice run in %s: %s", self._zone_id, err)
        except ChorusError as err:
            self._not_listening = _NotListening("not_listening", str(err))
            _LOGGER.warning("No voice run in %s: %s", self._zone_id, err)
        return None

    async def _async_open_audio(self, run: VoiceRun) -> VoiceAudio | None:
        """Become the run's one reader; None when the server serves it no audio."""
        try:
            return await self.coordinator.client.voice_audio(run)
        except ChorusVoiceRouteError as err:
            self._not_listening = _NotListening("not_listening", str(err))
            if err.reason == _NOT_THE_VOICE_INTEGRATION:
                self._async_create_voice_issue("not_the_voice_integration")
            else:
                _LOGGER.warning(
                    "The voice run in %s has no audio: %s", self._zone_id, err
                )
        except ChorusError as err:
            self._not_listening = _NotListening("not_listening", str(err))
            _LOGGER.warning("The voice run in %s has no audio: %s", self._zone_id, err)
        return None

    async def _async_stop_run(self) -> None:
        """Ask the server to end the room's run, once per run."""
        if not self._run_open:
            return
        self._run_open = False
        try:
            await self.coordinator.async_command(commands.voice_stop(self._zone_id))
        except HomeAssistantError as err:
            # The run ends at its limit whatever happens here.
            _LOGGER.debug("The voice run in %s was not stopped: %s", self._zone_id, err)

    @callback
    def on_pipeline_event(self, event: PipelineEvent) -> None:
        """Close the microphone when it is no longer needed; play the reply."""
        if event.type in (
            PipelineEventType.STT_END,
            PipelineEventType.ERROR,
            PipelineEventType.RUN_END,
        ):
            # Speech-to-text has what it needs, or nothing will read on: the
            # run ends now and not at its limit.
            self._async_background(self._async_stop_run(), "voice stop")
        if event.type is PipelineEventType.INTENT_END:
            # The answer asks for more: listen again once it has been said.
            output = (event.data or {}).get("intent_output") or {}
            self._continue = bool(output.get("continue_conversation"))
        elif event.type is PipelineEventType.TTS_START:
            self._reply_expected = True
        elif event.type is PipelineEventType.TTS_END:
            output = (event.data or {}).get("tts_output") or {}
            url = output.get("url")
            if isinstance(url, str) and url:
                self._reply_started = True
                self._reply_task = self._async_background(
                    self._async_play_reply(url), "voice reply"
                )
        elif event.type is PipelineEventType.RUN_END and (
            self._reply_expected and not self._reply_started
        ):
            # The pipeline said it would speak and gave nothing to play.
            self.tts_response_finished()

    def _async_background(
        self, target: Coroutine[Any, Any, None], what: str
    ) -> asyncio.Task[None]:
        return self.coordinator.config_entry.async_create_background_task(
            self.hass, target, f"{DOMAIN} {what} {self._zone_id}"
        )

    # --- what the room says --------------------------------------------------

    async def _async_play(self, media_id: str) -> None:
        """Play a clip Home Assistant serves in the room; return when it is over."""
        answer = await async_announce(
            self.hass, self.coordinator, self._zone_id, media_id
        )
        ended = await self.coordinator.async_wait_for_announcement(answer)
        if ended is not None and ended.state == ANNOUNCEMENT_FAILED:
            raise HomeAssistantError(
                translation_domain=DOMAIN,
                translation_key="announcement_failed",
                translation_placeholders={"reason": ended.reason or ""},
            )

    async def _async_play_reply(self, url: str) -> None:
        """Play the pipeline's reply, then say the satellite is done speaking.

        A reply that asked for more is followed by a run with no wake word,
        if it was heard: one that could not be played asked nobody anything.
        """
        played = False
        try:
            await self._async_play(url)
            played = True
        except HomeAssistantError as err:
            _LOGGER.warning("The reply was not played in %s: %s", self._zone_id, err)
        finally:
            if not self._removed:
                self.tts_response_finished()
        if played and self._continue and not self._removed:
            self._async_continue()

    @callback
    def _async_continue(self) -> None:
        """Listen again in the room, as after a wake word but without one."""
        self._continue = False
        if self._busy:
            # A wake word or a prompt came first; that run is the room's.
            return
        if not self.available or self._gate() is not None:
            _LOGGER.debug(
                "The conversation in %s is not continued: the room does not listen",
                self._zone_id,
            )
            return
        self._wake_task = self._async_background(self._async_run(None), "voice run")

    async def async_announce(self, announcement: AssistSatelliteAnnouncement) -> None:
        """Play an announcement in the room; return when the server says it is over.

        It does not depend on the microphone: it plays while the room's voice
        is switched off and while its microphone is muted.
        """
        if announcement.preannounce_media_id:
            try:
                await self._async_play(announcement.preannounce_media_id)
            except HomeAssistantError as err:
                # The chime is a courtesy; the announcement is the point.
                _LOGGER.debug(
                    "The chime before an announcement was not played in %s: %s",
                    self._zone_id,
                    err,
                )
        await self._async_play(announcement.media_id)

    # --- the server's side of the mic route -----------------------------------

    @property
    def _voice_issue_id(self) -> str:
        return voice_issue_id(self.coordinator.config_entry)

    @callback
    def _async_create_voice_issue(self, key: str) -> None:
        """Tell the owner why a wake word in this house starts nothing."""
        ir.async_create_issue(
            self.hass,
            DOMAIN,
            self._voice_issue_id,
            is_fixable=False,
            severity=ir.IssueSeverity.WARNING,
            translation_key=key,
            translation_placeholders={"title": self.coordinator.config_entry.title},
        )

    # --- a conversation the house starts, and a question it asks ---------------

    async def async_start_conversation(
        self, start_announcement: AssistSatelliteAnnouncement
    ) -> None:
        """Play a prompt in the room, then listen for what is said to it.

        Home Assistant calls this for `start_conversation` and for
        `ask_question`. It returns once the prompt has been played and the run
        is open; the pipeline then runs like one a wake word started, with no
        wake word. Where the room does not listen (its voice is switched off,
        its microphone is muted, the server opens no run) the prompt is still
        played, since it uses the speaker, and the call then ends in an error
        that says why nothing listened.
        """
        self._prompting = True
        try:
            await self.async_announce(start_announcement)
            # A run that was in progress was cancelled by Home Assistant
            # before the prompt; let it finish ending before the next opens.
            if self._wake_task is not None and not self._wake_task.done():
                self._wake_task.cancel()
                await asyncio.wait([self._wake_task])
            self._not_listening = self._gate()
            audio = None if self._not_listening else await self._async_open()
        finally:
            self._prompting = False
        if audio is None:
            why = self._not_listening or _NotListening("not_listening")
            zone = self.zone
            raise HomeAssistantError(
                translation_domain=DOMAIN,
                translation_key=why.key,
                translation_placeholders={
                    "room": zone.name if zone is not None else self._zone_id,
                    "reason": why.reason,
                },
            )
        self._wake_task = self._async_background(
            self._async_listen(audio, None, self._ask_question_future), "voice run"
        )

    # --- timers ------------------------------------------------------------------

    @callback
    def _handle_timer(self, event_type: TimerEventType, timer: TimerInfo) -> None:
        """Play a sound in the room when a timer set on its device finishes.

        A timer that starts, changes or is cancelled shows nothing here: the
        room has no display, and no command of the server lights a speaker.
        """
        if event_type is not TimerEventType.FINISHED:
            _LOGGER.debug("Timer %s in %s: %s", timer.id, self._zone_id, event_type)
            return
        if self._timer_task is not None and not self._timer_task.done():
            # Two timers that end together ring once.
            return
        self._timer_task = self._async_background(self._async_ring(), "timer sound")

    async def _async_ring(self) -> None:
        """Play the timer sound; it uses the speaker, so it plays while muted."""
        try:
            for _ in range(TIMER_SOUND_TIMES):
                await self._async_play(TIMER_SOUND)
        except HomeAssistantError as err:
            _LOGGER.warning(
                "The timer sound was not played in %s: %s", self._zone_id, err
            )
