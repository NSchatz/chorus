"""The group volume of the live or saved group a room plays in."""

from __future__ import annotations

from homeassistant.components.number import NumberEntity, NumberMode
from homeassistant.const import PERCENTAGE
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
    """Add a group-volume number per room, now and as rooms appear."""
    coordinator = entry.runtime_data
    rooms: set[str] = set()

    @callback
    def _add_new() -> None:
        zones = coordinator.data.zones
        new = [
            ChorusGroupVolumeNumber(coordinator, zone)
            for zone in zones
            if zone.id not in rooms
        ]
        rooms.clear()
        rooms.update(zone.id for zone in zones)
        if new:
            async_add_entities(new)

    _add_new()
    entry.async_on_unload(coordinator.async_add_listener(_add_new))


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
