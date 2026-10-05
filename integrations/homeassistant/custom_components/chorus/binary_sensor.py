"""A voice room's `mic muted` sensor: the speaker's own hardware switch, read-only."""

from __future__ import annotations

from homeassistant.components.binary_sensor import BinarySensorEntity
from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from ._aiochorus import Zone
from .coordinator import ChorusConfigEntry, ChorusCoordinator
from .entity import (
    ChorusRoomEntity,
    async_setup_voice_room_entities,
    voice_room_unique_id,
)

# Read-only: the platform sends nothing to the server.
PARALLEL_UPDATES = 0

# The key of a voice room's sensor in its unique id.
MIC_MUTED_KEY = "mic_muted"


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ChorusConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    """Add a `mic muted` sensor per room that has a microphone, as they appear."""
    async_setup_voice_room_entities(
        entry,
        async_add_entities,
        domain="binary_sensor",
        key=MIC_MUTED_KEY,
        make=ChorusMicMutedBinarySensor,
    )


class ChorusMicMutedBinarySensor(ChorusRoomEntity, BinarySensorEntity):
    """Whether no microphone of the room is live.

    On unless a speaker present in the room reports its gate live: muted at
    its hardware switch, away, or not heard from yet all read on
    (`docs/control-plane.md`, "Voice: `voice_enabled` and `mic_muted`"). The
    switch is the speaker's own. Nothing in Home Assistant sets it and no
    command opens a microphone, so this entity has no action.
    """

    _attr_entity_category = EntityCategory.DIAGNOSTIC
    _attr_translation_key = "mic_muted"

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone) -> None:
        """Name the entity for the room."""
        super().__init__(coordinator, zone)
        self._attr_unique_id = voice_room_unique_id(
            coordinator.server.id, zone.id, MIC_MUTED_KEY
        )

    @property
    def is_on(self) -> bool | None:
        """Return whether the room's microphones are all muted."""
        zone = self.zone
        return None if zone is None else zone.mic_muted
