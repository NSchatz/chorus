"""The sensors: a speaker's diagnostics, and a room's visualizer.

A room's visualizer (`visualizer.py`) is one sensor per room, disabled by
default, fed by the room's visualizer stream and written at a bounded rate.

A speaker's diagnostics: what it reports about itself, read from `GET /metrics`.

Every sensor here has the diagnostic entity category, and all but the two that
almost never change (the link and the firmware version) start disabled: a
speaker reports once a second, and a house of them would otherwise fill the
recorder with numbers nobody asked for. The scrape runs at a bounded rate and
only while at least one of these sensors is enabled (`ChorusMetricsCoordinator`).

The sync error is the speaker's OWN estimate of its playout error. It is not a
measured error between speakers and it is not timing evidence
(`docs/telemetry.md`).
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass

from homeassistant.components.sensor import (
    SensorDeviceClass,
    SensorEntity,
    SensorEntityDescription,
    SensorStateClass,
)
from homeassistant.const import (
    SIGNAL_STRENGTH_DECIBELS_MILLIWATT,
    EntityCategory,
    UnitOfTemperature,
    UnitOfTime,
)
from homeassistant.core import HomeAssistant, callback
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback
from homeassistant.helpers.typing import StateType
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from ._aiochorus import Speaker, SpeakerMetrics
from ._aiochorus.metrics import LINKS
from .coordinator import (
    ChorusConfigEntry,
    ChorusCoordinator,
    ChorusMetricsCoordinator,
    speaker_identifier,
)
from .visualizer import ChorusVisualizerSensor

# Read-only entities fed by one coordinator: nothing to serialise.
PARALLEL_UPDATES = 0


@dataclass(frozen=True, kw_only=True)
class ChorusSensorDescription(SensorEntityDescription):
    """One diagnostic of a speaker and how to read it from a scrape."""

    value_fn: Callable[[SpeakerMetrics], StateType]
    # The scrape keeps this series for a speaker whose session ended.
    kept_while_disconnected: bool = False


def _scaled(value: float | None, factor: float) -> float | None:
    """Return a value in the sensor's unit, to the exporter's own precision."""
    return None if value is None else round(value * factor, 3)


SENSORS: tuple[ChorusSensorDescription, ...] = (
    ChorusSensorDescription(
        key="sync_error",
        translation_key="sync_error",
        device_class=SensorDeviceClass.DURATION,
        native_unit_of_measurement=UnitOfTime.MICROSECONDS,
        state_class=SensorStateClass.MEASUREMENT,
        suggested_display_precision=0,
        entity_registry_enabled_default=False,
        value_fn=lambda m: _scaled(m.sync_error_seconds, 1e6),
    ),
    ChorusSensorDescription(
        key="buffer_fill",
        translation_key="buffer_fill",
        device_class=SensorDeviceClass.DURATION,
        native_unit_of_measurement=UnitOfTime.MILLISECONDS,
        state_class=SensorStateClass.MEASUREMENT,
        suggested_display_precision=0,
        entity_registry_enabled_default=False,
        value_fn=lambda m: _scaled(m.buffer_fill_seconds, 1e3),
    ),
    ChorusSensorDescription(
        key="rate_correction",
        translation_key="rate_correction",
        native_unit_of_measurement="ppm",
        state_class=SensorStateClass.MEASUREMENT,
        suggested_display_precision=1,
        entity_registry_enabled_default=False,
        value_fn=lambda m: _scaled(m.rate_correction_ratio, 1e6),
    ),
    ChorusSensorDescription(
        key="resyncs",
        translation_key="resyncs",
        # Since the speaker's session began: it starts again at 0 on a reconnect.
        state_class=SensorStateClass.TOTAL_INCREASING,
        entity_registry_enabled_default=False,
        value_fn=lambda m: m.resyncs,
    ),
    ChorusSensorDescription(
        key="link",
        translation_key="link",
        device_class=SensorDeviceClass.ENUM,
        options=list(LINKS),
        value_fn=lambda m: m.link,
    ),
    ChorusSensorDescription(
        key="rssi",
        device_class=SensorDeviceClass.SIGNAL_STRENGTH,
        native_unit_of_measurement=SIGNAL_STRENGTH_DECIBELS_MILLIWATT,
        state_class=SensorStateClass.MEASUREMENT,
        entity_registry_enabled_default=False,
        value_fn=lambda m: None if m.rssi_dbm is None else round(m.rssi_dbm),
    ),
    ChorusSensorDescription(
        key="temperature",
        device_class=SensorDeviceClass.TEMPERATURE,
        native_unit_of_measurement=UnitOfTemperature.CELSIUS,
        state_class=SensorStateClass.MEASUREMENT,
        entity_registry_enabled_default=False,
        value_fn=lambda m: m.temperature_celsius,
    ),
    ChorusSensorDescription(
        key="firmware_version",
        translation_key="firmware_version",
        kept_while_disconnected=True,
        value_fn=lambda m: m.firmware_version,
    ),
)


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ChorusConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    """Add each speaker's diagnostic sensors and each room's visualizer, as they appear."""
    coordinator = entry.runtime_data
    known: set[str] = set()

    @callback
    def _add_new() -> None:
        speakers = coordinator.data.speakers
        # A speaker that left `speakers[]` lost its device and its entities
        # with it; one adopted again gets them again.
        known.intersection_update(speaker.id for speaker in speakers)
        new = [speaker for speaker in speakers if speaker.id not in known]
        known.update(speaker.id for speaker in new)
        if new:
            async_add_entities(
                ChorusSpeakerSensor(coordinator, speaker, description)
                for speaker in new
                for description in SENSORS
            )

    rooms: set[str] = set()

    @callback
    def _add_rooms() -> None:
        zones = coordinator.data.zones
        new = [zone for zone in zones if zone.id not in rooms]
        rooms.clear()
        rooms.update(zone.id for zone in zones)
        if new:
            async_add_entities(
                ChorusVisualizerSensor(coordinator, zone) for zone in new
            )

    _add_new()
    _add_rooms()
    entry.async_on_unload(coordinator.async_add_listener(_add_new))
    entry.async_on_unload(coordinator.async_add_listener(_add_rooms))


class ChorusSpeakerSensor(CoordinatorEntity[ChorusMetricsCoordinator], SensorEntity):
    """One diagnostic of one adopted speaker."""

    _attr_has_entity_name = True
    _attr_entity_category = EntityCategory.DIAGNOSTIC
    entity_description: ChorusSensorDescription

    def __init__(
        self,
        coordinator: ChorusCoordinator,
        speaker: Speaker,
        description: ChorusSensorDescription,
    ) -> None:
        """Attach to the speaker's device; the values come from the scrapes."""
        super().__init__(coordinator.metrics)
        self.entity_description = description
        self._speaker_id = speaker.id
        self._attr_device_info = coordinator.speaker_device_info(
            coordinator.data, speaker
        )
        self._attr_unique_id = (
            f"{speaker_identifier(coordinator.server.id, speaker.id)}:{description.key}"
        )

    async def async_added_to_hass(self) -> None:
        """Start the scrapes with the first sensor that is enabled."""
        await super().async_added_to_hass()
        self.coordinator.async_start()

    @property
    def metrics(self) -> SpeakerMetrics | None:
        """Return what the last scrape said about the speaker."""
        data = self.coordinator.data
        return None if data is None else data.speaker(self._speaker_id)

    @property
    def available(self) -> bool:
        """Available while the last scrape worked and still has the series.

        A speaker whose session ended keeps its firmware version in the scrape
        and nothing else of these, so the others are unavailable until it is
        back. A value the speaker does not know (no radio, no sensor) is left
        out of the scrape of a connected speaker: that is unknown, not
        unavailable.
        """
        metrics = self.metrics
        return (
            super().available
            and metrics is not None
            and (metrics.connected or self.entity_description.kept_while_disconnected)
        )

    @property
    def native_value(self) -> StateType:
        """Return the value of the last scrape, or None when it has none."""
        metrics = self.metrics
        return None if metrics is None else self.entity_description.value_fn(metrics)
