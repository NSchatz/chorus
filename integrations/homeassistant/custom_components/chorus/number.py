"""A room's numbers: the group volume of its group, and its bass and treble."""

from __future__ import annotations

from homeassistant.components.number import NumberEntity, NumberMode
from homeassistant.const import PERCENTAGE, EntityCategory
from homeassistant.core import HomeAssistant, callback
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from ._aiochorus import Zone, commands
from .const import DOMAIN
from .coordinator import ChorusConfigEntry, ChorusCoordinator, room_identifier
from .entity import ChorusRoomEntity

# Commands go to the server one at a time per platform.
PARALLEL_UPDATES = 1


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ChorusConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    """Add a room's numbers, now and as rooms appear."""
    coordinator = entry.runtime_data
    rooms: set[str] = set()

    @callback
    def _add_new() -> None:
        zones = coordinator.data.zones
        new: list[NumberEntity] = []
        for zone in zones:
            if zone.id in rooms:
                continue
            new.append(ChorusGroupVolumeNumber(coordinator, zone))
            new.extend(ChorusToneNumber(coordinator, zone, key) for key in TONES)
        rooms.clear()
        rooms.update(zone.id for zone in zones)
        if new:
            async_add_entities(new)

    _add_new()
    entry.async_on_unload(coordinator.async_add_listener(_add_new))


# The two tone controls of a room's sound, in the catalog's spelling.
TONES = ("bass", "treble")


class ChorusGroupVolumeNumber(ChorusRoomEntity, NumberEntity):
    """Sonos-style group volume, available while the room is grouped."""

    _attr_translation_key = "group_volume"
    _attr_native_min_value = 0
    _attr_native_max_value = 100
    _attr_native_step = 1
    _attr_native_unit_of_measurement = PERCENTAGE
    _attr_mode = NumberMode.SLIDER

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone) -> None:
        """Name the entity for the room."""
        super().__init__(coordinator, zone)
        self._attr_unique_id = (
            f"{room_identifier(coordinator.server.id, zone.id)}:group_volume"
        )

    @property
    def available(self) -> bool:
        """Available only while the room plays with at least one other room."""
        group = self.formed_group
        return super().available and group is not None and len(group.zones) > 1

    @property
    def native_value(self) -> float | None:
        """Return the group volume in percent."""
        group = self.formed_group
        return None if group is None else round(group.volume * 100, 1)

    async def async_set_native_value(self, value: float) -> None:
        """Set the group volume of the group the room plays in."""
        group = self.formed_group
        if group is None:
            raise HomeAssistantError(
                translation_domain=DOMAIN,
                translation_key="room_gone",
                translation_placeholders={"room": self._zone_id},
            )
        await self.coordinator.async_command(
            commands.group_volume(group.id, commands.volume_from_level(value / 100))
        )


class ChorusToneNumber(ChorusRoomEntity, NumberEntity):
    """A room's bass or treble, in whole dB."""

    _attr_entity_category = EntityCategory.CONFIG
    _attr_native_min_value = -10
    _attr_native_max_value = 10
    _attr_native_step = 1
    _attr_native_unit_of_measurement = "dB"
    _attr_mode = NumberMode.SLIDER

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone, key: str) -> None:
        """Name the entity for the room and the tone control."""
        super().__init__(coordinator, zone)
        self._key = key
        self._attr_translation_key = key
        self._attr_unique_id = (
            f"{room_identifier(coordinator.server.id, zone.id)}:{key}"
        )

    @property
    def available(self) -> bool:
        """Available while the server says what the room's sound is."""
        zone = self.zone
        return super().available and zone is not None and zone.sound is not None

    @property
    def native_value(self) -> float | None:
        """Return the tone control in dB."""
        zone = self.zone
        if zone is None or zone.sound is None:
            return None
        return zone.sound.bass if self._key == "bass" else zone.sound.treble

    async def async_set_native_value(self, value: float) -> None:
        """Set the room's bass or treble and leave the rest of its sound alone."""
        whole = round(value)
        await self.coordinator.async_command(
            commands.sound(self._zone_id, bass=whole)
            if self._key == "bass"
            else commands.sound(self._zone_id, treble=whole)
        )
