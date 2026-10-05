"""The chorus server's state, pushed over one event stream per config entry.

Beside it, the button presses the server accepted, pushed over a second stream
and handed to the event entity of the speaker and button each came from.

Beside it, the wake words the server's models heard, pushed over a third stream
that is opened only once a room has a voice satellite, and handed to the
satellite of the room each was heard in.

Beside it, the speakers' telemetry, read from `GET /metrics` at a bounded rate
and only while a diagnostic sensor is enabled (`ChorusMetricsCoordinator`).
"""

from __future__ import annotations

import asyncio
from collections.abc import Callable, Mapping
import logging
import time

from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant, callback
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers import device_registry as dr, issue_registry as ir
from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from ._aiochorus import (
    Announcement,
    ChorusClient,
    ChorusCommandError,
    ChorusError,
    ChorusRefusedError,
    ControllerEvent,
    Metrics,
    ServerInfo,
    Speaker,
    State,
    VoiceWake,
    Zone,
)
from .const import (
    ANNOUNCEMENT_WAIT_SECONDS,
    DOMAIN,
    METRICS_MIN_GAP_SECONDS,
    METRICS_SCAN_INTERVAL,
)

_LOGGER = logging.getLogger(__name__)

type ChorusConfigEntry = ConfigEntry[ChorusCoordinator]

# The field an `error` names decides the message a person sees; the server's
# `detail` wording is passed through and never matched.
_FIELD_KEYS = {
    "url": "refused_url",
    "target": "refused_target",
    "source": "refused_source",
    "group": "refused_group",
    "zone": "refused_zone",
    "volume": "refused_volume",
    "input": "refused_input",
    "speaker": "refused_speaker",
    "image": "refused_image",
    # The members of a room's sound (`sound`).
    "bass": "refused_sound",
    "treble": "refused_sound",
    "loudness": "refused_sound",
    "night": "refused_sound",
    "speech": "refused_sound",
    # The command itself cannot be served now (no player, or none free).
    "t": "refused_unavailable",
}


def room_identifier(server_id: str, zone_id: str) -> str:
    """Return the unique id of a room's device and media player."""
    return f"{server_id}:room:{zone_id}"


def saved_group_identifier(server_id: str, group_id: str) -> str:
    """Return the unique id of a saved group's device and media player."""
    return f"{server_id}:group:{group_id}"


def speaker_identifier(server_id: str, speaker_id: str) -> str:
    """Return the unique id of an adopted speaker's device."""
    return f"{server_id}:speaker:{speaker_id}"


def unsupported_issue_id(entry: ChorusConfigEntry) -> str:
    """Return the id of the repair issue for a server without catalog 2."""
    return f"unsupported_catalog_{entry.entry_id}"


class ChorusCoordinator(DataUpdateCoordinator[State]):
    """Holds the latest state message and sends commands."""

    config_entry: ChorusConfigEntry

    def __init__(
        self,
        hass: HomeAssistant,
        entry: ChorusConfigEntry,
        client: ChorusClient,
        server: ServerInfo,
    ) -> None:
        """Take the client and what the server said about itself."""
        super().__init__(hass, _LOGGER, config_entry=entry, name=DOMAIN)
        self.client = client
        self.server = server
        self.server_device_id = ""
        self.metrics = ChorusMetricsCoordinator(hass, entry, client)
        self._stream_lost = False
        # Whether the stream of button presses is attached. While it is not, a
        # press is never delivered, so the event entities say unavailable.
        self.presses_connected = False
        self._presses_lost_logged = False
        # (speaker id, button) to the event entity that takes its presses.
        self._press_listeners: dict[
            tuple[str, str], Callable[[str, ControllerEvent], None]
        ] = {}
        # The stream of wake words: opened by the first voice satellite.
        self._wakes_started = False
        self._wakes_lost_logged = False
        # Room id to the satellite that takes its wake words.
        self._wake_listeners: dict[str, Callable[[VoiceWake], None]] = {}

    async def _async_update_data(self) -> State:
        try:
            return await self.client.state()
        except ChorusError as err:
            raise UpdateFailed(
                translation_domain=DOMAIN,
                translation_key="cannot_connect",
                translation_placeholders={"error": str(err)},
            ) from err

    async def async_listen(self) -> None:
        """Run the event stream until cancelled."""
        await self.client.events().run(self.handle_state, self.handle_disconnect)

    @callback
    def handle_state(self, state: State) -> None:
        """Take a state the server sent."""
        if not self.last_update_success:
            _LOGGER.info(
                "The chorus server at %s:%s is back",
                self.client.host,
                self.client.port,
            )
        elif not self._stream_lost and (
            state == self.data or state.serial < self.data.serial
        ):
            # Nothing new, or an answer that was overtaken on the way here.
            return
        self._stream_lost = False
        self.sync_devices(state)
        self.async_set_updated_data(state)

    @callback
    def handle_disconnect(self, err: ChorusError) -> None:
        """Mark every entity unavailable: the event stream was lost."""
        self._stream_lost = True
        if not self.last_update_success:
            return
        _LOGGER.info(
            "The chorus server at %s:%s is unavailable: %s",
            self.client.host,
            self.client.port,
            err,
        )
        self.last_exception = err
        self.last_update_success = False
        self.async_update_listeners()

    async def async_listen_presses(self) -> None:
        """Run the stream of button presses until cancelled."""
        await self.client.controller_events().run(
            self.handle_controller_event,
            self.handle_presses_disconnect,
            self.handle_presses_connect,
        )

    @callback
    def async_add_press_listener(
        self,
        speaker_id: str,
        button: str,
        listener: Callable[[str, ControllerEvent], None],
    ) -> Callable[[], None]:
        """Send the presses of one speaker's button to `listener`.

        It is called with how the button was pressed and the event. Returns
        the function that stops it.
        """
        key = (speaker_id, button)
        self._press_listeners[key] = listener

        @callback
        def remove() -> None:
            if self._press_listeners.get(key) is listener:
                del self._press_listeners[key]

        return remove

    @callback
    def handle_controller_event(self, event: ControllerEvent) -> None:
        """Hand one accepted press to the entity of its speaker and button.

        Exactly one entity fires, once. An event that is no button of a
        speaker this integration has an entity for (a wall remote, a speaker
        that is not adopted, a command no button sends, a disabled entity) is
        ignored with one log line.
        """
        pressed = event.button()
        if pressed is None:
            _LOGGER.debug(
                "Ignored a controller event from %s: no speaker button sends "
                "the command %s (value %s)",
                event.endpoint,
                event.command,
                event.value,
            )
            return
        button, press = pressed
        listener = self._press_listeners.get((event.endpoint, button))
        if listener is None:
            _LOGGER.debug(
                "Ignored a controller event (%s) from %s: it is not a speaker "
                "with an enabled %s button entity",
                event.command,
                event.endpoint,
                button,
            )
            return
        listener(press, event)

    @callback
    def handle_presses_connect(self) -> None:
        """Take note that the stream of button presses is attached."""
        if self._presses_lost_logged:
            _LOGGER.info(
                "The button presses of the chorus server at %s:%s are back",
                self.client.host,
                self.client.port,
            )
            self._presses_lost_logged = False
        self.presses_connected = True
        self.async_update_listeners()

    @callback
    def handle_presses_disconnect(self, err: ChorusError) -> None:
        """Mark the event entities unavailable: presses would be missed."""
        if not self._presses_lost_logged:
            _LOGGER.info(
                "The button presses of the chorus server at %s:%s are unavailable: %s",
                self.client.host,
                self.client.port,
                err,
            )
            self._presses_lost_logged = True
        if not self.presses_connected:
            return
        self.presses_connected = False
        self.async_update_listeners()

    @callback
    def async_add_wake_listener(
        self, zone_id: str, listener: Callable[[VoiceWake], None]
    ) -> Callable[[], None]:
        """Send the wake words heard in one room to `listener`.

        The first listener opens the stream of wake words, which then runs
        until the entry is unloaded: a server with no voice room is never
        asked for it. Returns the function that stops the listener.
        """
        self._wake_listeners[zone_id] = listener
        if not self._wakes_started:
            self._wakes_started = True
            self.config_entry.async_create_background_task(
                self.hass,
                self.client.voice_events().run(
                    self.handle_voice_wake,
                    self.handle_wakes_disconnect,
                    self.handle_wakes_connect,
                ),
                f"{DOMAIN} wake words {self.config_entry.entry_id}",
            )

        @callback
        def remove() -> None:
            if self._wake_listeners.get(zone_id) is listener:
                del self._wake_listeners[zone_id]

        return remove

    @callback
    def handle_voice_wake(self, wake: VoiceWake) -> None:
        """Hand one wake word to the satellite of the room it was heard in."""
        listener = self._wake_listeners.get(wake.zone)
        if listener is None:
            _LOGGER.debug(
                "Ignored a wake word heard in %s: the room has no enabled "
                "voice satellite entity",
                wake.zone,
            )
            return
        listener(wake)

    @callback
    def handle_wakes_connect(self) -> None:
        """Take note that the stream of wake words is attached."""
        if self._wakes_lost_logged:
            _LOGGER.info(
                "The wake words of the chorus server at %s:%s are back",
                self.client.host,
                self.client.port,
            )
            self._wakes_lost_logged = False

    @callback
    def handle_wakes_disconnect(self, err: ChorusError) -> None:
        """Say once that wake words would be missed until the stream is back."""
        if self._wakes_lost_logged:
            return
        _LOGGER.info(
            "The wake words of the chorus server at %s:%s are unavailable: %s",
            self.client.host,
            self.client.port,
            err,
        )
        self._wakes_lost_logged = True

    def room_device_info(self, zone: Zone) -> DeviceInfo:
        """Return the device of a room."""
        return DeviceInfo(
            identifiers={(DOMAIN, room_identifier(self.server.id, zone.id))},
            manufacturer="chorus",
            model="Room",
            name=zone.name,
            # A suggestion only: areas are Home Assistant's, never created here.
            suggested_area=zone.name,
            via_device_id=self.server_device_id,
        )

    def speaker_device_info(self, state: State, speaker: Speaker) -> DeviceInfo:
        """Return the device of an adopted speaker.

        It hangs under its room's device while it has a room, and under the
        server otherwise. The room's device must exist first (`sync_devices`).
        """
        zone = None if speaker.room is None else state.zone(speaker.room)
        via = self.server_device_id
        if zone is not None:
            room = dr.async_get(self.hass).async_get_device_by_identifier(
                (DOMAIN, room_identifier(self.server.id, zone.id)),
                config_entry_id=self.config_entry.entry_id,
            )
            if room is not None:
                via = room.id
        running = speaker.firmware
        info = DeviceInfo(
            identifiers={(DOMAIN, speaker_identifier(self.server.id, speaker.id))},
            manufacturer="chorus",
            model="Speaker",
            name=speaker.name,
            sw_version=(
                running.version if running is not None else speaker.software or None
            ),
            via_device_id=via,
        )
        if running is not None:
            info["model_id"] = running.board
        if zone is not None:
            info["suggested_area"] = zone.name
        return info

    @callback
    def sync_devices(self, state: State) -> None:
        """Make the devices the state names, rename them, remove the ones gone.

        A room's device and a speaker's device exist whether or not they have
        an entity: a speaker that takes no updates is still a device, and a
        speaker's device is linked to its room's.
        """
        server_id = self.server.id
        names = {(DOMAIN, server_id): None} | {
            (DOMAIN, room_identifier(server_id, zone.id)): zone.name
            for zone in state.zones
        }
        names |= {
            (DOMAIN, saved_group_identifier(server_id, group.id)): group.name
            for group in state.saved_groups
        }
        names |= {
            (DOMAIN, speaker_identifier(server_id, speaker.id)): speaker.name
            for speaker in state.speakers
        }
        registry = dr.async_get(self.hass)
        for device in dr.async_entries_for_config_entry(
            registry, self.config_entry.entry_id
        ):
            identifier = next((i for i in device.identifiers if i[0] == DOMAIN), None)
            if identifier is None or identifier not in names:
                registry.async_update_device(
                    device.id, remove_config_entry_id=self.config_entry.entry_id
                )
                continue
            name = names[identifier]
            if name is not None and device.name != name:
                registry.async_update_device(device.id, name=name)
        entry_id = self.config_entry.entry_id
        for speaker in state.speakers:
            zone = None if speaker.room is None else state.zone(speaker.room)
            if zone is not None:
                # The room's device first: the speaker's is linked to it.
                registry.async_get_or_create(
                    config_entry_id=entry_id, **self.room_device_info(zone)
                )
            registry.async_get_or_create(
                config_entry_id=entry_id, **self.speaker_device_info(state, speaker)
            )

    async def async_command(
        self, message: bytes, refusals: Mapping[str, str] | None = None
    ) -> State:
        """Send one command; a refusal becomes a translated error.

        `refusals` maps the names a command's refusals start their detail with
        (the catalog's contract for them) to messages; any other refusal is
        mapped by the field it names.
        """
        try:
            state = await self.client.command(message)
        except ChorusCommandError as err:
            key = (refusals or {}).get(err.name or "")
            raise HomeAssistantError(
                translation_domain=DOMAIN,
                translation_key=key or _FIELD_KEYS.get(err.field, "refused_command"),
                translation_placeholders={"field": err.field, "detail": err.detail},
            ) from err
        except ChorusRefusedError as err:
            self.async_create_unsupported_issue()
            raise HomeAssistantError(
                translation_domain=DOMAIN, translation_key="unsupported_catalog"
            ) from err
        except ChorusError as err:
            raise HomeAssistantError(
                translation_domain=DOMAIN,
                translation_key="cannot_connect",
                translation_placeholders={"error": str(err)},
            ) from err
        if self.last_update_success:
            self.handle_state(state)
        return state

    async def async_wait_for_announcement(self, answer: State) -> Announcement | None:
        """Wait until the announcement an `announce` answer started is over.

        The server numbers each announcement in its command's answer and lists
        it in the state as `playing` until its clip has ended, failed or been
        displaced (`docs/control-plane.md`, "How it ended"). Returns how it
        ended; None when the server gave it no number (nothing to wait on) or
        no longer lists it (it ended more than eight announcements ago, or the
        server restarted). Raises a translated error when the server has not
        said it is over within the bound.
        """
        number = answer.announcement
        if number is None:
            return None
        if (ended := _ended(answer, number)) is not None:
            return ended

        over: asyncio.Future[Announcement | None] = self.hass.loop.create_future()

        @callback
        def check() -> None:
            if over.done() or not self.last_update_success:
                # While the server is away nothing is known; its next state says.
                return
            state = self.data
            listed = state.announcement_numbered(number)
            if listed is None:
                # A state from before the answer does not list it yet.
                if state.serial >= answer.serial:
                    over.set_result(None)
            elif not listed.playing:
                over.set_result(listed)

        remove = self.async_add_listener(check)
        try:
            check()
            async with asyncio.timeout(ANNOUNCEMENT_WAIT_SECONDS):
                return await over
        except TimeoutError as err:
            raise HomeAssistantError(
                translation_domain=DOMAIN, translation_key="announcement_timeout"
            ) from err
        finally:
            remove()

    async def async_refresh_server(self) -> None:
        """Ask the server again who it is; keep what is known if it cannot say."""
        try:
            self.server = await self.client.server()
        except ChorusError:
            return

    @callback
    def async_create_unsupported_issue(self) -> None:
        """Tell the owner the server does not speak the catalog this needs."""
        async_create_unsupported_issue(self.hass, self.config_entry)


def _ended(state: State, number: int) -> Announcement | None:
    """Return an announcement the state already lists as over."""
    listed = state.announcement_numbered(number)
    return None if listed is None or listed.playing else listed


class ChorusMetricsCoordinator(DataUpdateCoordinator[Metrics | None]):
    """Holds the latest scrape of the speakers' telemetry.

    A coordinator polls only while it has a listener, and an entity that is
    disabled is never added, so with no diagnostic sensor enabled the server is
    never asked. The state's coordinator does not depend on this one: a scrape
    that fails makes the diagnostic sensors unavailable and nothing else.
    """

    config_entry: ChorusConfigEntry

    def __init__(
        self, hass: HomeAssistant, entry: ChorusConfigEntry, client: ChorusClient
    ) -> None:
        """Take the client; nothing is read until a sensor asks."""
        super().__init__(
            hass,
            _LOGGER,
            config_entry=entry,
            name=f"{DOMAIN} speaker diagnostics",
            update_interval=METRICS_SCAN_INTERVAL,
        )
        self.client = client
        self.data = None
        self._started = False
        self._scraped_at: float | None = None
        self._failure: UpdateFailed | None = None

    @callback
    def async_start(self) -> None:
        """Scrape once now, for the first sensor that is added.

        Without it an enabled sensor would have no value until the first
        scheduled scrape, a whole interval after the entry loaded.
        """
        if self._started:
            return
        self._started = True
        self.config_entry.async_create_background_task(
            self.hass,
            self.async_refresh(),
            f"{DOMAIN} first metrics scrape {self.config_entry.entry_id}",
        )

    async def _async_update_data(self) -> Metrics | None:
        # A monotonic clock: the gap is an interval, never a time of day.
        now = time.monotonic()
        if (
            self._scraped_at is not None
            and now - self._scraped_at < METRICS_MIN_GAP_SECONDS
        ):
            # Asked again too soon (an `update_entity` action, a second
            # sensor): the last scrape is the answer, good or bad.
            if self._failure is not None:
                raise self._failure
            return self.data
        self._scraped_at = now
        try:
            metrics = await self.client.metrics()
        except ChorusError as err:
            self._failure = UpdateFailed(
                translation_domain=DOMAIN,
                translation_key="metrics_unavailable",
                translation_placeholders={"error": str(err)},
            )
            raise self._failure from err
        self._failure = None
        return metrics


@callback
def async_create_unsupported_issue(
    hass: HomeAssistant, entry: ChorusConfigEntry
) -> None:
    """Raise the repair issue for a server without catalog version 2."""
    ir.async_create_issue(
        hass,
        DOMAIN,
        unsupported_issue_id(entry),
        is_fixable=False,
        severity=ir.IssueSeverity.ERROR,
        translation_key="unsupported_catalog",
        translation_placeholders={"title": entry.title},
    )
