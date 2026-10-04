"""A room's visualizer: the colour, the level and the beat of what it plays.

One sensor per room, for an automation to map to a light. It reads the
server's `GET /api/visualizer?zone=<room>` (`docs/visualizer.md`, "The HTTP
stream"), which sends at most ten frames a second, the latest only. The entity
starts disabled, and a disabled entity is never added, so the stream is opened
only for a room whose visualizer someone enabled.

The state is written at most once every `VISUALIZER_MIN_WRITE_INTERVAL`
whatever arrives, by the server's own rule: the latest frame supersedes the
ones before it, and a beat that was not shown rides in the next write. Nothing
here sends a command or drives a light: the mapping is the owner's automation.
"""

from __future__ import annotations

from asyncio import Task
from dataclasses import dataclass
import logging
from typing import Any

from homeassistant.components.sensor import SensorEntity
from homeassistant.const import MATCH_ALL, PERCENTAGE
from homeassistant.core import CALLBACK_TYPE, callback
from homeassistant.helpers.event import async_call_later

from ._aiochorus import ChorusError, VisualizerFrame, Zone
from .const import DOMAIN, VISUALIZER_IDLE_AFTER, VISUALIZER_MIN_WRITE_INTERVAL
from .coordinator import ChorusCoordinator, room_identifier
from .entity import ChorusRoomEntity

_LOGGER = logging.getLogger(__name__)


@dataclass(frozen=True, slots=True)
class Shown:
    """What the entity's state says: one frame, or the idle value."""

    level: int
    rgb_color: tuple[int, int, int]
    brightness: int
    beat: int
    transition: float
    lead_ms: int


# The idle value: nothing to show. A light mapped to it goes dark
# (`light.turn_on` with a brightness of 0 turns the light off).
IDLE = Shown(
    level=0, rgb_color=(0, 0, 0), brightness=0, beat=0, transition=0.0, lead_ms=0
)


def shown(frame: VisualizerFrame, beat: int) -> Shown:
    """Return a frame as the state says it, with the beat to show."""
    return Shown(
        # The level byte spans 60 dB (docs/visualizer.md); a percentage of it.
        level=round(frame.peak * 100 / 255),
        rgb_color=(frame.red, frame.green, frame.blue),
        brightness=frame.brightness,
        beat=beat,
        transition=frame.transition_ms / 1000,
        lead_ms=frame.lead_ms,
    )


class ChorusVisualizerSensor(ChorusRoomEntity, SensorEntity):
    """The visualizer stream of one room."""

    _attr_translation_key = "visualizer"
    _attr_entity_registry_enabled_default = False
    _attr_native_unit_of_measurement = PERCENTAGE
    _attr_suggested_display_precision = 0
    # No state class: no long-term statistics are ever compiled for it. And no
    # attribute is recorded: a colour a second apart is not history.
    _unrecorded_attributes = frozenset({MATCH_ALL})

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone) -> None:
        """Name the entity for the room; nothing is opened until it is added."""
        super().__init__(coordinator, zone)
        self._attr_unique_id = (
            f"{room_identifier(coordinator.server.id, zone.id)}:visualizer"
        )
        self._connected = False
        self._lost_logged = False
        # The frame to show at the next write (None: idle) and the strongest
        # news no write has shown yet: the last beat.
        self._latest: VisualizerFrame | None = None
        self._beat = 0
        # What the last write said, and when (the loop's monotonic clock).
        self._shown = IDLE
        self._written: tuple[bool, Shown] | None = None
        self._written_at: float | None = None
        self._cancel_write: CALLBACK_TYPE | None = None
        self._cancel_idle: CALLBACK_TYPE | None = None

    @property
    def available(self) -> bool:
        """Available while the server has the room and its stream is attached."""
        return super().available and self._connected

    @property
    def native_value(self) -> int:
        """Return the level, in percent of the level byte's span."""
        return self._shown.level

    @property
    def extra_state_attributes(self) -> dict[str, Any]:
        """Return what `light.turn_on` takes, the beat and the lead."""
        value = self._shown
        return {
            "rgb_color": value.rgb_color,
            "brightness": value.brightness,
            "beat": value.beat,
            "transition": value.transition,
            "lead_ms": value.lead_ms,
        }

    async def async_added_to_hass(self) -> None:
        """Open the room's stream: the entity is enabled."""
        await super().async_added_to_hass()
        # The platform writes the first state itself, right after this.
        self._written_at = self.hass.loop.time()
        self._written = (self.available, self._shown)
        entry = self.coordinator.config_entry
        task: Task[None] = entry.async_create_background_task(
            self.hass,
            self.coordinator.client.visualizer(self._zone_id).run(
                self._handle_frame, self._handle_disconnect, self._handle_connect
            ),
            f"{DOMAIN} visualizer {entry.entry_id} {self._zone_id}",
        )

        @callback
        def stop() -> None:
            """Close the stream and drop what was waiting to be written."""
            task.cancel()
            self._stop_timers()

        self.async_on_remove(stop)

    @callback
    def _stop_timers(self) -> None:
        for cancel in (self._cancel_write, self._cancel_idle):
            if cancel is not None:
                cancel()
        self._cancel_write = self._cancel_idle = None

    @callback
    def _handle_coordinator_update(self) -> None:
        """Take a change of the house: only what this entity says is written."""
        self._request_write()

    @callback
    def _handle_connect(self) -> None:
        if self._lost_logged:
            _LOGGER.info("The visualizer stream of %s is back", self._zone_id)
            self._lost_logged = False
        self._connected = True
        self._request_write()

    @callback
    def _handle_disconnect(self, err: ChorusError) -> None:
        if not self._lost_logged:
            _LOGGER.info(
                "The visualizer stream of %s is unavailable: %s", self._zone_id, err
            )
            self._lost_logged = True
        self._connected = False
        self._go_idle()

    @callback
    def _handle_frame(self, frame: VisualizerFrame) -> None:
        """Take one frame: it supersedes the one before, and a beat is kept."""
        if self._cancel_idle is not None:
            self._cancel_idle()
            self._cancel_idle = None
        if frame.silent:
            self._go_idle()
            return
        if frame.beat:
            self._beat = frame.beat
        self._latest = frame
        # A room that stops being sent frames without a silent one (its group
        # was given no source) is idle too.
        self._cancel_idle = async_call_later(
            self.hass, VISUALIZER_IDLE_AFTER, self._idle_timeout
        )
        self._request_write()

    @callback
    def _idle_timeout(self, _now: Any) -> None:
        self._cancel_idle = None
        self._go_idle()

    @callback
    def _go_idle(self) -> None:
        if self._cancel_idle is not None:
            self._cancel_idle()
            self._cancel_idle = None
        self._latest = None
        self._beat = 0
        self._request_write()

    @callback
    def _request_write(self) -> None:
        """Write now when the cap allows it, else when its interval runs out."""
        if self._cancel_write is not None:
            return
        wait = 0.0
        if self._written_at is not None:
            wait = (
                self._written_at + VISUALIZER_MIN_WRITE_INTERVAL - self.hass.loop.time()
            )
        if wait <= 0:
            self._write()
        else:
            self._cancel_write = async_call_later(self.hass, wait, self._write_later)

    @callback
    def _write_later(self, _now: Any) -> None:
        self._cancel_write = None
        self._write()

    @callback
    def _write(self) -> None:
        """Write the latest frame, once, with the beat no write has shown."""
        value = IDLE if self._latest is None else shown(self._latest, self._beat)
        state = (self.available, value)
        if state == self._written:
            return
        self._beat = 0
        self._shown = value
        self._written = state
        self._written_at = self.hass.loop.time()
        self.async_write_ha_state()
