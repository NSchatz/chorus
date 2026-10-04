"""Speaker devices and their firmware update entities.

Nothing installs without Home Assistant's install action (K93, I13): the
first test here mirrors the server's own
`nothing_installs_until_the_explicit_install_action`
(`crates/server/tests/firmware_install.rs`) from the integration's side.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from homeassistant.config_entries import ConfigEntryState
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers import device_registry as dr, entity_registry as er
import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.chorus.const import DOMAIN

from .conftest import SERVER_ID, speaker_firmware, wait_for
from .fake_server import FakeChorusServer, shared

FIRST = "chorus-0123456789ab"
SECOND = "chorus-ba9876543210"
STRINGS = Path(__file__).parents[1] / "custom_components" / "chorus" / "strings.json"


def house(**first: Any) -> dict[str, Any]:
    """The shared firmware state, its first speaker idle with an update available.

    The vector's first speaker was just asked to install; these tests start a
    step earlier, from the speaker the vector's own `report.0` line describes.
    """
    raw = json.loads(shared("state-firmware.json"))
    raw["speakers"][0]["firmware"] |= {
        "state": "idle",
        "image": None,
        "image_version": "",
        "received": 0,
        "size": 0,
    } | first
    return raw


def encode(raw: dict[str, Any]) -> bytes:
    return json.dumps(raw, separators=(",", ":"), ensure_ascii=False).encode()


async def start(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    raw: dict[str, Any],
) -> None:
    """Set the entry up against a server that says `raw`."""
    server.state_bytes = encode(raw)
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    await wait_for(lambda: server.subscribers == 1)


async def install(hass: HomeAssistant, entity_id: str) -> None:
    await hass.services.async_call(
        "update", "install", {"entity_id": entity_id}, blocking=True
    )


def attributes(hass: HomeAssistant, entity_id: str) -> dict[str, Any]:
    state = hass.states.get(entity_id)
    assert state is not None
    return dict(state.attributes)


def device(hass: HomeAssistant, entry: MockConfigEntry, identifier: str):
    return dr.async_get(hass).async_get_device_by_identifier(
        (DOMAIN, identifier), config_entry_id=entry.entry_id
    )


async def test_firmware_nothing_installs_until_the_explicit_install_action(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        "custom_components.chorus._aiochorus.client.BACKOFF_FIRST", 0.01
    )
    # Set up with an update available: a verified image of another version.
    await start(hass, entry, server, house())
    first = speaker_firmware(hass, FIRST)
    assert hass.states.get(first).state == "on"
    assert attributes(hass, first)["installed_version"] == "1.0.0"
    assert attributes(hass, first)["latest_version"] == "2.0.0"
    assert hass.states.get(speaker_firmware(hass, SECOND)).state == "on"

    # Reload the entry.
    await hass.config_entries.async_reload(entry.entry_id)
    await hass.async_block_till_done()
    assert entry.state is ConfigEntryState.LOADED
    await wait_for(lambda: server.subscribers == 1)
    assert hass.states.get(first).state == "on"

    # Lose the stream and get it back.
    server.refuse_connections = True
    server.drop_streams()
    await wait_for(lambda: hass.states.get(first).state == "unavailable")
    server.refuse_connections = False
    await wait_for(lambda: hass.states.get(first).state == "on")
    assert server.subscribers == 1

    # The state changes: the speaker drops and comes back, an image is staged.
    raw = house()
    raw["speakers"][0]["present"] = False
    server.set_state(encode(raw))
    await wait_for(lambda: not entry.runtime_data.data.speakers[0].present)
    raw = house()
    raw["firmware"]["images"].append(
        raw["firmware"]["images"][0] | {"name": "brick-2-1-0", "version": "2.1.0"}
    )
    server.set_state(encode(raw))
    await wait_for(lambda: attributes(hass, first)["latest_version"] == "2.1.0")
    await hass.async_block_till_done()

    # Through all of it the server was sent no command at all.
    assert server.bodies == []
    assert "POST /api/command" not in server.requests
    assert not attributes(hass, first)["in_progress"]


async def test_firmware_install_action_sends_exactly_one_firmware_install(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    await start(hass, entry, server, house())
    first = speaker_firmware(hass, FIRST)
    assert server.bodies == []

    await install(hass, first)
    # One command, naming that speaker and the verified image: the vector's bytes.
    assert server.bodies == [shared("firmware_install.json")]
    assert json.loads(server.bodies[0]) == {
        "v": 2,
        "t": "firmware_install",
        "speaker": FIRST,
        "image": "brick-2-0-0",
    }
    await wait_for(lambda: attributes(hass, first)["in_progress"])
    assert attributes(hass, first)["install_state"] == "requested"
    assert attributes(hass, first)["image"] == "brick-2-0-0"
    # The other speaker was not named and nothing else was sent.
    assert not attributes(hass, speaker_firmware(hass, SECOND))["in_progress"]

    # While it installs, Home Assistant sends nothing more for that speaker.
    with pytest.raises(HomeAssistantError):
        await install(hass, first)
    assert server.bodies == [shared("firmware_install.json")]


async def test_firmware_refusals_are_translated_errors(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    await start(hass, entry, server, house())
    first = speaker_firmware(hass, FIRST)
    messages = json.loads(STRINGS.read_bytes())["exceptions"]

    # The speaker is a real device somewhere else and the owner is not at the bench.
    server.remote_speakers.add(FIRST)
    with pytest.raises(HomeAssistantError) as caught:
        await install(hass, first)
    assert caught.value.translation_domain == DOMAIN
    assert caught.value.translation_key == "firmware_owner_not_at_bench"
    assert "docs/firmware-updates.md" in messages["firmware_owner_not_at_bench"]["message"]
    assert "docs/firmware-updates.md" in str(caught.value)
    server.remote_speakers.clear()

    # The server's own refusals, byte for byte from the shared vectors.
    for vector, key in (
        ("error-firmware-install-busy.json", "firmware_busy"),
        ("error-firmware-install-not-verified.json", "firmware_image_not_verified"),
    ):
        server.script(400, shared(vector))
        with pytest.raises(HomeAssistantError) as caught:
            await install(hass, first)
        assert caught.value.translation_domain == DOMAIN
        assert caught.value.translation_key == key
        assert key in messages
        detail = json.loads(shared(vector))["detail"]
        assert caught.value.translation_placeholders["detail"] == detail
        assert detail in str(caught.value)

    # Any other refusal is mapped by the field it names.
    for vector, key in (
        ("error-firmware-install-absent.json", "refused_speaker"),
        ("error-firmware-install-unknown-image.json", "refused_image"),
    ):
        server.script(400, shared(vector))
        with pytest.raises(HomeAssistantError) as caught:
            await install(hass, first)
        assert caught.value.translation_key == key
        assert key in messages

    # Each attempt was one explicit install, and none of them started anything.
    assert server.bodies == [shared("firmware_install.json")] * 5
    assert not attributes(hass, first)["in_progress"]
    assert attributes(hass, first)["install_state"] == "idle"
    assert hass.states.get(first).state == "on"


@pytest.mark.parametrize(
    "verdict",
    [{"verdict": "refused", "reason": "digest-mismatch"}, {"verdict": "staged"}, {}],
    ids=["refused", "unknown-verdict", "no-verdict"],
)
async def test_firmware_refused_or_unverified_image_is_never_the_latest_version(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    verdict: dict[str, str],
) -> None:
    # The only image of another version for the board is not a verified one
    # (and brick-tampered, refused in the vector, is there too), while the
    # speaker's state still says an update is available.
    raw = house()
    images = raw["firmware"]["images"]
    good = images[0]
    assert (good["name"], good["verdict"]) == ("brick-2-0-0", "verified")
    del good["verdict"]
    good |= verdict
    assert images[1]["verdict"] == "refused"
    await start(hass, entry, server, raw)
    first = speaker_firmware(hass, FIRST)

    assert hass.states.get(first).state == "off"
    assert attributes(hass, first)["installed_version"] == "1.0.0"
    assert attributes(hass, first)["latest_version"] == "1.0.0"

    # Home Assistant's install action has nothing to install and sends nothing.
    with pytest.raises(HomeAssistantError):
        await install(hass, first)
    entity = hass.data["update"].get_entity(first)
    with pytest.raises(HomeAssistantError) as caught:
        await entity.async_install(None, False)
    assert caught.value.translation_key == "firmware_nothing_to_install"
    assert server.bodies == []

    # Verified after a rescan: now, and only now, it is the latest version.
    good.pop("reason", None)
    good["verdict"] = "verified"
    server.set_state(encode(raw))
    await wait_for(lambda: hass.states.get(first).state == "on")
    assert attributes(hass, first)["latest_version"] == "2.0.0"
    assert server.bodies == []


async def test_firmware_entity_absent_until_reported_and_progress_follows_received(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    # Two adopted speakers, neither has reported what it runs.
    await start(hass, entry, server, json.loads(shared("state-speakers.json")))
    assert hass.states.async_entity_ids("update") == []
    registry = er.async_get(hass)
    unique = f"{SERVER_ID}:speaker:{FIRST}:firmware"
    assert registry.async_get_entity_id("update", DOMAIN, unique) is None
    # Each is a device all the same.
    assert device(hass, entry, f"{SERVER_ID}:speaker:{FIRST}") is not None
    assert device(hass, entry, f"{SERVER_ID}:speaker:{SECOND}") is not None

    # The first reports; the second still has not, and has no entity.
    raw = house()
    del raw["speakers"][1]["firmware"]
    server.set_state(encode(raw))
    await wait_for(lambda: len(hass.states.async_entity_ids("update")) == 1)
    first = speaker_firmware(hass, FIRST)
    assert hass.states.async_entity_ids("update") == [first]
    assert hass.states.get(first).state == "on"
    assert not attributes(hass, first)["in_progress"]
    assert attributes(hass, first)["update_percentage"] is None

    async def report(**firmware: Any) -> dict[str, Any]:
        raw["speakers"][0]["firmware"] |= firmware
        server.set_state(encode(raw))
        want = raw["speakers"][0]["firmware"]
        await wait_for(
            lambda: (
                (now := entry.runtime_data.data.speaker(FIRST).firmware).state
                == want["state"]
                and now.received == want["received"]
            )
        )
        await hass.async_block_till_done()
        return attributes(hass, first)

    size = 1536000
    installing = {"image": "brick-2-0-0", "image_version": "2.0.0", "size": size}
    now = await report(state="requested", received=0, **installing)
    assert (now["in_progress"], now["update_percentage"]) == (True, 0)
    for received, percent in ((384000, 25), (768000, 50), (1535999, 99), (size, 100)):
        now = await report(state="receiving", received=received)
        assert (now["in_progress"], now["update_percentage"]) == (True, percent)
        assert now["install_state"] == "receiving"
    # Written and verified, then running on trial: in progress, nothing to count.
    for state in ("verified", "pending_verify"):
        now = await report(state=state, received=size)
        assert (now["in_progress"], now["update_percentage"]) == (True, None)
        assert now["install_state"] == state

    # Confirmed: the speaker runs the image and the outcome stays shown.
    now = await report(state="confirmed", version="2.0.0", slot=1)
    assert hass.states.get(first).state == "off"
    assert (now["in_progress"], now["update_percentage"]) == (False, None)
    # The older image still staged (brick-1-0-0) is not offered as an update,
    # though the server says one is available: going back is not offered here.
    assert raw["speakers"][0]["firmware"]["update_available"]
    assert (now["installed_version"], now["latest_version"]) == ("2.0.0", "2.0.0")
    assert now["install_state"] == "confirmed"
    speaker = device(hass, entry, f"{SERVER_ID}:speaker:{FIRST}")
    assert speaker.sw_version == "2.0.0"
    # Nothing in any of this was sent by Home Assistant.
    assert server.bodies == []


async def test_firmware_reported_again_after_a_server_restart_adds_no_second_entity(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    caplog: pytest.LogCaptureFixture,
) -> None:
    raw = house()
    await start(hass, entry, server, raw)
    first = speaker_firmware(hass, FIRST)
    assert hass.states.get(first).state == "on"
    count = len(hass.states.async_entity_ids("update"))

    # The server restarted: the speaker is still adopted, but what it runs is
    # not known until its session reports again. Its entity stays, unavailable.
    silent = house()
    del silent["speakers"][0]["firmware"]
    server.set_state(encode(silent))
    await wait_for(lambda: hass.states.get(first).state == "unavailable")
    assert len(hass.states.async_entity_ids("update")) == count

    # It reports again: the same entity comes back, and no second one is built.
    server.set_state(encode(raw))
    await wait_for(lambda: hass.states.get(first).state == "on")
    await hass.async_block_till_done()
    assert len(hass.states.async_entity_ids("update")) == count
    assert speaker_firmware(hass, FIRST) == first
    assert "does not generate unique IDs" not in caplog.text
    assert not [r for r in caplog.records if r.levelname == "ERROR"]
    assert server.bodies == []


async def test_firmware_outcomes_are_shown(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    # The vector as it is: one speaker just asked to install, one rolled back.
    await start(hass, entry, server, json.loads(shared("state-firmware.json")))
    first = attributes(hass, speaker_firmware(hass, FIRST))
    assert first["in_progress"]
    assert first["install_state"] == "requested"
    second = speaker_firmware(hass, SECOND)
    back = attributes(hass, second)
    assert back["install_state"] == "rolled_back"
    assert back["reason"] == "not_confirmed"
    assert back["image_version"] == "3.0.0"
    assert back["image"] is None
    assert back["board"] == "brick-s3-wired"
    assert not back["in_progress"]
    # The image is still staged, so the update is still available; nothing
    # retries it.
    assert hass.states.get(second).state == "on"
    assert back["installed_version"] == "1.0.0"
    assert back["title"] == "chorus speaker firmware"

    raw = json.loads(shared("state-firmware.json"))
    raw["speakers"][1]["firmware"] |= {"state": "refused", "reason": "bad_digest"}
    server.set_state(encode(raw))
    await wait_for(lambda: attributes(hass, second)["install_state"] == "refused")
    assert attributes(hass, second)["reason"] == "bad_digest"
    assert not attributes(hass, second)["in_progress"]
    assert server.bodies == []

    # Every state the server can say has a translated name.
    names = json.loads(STRINGS.read_bytes())["entity"]["update"]["firmware"]
    assert set(names["state_attributes"]["install_state"]["state"]) == {
        "idle",
        "requested",
        "receiving",
        "verified",
        "pending_verify",
        "confirmed",
        "rolled_back",
        "refused",
        "interrupted",
        "cancelled",
    }


async def test_firmware_speaker_devices_follow_the_state(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    raw = house()
    raw["speakers"][0] |= {"name": "Kitchen left", "named": True, "room": "kitchen"}
    await start(hass, entry, server, raw)
    hub = device(hass, entry, SERVER_ID)
    kitchen = device(hass, entry, f"{SERVER_ID}:room:kitchen")
    left = device(hass, entry, f"{SERVER_ID}:speaker:{FIRST}")
    other = device(hass, entry, f"{SERVER_ID}:speaker:{SECOND}")
    # A speaker with a room is linked to its room's device; one without is
    # under the server.
    assert kitchen.via_device_id == hub.id
    assert left.via_device_id == kitchen.id
    assert left.name == "Kitchen left"
    assert left.suggested_area == "kitchen"
    assert (left.manufacturer, left.model, left.model_id) == (
        "chorus",
        "Speaker",
        "brick-s3-wired",
    )
    assert left.sw_version == "1.0.0"
    assert other.via_device_id == hub.id
    assert other.name == "Speaker 3210"
    assert other.suggested_area is None
    entities = er.async_get(hass)
    first = speaker_firmware(hass, FIRST)
    assert entities.async_get(first).device_id == left.id
    assert hass.states.get(first).name == "Kitchen left Firmware"

    # Renamed and moved to another room: the device follows.
    raw["speakers"][0] |= {"name": "Sofa", "room": "living"}
    server.set_state(encode(raw))
    await wait_for(
        lambda: device(hass, entry, f"{SERVER_ID}:speaker:{FIRST}").name == "Sofa"
    )
    living = device(hass, entry, f"{SERVER_ID}:room:living")
    assert (
        device(hass, entry, f"{SERVER_ID}:speaker:{FIRST}").via_device_id == living.id
    )

    # Unassigned: back under the server. Its firmware gone: the entity is
    # unavailable, and nothing can be installed through it.
    raw["speakers"][0]["room"] = None
    del raw["speakers"][0]["firmware"]
    server.set_state(encode(raw))
    await wait_for(lambda: hass.states.get(first).state == "unavailable")
    assert device(hass, entry, f"{SERVER_ID}:speaker:{FIRST}").via_device_id == hub.id

    # Forgotten: the device and its entity go; the other speaker stays.
    del raw["speakers"][0]
    server.set_state(encode(raw))
    await wait_for(lambda: hass.states.get(first) is None)
    await hass.async_block_till_done()
    assert device(hass, entry, f"{SERVER_ID}:speaker:{FIRST}") is None
    assert entities.async_get(first) is None
    assert device(hass, entry, f"{SERVER_ID}:speaker:{SECOND}") is not None

    # Adopted again and reporting: a new device and a new entity.
    server.set_state(encode(house()))
    await wait_for(lambda: len(hass.states.async_entity_ids("update")) == 2)
    assert hass.states.get(speaker_firmware(hass, FIRST)).state == "on"
    assert server.bodies == []
