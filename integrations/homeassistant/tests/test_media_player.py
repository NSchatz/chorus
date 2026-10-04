"""Rooms and saved groups as media players, against the fake server.

The fake serves the repository's shared state vectors and records every
command byte for byte; the assertions on `server.bodies` are the exact bytes
the integration POSTs.
"""

from __future__ import annotations

import json
from unittest.mock import patch

from homeassistant.components.media_player import (
    ATTR_GROUP_MEMBERS,
    ATTR_INPUT_SOURCE,
    ATTR_INPUT_SOURCE_LIST,
    ATTR_MEDIA_ALBUM_NAME,
    ATTR_MEDIA_ANNOUNCE,
    ATTR_MEDIA_ARTIST,
    ATTR_MEDIA_CONTENT_TYPE,
    ATTR_MEDIA_DURATION,
    ATTR_MEDIA_EXTRA,
    ATTR_MEDIA_TITLE,
    ATTR_MEDIA_VOLUME_LEVEL,
    ATTR_MEDIA_VOLUME_MUTED,
    DOMAIN as MP_DOMAIN,
    BrowseError,
    MediaPlayerEntityFeature,
)
from homeassistant.components.media_source import PlayMedia
from homeassistant.const import ATTR_ENTITY_ID, ATTR_SUPPORTED_FEATURES
from homeassistant.core import HomeAssistant
from homeassistant.core_config import async_process_ha_core_config
from homeassistant.exceptions import (
    HomeAssistantError,
    ServiceNotSupported,
    ServiceValidationError,
)
import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.chorus.const import DOMAIN

from .conftest import room, saved_group, wait_for
from .fake_server import FakeChorusServer, local, shared

HA_URL = "http://ha.example:8123"
CLIP = f"{HA_URL}/api/tts_proxy/abc.mp3"


async def call(hass: HomeAssistant, service: str, entity_id: str, **data) -> None:
    await hass.services.async_call(
        MP_DOMAIN, service, {ATTR_ENTITY_ID: entity_id, **data}, blocking=True
    )
    await hass.async_block_till_done()


def attr(hass: HomeAssistant, entity_id: str, name: str):
    state = hass.states.get(entity_id)
    assert state is not None, entity_id
    return state.attributes.get(name)


@pytest.fixture
async def ha_url(hass: HomeAssistant) -> None:
    await async_process_ha_core_config(hass, {"internal_url": HA_URL})


# --- what the entities show ------------------------------------------------------


async def test_rooms_and_saved_groups_from_the_rich_vector(
    hass: HomeAssistant, setup: MockConfigEntry
) -> None:
    living, kitchen = room(hass, "living"), room(hass, "kitchen")
    study, bedroom = room(hass, "study"), room(hass, "bedroom")
    downstairs = saved_group(hass, "downstairs")

    state = hass.states.get(living)
    assert state is not None
    assert state.state == "on"
    assert state.name == "Living Room"
    assert state.attributes[ATTR_MEDIA_VOLUME_LEVEL] == 0.857
    assert state.attributes[ATTR_MEDIA_VOLUME_MUTED] is False
    assert state.attributes[ATTR_INPUT_SOURCE] == "endpoint-c/line-1"
    assert state.attributes[ATTR_INPUT_SOURCE_LIST] == ["stream", "endpoint-c/line-1"]
    # Live and saved groups alike are the rooms' members, the leader first.
    assert state.attributes[ATTR_GROUP_MEMBERS] == [living, kitchen]
    assert attr(hass, kitchen, ATTR_GROUP_MEMBERS) == [living, kitchen]
    assert attr(hass, study, ATTR_GROUP_MEMBERS) == [study, bedroom]
    assert attr(hass, bedroom, ATTR_GROUP_MEMBERS) == [study, bedroom]
    assert attr(hass, study, ATTR_INPUT_SOURCE) == "stream"
    assert state.attributes[ATTR_SUPPORTED_FEATURES] & MediaPlayerEntityFeature.GROUPING

    group = hass.states.get(downstairs)
    assert group is not None
    assert group.state == "on"
    assert group.name == "Downstairs"
    assert group.attributes[ATTR_MEDIA_VOLUME_LEVEL] == 0.6
    assert group.attributes["rooms"] == [living, kitchen]
    assert group.attributes["active"] is True
    assert group.attributes[ATTR_MEDIA_VOLUME_MUTED] is False
    # A saved group is not something a room can join: no GROUPING, no members.
    features = group.attributes[ATTR_SUPPORTED_FEATURES]
    assert not features & MediaPlayerEntityFeature.GROUPING
    assert ATTR_GROUP_MEMBERS not in group.attributes
    # No transport: the catalog has it for a Spotify receiver only.
    assert not features & MediaPlayerEntityFeature.PAUSE


async def test_now_playing_maps_to_the_media_properties(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    server.set_state(shared("state-playing.json"))
    await wait_for(lambda: len(hass.states.async_entity_ids(MP_DOMAIN)) == 6)
    await hass.async_block_till_done()
    living = hass.states.get(room(hass, "living"))
    assert living is not None
    assert living.state == "playing"
    assert living.attributes[ATTR_MEDIA_TITLE] == "Morning Light"
    assert living.attributes[ATTR_MEDIA_ARTIST] == "The Example Quartet"
    assert living.attributes[ATTR_MEDIA_ALBUM_NAME] == "First Takes"
    assert living.attributes[ATTR_MEDIA_DURATION] == 215
    assert living.attributes[ATTR_MEDIA_CONTENT_TYPE] == "music"
    assert living.attributes["entity_picture"].startswith("/api/media_player_proxy/")
    assert living.attributes[ATTR_INPUT_SOURCE] == "player:p0"
    study = hass.states.get(room(hass, "study"))
    assert study is not None
    assert study.state == "paused"
    assert study.attributes[ATTR_MEDIA_TITLE] == "Evening news"
    assert ATTR_MEDIA_DURATION not in study.attributes
    # A player nothing was said about is idle; the stream is on.
    assert hass.states.get(room(hass, "bedroom")).state == "idle"
    assert hass.states.get(room(hass, "hall")).state == "on"
    assert hass.states.get(saved_group(hass, "downstairs")).state == "playing"


async def test_a_labelled_input_is_shown_by_its_name(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    server.set_state(shared("state-inputs.json"))
    await wait_for(
        lambda: (
            attr(hass, room(hass, "kitchen"), ATTR_INPUT_SOURCE) == "Kitchen streamer"
        )
    )
    kitchen = hass.states.get(room(hass, "kitchen"))
    assert kitchen is not None
    assert kitchen.attributes[ATTR_INPUT_SOURCE_LIST] == ["stream", "Kitchen streamer"]
    assert kitchen.attributes[ATTR_MEDIA_TITLE] == "Kitchen streamer"
    assert kitchen.state == "playing"
    await call(hass, "select_source", room(hass, "bedroom"), source="Kitchen streamer")
    assert server.bodies == [
        b'{"v":2,"t":"take","target":"bedroom","source":"line-in:endpoint-c/line-1"}'
    ]


async def test_two_inputs_with_one_label_stay_distinct(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    raw = json.loads(shared("state-inputs.json"))
    raw["inputs"] = ["endpoint-c/line-1", "endpoint-d/line-1"]
    raw["input_labels"].append(
        {"input": "endpoint-d/line-1", "name": "Kitchen streamer", "role": "line-in"}
    )
    server.set_state(json.dumps(raw).encode())
    await wait_for(
        lambda: (
            attr(hass, room(hass, "kitchen"), ATTR_INPUT_SOURCE_LIST)
            == ["stream", "Kitchen streamer", "Kitchen streamer (endpoint-d/line-1)"]
        )
    )


# --- join and unjoin: take the room (K78) ----------------------------------------


async def test_join_takes_each_room_into_the_leaders_group(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    living, kitchen = room(hass, "living"), room(hass, "kitchen")
    study, bedroom = room(hass, "study"), room(hass, "bedroom")
    # living is already in kitchen's group, and kitchen is itself: both no-ops.
    await call(hass, "join", kitchen, group_members=[living, study, kitchen, bedroom])
    assert server.bodies == [
        b'{"v":2,"t":"join","zone":"study","target":"kitchen"}',
        b'{"v":2,"t":"join","zone":"bedroom","target":"kitchen"}',
    ]
    await wait_for(
        lambda: (
            attr(hass, kitchen, ATTR_GROUP_MEMBERS) == [living, kitchen, study, bedroom]
        )
    )
    for member in (living, study, bedroom):
        assert attr(hass, member, ATTR_GROUP_MEMBERS) == [
            living,
            kitchen,
            study,
            bedroom,
        ]
    # Joining them again sends nothing: every member is already there.
    await call(hass, "join", kitchen, group_members=[study, bedroom])
    assert len(server.bodies) == 2


async def test_join_from_a_room_alone_forms_a_live_group(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    server.set_state(shared("state-soloist.json"))
    await wait_for(lambda: len(hass.states.async_entity_ids(MP_DOMAIN)) == 2)
    kitchen, den = room(hass, "kitchen"), room(hass, "den")
    assert attr(hass, kitchen, ATTR_GROUP_MEMBERS) == [kitchen]
    await call(hass, "join", kitchen, group_members=[den])
    assert server.bodies == [b'{"v":2,"t":"join","zone":"den","target":"kitchen"}']
    await wait_for(lambda: attr(hass, den, ATTR_GROUP_MEMBERS) == [kitchen, den])
    assert server.model["groups"][0]["id"] == "live-1"


async def test_join_refuses_a_saved_group_and_a_stranger_before_sending_anything(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    kitchen, study = room(hass, "kitchen"), room(hass, "study")
    with pytest.raises(ServiceValidationError) as caught:
        await call(
            hass,
            "join",
            kitchen,
            group_members=[study, saved_group(hass, "downstairs")],
        )
    assert caught.value.translation_domain == DOMAIN
    assert caught.value.translation_key == "join_saved_group"

    hass.states.async_set("media_player.other_brand", "idle")
    with pytest.raises(ServiceValidationError) as caught:
        await call(
            hass, "join", kitchen, group_members=[study, "media_player.other_brand"]
        )
    assert caught.value.translation_key == "join_not_a_room"
    with pytest.raises(ServiceValidationError) as caught:
        await call(
            hass,
            "join",
            kitchen,
            group_members=[hass.states.async_entity_ids("number")[0]],
        )
    assert caught.value.translation_key == "join_not_a_room"
    assert server.bodies == []
    assert "POST /api/command" not in server.requests


async def test_join_of_a_room_the_server_no_longer_has(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    kitchen, study = room(hass, "kitchen"), room(hass, "study")
    coordinator = setup.runtime_data
    raw = json.loads(shared("state-rich.json"))
    raw["zones"] = [z for z in raw["zones"] if z["id"] != "study"]
    with (
        patch.object(coordinator, "sync_devices"),
        pytest.raises(HomeAssistantError) as caught,
    ):
        server.set_state(json.dumps(raw).encode())
        await wait_for(lambda: coordinator.data.zone("study") is None)
        await call(hass, "join", kitchen, group_members=[study])
    assert caught.value.translation_key == "room_gone"
    assert server.bodies == []


async def test_unjoin_takes_the_room_and_the_live_group_dissolves(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    study, bedroom = room(hass, "study"), room(hass, "bedroom")
    await call(hass, "unjoin", bedroom)
    assert server.bodies == [b'{"v":2,"t":"take","target":"bedroom"}']
    await wait_for(lambda: attr(hass, bedroom, ATTR_GROUP_MEMBERS) == [bedroom])
    # live-1 was left with one room and dissolved into the study's own group.
    assert attr(hass, study, ATTR_GROUP_MEMBERS) == [study]
    assert {g["id"]: g["kind"] for g in server.model["groups"]} == {
        "downstairs": "saved",
        "study": "room",
        "bedroom": "room",
    }
    # A room already alone in its own group is left alone: nothing is sent.
    await call(hass, "unjoin", bedroom)
    assert len(server.bodies) == 1


async def test_unjoin_from_a_saved_group_leaves_it_inactive(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    living, kitchen = room(hass, "living"), room(hass, "kitchen")
    downstairs = saved_group(hass, "downstairs")
    await call(hass, "unjoin", kitchen)
    assert server.bodies == [b'{"v":2,"t":"take","target":"kitchen"}']
    await wait_for(lambda: attr(hass, kitchen, ATTR_GROUP_MEMBERS) == [kitchen])
    assert attr(hass, living, ATTR_GROUP_MEMBERS) == [living]
    group = hass.states.get(downstairs)
    assert group is not None
    assert group.state == "off"
    assert group.attributes["active"] is False
    assert ATTR_MEDIA_VOLUME_LEVEL not in group.attributes


# --- volume ------------------------------------------------------------------


async def test_room_volume_is_what_the_server_says_afterwards(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    living, kitchen = room(hass, "living"), room(hass, "kitchen")
    await call(hass, "volume_set", living, volume_level=0.5)
    # The kitchen's limit is 0.400: the server clamps, and the entity shows that.
    await call(hass, "volume_set", kitchen, volume_level=0.9)
    await call(hass, "volume_up", living)
    await call(hass, "volume_down", kitchen)
    await call(hass, "volume_mute", living, is_volume_muted=True)
    assert server.bodies == [
        b'{"v":1,"t":"volume","zone":"living","volume":0.500}',
        b'{"v":1,"t":"volume","zone":"kitchen","volume":0.900}',
        b'{"v":2,"t":"volume_step","zone":"living","step":50}',
        b'{"v":2,"t":"volume_step","zone":"kitchen","step":-50}',
        b'{"v":1,"t":"mute","zone":"living","muted":true}',
    ]
    assert attr(hass, living, ATTR_MEDIA_VOLUME_LEVEL) == 0.55
    assert attr(hass, kitchen, ATTR_MEDIA_VOLUME_LEVEL) == 0.35
    assert attr(hass, living, ATTR_MEDIA_VOLUME_MUTED) is True


async def test_saved_group_volume_is_the_group_volume(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    downstairs = saved_group(hass, "downstairs")
    await call(hass, "volume_set", downstairs, volume_level=0.4)
    assert server.bodies == [
        b'{"v":2,"t":"group_volume","group":"downstairs","volume":0.400}'
    ]
    # 0.857 and 0.343 scaled by 0.4/0.6: the rooms keep their balance.
    assert attr(hass, room(hass, "living"), ATTR_MEDIA_VOLUME_LEVEL) == 0.571
    assert attr(hass, room(hass, "kitchen"), ATTR_MEDIA_VOLUME_LEVEL) == 0.229
    assert attr(hass, downstairs, ATTR_MEDIA_VOLUME_LEVEL) == 0.4
    await call(hass, "volume_up", downstairs)
    await call(hass, "volume_down", downstairs)
    await call(hass, "volume_mute", downstairs, is_volume_muted=True)
    assert server.bodies[1:] == [
        b'{"v":2,"t":"group_volume_step","group":"downstairs","step":50}',
        b'{"v":2,"t":"group_volume_step","group":"downstairs","step":-50}',
        b'{"v":1,"t":"mute","zone":"living","muted":true}',
        b'{"v":1,"t":"mute","zone":"kitchen","muted":true}',
    ]
    assert attr(hass, downstairs, ATTR_MEDIA_VOLUME_MUTED) is True


async def test_a_saved_group_that_is_not_assembled_has_no_volume_to_set(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    downstairs = saved_group(hass, "downstairs")
    await call(hass, "unjoin", room(hass, "kitchen"))
    await wait_for(lambda: hass.states.get(downstairs).state == "off")
    sent = len(server.bodies)
    for service, data in (
        ("volume_set", {"volume_level": 0.4}),
        ("volume_up", {}),
        ("volume_down", {}),
    ):
        with pytest.raises(ServiceValidationError) as caught:
            await call(hass, service, downstairs, **data)
        assert caught.value.translation_key == "group_not_active"
    assert len(server.bodies) == sent


# --- sources, on and off -----------------------------------------------------


async def test_select_source_takes_the_room_or_assembles_the_saved_group(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    kitchen = room(hass, "kitchen")
    downstairs = saved_group(hass, "downstairs")
    await call(hass, "select_source", kitchen, source="endpoint-c/line-1")
    assert server.bodies == [
        b'{"v":2,"t":"take","target":"kitchen","source":"line-in:endpoint-c/line-1"}'
    ]
    await wait_for(lambda: attr(hass, kitchen, ATTR_GROUP_MEMBERS) == [kitchen])
    assert hass.states.get(downstairs).state == "off"
    # Selecting a source on the saved group assembles it again.
    await call(hass, "select_source", downstairs, source="stream")
    assert server.bodies[1] == (
        b'{"v":2,"t":"take","target":"downstairs","source":"stream"}'
    )
    await wait_for(lambda: hass.states.get(downstairs).state == "on")
    assert attr(hass, downstairs, ATTR_INPUT_SOURCE) == "stream"
    assert attr(hass, kitchen, ATTR_GROUP_MEMBERS) == [room(hass, "living"), kitchen]

    with pytest.raises(ServiceValidationError) as caught:
        await call(hass, "select_source", kitchen, source="Tape deck")
    assert caught.value.translation_key == "unknown_source"
    assert len(server.bodies) == 2


async def test_turn_off_and_on(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    study = room(hass, "study")
    downstairs = saved_group(hass, "downstairs")
    await call(hass, "turn_off", study)
    await wait_for(lambda: hass.states.get(study).state == "off")
    assert attr(hass, study, ATTR_INPUT_SOURCE) is None
    await call(hass, "turn_on", study)
    await wait_for(lambda: hass.states.get(study).state == "on")
    await call(hass, "turn_off", downstairs)
    await wait_for(lambda: hass.states.get(downstairs).state == "off")
    assert server.bodies == [
        b'{"v":2,"t":"take","target":"study","source":"none"}',
        b'{"v":2,"t":"take","target":"study","source":"stream"}',
        b'{"v":2,"t":"take","target":"downstairs","source":"none"}',
    ]


async def test_play_media_plays_inputs_only(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    kitchen = room(hass, "kitchen")
    await call(
        hass,
        "play_media",
        kitchen,
        media_content_type="chorus_input",
        media_content_id="chorus://input/endpoint-c/line-1",
    )
    await call(
        hass,
        "play_media",
        saved_group(hass, "downstairs"),
        media_content_type="chorus_input",
        media_content_id="chorus://input/stream",
    )
    assert server.bodies == [
        b'{"v":2,"t":"take","target":"kitchen","source":"line-in:endpoint-c/line-1"}',
        b'{"v":2,"t":"take","target":"downstairs","source":"stream"}',
    ]
    for media_id, key in (
        ("chorus://input/endpoint-z/line-9", "unknown_input"),
        ("https://radio.example/stream.mp3", "unsupported_media"),
        ("media-source://radio_browser/x", "unsupported_media"),
    ):
        with pytest.raises(ServiceValidationError) as caught:
            await call(
                hass,
                "play_media",
                kitchen,
                media_content_type="music",
                media_content_id=media_id,
            )
        assert caught.value.translation_key == key
    assert len(server.bodies) == 2


async def test_browse_media_offers_the_inputs(
    hass: HomeAssistant, setup: MockConfigEntry
) -> None:
    entity = hass.data[MP_DOMAIN].get_entity(room(hass, "kitchen"))
    root = await entity.async_browse_media()
    assert root.media_content_id == "chorus://input"
    assert not root.can_play
    assert [(c.title, c.media_content_id, c.can_play) for c in root.children] == [
        ("stream", "chorus://input/stream", True),
        ("endpoint-c/line-1", "chorus://input/endpoint-c/line-1", True),
    ]
    assert (await entity.async_browse_media("chorus_input", "chorus://input")).children
    with pytest.raises(BrowseError) as caught:
        await entity.async_browse_media("chorus_input", "chorus://input/stream")
    assert caught.value.translation_key == "browse_unknown"


# --- announce ------------------------------------------------------------------


async def test_announce_in_a_room(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry, ha_url: None
) -> None:
    await call(
        hass,
        "play_media",
        room(hass, "kitchen"),
        media_content_type="music",
        media_content_id=CLIP,
        **{ATTR_MEDIA_ANNOUNCE: True},
    )
    assert server.bodies == [local("announce.json")]
    assert server.bodies == [
        b'{"v":2,"t":"announce","target":"kitchen",'
        b'"url":"http://ha.example:8123/api/tts_proxy/abc.mp3"}'
    ]


async def test_announce_in_a_saved_group_with_a_volume(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry, ha_url: None
) -> None:
    await call(
        hass,
        "play_media",
        saved_group(hass, "downstairs"),
        media_content_type="music",
        media_content_id=CLIP,
        **{ATTR_MEDIA_ANNOUNCE: True, ATTR_MEDIA_EXTRA: {"volume": 0.3}},
    )
    assert server.bodies == [local("announce-volume.json")]
    assert server.bodies[0].endswith(b'"volume":0.300}')


async def test_announce_resolves_a_media_source_inside_home_assistant(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry, ha_url: None
) -> None:
    with patch(
        "custom_components.chorus.media_player.media_source.async_resolve_media",
        return_value=PlayMedia(url="/api/tts_proxy/abc.mp3", mime_type="audio/mpeg"),
    ) as resolve:
        await call(
            hass,
            "play_media",
            room(hass, "kitchen"),
            media_content_type="music",
            media_content_id="media-source://tts/demo?message=dinner",
            **{ATTR_MEDIA_ANNOUNCE: True},
        )
    assert resolve.call_args.args[1] == "media-source://tts/demo?message=dinner"
    assert server.bodies == [local("announce.json")]


async def test_announce_signs_a_path_that_needs_auth(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry, ha_url: None
) -> None:
    await call(
        hass,
        "play_media",
        room(hass, "kitchen"),
        media_content_type="music",
        media_content_id="/api/media/clip.mp3",
        **{ATTR_MEDIA_ANNOUNCE: True},
    )
    sent = json.loads(server.bodies[0])
    assert sent["url"].startswith(f"{HA_URL}/api/media/clip.mp3?authSig=")
    assert list(sent) == ["v", "t", "target", "url"]


@pytest.mark.parametrize(
    "url",
    [
        "http://elsewhere.example:8123/api/tts_proxy/abc.mp3",
        "https://ha.example:8123/api/tts_proxy/abc.mp3",
        "http://ha.example:8124/api/tts_proxy/abc.mp3",
        "http://ha.example.evil.example:8123/a.mp3",
        "http://ha.example:8123@evil.example/a.mp3",
        "ftp://ha.example:8123/a.mp3",
        "file:///etc/passwd",
        "http://[::1/a.mp3",
    ],
)
async def test_announce_off_origin_is_refused_before_any_request(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    ha_url: None,
    url: str,
) -> None:
    before = list(server.requests)
    with pytest.raises(ServiceValidationError) as caught:
        await call(
            hass,
            "play_media",
            room(hass, "kitchen"),
            media_content_type="music",
            media_content_id=url,
            **{ATTR_MEDIA_ANNOUNCE: True},
        )
    assert caught.value.translation_domain == DOMAIN
    assert caught.value.translation_key == "announce_origin"
    assert server.bodies == []
    assert server.requests == before


async def test_announce_from_the_external_url_is_home_assistants_own_too(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    await async_process_ha_core_config(
        hass,
        {"internal_url": HA_URL, "external_url": "https://ha-outside.example"},
    )
    server.script(200, shared("state-rich.json"))
    await call(
        hass,
        "play_media",
        room(hass, "kitchen"),
        media_content_type="music",
        media_content_id="https://ha-outside.example/api/tts_proxy/abc.mp3",
        **{ATTR_MEDIA_ANNOUNCE: True},
    )
    assert json.loads(server.bodies[0])["url"] == (
        "https://ha-outside.example/api/tts_proxy/abc.mp3"
    )


async def test_the_servers_url_refusal_is_a_translated_error(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry, ha_url: None
) -> None:
    server.script(400, local("error-announce-origin.json"))
    with pytest.raises(HomeAssistantError) as caught:
        await call(
            hass,
            "play_media",
            room(hass, "kitchen"),
            media_content_type="music",
            media_content_id=CLIP,
            **{ATTR_MEDIA_ANNOUNCE: True},
        )
    assert not isinstance(caught.value, ServiceValidationError)
    assert caught.value.translation_domain == DOMAIN
    assert caught.value.translation_key == "refused_url"
    placeholders = caught.value.translation_placeholders
    assert placeholders is not None
    assert placeholders["field"] == "url"
    assert server.bodies == [local("announce.json")]


@pytest.mark.parametrize("volume", [1.5, -0.1, "loud", True])
async def test_an_announce_volume_outside_the_range_is_refused(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    ha_url: None,
    volume: object,
) -> None:
    with pytest.raises(ServiceValidationError) as caught:
        await call(
            hass,
            "play_media",
            room(hass, "kitchen"),
            media_content_type="music",
            media_content_id=CLIP,
            **{ATTR_MEDIA_ANNOUNCE: True, ATTR_MEDIA_EXTRA: {"volume": volume}},
        )
    assert caught.value.translation_key == "announce_volume"
    assert server.bodies == []


# --- transport and refusals ----------------------------------------------------


async def test_transport_only_while_the_group_plays_a_spotify_receiver(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    raw = json.loads(shared("state-soloist.json"))
    raw["groups"][0]["source"] = "soloist:r0"
    raw["groups"][0]["now_playing"] = {
        "title": "A song",
        "artist": None,
        "album": None,
        "art_url": None,
        "duration_ms": 1000,
        "state": "playing",
        "via": "spotify",
    }
    server.set_state(json.dumps(raw).encode())
    await wait_for(lambda: len(hass.states.async_entity_ids(MP_DOMAIN)) == 2)
    kitchen, den = room(hass, "kitchen"), room(hass, "den")
    await wait_for(lambda: hass.states.get(kitchen).state == "playing")
    transport = (
        MediaPlayerEntityFeature.PAUSE
        | MediaPlayerEntityFeature.PLAY
        | MediaPlayerEntityFeature.NEXT_TRACK
        | MediaPlayerEntityFeature.PREVIOUS_TRACK
    )
    assert attr(hass, kitchen, ATTR_SUPPORTED_FEATURES) & transport == transport
    assert attr(hass, kitchen, ATTR_INPUT_SOURCE) == "soloist:r0"
    assert not attr(hass, den, ATTR_SUPPORTED_FEATURES) & transport
    for service in (
        "media_pause",
        "media_play",
        "media_next_track",
        "media_previous_track",
    ):
        await call(hass, service, kitchen)
    assert server.bodies == [
        b'{"v":2,"t":"playback","target":"kitchen","action":"pause"}',
        b'{"v":2,"t":"playback","target":"kitchen","action":"resume"}',
        b'{"v":2,"t":"playback","target":"kitchen","action":"next"}',
        b'{"v":2,"t":"playback","target":"kitchen","action":"previous"}',
    ]
    with pytest.raises(ServiceNotSupported):
        await call(hass, "media_pause", den)
    assert len(server.bodies) == 4


@pytest.mark.parametrize(
    ("vector", "key", "field"),
    [
        ("error-take-unknown-target.json", "refused_target", "target"),
        ("error-source.json", "refused_source", "source"),
        ("error-group-volume-unknown-group.json", "refused_group", "group"),
        ("error-unknown-zone.json", "refused_zone", "zone"),
        ("error-volume-out-of-range.json", "refused_volume", "volume"),
        ("error-malformed.json", "refused_command", ""),
    ],
)
async def test_a_refusal_is_translated_by_its_field(
    hass: HomeAssistant,
    server: FakeChorusServer,
    setup: MockConfigEntry,
    vector: str,
    key: str,
    field: str,
) -> None:
    refusal = shared(vector)
    assert json.loads(refusal)["field"] == field
    server.script(400, refusal)
    with pytest.raises(HomeAssistantError) as caught:
        await call(hass, "turn_on", room(hass, "kitchen"))
    assert caught.value.translation_domain == DOMAIN
    assert caught.value.translation_key == key
    placeholders = caught.value.translation_placeholders
    assert placeholders is not None
    assert placeholders["detail"] == json.loads(refusal)["detail"]


async def test_a_server_that_does_not_answer_a_command(
    hass: HomeAssistant, server: FakeChorusServer, setup: MockConfigEntry
) -> None:
    server.script(503, b"every worker is busy")
    with pytest.raises(HomeAssistantError) as caught:
        await call(hass, "turn_on", room(hass, "kitchen"))
    assert caught.value.translation_key == "cannot_connect"
