# The chorus wire protocol

Version: **2** (goal 5, 2026-09-30). Every multi-byte header and message field
is big-endian (network byte order) unless a table says otherwise. Every
timestamp is nanoseconds from a monotonic source on the device that took it,
never wall clock.

This document and the vectors under `fixtures/protocol/` are the contract. A
second implementation is correct when it produces those bytes and recovers
those fields, not when it matches the Rust code (`crates/protocol`, the v2
half in `crates/protocol/src/v2/`).

Why it is shaped this way: `docs/decisions/0003-wire-protocol-framing.md` (the
frame), `docs/decisions/0041-protocol-v2-framing.md` (what v2 keeps, adds and
encrypts, and v1 refused by name), `docs/decisions/0039-the-v2-key-exchange.md` (the session, and the
vendored crypto primitives) and `docs/decisions/0040-codec-negotiation.md` (codecs).
What a decoder does when it cannot accept a frame:
`docs/decisions/0005-decoder-frame-validation.md`.

## Versions

- **v1** is the FOUNDATION-1 catalog: `time_sync`, `audio_chunk` and
  `stream_end` in the clear on one TCP connection. Nothing in this tree
  speaks it any more: the ESP32-S3 endpoint moved to v2 in goal 6
  (2026-09-30).
- **v2** keeps v1's frame and v1's three messages byte for byte, and adds a
  session (an authenticated key exchange with the endpoint's key pinned at
  adoption; everything after it encrypted), hello and capabilities, a stream
  format with an explicit channel map, FLAC and Opus beside PCM, a per-endpoint
  output delay, telemetry, and the four roles: metadata and artwork,
  controller, visualizer and colour, and source.
- **A v1 peer is refused by name.** A v2 server whose first frame from a peer
  is a v1 message answers `session_refused` with reason `protocol_version` and
  the detail "refused a chorus protocol v1 peer (its first frame was
  time_sync); this side speaks v2 only" (the frame is the vector
  `v2/session_refused_v1_peer.hex`), logs the refusal naming v1, and closes. A
  v1 peer steps over the unknown type 0x23 by its length, so it sees a clean
  close. A v2 endpoint whose `handshake_init` gets no answer within its
  timeout (a v1 server steps over type 0x20 without replying) refuses the
  server by name: "the server may speak chorus protocol v1".

## Frame

```
 0        1                 3                        3 + payload length
 +--------+-----------------+------------------------+
 | type   | payload length  | payload                |
 | u8     | u16 big-endian  | payload length bytes   |
 +--------+-----------------+------------------------+
```

- Header is 3 bytes.
- Maximum payload is 65535 bytes, so the largest frame is 65538 bytes.
- Frames are self-delimiting, so any mix of types shares one connection.
- Inside a v2 session a frame must also fit a record: at most 65519 bytes,
  header included (see "Records").

## Message catalog

| type | name | since | travels | payload |
|---|---|---|---|---|
| 0x00 | unassigned, never used | | | an all-zero buffer is an unknown type, not a message |
| 0x01 | `time_sync` | v1 | in a record | 32 bytes, fixed |
| 0x02 | `audio_chunk` | v1 | in a record | 32-byte chunk header, then PCM |
| 0x03 | `stream_end` | v1 | in a record | 12 bytes, fixed |
| 0x10 | `hello` | v2 | in a record | version, roles, name, software |
| 0x11 | `capabilities` | v2 | in a record | codecs, formats, channels, rates, buffer, latency, lights |
| 0x12 | `stream_format` | v2 | in a record | codec, format, rate, channel map, chunk length, codec setup |
| 0x13 | `coded_chunk` | v2 | in a record | one FLAC frame or Opus packet on the server timeline |
| 0x14 | `output_delay` | v2 | in a record | 8 bytes, fixed |
| 0x15 | `telemetry` | v2 | in a record | 36 bytes, fixed |
| 0x20 | `handshake_init` | v2 | in the clear | magic, version, suite, Noise message 1 |
| 0x21 | `handshake_response` | v2 | in the clear | Noise message 2 |
| 0x22 | `handshake_finish` | v2 | in the clear | Noise message 3 |
| 0x23 | `session_refused` | v2 | in the clear (or in a record) | reason, detail |
| 0x24 | `secure_record` | v2 | in the clear (it is the envelope) | ciphertext of whole frames, then a 16-byte tag |
| 0x30 | `metadata` | v2 | in a record | now playing |
| 0x31 | `artwork` | v2 | in a record | one piece of an image |
| 0x32 | `controller_command` | v2 | in a record | command, value, target |
| 0x33 | `controller_state` | v2 | in a record | volume, mute, playback, group |
| 0x34 | `visualizer_frame` | v2 | in a record | timestamp, beat, peak, bands |
| 0x35 | `color` | v2 | in a record | timestamp, RGB, brightness, fade |
| 0x36 | `source_offer` | v2 | in a record | an input an endpoint can share |
| 0x37 | `source_control` | v2 | in a record | start or stop sending an input |
| 0x38 | `room_volume` | v2 (goal 11) | in a record | 6 bytes, fixed: gain, limit, ramp |
| all others | unassigned | | | skipped by a decoder that meets one |

Common field encodings:

- `short text`: a u8 length, then that many bytes of UTF-8 (0 to 255).
- `long text`: a u16 length, then that many bytes of UTF-8 (0 to 1024; more
  is rejected).
- `bool`: one byte, 0 or 1; any other value is rejected.
- Signed integers are two's complement, big-endian.

## The v1 messages, carried into v2 unchanged

Bytes identical to v1, so their v1 golden vectors are also their v2 vectors.
In a v2 session they travel inside records.

### 0x01 time sync

The four timestamps of one RFC 5905 section 8 exchange. Minimum and canonical
payload length is 32 bytes.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 8 | `t0_ns` | client transmit, client clock |
| 8 | 8 | `t1_ns` | server receive, server clock |
| 16 | 8 | `t2_ns` | server transmit, server clock |
| 24 | 8 | `t3_ns` | client receive, client clock |

From these, `rtt = (t3 - t0) - (t2 - t1)` and
`offset = ((t1 - t0) + (t2 - t3)) / 2`. The offset is exact only for a
symmetric path, which is why the client filters a window of them rather than
trusting one.

The two clocks have unrelated epochs. Comparing a `t0` with a `t1` as if they
were the same timeline is the mistake this whole exchange exists to avoid.

**Who fills in which field.** A client sends a request with `t0` stamped and
the other three zero. The server answers on the same connection, echoing `t0`
untouched and filling `t1` and `t2` from its own monotonic timeline: `t1` when
it decoded the request off the socket and `t2` when it encoded the reply, so
its own queueing is inside `t2 - t1` and is subtracted out rather than being
counted as network time. `t3` stays zero on the wire. It is the client's
receive stamp on the client's clock, the server cannot know it, and a server
that invented one would be handing the client a round trip it made up. The
client stamps `t3` where the reply arrives, and only then is the exchange four
timestamps.

### 0x02 audio chunk

A 32-byte chunk header followed by PCM. Minimum payload length is 33 bytes: a
chunk with no samples is not representable.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 4 | `sequence` | per stream, wraps |
| 4 | 8 | `timestamp_ns` | presentation time of the first sample, server timeline |
| 12 | 4 | `sample_rate_hz` | 8000 to 384000 |
| 16 | 1 | `channels` | 1 to 8 |
| 17 | 1 | `sample_format` | 1 `pcm_s16le`, 2 `pcm_s24le`, 3 `pcm_f32le` |
| 18 | 14 | reserved | opaque, no semantics; see below |
| 32 | rest | PCM | in the announced format's byte order |

The PCM byte count must be a nonzero whole number of frames, where a frame is
`channels * bytes per sample` and bytes per sample is 2, 3 or 4 for the three
formats above.

The PCM bytes are the one part of the protocol that is not big-endian: they
are carried exactly as the announced format names them (all three defined
formats are little-endian) because they pass through to a DAC untouched.

#### The reserved block

Fourteen bytes at offset 18, reserved for the low-latency TV path. See
`docs/decisions/0004-audio-chunk-reserved-bytes.md` for why there are fourteen
and why they are opaque.

Rules, which every implementation has to follow for the reservation to be
worth anything:

- An encoder writes zeros.
- A decoder ignores their value. It does not require zeros, and it does not
  reject a frame because they are not zero.
- A decoder makes the raw bytes available to its caller rather than dropping
  them.

### 0x03 stream end

Sent once, after the final chunk of a stream and before the connection is
closed. Minimum and canonical payload length is 12 bytes.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 4 | `final_sequence` | sequence of the final chunk |
| 4 | 8 | `end_timestamp_ns` | one chunk duration past the final chunk's presentation timestamp, server timeline |

**This document is the normative definition of `end_timestamp_ns`, and any
other statement of it in this tree that disagrees with the row above is the
defect.** That includes a doc comment, a fixture comment, a test and a line of
server source: a formula living in one server's source cannot bind a
third-party encoder written in another language, and this table is what such an
encoder is told to satisfy. Written out, the relation is:

```text
end_timestamp_ns = the final chunk's timestamp_ns + one configured chunk duration, in ns
```

The duration added is always the **configured** chunk duration, never the final
chunk's own. Only the last chunk of a stream may be short, so when it is short
this instant is a little past the point the audio stops; the field is the end
of the final chunk's nominal slot on the server timeline, which is what makes
it a value a receiver can compute from what it was told. It is not an elapsed
duration: the server's timeline has its own epoch, taken at process start, and
every chunk on the wire carries that origin.

This message exists because a transport close and a transport that broke look
identical to the peer: both are a read returning zero. A receiver that saw
`stream_end` knows the sender finished; one that did not knows it lost the
sender. Timing cannot tell those apart, so a close is never the signal.

It was added by SOUND-2, after `time_sync` and `audio_chunk` were already
committed. That addition changed neither of their golden vectors, and a
decoder built before it exists steps over type 0x03 with its length prefix and
keeps the session open, which is rule 3 below. That is what the reservation of
the unassigned type space is for.

## The session: encryption with adoption

Decisions K62 and K92: speaker sessions are encrypted, and pairing is trust on
first use. An endpoint's long-term key is pinned when it is first adopted; a
changed key is refused and surfaced. Why Noise XX and not TLS with a
pre-shared key: `docs/decisions/0039-the-v2-key-exchange.md`.

### The construction

`Noise_XX_25519_ChaChaPoly_SHA256`, exactly as the Noise Protocol Framework
specifies it (revision 34, 2018-07-11, <https://noiseprotocol.org/noise.html>,
read 2026-09-30): X25519 Diffie-Hellman; the ChaCha20-Poly1305 AEAD, whose
96-bit nonce is 32 zero bits then the 64-bit counter little-endian; SHA-256;
and HKDF over HMAC-SHA256. The endpoint is the initiator and the server the responder:

```
XX:
  -> e                  handshake_init      (endpoint to server)
  <- e, ee, s, es       handshake_response  (server to endpoint)
  -> s, se              handshake_finish    (endpoint to server)
```

- **Prologue**: the 7 bytes of `handshake_init` before the Noise message
  (magic, version, suite: `43 48 52 53 00 02 01`). A peer that changes the
  version or suite in transit fails the handshake instead of negotiating it
  down.
- **Payloads**: message 1 carries none. Message 2 carries the server's id and
  message 3 the endpoint's id, each as a `short text` of 1 to 255 bytes, both
  encrypted. The id is what a key is pinned to; it must be stable across
  restarts (an endpoint's configured id, not its address).
- **Keys**: 32-byte X25519 secrets from the operating system's random source,
  a fresh ephemeral key per handshake. The long-term keys live with each side's
  state (server: its identity directory; endpoint: its own storage) and never
  appear on the wire in the clear.
- **After message 3** both sides call `Split()`: the first key encrypts
  endpoint to server, the second server to endpoint. The Noise handshake hash
  is the same on both sides and is available as a channel binding value.
- The implementation is held to cacophony's published vector for this
  protocol name (`fixtures/protocol/v2/noise/cacophony_xx.fields`, Unlicense,
  read 2026-09-30) in both roles.

### 0x20 handshake init

| offset | size | field | notes |
|---|---|---|---|
| 0 | 4 | `magic` | ASCII `CHRS` (`43 48 52 53`); anything else is not a chorus peer |
| 4 | 2 | `protocol_version` | 2 |
| 6 | 1 | `suite` | 1 = `Noise_XX_25519_ChaChaPoly_SHA256`; no other value is defined |
| 7 | rest | `noise` | Noise message 1: the endpoint's 32-byte ephemeral public key |

Minimum payload 39 bytes. A server answers a `protocol_version` other than 2
with `session_refused` `protocol_version`, and an undefined suite with
`unsupported_suite`.

### 0x21 handshake response, 0x22 handshake finish

The payload is the Noise message, whole: for message 2, 32 bytes of the
server's ephemeral key, 48 bytes of its encrypted static key and tag, then the
encrypted id payload and its tag (minimum 96 bytes); for message 3, 48 bytes of
the endpoint's encrypted static key and tag, then the encrypted id payload and
its tag (minimum 64 bytes).

### Adoption: trust on first use

The server keeps an adoption store: one pinned public key per endpoint id,
persisted with the server's state.

| The endpoint's id is | and its key is | the server |
|---|---|---|
| new | any | **adopts it**: pins the key to the id, logs `endpoint adopted id=<id> key=<fingerprint>`, and proceeds |
| pinned | the pinned key | proceeds |
| pinned | another key | **refuses**: sends `session_refused` `key_changed` naming the id and both fingerprints, records the change where people look (the server's log line `endpoint key changed id=<id> pinned=<fp> offered=<fp>; refused`, and its store of refused key changes), keeps the old pin, and closes |
| removed by the owner | any | refuses with `not_adopted` |

A fingerprint is the first 8 bytes of the SHA-256 of the public key, as four
colon-separated groups of 4 hex digits, for example `1a2b:3c4d:5e6f:7081`.

Only the owner changes a pin: forgetting an endpoint lets its next handshake
adopt it afresh, which is how a replaced board or a re-flashed key is
accepted. Nothing a peer sends changes a pin. The endpoint keeps the same kind
of store for the server it talks to, pins the server on first use, and refuses
a server whose key changed in the same way (it sends `session_refused`
`key_changed` and stops). The vectors `v2/session_refused_key_changed.hex` and
the test `an_endpoint_whose_key_changed_is_refused_and_surfaced` hold the
refusal to its bytes.

### 0x23 session refused

| offset | size | field | notes |
|---|---|---|---|
| 0 | 1 | `reason` | see below |
| 1 | long text | `detail` | a sentence for a person, naming what was refused |

| reason | name | sent when |
|---|---|---|
| 1 | `protocol_version` | the peer spoke v1, or offered a version other than 2 |
| 2 | `key_changed` | the peer's key is not the one pinned to its id |
| 3 | `handshake_failed` | a malformed or undecryptable handshake message, or the wrong message |
| 4 | `unsupported_suite` | the peer asked for a suite this side does not offer |
| 5 | `server_full` | no free slot for another endpoint |
| 6 | `not_adopted` | the owner removed this endpoint |

`session_refused` travels in the clear during the handshake, so it is
information and never a command: a receiver surfaces it and closes, and
nothing it holds (a pin, an adoption) changes because of it. After the
handshake a refusal may also travel inside a record.

### 0x24 secure record

| offset | size | field | notes |
|---|---|---|---|
| 0 | rest | `ciphertext` | the sealed plaintext, then the 16-byte Poly1305 tag |

- The plaintext is **one or more whole frames** (at most 65519 bytes, so the
  record's payload stays within 65535): a sender never splits a frame across
  records, and a receiver refuses a record that ends inside a frame.
- The associated data is the record frame's own 3-byte header, so its type and
  length are authenticated too.
- The nonce is the per-direction counter, starting at 0 and never sent; a
  record that fails to decrypt (altered, replayed, reordered) ends the session.
- A frame of a "travels in the clear" type found inside a record (a handshake
  message, or a record in a record) ends the session.
- Minimum payload 19 bytes: a tag and at least one frame header.

## Hello and capabilities

### 0x10 hello

The first message inside the session, in both directions.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 2 | `protocol_version` | 2 |
| 2 | 2 | `roles` | bit set, below; undefined bits are rejected |
| 4 | short text | `name` | a friendly name; may be empty (an endpoint is named after adoption) |
| .. | short text | `software` | the sender's software or firmware version |

| bit | role | the peer |
|---|---|---|
| 0 | `player` | plays a stream (every speaker) |
| 1 | `metadata` | takes now playing and artwork |
| 2 | `controller` | sends controller commands and shows controller state |
| 3 | `visualizer` | takes visualizer frames and colours |
| 4 | `source` | offers inputs and streams them upstream |

The server's `hello` sets no role bits.

### 0x11 capabilities

Sent by an endpoint after its `hello`; what it can accept.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 1 | `codecs` | bit set: bit 0 `pcm`, bit 1 `flac`, bit 2 `opus`; **PCM is mandatory** |
| 1 | 1 | `sample_formats` | bit set: bit 0 `pcm_s16le`, bit 1 `pcm_s24le`, bit 2 `pcm_f32le`; at least one |
| 2 | 1 | `max_channels` | 1 to 8 |
| 3 | 1 | rate count | 1 to 16 |
| 4 | 4 each | `sample_rates_hz` | each 8000 to 384000; the rates it plays natively |
| .. | 2 | `buffer_ms` | audio it can hold ahead of its playout point |
| .. | 4 | `intrinsic_latency_ns` | its own delay from playout point to sound (a measured value, or `ASSUMED` where not yet measured) |
| .. | 2 | `led_count` | addressable lights for the colour role; 0 for none |
| .. | 1 | `visualizer_bands` | most bands it wants per visualizer frame, 0 to 64 |

## The stream format and the channel map

### 0x12 stream format

Sent by whichever side is about to send audio (the server to a player; an
endpoint to the server for a started source), before the first chunk of every
stream, and again whenever any field changes.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 1 | `codec` | 1 `pcm`, 2 `flac`, 3 `opus` |
| 1 | 1 | `sample_format` | 1 `pcm_s16le`, 2 `pcm_s24le`, 3 `pcm_f32le`: the wire layout for PCM, the decoder's output otherwise |
| 2 | 4 | `sample_rate_hz` | 8000 to 384000 |
| 6 | 1 | `channels` | 1 to 8 |
| 7 | 1 each | `channel_map` | one position per channel, in interleave order |
| .. | 4 | `frames_per_chunk` | frames in one nominal chunk or packet; not 0 |
| .. | 2 | codec setup length | |
| .. | that many | `codec_config` | empty for PCM; the 34-byte STREAMINFO body for FLAC; an OpusHead for Opus |

Rules, rejected otherwise: no position appears twice; `MONO` only in a
one-channel stream; for PCM the codec setup is empty; for FLAC it is exactly
34 bytes and the format is not `pcm_f32le`; for Opus the rate is 48000,
`frames_per_chunk` is an Opus frame duration (120, 240, 480, 960, 1920 or 2880
frames: 2.5 to 60 ms, RFC 6716 section 2.1.4) and the setup is an OpusHead
whose channel count (its byte 9) equals `channels`. Every `audio_chunk` of a
PCM stream must agree with the announcement's rate, channel count and format;
a receiver treats a disagreement as a framing error.

### The channel map

5.1 is ordered differently by different transports, so no order is implied:
the map lists each channel's position explicitly, and every edge that meets a
transport remaps once, where it meets it (`research-theater.md` section 5.1).

Positions 1 to 18 follow the bit order of `WAVEFORMATEXTENSIBLE`'s
`dwChannelMask`: position `p` is mask bit `p - 1` ("The least significant bit
corresponds with the front left speaker ... The channels specified in
dwChannelMask must be present in the prescribed order",
<https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible>,
read 2026-09-30), so a WAV file's mask maps onto them without a table. The ALSA
names are from alsa-lib's `snd_pcm_chmap_position`
(<https://www.alsa-project.org/alsa-doc/alsa-lib/group___p_c_m.html>, read
2026-09-30; its documentation only).

| position | name | speaker | WAV mask bit | ALSA |
|---|---|---|---|---|
| 0 | `MONO` | a single channel with no surround place | none | `MONO` |
| 1 | `FL` | front left | 0x1 | `FL` |
| 2 | `FR` | front right | 0x2 | `FR` |
| 3 | `FC` | front centre | 0x4 | `FC` |
| 4 | `LFE` | low-frequency effects | 0x8 | `LFE` |
| 5 | `BL` | back (rear) left | 0x10 | `RL` |
| 6 | `BR` | back (rear) right | 0x20 | `RR` |
| 7 | `FLC` | front left of centre | 0x40 | `FLC` |
| 8 | `FRC` | front right of centre | 0x80 | `FRC` |
| 9 | `BC` | back centre | 0x100 | `RC` |
| 10 | `SL` | side left | 0x200 | `SL` |
| 11 | `SR` | side right | 0x400 | `SR` |
| 12 | `TC` | top centre | 0x800 | `TC` |
| 13 | `TFL` | top front left | 0x1000 | `TFL` |
| 14 | `TFC` | top front centre | 0x2000 | `TFC` |
| 15 | `TFR` | top front right | 0x4000 | `TFR` |
| 16 | `TBL` | top back left | 0x8000 | `TRL` |
| 17 | `TBC` | top back centre | 0x10000 | `TRC` |
| 18 | `TBR` | top back right | 0x20000 | `TRR` |

5.1 as each transport orders it:

| transport | 5.1 order | source |
|---|---|---|
| WAV `WAVE_FORMAT_EXTENSIBLE`, FLAC | FL FR FC LFE BL BR | the page above; RFC 9639 section 9.1.3, read 2026-09-30 |
| ALSA `surround51` | FL FR RL RR FC LFE | `research-theater.md` [M2] (a snippet, LEAD) |
| Opus mapping family 1 (Vorbis order) | FL FC FR RL RR LFE | RFC 7845 section 5.1.1.2, read 2026-09-30 |
| HDMI / eARC, CTA-861 allocation 0x0B | FL FR LFE FC RL RR | **LEAD**: only a search summary; not built on until confirmed against CTA-861 (goal 13) |

The map describes the order of the channels as they arrive at the receiver:
for PCM the interleave order on the wire, for FLAC and Opus the order the
decoder outputs them in (for Opus family 1, Vorbis order).

The Linux client (`chorus-client`) remaps at its ALSA edge when it is given an
output map (`--output-channels N` and one `--output <index>=<source>` per used
device channel; `crates/client-linux/src/outmap.rs`): each device channel plays
one position, an equal-weight downmix of positions (`FL+FR`) or silence, with
its own gain and delay. It then advertises `max_channels` as the number of
distinct positions its map reads, since a stream with more would carry
channels it discards; without a map it advertises 8 and plays the stream's
channels in the stream's order, as before. A position the map reads and the
stream lacks is silence on that output and is reported on an
`output-map-missing` line; the one fallback, also reported, is a `MONO` stream
feeding outputs that read `FL`, `FR` or `FC`. Semantics and bounds are ADR
0068's (`docs/decisions/0068-the-output-map.md`); the hardware it drives is
`docs/hardware/linux-multichannel.md`.

## Codecs: PCM, FLAC and Opus

Negotiation is the server's, in one step, from the endpoint's `capabilities`
(`docs/decisions/0040-codec-negotiation.md`, `crates/protocol/src/v2/negotiate.rs`):
the server takes its configured preference for the endpoint (by link: lossless PCM or FLAC on a wired link,
FLAC on Wi-Fi, Opus where bandwidth is short; K62) and uses the first codec in
that list the endpoint's `codecs` bit set contains. PCM is in every endpoint's
set, so negotiation always ends somewhere. The rate, channel count and format
must be ones the endpoint listed; a stream it cannot play is refused by name,
never sent in the hope that it copes.

- **PCM** travels in `audio_chunk` (0x02), unchanged from v1.
- **FLAC** (RFC 9639, <https://www.rfc-editor.org/rfc/rfc9639.txt>, read
  2026-09-30): one FLAC frame per `coded_chunk`. The stream format carries the
  34-byte STREAMINFO body (section 8.2: block sizes, frame sizes, rate,
  channels, bit depth, total samples, MD5), and frames are from the streamable
  subset (section 7), so each frame header carries its own rate and bit depth
  and a receiver can start at any frame.
- **Opus** (RFC 6716, <https://www.rfc-editor.org/rfc/rfc6716.txt>, and RFC
  7845 section 5.1, <https://www.rfc-editor.org/rfc/rfc7845.txt>, both read
  2026-09-30): one Opus packet per `coded_chunk`, 48 kHz, frames of 2.5 to 60
  ms. The stream format carries the OpusHead ID header (19 bytes for mapping
  family 0, 21 + channels bytes otherwise; its own fields are little-endian).
  The pre-skip it names is dropped by the receiver at the start of the stream.

Decoding is the endpoint's (goal 6: the C endpoint and the Linux client, from
the decoders proposal P9 settled); goal 5 carries the three on the wire, and
the Linux client and the C endpoint advertise PCM only until their decoders
land.

### 0x13 coded chunk

| offset | size | field | notes |
|---|---|---|---|
| 0 | 4 | `sequence` | per stream, wraps; the same sequence space as `audio_chunk` |
| 4 | 8 | `timestamp_ns` | presentation time of the first decoded sample, server timeline |
| 12 | 4 | `frames` | PCM frames the packet decodes to; not 0 |
| 16 | rest | `data` | the FLAC frame or Opus packet, opaque to the protocol; at least 1 byte |

A stream of either codec ends with `stream_end`, exactly as a PCM stream does.

## Output delay

### 0x14 output delay

Server to endpoint: the delay this endpoint adds to every presentation
timestamp before playout, on top of its own `intrinsic_latency_ns`. It is how
per-endpoint trims (speaker distance in a theater set, lip sync) reach the
endpoint without moving the stream's timeline.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 8 | `delay_ns` | 0 to 5000000000 (5 s) |

Sent after `stream_format` and whenever it changes; the latest one applies
from the next chunk the endpoint schedules.

## Telemetry

### 0x15 telemetry

Endpoint to server, periodically (the interval is the endpoint's; about once a
second is the intent). It carries the diagnostics decision K83 surfaces.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 8 | `taken_ns` | when, on the endpoint's monotonic clock |
| 8 | 8 | `sync_error_ns` | signed playout error estimate; `i64` minimum when unknown |
| 16 | 4 | `buffer_fill_us` | audio buffered ahead of the playout point |
| 20 | 4 | `underruns` | since the session began |
| 24 | 4 | `resyncs` | since the session began |
| 28 | 4 | `correction_ppb` | signed; the servo's rate correction in parts per billion |
| 32 | 1 | `link` | 0 unknown, 1 wired, 2 wireless |
| 33 | 1 | `rssi_dbm` | signed; `i8` minimum (-128) when unknown or wired |
| 34 | 2 | `temperature_centi_c` | signed, hundredths of a degree Celsius; `i16` minimum when unknown |

The firmware version is in the endpoint's `hello`.

## Room volume

### 0x38 room volume

Server to player (goal 11, `docs/decisions/0074-room-volume-on-the-audio-wire.md`).
The room's (the zone's, on the wire) gain and limit, carried on the audio
session so that the endpoint playing the audio is the one that enforces them
(K81, I10, brief section 4.8): a server bug or a hostile control path cannot
make an endpoint play above its room's limit or its own ceiling.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 2 | `gain` | thousandths of full amplitude, 0 to 1000: what to play at; already 0 when the room is muted |
| 2 | 2 | `limit` | thousandths, 0 to 1000: the room's effective limit (its limit, less any quiet-hours cap in force) |
| 4 | 2 | `ramp_ms` | 0 to 60000: how long to move from the gain being applied to `gain`, linear in amplitude; 0 is at once |

A value outside its range is rejected (decoder step 5), never clamped: a
decoder that clamped an out-of-range limit would be choosing a limit nobody
sent. Each field is checked on its own, so a `gain` above `limit` is a valid
message, and it plays at the limit.

What a player plays at, at every frame, is

    applied = min(ramped gain, last limit received, the endpoint's own ceiling)

- **The ramp** starts from the gain being applied when the message arrives
  (after the clamp, so what is heard never jumps) and reaches `gain` exactly
  after `ramp_ms`, counted in frames at the stream's rate (`ramp_ms` times the
  rate over 1000), which is the endpoint's monotonic clock by construction.
  A new message starts a new ramp from wherever the old one had got to.
- **The limit** applies at once, at the next frame, and is never ramped: a
  lowered limit pulls the sound down immediately. A server that wants a
  lowered limit to be gentle ramps the gain down first.
- **The ceiling** is the endpoint's own configuration (`max_volume`), default
  1.000, and nothing on the wire raises it.
- Before the first `room_volume` of its life, an endpoint plays at its
  ceiling, so a server that does not send this message is heard as before.
  The gain and limit last received are kept across a new stream and a new
  session, never reset upward.
- A gain or a limit never changes how many frames are written: a muted room
  writes as many frames as an unmuted one, at the same instants, as zeros.

The server (with a control plane; one without has no room to send) sends it:

- at the start of every session of an endpoint a room names, in the greeting
  after `stream_format` and `output_delay` and before the first chunk: the
  room's gain (its volume, 0 when muted), its effective limit, `ramp_ms` 0;
- whenever that room's volume, mute or effective limit changes, wherever the
  change was made, to every player session of the room (a room in a group gets
  its own room's values), at once (`ramp_ms` 0, ASSUMED: no de-click ramp for a
  person's change), and only when what it would say differs from what that
  session was last sent.

The gain is never above the limit in what the server sends; the endpoint's
clamp is the second line, not the first. `fixtures/volume/` holds the sequence
the real server sent one player over every volume path, and both endpoint
kinds are tested on it.

## The four roles

Decision K65. A peer declares its roles in `hello`; the server sends a role's
messages only to peers that declared it, and a peer ignores (skips) messages of
a role it did not declare.

### Metadata and artwork

Now playing flows from the input through the server to every endpoint with the
`metadata` role (and to the app and Home Assistant by the control plane).

#### 0x30 metadata

| offset | size | field | notes |
|---|---|---|---|
| 0 | 1 | `playback` | 0 stopped, 1 playing, 2 paused |
| 1 | 4 | `position_ms` | position in the item at `position_at_ns` |
| 5 | 4 | `duration_ms` | 0 when unknown (a live input) |
| 9 | 8 | `position_at_ns` | server timeline |
| 17 | 4 | `artwork_id` | 0 for none |
| 21 | long text | `title` | |
| .. | long text | `artist` | |
| .. | long text | `album` | |
| .. | long text | `source` | the input it arrived on, e.g. `UPnP`, `Spotify`, `Line in` |

#### 0x31 artwork

An image in pieces, because an image is larger than a frame.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 4 | `artwork_id` | not 0; matches `metadata.artwork_id` |
| 4 | 4 | `total_len` | bytes in the whole image, 1 to 4194304 (4 MiB) |
| 8 | 4 | `offset` | where this piece starts |
| 12 | short text | `mime` | non-empty ASCII, e.g. `image/jpeg` |
| .. | rest | `data` | at least 1 byte; `offset + len(data)` is at most `total_len` |

A receiver assembles pieces by id and discards an image whose pieces stop
arriving; a new id replaces the old one.

### Controller

Any endpoint with buttons (or a wall remote) controls the room or group it
plays in.

#### 0x32 controller command

| offset | size | field | notes |
|---|---|---|---|
| 0 | 1 | `command` | below |
| 1 | 2 | `value` | signed; per command |
| 3 | short text | `target` | the room or group for `join`; empty for every other command |

| command | name | value |
|---|---|---|
| 1 | `play` | 0 |
| 2 | `pause` | 0 |
| 3 | `toggle` | 0 |
| 4 | `next` | 0 |
| 5 | `previous` | 0 |
| 6 | `volume_set` | 0 to 100 |
| 7 | `volume_step` | -100 to 100 |
| 8 | `mute_set` | 1 mute, 0 unmute |
| 9 | `join` | 0; `target` names the room or group |
| 10 | `leave` | 0 |

Every volume a controller asks for is clamped by the room's limits on the
server (K81, I10); a command is a request, never a bypass.

#### 0x33 controller state

Server to controller, whenever it changes. The server sends it to an endpoint
that declared the `controller` role after each of that endpoint's commands
(ADR 0067); on changes made elsewhere, from goal 11.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 1 | `volume` | 0 to 100 |
| 1 | 1 | `muted` | bool |
| 2 | 1 | `playback` | 0 stopped, 1 playing, 2 paused |
| 3 | short text | `group` | the room or group the endpoint plays in |

### Visualizer and colour

A light stream for speaker LEDs and, through Home Assistant, lights. Levels
and beats are computed on the server from the audio (the DSP library, goal 12)
and timestamped on the server timeline, so an endpoint shows them when the
audio they describe is heard.

#### 0x34 visualizer frame

| offset | size | field | notes |
|---|---|---|---|
| 0 | 8 | `timestamp_ns` | when the frame is heard, server timeline |
| 8 | 1 | `beat` | beat strength, 0 for none |
| 9 | 1 | `peak` | peak level, 0 to 255 |
| 10 | 1 | band count | 0 to 64, at most the endpoint's `visualizer_bands` |
| 11 | that many | `bands` | levels, low to high |

#### 0x35 color

| offset | size | field | notes |
|---|---|---|---|
| 0 | 8 | `timestamp_ns` | when it applies, server timeline |
| 8 | 1 | `red` | |
| 9 | 1 | `green` | |
| 10 | 1 | `blue` | |
| 11 | 1 | `brightness` | 0 off to 255 |
| 12 | 2 | `transition_ms` | fade time from the previous colour |

### Source

Any endpoint can offer an input (line-in, optical, HDMI ARC) as a source any
room or group can play. A speaker microphone is never a source: it feeds only
the voice path (brief section 4.8, I4), so there is no microphone kind.

#### 0x36 source offer

Endpoint to server, after `capabilities` and whenever the signal comes or goes.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 1 | `source_id` | the endpoint's own number for the input |
| 1 | 1 | `kind` | 1 `line_in`, 2 `optical`, 3 `hdmi_arc` |
| 2 | 1 | `signal` | bool: a signal is present now |
| 3 | short text | `name` | may be empty |

#### 0x37 source control

Server to endpoint.

| offset | size | field | notes |
|---|---|---|---|
| 0 | 1 | `source_id` | |
| 1 | 1 | `action` | 1 `start`, 2 `stop` |
| 2 | 1 | `codec` | the codec the server wants the input in; one the endpoint listed |

On `start` the endpoint sends a `stream_format`, then `audio_chunk` or
`coded_chunk` frames timestamped on the server timeline (through its sync
offset), and `stream_end` on `stop`, all upstream on the same session.

Clarified in goal 10 (ADR 0066, the Linux client's line-in):

- A chunk's `timestamp_ns` is the instant its first frame was digitized (the
  read's return time less the capture device's reported delay and the chunk's
  own length), mapped onto the server timeline through the endpoint's sync
  offset. An endpoint with no offset yet holds what it captured and stamps it
  once it has one; it never sends a guessed timestamp.
- The codec a `start` names has to be one the endpoint listed in
  `capabilities` and one it can send an input in. `capabilities` lists what an
  endpoint plays; an endpoint that decodes FLAC and Opus but encodes nothing
  sends its inputs in PCM only. A `start` it cannot honour (an unknown
  `source_id`, a codec not listed, a codec it cannot send) is refused on the
  endpoint by name, no `stream_format` follows, and the input stays offered;
  the catalog has no message for the refusal, so the server sees none.
- Sequences start at 0 for each started stream. Frames an overrun lost on the
  capture device show as a gap between two timestamps, not in the sequence.

Clarified in goal 11 (ADR 0079, the server accepting a line-in):

- The server names an offered input `<endpoint>/<name>` when `name` is a
  catalog identifier (lower-case letters, digits and `-`), else
  `<endpoint>/line-<source_id>`; that is the `line-in:<endpoint>/<input>`
  source and the `inputs[]` entry of the control state. Only a session whose
  `hello` declared the source role is listened to; a server with no stream
  slots (`--slots 0`) ignores source messages as before.
- The server sends `source_control` only inside the input's own session, and
  `start` only names `pcm` (this server decodes no input codec).
- The `stream_format` a started input answers with must be the server's own
  stream format: codec `pcm`, the same rate, channel count and sample format.
  One that is not is refused on the server: it sends `stop` and logs `line-in
  refused reason=format-mismatch`, and nothing that input sends is played.
  (Conversion is a follow-up.)
- The server plays an input's PCM in order and does not read its chunk
  timestamps: a line-in goes out on the server's own grid of stream slots
  (`docs/control-plane.md`, "The schedule runtime"), which is what lets a
  session move between slots with no timestamp jump. Upstream frames past what
  the server holds (one second, ASSUMED) are dropped whole and counted.
- The input's session ending is the input going: whatever plays it stops.

## The session, in order

1. The endpoint connects and sends `handshake_init`.
2. The server answers `handshake_response`, or `session_refused`.
3. The endpoint checks the server's key against its pin, then sends
   `handshake_finish` (or `session_refused` `key_changed`).
4. The server checks the endpoint's key against its adoption store, adopting a
   new id; a changed key is refused.
5. From here every frame on the connection is a `secure_record`. The endpoint
   sends `hello` and `capabilities`; the server sends `hello`, negotiates, and
   sends `stream_format`, `output_delay` (with a control plane, the room's
   `room_volume` for a player and its `controller_state` for a controller) and
   the audio. With stream slots (`docs/control-plane.md`) the audio is the
   stream of the group the endpoint's room is in, routed inside this session:
   a move between groups changes the content, never the sequence or the
   timestamps, because every slot is cut on one grid. `time_sync` requests
   and replies, `telemetry`, and the role messages share the connection.

## Decoder behaviour

In order. The order is the contract: it decides which answer a frame gets. It
is the same for v1 and v2; only the catalog in step 3 differs.

1. Fewer than 3 bytes remain: reject, truncated header. The next frame
   boundary is unknown, so stop consuming this buffer.
2. The payload length field exceeds the bytes actually remaining: reject. The
   next frame boundary is unknown, so stop consuming this buffer. This check
   comes before any payload is sliced.
3. The type byte is not in the catalog: skip the frame using the length
   prefix, and carry on with the next one. This is not an error.
4. The payload length is below the declared type's minimum: reject that frame,
   then carry on with the next one, which the length prefix locates.
5. A field value is not one the format accepts: reject that frame, then carry
   on. In v2 this includes a length-prefixed field (a text, a list, the codec
   setup) that runs past the end of the payload, text that is not UTF-8, and
   the value rules each message's section states (for example
   `room_volume`'s ranges, held by the rejection vectors).

A payload longer than the fields a decoder knows about is accepted and the
excess ignored, so that a field added later is not fatal to a decoder built
today. (Where a message's last field is "rest", there is no excess: the rest
is that field.)

None of the above closes a session. A malformed frame costs one frame. The
session layer is stricter about its own envelope, and those rules are in "The
session": a record that fails to decrypt, ends inside a frame, or carries a
frame that travels in the clear ends the session, because nothing after it can
be trusted to be aligned or authentic.

## Encoder behaviour

An encoder refuses, and emits nothing at all, when:

- a value does not fit its wire field (for example more than 255 channels in a
  `u8`). It is never truncated and never allowed to wrap;
- the payload would exceed 65535 bytes;
- a field carries a value the decoder above would reject.

So valid encoder output always decodes. For v1 that invariant is asserted in
`crates/protocol/tests/encoder_range.rs`; for v2 one validation routine serves
both directions (`crates/protocol/src/v2/codec.rs`, `validate`), so the
encoder refuses exactly what the decoder rejects.

## Golden vectors

- `fixtures/protocol/*.hex|fields`: the three v1 messages, which are also the
  vectors for v2's types 0x01 to 0x03 (their bytes did not change).
- `fixtures/protocol/v2/*.hex|fields`: at least one vector for every type v2
  added, including the handshake, the refusals and a first record, all made
  with the public test keys their `.fields` files list. The handshake vectors
  are reproduced byte for byte by a real handshake between the Rust server and
  endpoint sides (`crates/protocol/tests/v2_vectors.rs`).
- `fixtures/protocol/v2/rejected/*.hex|fields`: REJECTION vectors, frames
  the format does not accept. Their `.fields` add `rejected_field` and
  `problem`: an encoder must refuse the fields naming that field and emit
  nothing, and a decoder must reject the frame as that field, consume it
  whole and decode the frame after it (goal 11: `room_volume` with each of
  its fields out of range).
- `fixtures/protocol/v2/noise/cacophony_xx.fields`: the published Noise test
  vector the key exchange is held to.

Both implementations read every one of them: the Rust tests under
`crates/protocol/tests/`, and the C endpoint's `firmware/tests/test_protocol_v2.c`
(every v2 vector, both directions, and every rejection vector) and `firmware/tests/test_noise.c` (the
Noise vector in both roles, and the four session vectors from their keys).

`fixtures/README.md` gives the file format and who reads what.

## Control messages are a separate catalog

BRIEF.md 5.2 lists volume, grouping and configuration beside the above, and
PRODUCT-6 put them in a SECOND catalog rather than in this one:
`docs/control-plane.md`, pinned by `fixtures/control/`, carried as JSON on its
own connection. `docs/decisions/0016-the-control-catalog.md` records why they
are apart: this catalog is decoded by a C endpoint with a fixed frame budget
and takes the forward-compatible reading of an unknown type, while a control
message that a decoder half-understands changes what a house is doing and has
to be refused instead. v2's controller role is not a second control plane: a
`controller_command` is a request from a button, which the server turns into
the same state change the control plane would make, clamped by the same
limits.

## Carrying this on a stream transport

SOUND-2 puts these frames on TCP, which does not preserve message boundaries.
The framing above is what restores them: a reader accumulates bytes, decodes
whole frames from the front of its buffer, and keeps the tail. Two rules make
that safe, and both are already in the decoder behaviour above:

- A declared length longer than the bytes in hand consumes nothing, so the
  reader waits for more rather than guessing where the next frame starts.
- A frame whose type is in the catalog but whose payload is short for that
  type, or whose fields are out of range, is rejected and stepped over by its
  own length prefix. The stream stays aligned.

Alignment is only lost if a length prefix itself is wrong, which on TCP means
the peer is not speaking this protocol. A reader that finds the next header
undecodable after a correctly consumed frame reports a framing error and stops;
it never scans forward for something that looks like a header, because
resynchronising by pattern search is how mis-framed bytes reach a DAC as noise.

### Where the skip rule stops applying

Rule 3 above, "skip an unassigned type using its length prefix", is a rule
about a decoder handed one frame. It is exactly right when the transport
preserved that frame's boundaries, and it is what makes adding a type byte a
non-event for every existing implementation, including the future C mirror.

On a stream transport it needs care, and this is a property of the transport
rather than of the protocol. If alignment has already been lost, the byte a
reader reads as a "type" is a PCM sample and the two bytes it reads as a
"length" are two more, so stepping over that length is how a reader stays
lost rather than how it recovers. A reader on a stream transport is therefore
entitled to treat an unassigned type as lost alignment and stop, and the Linux
client does exactly that, for the reason that it is the component holding a
DAC. A reader whose transport preserves message boundaries skips, as rule 3
says.

Both readings are conformant. What is not conformant is stepping over an
uncorroborated length and then playing what follows.

Port numbers are still not assigned by this document. The server takes its
listen address from configuration.

Inside a v2 session the question above does not arise for the frames inside a
record: a record carries whole frames and is authenticated, so its frame
boundaries are known, and an unassigned type inside one is skipped as rule 3
says. The records themselves are the frames on the TCP stream, and a reader
treats anything on the stream that is not a record (or, during the handshake,
a handshake frame or a refusal) as the end of the session.
