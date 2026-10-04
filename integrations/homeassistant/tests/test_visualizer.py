"""A room's visualizer sensor: frames in, a colour for a light out, at a bounded rate.

The frame is the repository's shared vector (`fixtures/visualizer/http-frame.json`),
sent by the fake server to the subscribers of the den's `GET /api/visualizer`.
The fake holds no rate cap of its own: it sends whatever a test makes, which is
what the entity's own cap is tested against.
"""

from __future__ import annotations

import asyncio
from collections.abc import Callable
from datetime import timedelta
from itertools import pairwise
import json
import logging
from pathlib import Path
import re
from typing import Any

from freezegun.api import FrozenDateTimeFactory, real_monotonic
from homeassistant.const import MATCH_ALL, STATE_UNAVAILABLE
from homeassistant.core import HomeAssistant
from homeassistant.helpers import device_registry as dr, entity_registry as er
from homeassistant.setup import async_setup_component
from homeassistant.util.yaml import parse_yaml
import pytest
from pytest_homeassistant_custom_component.common import (
    MockConfigEntry,
    async_fire_time_changed_exact,
    async_mock_service,
)

from custom_components.chorus.const import (
    DOMAIN,
    VISUALIZER_IDLE_AFTER,
    VISUALIZER_MIN_WRITE_INTERVAL,
)
from custom_components.chorus.visualizer import ChorusVisualizerSensor

from .conftest import SERVER_ID, room, wait_for
from .fake_server import REPO, FakeChorusServer

FRAME = (REPO / "fixtures" / "visualizer" / "http-frame.json").read_bytes().strip()
README = Path(__file__).resolve().parents[1] / "README.md"
ROOMS = ("bedroom", "den", "living", "study")
LOG = "custom_components.chorus.visualizer"
CAP = VISUALIZER_MIN_WRITE_INTERVAL

# What the entity says with nothing to show.
IDLE = {
    "state": "0",
    "rgb_color": (0, 0, 0),
    "brightness": 0,
    "beat": 0,
    "transition": 0.0,
    "lead_ms": 0,
}
# The shared frame: a kick heard in the den.
KICK = {
    "state": "79",  # the peak byte 201 of 255
    "rgb_color": (255, 96, 0),
    "brightness": 180,
    "beat": 255,
    "transition": 0.5,
    "lead_ms": 85,
}


def frame(**members: Any) -> bytes:
    """The shared frame, as it is or with some members changed."""
    if not members:
        return FRAME
    return json.dumps(json.loads(FRAME) | members, separators=(",", ":")).encode()


def visualizer(hass: HomeAssistant, zone: str = "den") -> str:
    entity_id = er.async_get(hass).async_get_entity_id(
        "sensor", DOMAIN, f"{SERVER_ID}:room:{zone}:visualizer"
    )
    assert entity_id is not None, zone
    return entity_id


def says(hass: HomeAssistant, zone: str = "den") -> dict[str, Any]:
    """What the entity says now: its state and the attributes a light takes."""
    state = hass.states.get(visualizer(hass, zone))
    assert state is not None
    if state.state == STATE_UNAVAILABLE:
        return {"state": STATE_UNAVAILABLE}
    return {"state": state.state} | {
        key: state.attributes[key] for key in IDLE if key != "state"
    }


async def real_wait(condition: Callable[[], bool]) -> None:
    """Wait until a condition holds, without a timer.

    Under the `freezer` fixture the loop's clock stands still between ticks, so
    a sleep never ends: this yields to the loop (which still serves the fake
    server's sockets) and bounds the wait on the real clock.
    """
    deadline = real_monotonic() + 5.0
    while not condition():
        assert real_monotonic() < deadline, "the condition did not come to hold"
        await asyncio.sleep(0)


async def start(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    enabled: tuple[str, ...] = ("den",),
    wait: Callable[..., Any] = wait_for,
) -> None:
    """Set the entry up with the visualizer of these rooms enabled.

    The shared frame is the den's, so the fake's kitchen is the den here. An
    entity is enabled as an owner enables it: in the registry, then a reload.
    """
    server.state_bytes = server.state_bytes.replace(b'"kitchen"', b'"den"')
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    await wait(lambda: server.subscribers == 1)
    if not enabled:
        return
    registry = er.async_get(hass)
    for zone in enabled:
        registry.async_update_entity(visualizer(hass, zone), disabled_by=None)
    await hass.config_entries.async_reload(entry.entry_id)
    await hass.async_block_till_done()
    await wait(lambda: server.visualizer_subscribers == sorted(enabled))


class Recorder:
    """Every frame the entity took and every state write it made, with its time.

    The time is the loop's monotonic clock, which is what the entity's cap is
    taken on. A write is a call of `async_write_ha_state`, whether or not the
    state it wrote differs from the one before.
    """

    def __init__(self, hass: HomeAssistant, monkeypatch: pytest.MonkeyPatch) -> None:
        self.hass = hass
        self.frames = 0
        self.writes: list[float] = []
        handle = ChorusVisualizerSensor._handle_frame
        write = ChorusVisualizerSensor.async_write_ha_state
        recorder = self

        def handled(entity: ChorusVisualizerSensor, taken: Any) -> None:
            handle(entity, taken)
            recorder.frames += 1

        def written(entity: ChorusVisualizerSensor) -> None:
            recorder.writes.append(hass.loop.time())
            write(entity)

        monkeypatch.setattr(ChorusVisualizerSensor, "_handle_frame", handled)
        monkeypatch.setattr(ChorusVisualizerSensor, "async_write_ha_state", written)


class SimulatedTime:
    """Simulated time, with frames sent through a real socket in between."""

    def __init__(
        self,
        hass: HomeAssistant,
        freezer: FrozenDateTimeFactory,
        server: FakeChorusServer,
        recorder: Recorder,
    ) -> None:
        self.hass = hass
        self.freezer = freezer
        self.server = server
        self.recorder = recorder

    async def send(self, *frames: bytes) -> None:
        """Send frames at this instant and wait until the entity took them all."""
        want = self.recorder.frames + len(frames)
        for one in frames:
            self.server.frame(one)
        await real_wait(lambda: self.recorder.frames == want)

    async def advance(self, seconds: float) -> None:
        """Move simulated time on and let the timers it fires finish."""
        self.freezer.tick(timedelta(seconds=seconds))
        # The exact form: the plain one fires every timer due within the next
        # half second, which would run the entity's own timers early.
        async_fire_time_changed_exact(self.hass)
        for _ in range(5):
            await asyncio.sleep(0)
        await self.hass.async_block_till_done()


@pytest.fixture
def recorder(hass: HomeAssistant, monkeypatch: pytest.MonkeyPatch) -> Recorder:
    return Recorder(hass, monkeypatch)


@pytest.fixture
def clock(
    hass: HomeAssistant,
    freezer: FrozenDateTimeFactory,
    server: FakeChorusServer,
    recorder: Recorder,
) -> SimulatedTime:
    return SimulatedTime(hass, freezer, server, recorder)


async def test_visualizer_frame_becomes_a_state_with_a_colour_for_a_light(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    await start(hass, entry, server)
    # Attached and nothing playing: the idle value.
    await wait_for(lambda: says(hass) == IDLE)

    # The shared vector's bytes, as the server sends them.
    server.frame(FRAME)
    await wait_for(lambda: says(hass) == KICK)

    state = hass.states.get(visualizer(hass))
    assert state.attributes["unit_of_measurement"] == "%"
    # On the room's device, beside its media player.
    registry = er.async_get(hass)
    device = dr.async_get(hass).async_get_device_by_identifier(
        (DOMAIN, f"{SERVER_ID}:room:den"), config_entry_id=entry.entry_id
    )
    assert device is not None
    assert registry.async_get(visualizer(hass)).device_id == device.id
    assert registry.async_get(visualizer(hass)).entity_category is None

    # The colour is what `light.turn_on` takes, as it is.
    calls = async_mock_service(hass, "light", "turn_on")
    await hass.services.async_call(
        "light",
        "turn_on",
        {
            "entity_id": "light.den_lamp",
            "rgb_color": state.attributes["rgb_color"],
            "brightness": state.attributes["brightness"],
            "transition": state.attributes["transition"],
        },
        blocking=True,
    )
    assert tuple(calls[0].data["rgb_color"]) == (255, 96, 0)

    # The next frame: the same colour, a lower level, no beat.
    server.frame(frame(peak=128, beat=0, lead_ms=40))
    await wait_for(lambda: says(hass)["state"] == "50")
    assert says(hass) == KICK | {"state": "50", "beat": 0, "lead_ms": 40}


async def test_visualizer_keeps_out_of_long_term_history(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    """No state class (so no long-term statistics) and no recorded attribute."""
    await start(hass, entry, server)
    server.frame(FRAME)
    await wait_for(lambda: says(hass) == KICK)
    state = hass.states.get(visualizer(hass))
    assert "state_class" not in state.attributes
    assert er.async_get(hass).async_get(visualizer(hass)).capabilities is None
    # The recorder drops every attribute of a state whose entity says so
    # (`homeassistant.components.recorder.db_schema`, `MATCH_ALL`).
    assert MATCH_ALL in state.state_info["unrecorded_attributes"]


async def test_visualizer_disabled_holds_no_stream_open(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    await start(hass, entry, server, enabled=())
    registry = er.async_get(hass)
    # One per room, every one disabled by the integration and never added.
    for zone in ROOMS:
        entity = registry.async_get(visualizer(hass, zone))
        assert entity.disabled_by is er.RegistryEntryDisabler.INTEGRATION
        assert hass.states.get(entity.entity_id) is None
    # Give a stream that should not be opened the time to be opened.
    await asyncio.sleep(0.2)
    await hass.async_block_till_done()
    assert server.visualizer_subscribers == []
    assert "GET /api/visualizer" not in server.requests
    # The rest of the room is there.
    assert hass.states.get(room(hass, "den")).state != STATE_UNAVAILABLE

    # Enabled for one room: that room's stream, and no other room's.
    registry.async_update_entity(visualizer(hass, "den"), disabled_by=None)
    await hass.config_entries.async_reload(entry.entry_id)
    await hass.async_block_till_done()
    await wait_for(lambda: server.visualizer_subscribers == ["den"])
    await wait_for(lambda: says(hass) == IDLE)
    assert server.requests.count("GET /api/visualizer") == 1

    # Disabled again: the stream is closed and stays closed.
    registry.async_update_entity(
        visualizer(hass, "den"), disabled_by=er.RegistryEntryDisabler.USER
    )
    await hass.config_entries.async_reload(entry.entry_id)
    await hass.async_block_till_done()
    await wait_for(lambda: server.visualizer_subscribers == [])
    await asyncio.sleep(0.2)
    assert server.visualizer_subscribers == []
    assert server.requests.count("GET /api/visualizer") == 1

    # Unloading the entry closes an enabled entity's stream too.
    registry.async_update_entity(visualizer(hass, "study"), disabled_by=None)
    await hass.config_entries.async_reload(entry.entry_id)
    await wait_for(lambda: server.visualizer_subscribers == ["study"])
    assert await hass.config_entries.async_unload(entry.entry_id)
    await wait_for(lambda: server.visualizer_subscribers == [])


async def test_visualizer_state_writes_never_exceed_the_cap_whatever_the_frame_rate(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    recorder: Recorder,
    clock: SimulatedTime,
) -> None:
    await start(hass, entry, server, wait=real_wait)
    # The first write (attached, idle) is held by the cap like any other; the
    # second step leaves the cap's interval behind it.
    await clock.advance(1.0)
    await clock.advance(1.0)
    assert says(hass) == IDLE
    recorder.writes.clear()
    sent = 0

    def another() -> bytes:
        # Every frame differs from the one before, so every write would show.
        nonlocal sent
        sent += 1
        return frame(peak=1 + sent % 255, beat=0, lead_ms=sent)

    began = hass.loop.time()
    # 100 frames a second, ten times the server's own cap, for 3 s.
    for _ in range(300):
        await clock.send(another())
        await clock.advance(0.01)
    fast = len(recorder.writes)
    # Bursts: 20 frames at one instant, every 50 ms, for 2 s.
    for _ in range(40):
        await clock.send(*(another() for _ in range(20)))
        await clock.advance(0.05)
    # The server's rate: one every 100 ms, for 2 s.
    for _ in range(20):
        await clock.send(another())
        await clock.advance(0.1)
    elapsed = hass.loop.time() - began
    assert elapsed == pytest.approx(7.0)

    writes = recorder.writes
    gaps = [later - earlier for earlier, later in pairwise(writes)]
    assert min(gaps) >= CAP - 1e-6, min(gaps)
    assert len(writes) <= elapsed / CAP + 1
    # The cap holds something back, and it does not starve the entity either.
    assert sent == 300 + 800 + 20
    assert 3.0 / CAP - 3 <= fast <= 3.0 / CAP + 1
    assert len(writes) >= elapsed / CAP - 12
    # What is shown at the end is the latest frame, never a backlog.
    await clock.advance(CAP)
    assert says(hass)["lead_ms"] == sent

    # Slower than the cap: every frame is its own write, at once.
    recorder.writes.clear()
    for _ in range(4):
        await clock.advance(0.5)
        await clock.send(another())
        assert says(hass)["lead_ms"] == sent
    assert len(recorder.writes) == 4


async def test_visualizer_beat_held_back_by_the_cap_rides_in_the_next_write(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    recorder: Recorder,
    clock: SimulatedTime,
) -> None:
    await start(hass, entry, server, wait=real_wait)
    # The first write (attached, idle) is held by the cap like any other; the
    # second step leaves the cap's interval behind it.
    await clock.advance(1.0)
    await clock.advance(1.0)
    await clock.send(frame(peak=90, beat=0))
    assert says(hass)["state"] == "35"
    # Inside the interval: a beat, then a frame without one that supersedes it.
    await clock.advance(0.05)
    await clock.send(frame(peak=201, beat=200))
    await clock.send(frame(peak=150, beat=0, lead_ms=7))
    assert says(hass)["state"] == "35"
    await clock.advance(CAP)
    # The latest frame's level, and the beat nothing had shown yet.
    assert says(hass) == KICK | {"state": "59", "beat": 200, "lead_ms": 7}
    # Shown once: the next frame carries none. The step goes past the cap's
    # interval, not onto its edge: there the frozen clock's float rounding
    # (a fraction of a microsecond) decides whether this write is held back.
    await clock.advance(CAP + 0.01)
    await clock.send(frame(peak=149, beat=0, lead_ms=7))
    assert says(hass)["beat"] == 0


async def test_visualizer_silence_leaves_the_idle_value(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    recorder: Recorder,
    clock: SimulatedTime,
) -> None:
    await start(hass, entry, server, wait=real_wait)
    # The first write (attached, idle) is held by the cap like any other; the
    # second step leaves the cap's interval behind it.
    await clock.advance(1.0)
    await clock.advance(1.0)
    assert says(hass) == IDLE

    # The server sends the first frame of a run of silence and then nothing.
    # It still carries the colour in force: the entity says idle all the same.
    await clock.send(FRAME)
    assert says(hass) == KICK
    await clock.advance(1.0)
    await clock.send(frame(peak=0, beat=0))
    assert says(hass) == IDLE
    # (Ten seconds, not more: enabling the entity in the registry leaves Home
    # Assistant's own reload of the entry due 30 s later.)
    await clock.advance(10.0)
    assert says(hass) == IDLE

    # A room that is just sent nothing more (its group was given no source,
    # so no silent frame comes): idle once the documented time has passed.
    await clock.send(FRAME)
    assert says(hass) == KICK
    await clock.advance(VISUALIZER_IDLE_AFTER - 0.1)
    assert says(hass) == KICK
    await clock.advance(0.2)
    assert says(hass) == IDLE
    # And frames while it plays keep it from idling.
    for n in range(30):
        await clock.send(frame(lead_ms=n))
        await clock.advance(0.1)
    assert says(hass)["state"] == KICK["state"]


async def test_visualizer_stream_lost_is_unavailable_then_idle_and_logged_once(
    hass: HomeAssistant,
    entry: MockConfigEntry,
    server: FakeChorusServer,
    caplog: pytest.LogCaptureFixture,
) -> None:
    caplog.set_level(logging.INFO, logger=LOG)
    await start(hass, entry, server)
    server.frame(FRAME)
    await wait_for(lambda: says(hass) == KICK)

    # The visualizer stream alone is lost, and the server no longer has the route.
    server.visualizer_status = 404
    server.drop_visualizer_streams()
    await wait_for(lambda: says(hass) == {"state": STATE_UNAVAILABLE})
    await wait_for(lambda: server.requests.count("GET /api/visualizer") >= 2)
    assert hass.states.get(room(hass, "den")).state != STATE_UNAVAILABLE
    lost = [r for r in caplog.records if r.name == LOG and "unavailable" in r.message]
    assert len(lost) == 1

    # Back: available, and idle until a frame says otherwise (nothing is kept).
    server.visualizer_status = 200
    await wait_for(lambda: says(hass) == IDLE, timeout=10.0)
    back = [r for r in caplog.records if r.name == LOG and "is back" in r.message]
    assert len(back) == 1
    server.frame(FRAME)
    await wait_for(lambda: says(hass) == KICK)

    # The whole server gone: unavailable with everything else.
    server.refuse_connections = True
    server.drop_streams()
    await wait_for(lambda: says(hass) == {"state": STATE_UNAVAILABLE})
    assert hass.states.get(room(hass, "den")).state == STATE_UNAVAILABLE


def documented_automation() -> list[dict[str, Any]]:
    """The README's example automation, as it is written there."""
    blocks = re.findall(r"```yaml\n(.*?)```", README.read_text(), flags=re.DOTALL)
    found = [block for block in blocks if "_visualizer" in block and "light." in block]
    assert len(found) == 1, "the README has one visualizer-to-light automation"
    config = parse_yaml(found[0])
    assert list(config) == ["automation"]
    return config["automation"]


async def test_visualizer_documented_automation_calls_the_light_with_the_frames_colour(
    hass: HomeAssistant, entry: MockConfigEntry, server: FakeChorusServer
) -> None:
    await start(hass, entry, server)
    await wait_for(lambda: says(hass) == IDLE)
    calls = async_mock_service(hass, "light", "turn_on")

    # The README writes the short entity id; this house's is the registry's.
    text = json.dumps(documented_automation())
    assert "sensor.den_visualizer" in text
    automation = json.loads(text.replace("sensor.den_visualizer", visualizer(hass)))
    assert await async_setup_component(hass, "automation", {"automation": automation})
    await hass.async_block_till_done()

    server.frame(FRAME)
    await wait_for(lambda: len(calls) == 1)
    await hass.async_block_till_done()
    call = calls[0]
    assert call.data["entity_id"] == ["light.den_lamp"]
    assert tuple(call.data["rgb_color"]) == (255, 96, 0)
    assert call.data["brightness"] == 180
    assert call.data["transition"] == 0.5

    # Silence: the idle value reaches the light too (a brightness of 0 is how
    # `light.turn_on` turns a light off).
    server.frame(frame(peak=0, beat=0))
    await wait_for(lambda: len(calls) == 2)
    assert calls[1].data["brightness"] == 0

    # Losing the stream calls nothing: unavailable is not a colour.
    server.visualizer_status = 404
    server.drop_visualizer_streams()
    await wait_for(lambda: says(hass) == {"state": STATE_UNAVAILABLE})
    await hass.async_block_till_done()
    assert len(calls) == 2
