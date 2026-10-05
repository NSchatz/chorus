"""The base of every chorus entity."""

from __future__ import annotations

from collections.abc import Callable

from homeassistant.core import callback
from homeassistant.helpers import entity_registry as er
from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.entity import Entity
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from ._aiochorus import Group, SavedGroup, Speaker, SpeakerFirmware, Zone
from .const import DOMAIN
from .coordinator import (
    ChorusConfigEntry,
    ChorusCoordinator,
    room_identifier,
    saved_group_identifier,
)


def saved_group_device_info(
    coordinator: ChorusCoordinator, saved: SavedGroup
) -> DeviceInfo:
    """Return the device of a saved group."""
    return DeviceInfo(
        identifiers={(DOMAIN, saved_group_identifier(coordinator.server.id, saved.id))},
        manufacturer="chorus",
        model="Saved group",
        name=saved.name,
        via_device_id=coordinator.server_device_id,
    )


class ChorusEntity(CoordinatorEntity[ChorusCoordinator]):
    """An entity fed by the server's state messages."""

    _attr_has_entity_name = True


class ChorusRoomEntity(ChorusEntity):
    """An entity of one room."""

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone) -> None:
        """Attach to the room's device."""
        super().__init__(coordinator)
        self._zone_id = zone.id
        self._attr_device_info = coordinator.room_device_info(zone)

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
        self._attr_device_info = saved_group_device_info(coordinator, saved)

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


class ChorusSpeakerEntity(ChorusEntity):
    """An entity of one adopted speaker."""

    def __init__(self, coordinator: ChorusCoordinator, speaker: Speaker) -> None:
        """Attach to the speaker's device."""
        super().__init__(coordinator)
        self._speaker_id = speaker.id
        self._attr_device_info = coordinator.speaker_device_info(
            coordinator.data, speaker
        )

    @property
    def speaker(self) -> Speaker | None:
        """Return the speaker as the server last described it."""
        return self.coordinator.data.speaker(self._speaker_id)

    @property
    def firmware(self) -> SpeakerFirmware | None:
        """Return what the speaker runs, once it has reported it."""
        speaker = self.speaker
        return None if speaker is None else speaker.firmware


def voice_room_unique_id(server_id: str, zone_id: str, key: str) -> str:
    """Return the unique id of one of a voice room's entities."""
    return f"{room_identifier(server_id, zone_id)}:{key}"


@callback
def async_setup_voice_room_entities(
    entry: ChorusConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
    *,
    domain: str,
    key: str,
    make: Callable[[ChorusCoordinator, Zone], Entity],
) -> None:
    """Add one entity per room that has a microphone, and follow the rooms.

    A room has a microphone while a speaker adopted into it declares the voice
    role (`State.voice_rooms`). A room that gains one gets the entity then; a
    room that loses its last one loses the entity, one left in the registry
    from before a restart included, so a room without a microphone never shows
    a voice entity.
    """
    coordinator = entry.runtime_data
    registry = er.async_get(coordinator.hass)
    prefix = f"{coordinator.server.id}:room:"
    suffix = f":{key}"
    known: set[str] = set()

    @callback
    def _sync() -> None:
        state = coordinator.data
        current = state.voice_rooms()
        new = [
            make(coordinator, zone)
            for zone in state.zones
            if zone.id in current and zone.id not in known
        ]
        known.clear()
        known.update(current)
        wanted = {
            voice_room_unique_id(coordinator.server.id, zone_id, key)
            for zone_id in current
        }
        for registered in er.async_entries_for_config_entry(registry, entry.entry_id):
            if (
                registered.domain == domain
                and registered.unique_id.startswith(prefix)
                and registered.unique_id.endswith(suffix)
                and registered.unique_id not in wanted
            ):
                registry.async_remove(registered.entity_id)
        if new:
            async_add_entities(new)

    _sync()
    entry.async_on_unload(coordinator.async_add_listener(_sync))
