"""Rooms and saved groups as media players."""

from __future__ import annotations

from typing import Any

from homeassistant.components import media_source
from homeassistant.components.media_player import (
    BrowseError,
    BrowseMedia,
    MediaClass,
    MediaPlayerDeviceClass,
    MediaPlayerEntity,
    MediaPlayerEntityFeature,
    MediaPlayerState,
    MediaType,
)
from homeassistant.core import HomeAssistant, callback
from homeassistant.exceptions import HomeAssistantError, ServiceValidationError
from homeassistant.helpers import entity_registry as er
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from ._aiochorus import Group, NowPlaying, SavedGroup, State, Zone, commands
from ._aiochorus.models import (
    LINE_IN_PREFIX,
    SOLOIST_PREFIX,
    SOURCE_NONE,
    SOURCE_STREAM,
)
from .announce import async_announce
from .const import (
    DOMAIN,
    MEDIA_ID_PREFIX,
    MEDIA_ID_ROOT,
    MEDIA_TYPE_INPUT,
    VOLUME_STEP_THOUSANDTHS,
)
from .coordinator import (
    ChorusConfigEntry,
    ChorusCoordinator,
    room_identifier,
    saved_group_identifier,
)
from .entity import ChorusEntity, ChorusRoomEntity, ChorusSavedGroupEntity

# Commands go to the server one at a time per platform.
PARALLEL_UPDATES = 1

_BASE_FEATURES = (
    MediaPlayerEntityFeature.VOLUME_SET
    | MediaPlayerEntityFeature.VOLUME_STEP
    | MediaPlayerEntityFeature.VOLUME_MUTE
    | MediaPlayerEntityFeature.SELECT_SOURCE
    | MediaPlayerEntityFeature.PLAY_MEDIA
    | MediaPlayerEntityFeature.MEDIA_ANNOUNCE
    | MediaPlayerEntityFeature.BROWSE_MEDIA
    | MediaPlayerEntityFeature.TURN_ON
    | MediaPlayerEntityFeature.TURN_OFF
)
# The catalog's one transport command reaches a Spotify receiver only.
_TRANSPORT_FEATURES = (
    MediaPlayerEntityFeature.PAUSE
    | MediaPlayerEntityFeature.PLAY
    | MediaPlayerEntityFeature.NEXT_TRACK
    | MediaPlayerEntityFeature.PREVIOUS_TRACK
)
_RECORD_STATES = {
    "playing": MediaPlayerState.PLAYING,
    "paused": MediaPlayerState.PAUSED,
    "buffering": MediaPlayerState.BUFFERING,
}


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ChorusConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    """Add a media player per room and per saved group, now and as they appear."""
    coordinator = entry.runtime_data
    rooms: set[str] = set()
    groups: set[str] = set()

    @callback
    def _add_new() -> None:
        state = coordinator.data
        new: list[MediaPlayerEntity] = []
        current_rooms = {zone.id for zone in state.zones}
        current_groups = {group.id for group in state.saved_groups}
        new.extend(
            ChorusRoomMediaPlayer(coordinator, zone)
            for zone in state.zones
            if zone.id not in rooms
        )
        new.extend(
            ChorusSavedGroupMediaPlayer(coordinator, saved)
            for saved in state.saved_groups
            if saved.id not in groups
        )
        # One that left is forgotten, so it is added again should it return.
        rooms.clear()
        rooms.update(current_rooms)
        groups.clear()
        groups.update(current_groups)
        if new:
            async_add_entities(new)

    _add_new()
    entry.async_on_unload(coordinator.async_add_listener(_add_new))


def source_map(state: State) -> dict[str, str]:
    """Return what a person picks from, each with its catalog source spelling.

    The server's stream first, then every line-in offered now, shown by its
    label where it has one.
    """
    sources = {SOURCE_STREAM: SOURCE_STREAM}
    for input_id in state.inputs:
        name = state.input_name(input_id) or input_id
        if name in sources:
            name = f"{name} ({input_id})"
        sources[name] = f"{LINE_IN_PREFIX}{input_id}"
    return sources


def source_from_media_id(state: State, media_id: str) -> str:
    """Return the catalog source a ``chorus://input/...`` id names."""
    if media_id.startswith(MEDIA_ID_PREFIX):
        input_id = media_id.removeprefix(MEDIA_ID_PREFIX)
        if input_id == SOURCE_STREAM:
            return SOURCE_STREAM
        if input_id in state.inputs:
            return f"{LINE_IN_PREFIX}{input_id}"
        raise ServiceValidationError(
            translation_domain=DOMAIN,
            translation_key="unknown_input",
            translation_placeholders={"input": input_id},
        )
    raise ServiceValidationError(
        translation_domain=DOMAIN,
        translation_key="unsupported_media",
        translation_placeholders={"media_id": media_id},
    )


class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
    """What a room's and a saved group's media player share."""

    _attr_name = None
    _attr_device_class = MediaPlayerDeviceClass.SPEAKER
    _attr_media_image_remotely_accessible = False
    _base_features = _BASE_FEATURES

    # The catalog target of this entity: a room id or a saved group id.
    _target: str

    @property
    def formed_group(self) -> Group | None:
        """Return the formed group this entity speaks for."""
        raise NotImplementedError

    async def async_added_to_hass(self) -> None:
        """Let the players added before this one name it among their members.

        Members are entity ids, and an entity id exists only once its entity is
        registered: the first room of a group is written before the second is.
        """
        await super().async_added_to_hass()
        self.coordinator.async_update_listeners()

    @property
    def _record(self) -> NowPlaying | None:
        group = self.formed_group
        return None if group is None else group.now_playing

    @property
    def state(self) -> MediaPlayerState:
        """Return what the group is doing."""
        group = self.formed_group
        if group is None or group.source == SOURCE_NONE:
            return MediaPlayerState.OFF
        if group.now_playing is not None:
            return _RECORD_STATES.get(group.now_playing.state, MediaPlayerState.PLAYING)
        if group.source.startswith(("player:", SOLOIST_PREFIX)):
            return MediaPlayerState.IDLE
        return MediaPlayerState.ON

    @property
    def supported_features(self) -> MediaPlayerEntityFeature:
        """Offer transport only while the group plays a Spotify receiver."""
        group = self.formed_group
        if group is not None and group.source.startswith(SOLOIST_PREFIX):
            return self._base_features | _TRANSPORT_FEATURES
        return self._base_features

    @property
    def source_list(self) -> list[str]:
        """Return the server's stream and the line-ins offered now."""
        return list(source_map(self.coordinator.data))

    @property
    def source(self) -> str | None:
        """Return what the group plays, by the name it is picked by."""
        group = self.formed_group
        if group is None or group.source == SOURCE_NONE:
            return None
        for name, source in source_map(self.coordinator.data).items():
            if source == group.source:
                return name
        return group.source

    @property
    def media_content_type(self) -> MediaType | None:
        """Return music while something says what is playing."""
        return None if self._record is None else MediaType.MUSIC

    @property
    def media_title(self) -> str | None:
        """Return the title of what is playing."""
        return None if (record := self._record) is None else record.title

    @property
    def media_artist(self) -> str | None:
        """Return the artist of what is playing."""
        return None if (record := self._record) is None else record.artist

    @property
    def media_album_name(self) -> str | None:
        """Return the album of what is playing."""
        return None if (record := self._record) is None else record.album

    @property
    def media_image_url(self) -> str | None:
        """Return the artwork of what is playing."""
        return None if (record := self._record) is None else record.art_url

    @property
    def media_duration(self) -> int | None:
        """Return the length of what is playing, in whole seconds."""
        record = self._record
        if record is None or record.duration_ms is None:
            return None
        return round(record.duration_ms / 1000)

    async def async_select_source(self, source: str) -> None:
        """Take the room (or assemble the saved group) with a source."""
        catalog = source_map(self.coordinator.data).get(source)
        if catalog is None:
            raise ServiceValidationError(
                translation_domain=DOMAIN,
                translation_key="unknown_source",
                translation_placeholders={"source": source},
            )
        await self.coordinator.async_command(commands.take(self._target, catalog))

    async def async_turn_on(self) -> None:
        """Take the room (or assemble the saved group) with the server's stream."""
        await self.coordinator.async_command(commands.take(self._target, SOURCE_STREAM))

    async def async_turn_off(self) -> None:
        """Take the room (or assemble the saved group) and play nothing."""
        await self.coordinator.async_command(commands.take(self._target, SOURCE_NONE))

    async def _async_playback(self, action: str) -> None:
        await self.coordinator.async_command(commands.playback(self._target, action))

    async def async_media_pause(self) -> None:
        """Pause the Spotify receiver the group plays."""
        await self._async_playback("pause")

    async def async_media_play(self) -> None:
        """Resume the Spotify receiver the group plays."""
        await self._async_playback("resume")

    async def async_media_next_track(self) -> None:
        """Skip to the next track on the Spotify receiver the group plays."""
        await self._async_playback("next")

    async def async_media_previous_track(self) -> None:
        """Go to the previous track on the Spotify receiver the group plays."""
        await self._async_playback("previous")

    async def async_play_media(
        self, media_type: MediaType | str, media_id: str, **kwargs: Any
    ) -> None:
        """Play a chorus input, or announce a clip Home Assistant serves."""
        if not kwargs.get("announce"):
            source = source_from_media_id(self.coordinator.data, media_id)
            await self.coordinator.async_command(commands.take(self._target, source))
            return

        if media_source.is_media_source_id(media_id):
            item = await media_source.async_resolve_media(
                self.hass, media_id, self.entity_id
            )
            media_id = item.url
        extra = kwargs.get("extra") or {}
        thousandths: int | None = None
        if (level := extra.get("volume")) is not None:
            if (
                isinstance(level, bool)
                or not isinstance(level, (int, float))
                or not 0 <= level <= 1
            ):
                raise ServiceValidationError(
                    translation_domain=DOMAIN, translation_key="announce_volume"
                )
            thousandths = commands.volume_from_level(level)
        await async_announce(
            self.hass, self.coordinator, self._target, media_id, thousandths
        )

    async def async_browse_media(
        self,
        media_content_type: MediaType | str | None = None,
        media_content_id: str | None = None,
    ) -> BrowseMedia:
        """Offer the chorus inputs; chorus has no content of its own to browse."""
        if media_content_id not in (None, "", MEDIA_ID_ROOT):
            raise BrowseError(
                translation_domain=DOMAIN,
                translation_key="browse_unknown",
                translation_placeholders={"media_id": media_content_id},
            )
        children = [
            BrowseMedia(
                media_class=MediaClass.CHANNEL,
                media_content_id=MEDIA_ID_PREFIX
                + (
                    SOURCE_STREAM
                    if source == SOURCE_STREAM
                    else source.removeprefix(LINE_IN_PREFIX)
                ),
                media_content_type=MEDIA_TYPE_INPUT,
                title=name,
                can_play=True,
                can_expand=False,
            )
            for name, source in source_map(self.coordinator.data).items()
        ]
        return BrowseMedia(
            media_class=MediaClass.DIRECTORY,
            media_content_id=MEDIA_ID_ROOT,
            media_content_type=MEDIA_TYPE_INPUT,
            title="chorus",
            can_play=False,
            can_expand=True,
            children=children,
            children_media_class=MediaClass.CHANNEL,
        )


class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
    """A room: its own volume, and the group it plays in as its members."""

    _attr_translation_key = "room"
    _base_features = _BASE_FEATURES | MediaPlayerEntityFeature.GROUPING

    def __init__(self, coordinator: ChorusCoordinator, zone: Zone) -> None:
        """Name the entity for the room."""
        super().__init__(coordinator, zone)
        self._target = zone.id
        self._attr_unique_id = room_identifier(coordinator.server.id, zone.id)

    @property
    def volume_level(self) -> float | None:
        """Return the room's volume as the server holds it."""
        return None if (zone := self.zone) is None else zone.volume

    @property
    def is_volume_muted(self) -> bool | None:
        """Return whether the room is muted."""
        return None if (zone := self.zone) is None else zone.muted

    @property
    def group_members(self) -> list[str]:
        """Return the rooms of the room's group, the leader first.

        The leader is the group's first room in the server's order. A room
        alone is its own only member.
        """
        group = self.formed_group
        if group is None or len(group.zones) < 2:
            return [self.entity_id]
        registry = er.async_get(self.hass)
        server_id = self.coordinator.server.id
        members = [
            entity_id
            for zone_id in group.zones
            if (
                entity_id := registry.async_get_entity_id(
                    "media_player", DOMAIN, room_identifier(server_id, zone_id)
                )
            )
            is not None
        ]
        return members or [self.entity_id]

    async def async_set_volume_level(self, volume: float) -> None:
        """Set the room's volume; the server clamps it to the room's limits."""
        await self.coordinator.async_command(
            commands.volume(self._zone_id, commands.volume_from_level(volume))
        )

    async def async_volume_up(self) -> None:
        """Raise the room's volume one step."""
        await self.coordinator.async_command(
            commands.volume_step(self._zone_id, VOLUME_STEP_THOUSANDTHS)
        )

    async def async_volume_down(self) -> None:
        """Lower the room's volume one step."""
        await self.coordinator.async_command(
            commands.volume_step(self._zone_id, -VOLUME_STEP_THOUSANDTHS)
        )

    async def async_mute_volume(self, mute: bool) -> None:
        """Mute or unmute the room."""
        await self.coordinator.async_command(commands.mute(self._zone_id, mute))

    def _zone_of(self, entity_id: str) -> str:
        """Return the room an entity id names, or refuse it by name."""
        entry = er.async_get(self.hass).async_get(entity_id)
        server_id = self.coordinator.server.id
        if (
            entry is not None
            and entry.domain == "media_player"
            and entry.platform == DOMAIN
            and entry.config_entry_id == self.coordinator.config_entry.entry_id
        ):
            prefix = room_identifier(server_id, "")
            if entry.unique_id.startswith(prefix):
                return entry.unique_id.removeprefix(prefix)
            if entry.unique_id.startswith(saved_group_identifier(server_id, "")):
                raise ServiceValidationError(
                    translation_domain=DOMAIN,
                    translation_key="join_saved_group",
                    translation_placeholders={"entity_id": entity_id},
                )
        raise ServiceValidationError(
            translation_domain=DOMAIN,
            translation_key="join_not_a_room",
            translation_placeholders={"entity_id": entity_id},
        )

    async def async_join_players(self, group_members: list[str]) -> None:
        """Take each named room into this room's group (K78).

        Every member is validated before the first command is sent. A room
        already in this room's group is left alone.
        """
        zones = [self._zone_of(entity_id) for entity_id in group_members]
        state = self.coordinator.data
        for zone_id in zones:
            leader = state.zone(self._zone_id)
            member = state.zone(zone_id)
            if leader is None or member is None:
                raise HomeAssistantError(
                    translation_domain=DOMAIN,
                    translation_key="room_gone",
                    translation_placeholders={"room": zone_id},
                )
            if zone_id == self._zone_id or member.group == leader.group:
                continue
            state = await self.coordinator.async_command(
                commands.join(zone_id, self._zone_id)
            )

    async def async_unjoin_player(self) -> None:
        """Leave the group for the room's own (take the room, K78)."""
        group = self.formed_group
        if group is not None and group.kind == "room" and len(group.zones) == 1:
            return
        await self.coordinator.async_command(commands.take(self._zone_id))


class ChorusSavedGroupMediaPlayer(ChorusSavedGroupEntity, ChorusMediaPlayer):
    """A saved group: always present, its volume the group volume while active."""

    _attr_translation_key = "saved_group"

    def __init__(self, coordinator: ChorusCoordinator, saved: SavedGroup) -> None:
        """Name the entity for the saved group."""
        super().__init__(coordinator, saved)
        self._target = saved.id
        self._attr_unique_id = saved_group_identifier(coordinator.server.id, saved.id)

    @property
    def volume_level(self) -> float | None:
        """Return the group volume while the group is active."""
        return None if (group := self.formed_group) is None else group.volume

    @property
    def is_volume_muted(self) -> bool | None:
        """Return whether every room of the group is muted."""
        saved = self.saved
        if saved is None:
            return None
        zones = [self.coordinator.data.zone(zone_id) for zone_id in saved.zones]
        return all(zone is not None and zone.muted for zone in zones)

    @property
    def extra_state_attributes(self) -> dict[str, Any]:
        """Return the group's rooms, as entity ids, and whether it is assembled."""
        saved = self.saved
        if saved is None:
            return {}
        registry = er.async_get(self.hass)
        server_id = self.coordinator.server.id
        return {
            "rooms": [
                entity_id
                for zone_id in saved.zones
                if (
                    entity_id := registry.async_get_entity_id(
                        "media_player", DOMAIN, room_identifier(server_id, zone_id)
                    )
                )
                is not None
            ],
            "active": saved.active,
        }

    def _active_group(self) -> Group:
        group = self.formed_group
        if group is None:
            raise ServiceValidationError(
                translation_domain=DOMAIN, translation_key="group_not_active"
            )
        return group

    async def async_set_volume_level(self, volume: float) -> None:
        """Set the group volume; the server scales and clamps each room."""
        await self.coordinator.async_command(
            commands.group_volume(
                self._active_group().id, commands.volume_from_level(volume)
            )
        )

    async def async_volume_up(self) -> None:
        """Raise the group volume one step."""
        await self.coordinator.async_command(
            commands.group_volume_step(self._active_group().id, VOLUME_STEP_THOUSANDTHS)
        )

    async def async_volume_down(self) -> None:
        """Lower the group volume one step."""
        await self.coordinator.async_command(
            commands.group_volume_step(
                self._active_group().id, -VOLUME_STEP_THOUSANDTHS
            )
        )

    async def async_mute_volume(self, mute: bool) -> None:
        """Mute or unmute every room of the group."""
        saved = self.saved
        for zone_id in () if saved is None else saved.zones:
            await self.coordinator.async_command(commands.mute(zone_id, mute))
