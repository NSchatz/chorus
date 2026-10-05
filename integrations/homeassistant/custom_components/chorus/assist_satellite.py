"""A room that has a microphone, as a Home Assistant Assist satellite.

`docs/decisions/0000-the-home-assistant-voice-satellite.md`; proposal P8,
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
"""

from __future__ import annotations

import asyncio
from collections.abc import Coroutine
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
    _attr_supported_features = AssistSatelliteEntityFeature.ANNOUNCE

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
        self._removed = False

    async def async_added_to_hass(self) -> None:
        """Take the wake words heard in the room from now on."""
        await super().async_added_to_hass()
        self.async_on_remove(
            self.coordinator.async_add_wake_listener(self._zone_id, self._handle_wake)
        )

    async def async_will_remove_from_hass(self) -> None:
        """End what is running: the pipeline, the reply, the server's run."""
        self._removed = True
        for task in (self._wake_task, self._reply_task):
            if task is not None and not task.done():
                task.cancel()
        await self._async_stop_run()
        await super().async_will_remove_from_hass()

    # --- configuration ------------------------------------------------------

    @callback
    def async_get_configuration(self) -> AssistSatelliteConfiguration:
        """Return the wake words the server runs: every one, in every voice room."""
        words = self.coordinator.data.wake_words
        return AssistSatelliteConfiguration(
            available_wake_words=[
                AssistSatelliteWakeWord(
                    id=word.id, wake_word=word.phrase, trained_languages=[]
                )
                for word in words
            ],
            active_wake_words=[word.id for word in words],
            max_active_wake_words=0,
        )

    async def async_set_configuration(
        self, config: AssistSatelliteConfiguration
    ) -> None:
        """Refuse: the server runs every model it has in every voice room."""
        raise HomeAssistantError(
            translation_domain=DOMAIN, translation_key="wake_words_fixed"
        )

    # --- the wake word, the run and the pipeline -----------------------------

    @callback
    def _handle_wake(self, wake: VoiceWake) -> None:
        """Start a pipeline for a wake word heard in the room, if the room listens."""
        if self._wake_task is not None and not self._wake_task.done():
            _LOGGER.debug(
                "Ignored a wake word in %s: a voice run is in progress", self._zone_id
            )
            return
        zone = self.zone
        if zone is None or not self.available:
            return
        if not zone.voice_enabled or zone.mic_muted:
            # The gate is the server's and the speaker's; this is the same
            # answer the server would give, without asking it.
            _LOGGER.debug(
                "Ignored a wake word in %s: voice is %s and the microphone is %s",
                self._zone_id,
                "on" if zone.voice_enabled else "off",
                "muted" if zone.mic_muted else "live",
            )
            return
        self._wake_task = self.coordinator.config_entry.async_create_background_task(
            self.hass,
            self._async_run(wake.phrase),
            f"{DOMAIN} voice run {self._zone_id}",
        )

    async def _async_run(self, phrase: str) -> None:
        """Open a run, feed its audio to a pipeline, and end the run."""
        run = await self._async_open_run()
        if run is None:
            return
        self._run_open = True
        try:
            audio = await self._async_open_audio(run)
            if audio is None:
                return
            ir.async_delete_issue(self.hass, DOMAIN, self._voice_issue_id)
            self._reply_expected = False
            self._reply_started = False
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

    async def _async_open_run(self) -> VoiceRun | None:
        """Ask the server for a run in the room; None when it opens none."""
        try:
            return await self.coordinator.client.voice_start(self._zone_id)
        except ChorusCommandError as err:
            if err.name in _GATE_REFUSALS:
                _LOGGER.debug("No voice run in %s: %s", self._zone_id, err.detail)
            elif err.name == _NO_VOICE_INTEGRATION:
                self._async_create_voice_issue("no_voice_integration")
            else:
                _LOGGER.warning("No voice run in %s: %s", self._zone_id, err)
        except ChorusError as err:
            _LOGGER.warning("No voice run in %s: %s", self._zone_id, err)
        return None

    async def _async_open_audio(self, run: VoiceRun) -> VoiceAudio | None:
        """Become the run's one reader; None when the server serves it no audio."""
        try:
            return await self.coordinator.client.voice_audio(run)
        except ChorusVoiceRouteError as err:
            if err.reason == _NOT_THE_VOICE_INTEGRATION:
                self._async_create_voice_issue("not_the_voice_integration")
            else:
                _LOGGER.warning(
                    "The voice run in %s has no audio: %s", self._zone_id, err
                )
        except ChorusError as err:
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
        if event.type is PipelineEventType.TTS_START:
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
        """Play the pipeline's reply, then say the satellite is done speaking."""
        try:
            await self._async_play(url)
        except HomeAssistantError as err:
            _LOGGER.warning("The reply was not played in %s: %s", self._zone_id, err)
        finally:
            if not self._removed:
                self.tts_response_finished()

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
