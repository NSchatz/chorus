"""A room's input: the server's stream or a line-in offered now."""

from __future__ import annotations

from homeassistant.components.select import SelectEntity
from homeassistant.core import HomeAssistant, callback
from homeassistant.exceptions import ServiceValidationError
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from ._aiochorus import Zone, commands
from .const import DOMAIN
from .coordinator import ChorusConfigEntry, ChorusCoordinator, room_identifier
from .entity import ChorusRoomEntity
from .media_player import source_map

# Commands go to the server one at a time per platform.
PARALLEL_UPDATES = 1


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ChorusConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    """Add an input select per room, now and as rooms appear."""
    coordinator = entry.runtime_data
    rooms: set[str] = set()

    @callback
    def _add_new() -> None:
        zones = coordinator.data.zones
        new = [
            ChorusInputSelect(coordinator, zone)
            for zone in zones
            if zone.id not in rooms
        ]
        rooms.clear()
        rooms.update(zone.id for zone in zones)
        if new:
            async_add_entities(new)

    _add_new()
    entry.async_on_unload(coordinator.async_add_listener(_add_new))


class ChorusInputSelect(ChorusRoomEntity, SelectEntity):
    """What a room plays, picked from the inputs the server offers now.

    The options are the room's media player's sources: the server's stream,
    then each line-in by its label. Picking one is take the room with that
    source, as the media player's select source is.
    """

    _attr_translation_key = "input"

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone) -> None:
        """Name the entity for the room."""
        super().__init__(coordinator, zone)
        self._attr_unique_id = (
            f"{room_identifier(coordinator.server.id, zone.id)}:input"
        )

    @property
    def options(self) -> list[str]:
        """Return the inputs offered now."""
        return list(source_map(self.coordinator.data))

    @property
    def current_option(self) -> str | None:
        """Return the input the room's group plays, when it is one of the options."""
        group = self.formed_group
        if group is None:
            return None
        return next(
            (
                name
                for name, source in source_map(self.coordinator.data).items()
                if source == group.source
            ),
            None,
        )

    async def async_select_option(self, option: str) -> None:
        """Take the room with the input."""
        source = source_map(self.coordinator.data).get(option)
        if source is None:
            raise ServiceValidationError(
                translation_domain=DOMAIN,
                translation_key="unknown_source",
                translation_placeholders={"source": option},
            )
        await self.coordinator.async_command(commands.take(self._zone_id, source))
