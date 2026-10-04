"""The base of every chorus entity."""

from __future__ import annotations

from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from ._aiochorus import Group, SavedGroup, Zone
from .const import DOMAIN
from .coordinator import ChorusCoordinator, room_identifier, saved_group_identifier


class ChorusEntity(CoordinatorEntity[ChorusCoordinator]):
    """An entity fed by the server's state messages."""

    _attr_has_entity_name = True


class ChorusRoomEntity(ChorusEntity):
    """An entity of one room."""

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone) -> None:
        """Attach to the room's device."""
        super().__init__(coordinator)
        self._zone_id = zone.id
        server_id = coordinator.server.id
        self._attr_device_info = DeviceInfo(
            identifiers={(DOMAIN, room_identifier(server_id, zone.id))},
            manufacturer="chorus",
            model="Room",
            name=zone.name,
            # A suggestion only: areas are Home Assistant's, never created here.
            suggested_area=zone.name,
            via_device_id=coordinator.server_device_id,
        )

    @property
    def zone(self) -> Zone | None:
        """Return the room as the server last described it."""
        return self.coordinator.data.zone(self._zone_id)

    @property
    def formed_group(self) -> Group | None:
        """Return the formed group the room plays in."""
        return self.coordinator.data.group_of(self._zone_id)

    @property
    def available(self) -> bool:
        """Available while the server is reachable and still has the room."""
        return super().available and self.zone is not None


class ChorusSavedGroupEntity(ChorusEntity):
    """An entity of one saved group."""

    def __init__(self, coordinator: ChorusCoordinator, saved: SavedGroup) -> None:
        """Attach to the saved group's device."""
        super().__init__(coordinator)
        self._group_id = saved.id
        server_id = coordinator.server.id
        self._attr_device_info = DeviceInfo(
            identifiers={(DOMAIN, saved_group_identifier(server_id, saved.id))},
            manufacturer="chorus",
            model="Saved group",
            name=saved.name,
            via_device_id=coordinator.server_device_id,
        )

    @property
    def saved(self) -> SavedGroup | None:
        """Return the saved definition as the server last described it."""
        return self.coordinator.data.saved_group(self._group_id)

    @property
    def formed_group(self) -> Group | None:
        """Return the formed group, while the saved group is active."""
        saved = self.saved
        if saved is None or not saved.active:
            return None
        return self.coordinator.data.group(self._group_id)

    @property
    def available(self) -> bool:
        """Available while the server is reachable and still has the group."""
        return super().available and self.saved is not None
