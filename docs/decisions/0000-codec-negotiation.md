# 0000: the server negotiates the codec in one step, from a per-link preference and the endpoint's capabilities

- Status: accepted (goal 5, 2026-09-30)
- Decided by: the goal, on decision K62 (FLAC and Opus on the wire beside PCM: FLAC for Wi-Fi,
  Opus for weak Wi-Fi or many rooms) and the brief's section 9 (codecs PCM, FLAC and Opus; an
  ADR for the codec negotiation)
- Implemented in: `crates/protocol/src/v2/negotiate.rs`, called by the server where it answers an
  endpoint's `capabilities` (`crates/server`, wired in by the end-to-end PR of goal 5); specified in `docs/protocol.md`, "Codecs: PCM, FLAC
  and Opus"

## Context

A v2 stream can be PCM, FLAC or Opus. Endpoints differ (the Linux client decodes only PCM until
goal 6; a Wi-Fi speaker wants a smaller stream), the server's encoders arrive later than the wire
format, and a codec can only carry some streams (Opus decodes at 48 kHz; FLAC has no
floating-point samples). Someone has to choose, once per stream, and the choice has to be
explainable when it refuses.

## What was read

- RFC 6716 (Opus), <https://www.rfc-editor.org/rfc/rfc6716.txt>, sections 2.1.3 and 2.1.4 (48 kHz
  internal rate; frames of 2.5 to 60 ms), read 2026-09-30.
- RFC 7845 section 5.1 (the OpusHead ID header), <https://www.rfc-editor.org/rfc/rfc7845.txt>,
  read 2026-09-30.
- RFC 9639 (FLAC) sections 6, 7 and 8.2 (frames carry their own format in the streamable subset;
  STREAMINFO's bit depth of 4 to 32), <https://www.rfc-editor.org/rfc/rfc9639.txt>, read
  2026-09-30.
- `docs/research/2026-09-protocol-v2-sources.md` sections 5 and 6.

## Options

1. **The endpoint chooses** from a list the server offers. The endpoint knows its decoders but
   not the room's situation (how many rooms share the link, whether the group is Wi-Fi heavy), and
   a group needs one decision per stream, not one per member. Rejected.
2. **Capability intersection with a fixed global order** (always the "best" common codec).
   Ignores K62's point that the right codec depends on the link. Rejected.
3. **The server chooses, per endpoint, from a preference list by link, intersected with what both
   sides can do and what the stream allows.** **Chosen.**

## Decision

- **Inputs:** the source stream (rate, channels, sample format), the server's preference list
  for the endpoint, the codecs the server can send today, and the endpoint's `capabilities`.
- **Order of checks:** the endpoint must play the stream's rate, channel count and sample format
  natively (the server does not resample yet), else the stream is refused by name. Then the first
  codec in the preference that the endpoint lists, the server can send, and that can carry this
  stream (Opus only at 48 kHz; FLAC not for 32-bit float) is chosen.
- **Default preference:** wired: PCM (bandwidth is not the constraint on the wired tier, and PCM
  costs no CPU on the endpoint); Wi-Fi: FLAC then PCM. Opus is lossy and is never a default: it
  is a per-room choice where a weak link or many rooms make lossless impossible.
- **PCM is mandatory in every endpoint's capabilities** (a `capabilities` without it is rejected
  by the decoder), so a server that can send PCM always finds a codec for a stream the endpoint
  can play.
- **Today the server sends PCM only;** FLAC and Opus encoders are added with the decoders
  (goal 6 onwards), and the negotiation picks them up by the server's list growing, with no
  change to the wire or to the endpoint.

## Consequences

- A stream the endpoint cannot play is refused with a sentence naming the rate, channels or
  format, in the server's log, instead of being sent and failing at the DAC.
- Resampling in the server (a later goal) turns the rate refusal into a conversion; the check's
  place in the order stays.
- A group's members may get different codecs for the same stream; the timeline (and so sync) is
  the same for all of them, since `coded_chunk` and `audio_chunk` share the sequence and
  timestamp space.
