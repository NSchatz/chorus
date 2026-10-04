"""The event-stream splitter: chunk boundaries, line endings, the size bound."""

from __future__ import annotations

import pytest

from custom_components.chorus._aiochorus import ChorusProtocolError, SSEParser


def test_one_event_in_one_chunk() -> None:
    assert SSEParser().feed(b'data: {"a":1}\n\n') == ['{"a":1}']


@pytest.mark.parametrize("ending", [b"\n", b"\r\n", b"\r"])
def test_split_at_every_byte_boundary(ending: bytes) -> None:
    stream = (
        b": a comment"
        + ending
        + b"data: first"
        + ending
        + ending
        + b"event: ignored"
        + ending
        + b"data:second"
        + ending
        + b"data: line two"
        + ending
        + ending
        + b"data"
        + ending
        + ending
        + b"data: third"
        + ending
        + ending
    )
    for cut in range(len(stream) + 1):
        parser = SSEParser()
        events = parser.feed(stream[:cut]) + parser.feed(stream[cut:])
        assert events == ["first", "second\nline two", "", "third"], cut
    parser = SSEParser()
    events = [e for i in range(len(stream)) for e in parser.feed(stream[i : i + 1])]
    assert events == ["first", "second\nline two", "", "third"]


def test_an_unfinished_event_is_not_delivered() -> None:
    parser = SSEParser()
    assert parser.feed(b"data: half") == []
    assert parser.feed(b" and half\n") == []
    assert parser.feed(b"\n") == ["half and half"]


def test_a_state_larger_than_64_kib_arrives_whole() -> None:
    payload = b'{"pad":"' + b"x" * (200 * 1024) + b'"}'
    parser = SSEParser()
    events: list[str] = []
    stream = b"data: " + payload + b"\n\n"
    for start in range(0, len(stream), 1000):
        events += parser.feed(stream[start : start + 1000])
    assert events == [payload.decode()]


def test_a_line_that_never_ends_is_cut_at_the_bound() -> None:
    parser = SSEParser(max_event_bytes=1024)
    with pytest.raises(ChorusProtocolError, match="larger than 1024 bytes"):
        for _ in range(3):
            parser.feed(b"data: " + b"x" * 600)


def test_many_data_lines_are_held_to_the_bound_too() -> None:
    parser = SSEParser(max_event_bytes=1024)
    with pytest.raises(ChorusProtocolError):
        for _ in range(200):
            parser.feed(b"data: 0123456789\n")


def test_an_event_that_is_not_utf8_is_refused() -> None:
    with pytest.raises(ChorusProtocolError, match="not UTF-8"):
        SSEParser().feed(b"data: \xff\xfe\n\n")
