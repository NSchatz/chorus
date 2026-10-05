"""A room's switches: loudness, night mode, speech enhancement, quiet hours, autoplay, voice."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from homeassistant.components.switch import SwitchDeviceClass, SwitchEntity
from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant, callback
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers import entity_registry as er
from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from ._aiochorus import AutoplayRule, Sound, State, Zone, commands
from .const import DOMAIN
from .coordinator import ChorusConfigEntry, ChorusCoordinator, room_identifier
from .entity import (
    ChorusEntity,
    ChorusRoomEntity,
    async_setup_voice_room_entities,
    saved_group_device_info,
    voice_room_unique_id,
)

# Commands go to the server one at a time per platform.
PARALLEL_UPDATES = 1

# Each boolean of a room's sound: its translation key, how the state says it
# and the command that sets it alone.
_SOUND_SWITCHES: dict[
    str, tuple[str, Callable[[Sound], bool], Callable[[str, bool], bytes]]
] = {
    "loudness": (
        "loudness",
        lambda sound: sound.loudness,
        lambda zone, on: commands.sound(zone, loudness=on),
    ),
    "night": (
        "night_mode",
        lambda sound: sound.night,
        lambda zone, on: commands.sound(zone, night=on),
    ),
    "speech": (
        "speech_enhancement",
        lambda sound: sound.speech,
        lambda zone, on: commands.sound(zone, speech=on),
    ),
}


# The key of a voice room's switch in its unique id.
VOICE_ENABLED_KEY = "voice_enabled"


def autoplay_unique_id(server_id: str, rule: AutoplayRule) -> str:
    """Return the unique id of an autoplay rule's switch.

    The target is part of it: a rule given another target is another switch,
    on the other room's device.
    """
    return f"{server_id}:autoplay:{rule.input}:{rule.target}"


def _autoplay_device(
    coordinator: ChorusCoordinator, state: State, rule: AutoplayRule
) -> DeviceInfo | None:
    """Return the device of the room or the saved group an autoplay rule targets."""
    zone = state.zone(rule.target)
    if zone is not None:
        return coordinator.room_device_info(zone)
    saved = state.saved_group(rule.target)
    if saved is not None:
        return saved_group_device_info(coordinator, saved)
    return None


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ChorusConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    """Add each room's switches and a switch per autoplay rule, as they appear."""
    coordinator = entry.runtime_data
    registry = er.async_get(hass)
    prefix = f"{coordinator.server.id}:autoplay:"
    rooms: set[str] = set()
    rules: set[str] = set()

    @callback
    def _sync() -> None:
        state = coordinator.data
        new: list[SwitchEntity] = []
        for zone in state.zones:
            if zone.id in rooms:
                continue
            new.extend(
                ChorusSoundSwitch(coordinator, zone, key) for key in _SOUND_SWITCHES
            )
            new.append(ChorusQuietHoursSwitch(coordinator, zone))
        rooms.clear()
        rooms.update(zone.id for zone in state.zones)

        current: set[str] = set()
        for rule in state.autoplay:
            device = _autoplay_device(coordinator, state, rule)
            if device is None:
                continue
            unique_id = autoplay_unique_id(coordinator.server.id, rule)
            current.add(unique_id)
            if unique_id not in rules:
                new.append(ChorusAutoplaySwitch(coordinator, rule, device))
        rules.clear()
        rules.update(current)
        # A rule the server no longer has takes its switch with it, one left
        # in the registry from before a restart included.
        for registered in er.async_entries_for_config_entry(registry, entry.entry_id):
            if (
                registered.domain == "switch"
                and registered.unique_id.startswith(prefix)
                and registered.unique_id not in current
            ):
                registry.async_remove(registered.entity_id)
        if new:
            async_add_entities(new)

    _sync()
    entry.async_on_unload(coordinator.async_add_listener(_sync))
    # A room that has a microphone has a switch for its voice path as well.
    async_setup_voice_room_entities(
        entry,
        async_add_entities,
        domain="switch",
        key=VOICE_ENABLED_KEY,
        make=ChorusVoiceSwitch,
    )


class ChorusSoundSwitch(ChorusRoomEntity, SwitchEntity):
    """Loudness, night mode or speech enhancement of a room."""

    _attr_device_class = SwitchDeviceClass.SWITCH
    _attr_entity_category = EntityCategory.CONFIG

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone, key: str) -> None:
        """Name the entity for the room and the setting."""
        super().__init__(coordinator, zone)
        self._attr_translation_key, self._read, self._command = _SOUND_SWITCHES[key]
        self._attr_unique_id = (
            f"{room_identifier(coordinator.server.id, zone.id)}:{key}"
        )

    @property
    def available(self) -> bool:
        """Available while the server says what the room's sound is."""
        zone = self.zone
        return super().available and zone is not None and zone.sound is not None

    @property
    def is_on(self) -> bool | None:
        """Return whether the setting is on."""
        zone = self.zone
        if zone is None or zone.sound is None:
            return None
        return self._read(zone.sound)

    async def async_turn_on(self, **kwargs: Any) -> None:
        """Switch the setting on and leave the rest of the room's sound alone."""
        await self.coordinator.async_command(self._command(self._zone_id, True))

    async def async_turn_off(self, **kwargs: Any) -> None:
        """Switch the setting off and leave the rest of the room's sound alone."""
        await self.coordinator.async_command(self._command(self._zone_id, False))


class ChorusQuietHoursSwitch(ChorusRoomEntity, SwitchEntity):
    """Whether a room's quiet-hours windows cap it; the windows are chorus's."""

    _attr_device_class = SwitchDeviceClass.SWITCH
    _attr_entity_category = EntityCategory.CONFIG
    _attr_translation_key = "quiet_hours"

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone) -> None:
        """Name the entity for the room."""
        super().__init__(coordinator, zone)
        self._attr_unique_id = (
            f"{room_identifier(coordinator.server.id, zone.id)}:quiet_hours"
        )

    @property
    def is_on(self) -> bool | None:
        """Return whether the room's quiet hours are switched on."""
        zone = self.zone
        return None if zone is None else zone.quiet_enabled

    async def async_turn_on(self, **kwargs: Any) -> None:
        """Switch the room's quiet hours on; its windows are kept."""
        await self.coordinator.async_command(
            commands.quiet_hours_enabled(self._zone_id, True)
        )

    async def async_turn_off(self, **kwargs: Any) -> None:
        """Switch the room's quiet hours off; its windows are kept."""
        await self.coordinator.async_command(
            commands.quiet_hours_enabled(self._zone_id, False)
        )


class ChorusVoiceSwitch(ChorusRoomEntity, SwitchEntity):
    """Whether a room's voice path is on: the software half of the microphone gate.

    Off by default, in every room, and kept by the server. On, the server asks
    the room's speakers for their microphone audio; it never opens a muted
    microphone, whose switch is the speaker's own (the `mic muted` sensor).
    """

    _attr_device_class = SwitchDeviceClass.SWITCH
    _attr_entity_category = EntityCategory.CONFIG
    _attr_translation_key = "voice_enabled"

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone) -> None:
        """Name the entity for the room."""
        super().__init__(coordinator, zone)
        self._attr_unique_id = voice_room_unique_id(
            coordinator.server.id, zone.id, VOICE_ENABLED_KEY
        )

    @property
    def is_on(self) -> bool | None:
        """Return whether the room's voice path is switched on."""
        zone = self.zone
        return None if zone is None else zone.voice_enabled

    async def async_turn_on(self, **kwargs: Any) -> None:
        """Switch the room's voice path on."""
        await self.coordinator.async_command(
            commands.voice_enabled(self._zone_id, True)
        )

    async def async_turn_off(self, **kwargs: Any) -> None:
        """Switch the room's voice path off; a run that is open ends."""
        await self.coordinator.async_command(
            commands.voice_enabled(self._zone_id, False)
        )


class ChorusAutoplaySwitch(ChorusEntity, SwitchEntity):
    """Whether one autoplay rule is enabled, on the device of what it targets."""

    _attr_device_class = SwitchDeviceClass.SWITCH
    _attr_entity_category = EntityCategory.CONFIG
    _attr_translation_key = "autoplay"

    def __init__(
        self, coordinator: ChorusCoordinator, rule: AutoplayRule, device: DeviceInfo
    ) -> None:
        """Name the entity for the rule's input.

        The name takes the input's label as it is now; Home Assistant builds an
        entity's name once, so a label changed later shows after a reload.
        """
        super().__init__(coordinator)
        self._input = rule.input
        self._target = rule.target
        self._attr_device_info = device
        self._attr_unique_id = autoplay_unique_id(coordinator.server.id, rule)
        self._attr_translation_placeholders = {
            "input": coordinator.data.input_name(rule.input) or rule.input
        }

    @property
    def rule(self) -> AutoplayRule | None:
        """Return the rule as the server last described it."""
        return self.coordinator.data.autoplay_rule(self._input, self._target)

    @property
    def available(self) -> bool:
        """Available while the server is reachable and still has the rule."""
        return super().available and self.rule is not None

    @property
    def is_on(self) -> bool | None:
        """Return whether the rule is enabled."""
        rule = self.rule
        return None if rule is None else rule.enabled

    async def _async_set(self, enabled: bool) -> None:
        rule = self.rule
        if rule is None:
            raise HomeAssistantError(
                translation_domain=DOMAIN,
                translation_key="autoplay_gone",
                translation_placeholders={"input": self._input},
            )
        # The command replaces the rule, so everything else it says is sent again.
        await self.coordinator.async_command(
            commands.autoplay(
                rule.input,
                rule.target,
                enabled,
                stop_on_standby=rule.stop_on_standby,
                low_latency=rule.low_latency,
            )
        )

    async def async_turn_on(self, **kwargs: Any) -> None:
        """Enable the rule."""
        await self._async_set(True)

    async def async_turn_off(self, **kwargs: Any) -> None:
        """Disable the rule."""
        await self._async_set(False)
