"""The commands' bytes, held to the repository's shared vectors."""

from __future__ import annotations

import json

import pytest

from custom_components.chorus._aiochorus import commands

from ..fake_server import local, shared


def test_bytes_equal_the_shared_vectors() -> None:
    assert commands.volume("kitchen", 500) == shared("volume.json")
    assert commands.mute("kitchen", True) == shared("mute.json")
    assert commands.join("kitchen", "study") == shared("join.json")
    assert commands.take("kitchen") == shared("take.json")
    assert commands.take("downstairs", "line-in:endpoint-c/line-1") == shared(
        "take-source.json"
    )
    assert commands.take("kitchen", "player:p0") == shared("take-player.json")
    assert commands.group_volume("downstairs", 400) == shared("group_volume.json")
    assert commands.group_volume_step("downstairs", -50) == shared(
        "group_volume_step.json"
    )
    assert commands.volume_step("kitchen", 25) == shared("volume_step.json")
    assert commands.playback("kitchen", "pause") == shared("playback.json")
    for name in (
        "playback-next.json",
        "playback-previous.json",
        "playback-resume.json",
    ):
        vector = shared(name)
        fields = json.loads(vector)
        assert commands.playback(fields["target"], fields["action"]) == vector


def test_announce_bytes() -> None:
    url = "http://ha.example:8123/api/tts_proxy/abc.mp3"
    assert commands.announce("kitchen", url) == local("announce.json")
    assert commands.announce("downstairs", url, 300) == local("announce-volume.json")


@pytest.mark.parametrize(
    ("thousandths", "text"),
    [
        (0, "0.000"),
        (1, "0.001"),
        (50, "0.050"),
        (500, "0.500"),
        (999, "0.999"),
        (1000, "1.000"),
    ],
)
def test_a_volume_has_exactly_three_decimals(thousandths: int, text: str) -> None:
    assert commands.encode_volume(thousandths) == text
    assert commands.volume("den", thousandths) == (
        f'{{"v":1,"t":"volume","zone":"den","volume":{text}}}'.encode()
    )


@pytest.mark.parametrize(
    ("level", "thousandths"),
    [
        (0.0, 0),
        (0.0004, 0),
        (0.0005, 1),
        (0.3, 300),
        (0.29999999, 300),
        (0.57, 570),
        (1.0, 1000),
        (1.7, 1000),
        (-0.2, 0),
    ],
)
def test_a_level_becomes_thousandths_rounded_half_up(
    level: float, thousandths: int
) -> None:
    assert commands.volume_from_level(level) == thousandths


def test_what_is_not_a_volume_a_step_or_an_action_is_refused() -> None:
    with pytest.raises(ValueError, match="volume"):
        commands.encode_volume(1001)
    with pytest.raises(ValueError, match="volume"):
        commands.volume_from_level(float("nan"))
    with pytest.raises(ValueError, match="step"):
        commands.volume_step("den", 1001)
    with pytest.raises(ValueError, match="not a playback action"):
        commands.playback("den", "stop")


def test_strings_are_escaped_as_the_catalog_says() -> None:
    # The quote, the backslash and the C0 controls are escaped; "/" and
    # characters above U+007F are written as themselves.
    assert commands.take('a"b\\c\n\x01/é') == (
        '{"v":2,"t":"take","target":"a\\"b\\\\c\\n\\u0001/é"}'.encode()
    )
    assert (
        commands.mute("den", False) == b'{"v":1,"t":"mute","zone":"den","muted":false}'
    )
