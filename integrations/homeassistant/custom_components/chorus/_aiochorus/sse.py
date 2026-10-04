"""A server-sent events line splitter that does not depend on a reader's line limit.

A chorus state message is one ``data:`` line and can be large, so the stream is
read in chunks and split here, with an explicit upper bound on one event.
"""

from __future__ import annotations

from .errors import ChorusProtocolError

# One state message of a large house is tens of kilobytes. The bound exists so a
# peer that never ends a line cannot grow the buffer without limit.
MAX_EVENT_BYTES = 8 * 1024 * 1024


class SSEParser:
    """Feed it bytes as they arrive; it returns the events completed so far.

    Implements the parts of the event-stream format the control plane uses:
    lines end with LF, CRLF or CR; ``data`` fields of one event are joined with
    a newline; a blank line dispatches; a line starting with a colon is a
    comment; other fields are ignored.
    """

    def __init__(self, max_event_bytes: int = MAX_EVENT_BYTES) -> None:
        """Start with nothing buffered."""
        self._max = max_event_bytes
        self._buffer = bytearray()
        self._data: list[bytes] = []
        self._data_bytes = 0
        self._skip_lf = False

    def feed(self, chunk: bytes) -> list[str]:
        """Take one chunk and return every event it completed, in order."""
        events: list[str] = []
        self._buffer.extend(chunk)
        start = 0
        buf = self._buffer
        length = len(buf)
        while start < length:
            if self._skip_lf:
                # The LF of a CRLF that was split across two chunks.
                self._skip_lf = False
                if buf[start] == 0x0A:
                    start += 1
                    continue
            lf = buf.find(b"\n", start)
            cr = buf.find(b"\r", start)
            if lf == -1 and cr == -1:
                break
            if cr != -1 and (lf == -1 or cr < lf):
                end = cr
                nxt = cr + 1
                if nxt < length:
                    if buf[nxt] == 0x0A:
                        nxt += 1
                else:
                    self._skip_lf = True
            else:
                end = lf
                nxt = lf + 1
            event = self._line(bytes(buf[start:end]))
            if event is not None:
                events.append(event)
            start = nxt
        del buf[:start]
        if len(buf) + self._data_bytes > self._max:
            raise ChorusProtocolError(
                f"an event on the stream is larger than {self._max} bytes"
            )
        return events

    def _line(self, line: bytes) -> str | None:
        if not line:
            if not self._data:
                return None
            payload = b"\n".join(self._data)
            self._data = []
            self._data_bytes = 0
            try:
                return payload.decode("utf-8")
            except UnicodeDecodeError as err:
                raise ChorusProtocolError("an event is not UTF-8") from err
        if line.startswith(b":"):
            return None
        name, _, value = line.partition(b":")
        if name == b"data":
            if value.startswith(b" "):
                value = value[1:]
            self._data.append(value)
            self._data_bytes += len(value) + 1
        return None
