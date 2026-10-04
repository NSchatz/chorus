"""A speaker's buttons: one event entity per button, fired once per accepted press.

The presses come from the server's `GET /api/controller-events`
(`docs/control-plane.md`): one message per controller command the server
accepted, sent once and never kept. Nothing here sends a command: what a
button does in chorus is the server's, and these entities only say that it was
pressed.
"""

from __future__ import annotations

from homeassistant.components.event import EventDeviceClass, EventEntity
from homeassistant.core import HomeAssistant, callback
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from ._aiochorus import BUTTONS, LONG_PRESS, PRESS, ControllerEvent, Speaker
from .coordinator import ChorusConfigEntry, ChorusCoordinator, speaker_identifier
from .entity import ChorusSpeakerEntity

# Nothing is sent to the server from here.
PARALLEL_UPDATES = 0

# The event types of each button. Only play/pause has a long press: the
# speaker sends a join or a leave for it (the event's `command` says which).
EVENT_TYPES: dict[str, list[str]] = {
    button: [PRESS, LONG_PRESS] if button == "play_pause" else [PRESS]
    for button in BUTTONS
}


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ChorusConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    """Add the button entities of each speaker that has buttons, as they appear."""
    coordinator = entry.runtime_data
    known: set[str] = set()

    @callback
    def _add_new() -> None:
        speakers = coordinator.data.speakers
        # A speaker is forgotten only when it leaves `speakers[]`: its device
        # and its entities are removed then. One that is still adopted but
        # away (its `roles` are its latest hello's, and empty before one)
        # keeps its entities and gets no second set.
        known.intersection_update(speaker.id for speaker in speakers)
        new = [
            speaker
            for speaker in speakers
            if speaker.controller and speaker.id not in known
        ]
        known.update(speaker.id for speaker in new)
        if new:
            async_add_entities(
                ChorusButtonEvent(coordinator, speaker, button)
                for speaker in new
                for button in BUTTONS
            )

    _add_new()
    entry.async_on_unload(coordinator.async_add_listener(_add_new))


class ChorusButtonEvent(ChorusSpeakerEntity, EventEntity):
    """One button of one adopted speaker."""

    _attr_device_class = EventDeviceClass.BUTTON

    def __init__(
        self, coordinator: ChorusCoordinator, speaker: Speaker, button: str
    ) -> None:
        """Name the entity for the button."""
        super().__init__(coordinator, speaker)
        self._button = button
        self._attr_translation_key = button
        self._attr_event_types = EVENT_TYPES[button]
        self._attr_unique_id = (
            f"{speaker_identifier(coordinator.server.id, speaker.id)}:button:{button}"
        )

    @property
    def available(self) -> bool:
        """Available while the server has the speaker and presses can arrive.

        A press made while the stream of presses is not attached is never
        delivered, so the entity does not claim to be listening then.
        """
        return (
            super().available
            and self.coordinator.presses_connected
            and self.speaker is not None
        )

    async def async_added_to_hass(self) -> None:
        """Take this button's presses from the coordinator."""
        await super().async_added_to_hass()
        self.async_on_remove(
            self.coordinator.async_add_press_listener(
                self._speaker_id, self._button, self._pressed
            )
        )

    @callback
    def _pressed(self, press: str, event: ControllerEvent) -> None:
        """Fire once for one press the server accepted."""
        self._trigger_event(
            press,
            {
                "command": event.command,
                "value": event.value,
                "target": event.target,
                "room": event.zone,
                "outcome": event.outcome,
            },
        )
        self.async_write_ha_state()
