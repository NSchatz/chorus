# 0121: the UPnP AV media renderer's protocol is one pure, dependency-free crate written from the specifications, with its own small XML reader that has no DTD half, identity by UUID version 5, a table that the descriptions, the argument checks and the tests all read, and a gapless handover that stays PLAYING

- Status: accepted (goal 16, 2026-10-03)
- Decided by: the owner for what is built (K57: every room and group is a UPnP AV media
  renderer; K59: every room, saved group and live group is a target; K60; P6's U1, "write it in
  Rust inside chorus-server, from UDA 1.1/2.0 and the AV specs", with its two security rules);
  the goal (program section 20, item 3) inside the coordinator's goal-16 design envelope
  ("Settled" and "chorus-upnp"), track `chorus-g16/upnp-core`, for everything else; every
  number below that is not cited is chorus's own choice and is said to be
- Implemented in: `crates/upnp` (new, package `chorus-upnp`; modules `uuid`, `ssdp`, `xml`,
  `description`, `soap`, `gena`, `lastchange`, `didl`, `avtransport`, `rendering`, `connmgr`,
  `time`, `client`; `src/bin/chorus-upnp-vectors.rs`), `crates/upnp/tests/fixtures.rs` and
  `state_machine.rs`, `fixtures/upnp` (113 vectors), `make upnp-vectors`. The sockets, the
  threads, the player and the owner's page (`docs/upnp.md`) are the renderer track's (track R),
  which builds on this crate

## Context

P6 settled that the UPnP AV renderer is chorus's own code inside chorus-server (U1), and the
goal-16 envelope settled the shape: hand-written, `std::net` only, no XML crate, no async
runtime, one HTTP port for all renderers, a UDA 1.1 device of type MediaRenderer:1 with
AVTransport:1, RenderingControl:1 and ConnectionManager:1. The product is called a "UPnP AV
media renderer"; chorus claims no certification and emits no `DLNA.ORG_PN`.

This record is the protocol half: everything about the renderer that needs no socket, no thread
and no clock. It is where the protocol's correctness lives, so it is a pure library held to
golden vectors (working agreement 5), and the server's part becomes plumbing: read a datagram or
a request, hand the text to this crate, do what it says, send what it returns.

## What was read

All on 2026-10-03.

- By this track, in the texts the goal's research extracted from the published PDFs:
  - UPnP Device Architecture 1.1 (15 October 2008),
    <https://upnp.org/specs/arch/UPnP-arch-DeviceArchitecture-v1.1.pdf>: section 1.3.3 (the
    search response's header list: DATE is RECOMMENDED, EXT is "REQUIRED for backwards
    compatibility with UPnP 1.0"), section 2.3 (friendlyName "SHOULD be < 64 characters"),
    section 2.5 (the `scpd` root with `configId`), section 3.2.2 (the action response template),
    table 3-3 (the UPnP error codes 401, 402, 501 and 600 to 605 with their wording), section
    4.1.2 (CALLBACK and TIMEOUT), section 4.3.2 (SEQ: "MUST wrap from 4294967295 to 1").
  - UPnP Device Architecture 2.0 (17 April 2020),
    <https://openconnectivity.org/upnp-specs/UPnP-arch-DeviceArchitecture-v2.0-20200417.pdf>:
    section 4.1, the delivery URL rule.
  - MediaRenderer:1 Device Template 1.01,
    <https://upnp.org/specs/av/UPnP-av-MediaRenderer-v1-Device.pdf>: table 1 and its footnote
    (the service ids, "Prefixed by urn:upnp-org:serviceId:"), section 2.5 (InstanceID 0 without
    PrepareForConnection), the description example of section 4.
  - AVTransport:1 Service Template 1.01,
    <https://upnp.org/specs/av/UPnP-av-AVTransport-v1-Service.pdf>: sections 2.2.25 to 2.2.30,
    2.3.1, 2.4.1, 2.4.2, 2.4.8 to 2.4.18 (every action's arguments, state rules and error
    table; table 21), 2.5.1, 2.5.3.
  - RenderingControl:1 Service Template 1.01,
    <https://upnp.org/specs/av/UPnP-av-RenderingControl-v1-Service.pdf>: sections 2.2.1, 2.2.2,
    2.3 (table 2: LastChange moderated, maximum event rate 0.2), 2.3.1, 2.4.1, 2.4.2, 2.4.27 to
    2.4.30 (arguments and error tables of the six actions chorus offers). The research's digest
    marked its RenderingControl table as partly from memory; the six actions' argument names,
    related variables and error codes here are from the specification's text.
  - ConnectionManager:1 Service Template 1.01,
    <https://upnp.org/specs/av/UPnP-av-ConnectionManager-v1-Service.pdf>: sections 2.2.2, 2.2.3,
    2.3 (table 2), 2.4.5 (connection 0's values and error 706), and the SCPD of section 3.
  - RFC 9562, <https://www.rfc-editor.org/rfc/rfc9562>: section 5.5 and appendix A.4.
- By the goal's research, as its digest (`upnp.md`, with section citations) records, and relied
  on here through it: the rest of UDA 1.1 (sections 1.1 to 1.3, 2.1, 2.3, 2.5, 2.11, 3.2.1,
  3.2.5, 4.1.1 to 4.1.4, 4.3.1); AVTransport:1 sections 2.2, 2.3, 2.4 table 3, 2.5.4, 3 and 5;
  AVTransport:3 (31 March 2013), <https://upnp.org/specs/av/UPnP-av-AVTransport-v3-Service.pdf>,
  sections 5.4.3 and 5.4.3.3; RenderingControl:1 sections 2.2.16 to 2.2.19 and 5;
  ConnectionManager:1 section 2.5.2; ContentDirectory:1,
  <https://upnp.org/specs/av/UPnP-av-ContentDirectory-v1-Service.pdf>, the DIDL-Lite namespaces
  of the section 2.8 examples and the class names; async-upnp-client's
  `async_upnp_client/profiles/dlna.py` (Apache-2.0,
  <https://github.com/StevenLooman/async_upnp_client>), for what one control point expects; the
  crates.io API for quick-xml, roxmltree, xmlparser, rupnp and ssdp-client.
- From memory, and marked ASSUMED where it is used: RFC 3174's SHA-1 description and its section
  7.3 test vectors (the digests were cross-checked with Python's `hashlib` on 2026-10-03, and
  the RFC 9562 appendix A.4 SHA-1 line, which was read, is a fifth vector); the SplitMix64
  constants of the test jitter source; the `res` attribute names of ContentDirectory:1 other
  than `size`.
- In the repository: the goal-16 design envelope; `docs/proposals/P6-casting-receivers.md`;
  `docs/conventions.md`; `crates/mqtt`, `crates/discovery`, `crates/cec/tests/fixtures.rs` and
  ADR 0116 (the shape of a pure protocol crate and its record);
  `tools/conventions/check-shared-fixtures.sh`, `check-identity.sh`, `check-adrs.sh`.
- **Not read, by rule:** no source file of any GPL or LGPL project was opened: not
  gmrender-resurrect, upmpdcli, MPD, GUPnP, Rygel, Platinum, Kodi, VLC, Snapcast or squeezelite.
  No source of rupnp, dlna-dmr, pupnp, quick-xml or roxmltree was opened either (registry
  metadata only). Nothing here was written from another implementation.

## Decision

1. **Hand-written from the specifications, in one crate with no dependency (P6's U1).**
   `chorus-upnp` depends on nothing, chorus's own crates included, forbids `unsafe`, opens no
   socket, starts no thread and reads no clock: time is an argument (`now_ms` from the caller's
   monotonic clock, `unix_s` for the two wall-clock labels, BOOTID and DATE) and randomness is
   an argument (16 octets for a subscription id, a `Jitter` for delays). Every rule carries its
   specification and section in the comment beside it. The server's scripted control point is
   written against the `client` module, which is held to the device side by round trips, so
   both ends of every message are exercised without a second implementation to keep in step.

2. **No XML crate; a reader with no DTD half.** The renderer reads three small dialects from
   untrusted peers (a SOAP envelope, DIDL-Lite, and for the control point side descriptions and
   event bodies) and writes fixed templates. `xml` is about 400 lines: escaping for text and for
   attributes, one `unescape`, a pull reader over elements, attributes, text and CDATA with
   prefixes resolved to namespace URIs, and a small tree built on it. Why that suffices and how
   its limits hold:
   - a `<!DOCTYPE`, and any other `<!` declaration that is not a comment or CDATA, ends the
     parse with an error (`XmlError::Doctype`). There is no code path that reads a DTD;
   - the only references ever replaced are the five predefined entities and numeric character
     references. An undeclared `&name;` stays text. So an external entity (XXE) and an
     expanding entity (billion laughs) cannot be expressed, by construction rather than by
     configuration, and tests feed both and assert the refusal;
   - the size (64 KiB), the nesting depth (32) and the attributes per element (32) are checked
     before the work they bound; the three numbers are chorus's own choice, generous for an
     action with metadata (a few kilobytes, under ten levels);
   - nesting is strict: a mismatched end tag is an error, since guessing would hand an action
     the wrong arguments. Prefixes, a missing declaration, a byte order mark, comments and
     trailing text are tolerated.
   Output escapes `&`, `<`, `>` (and `"` in attributes) and writes carriage return, and in
   attributes tab and line feed, as character references, so a value with line breaks comes back
   byte for byte through a conforming parser's normalisation.

3. **UDA 1.1, relative URLs, one port.** `specVersion` 1.1, `configId` on the root elements,
   `SERVER: <os>/<ver> UPnP/1.1 chorus/<ver>`, BOOTID, CONFIGID and optional SEARCHPORT. Every
   URL in a description is a path (`/upnp/<uuid>/desc.xml`, `/upnp/<uuid>/<avt|rcs|cm>/
   {scpd.xml,control,event}`), never a URL with a host, and there is no `URLBase` (UDA 1.1
   section 2.3). One listener then serves every renderer and `description::route` finds the
   device and resource from the path alone. **A departure from the research's recommendation,
   chosen here:** the SCPDs are under each device's own path rather than shared, because the
   `scpd` root carries the device's `configId`.
   CONFIGID is 24 bits of the SHA-1 of the description and the three SCPDs written with
   `configId="0"`: stable across restarts, changed by a rename, no state. BOOTID is
   `max(unix seconds, last + 1)` masked to 31 bits.

4. **Discovery timing is arithmetic here, and searches are rate limited.** `Announcer`: the
   first alive set after a 0 to 100 ms jitter, the set sent twice 200 to 300 ms apart (UDA 1.1:
   more than once, at most three times), then a refresh at a random interval between one third
   and 29/60 of max-age (600 to 870 s for 1800 s), which is under half of max-age as the
   specification recommends. Byebye sets are sent twice as well (an inference from the alive
   rule). Search responses are spread over nine tenths of the MX window, MX capped at 5 s.
   `SearchLimiter` answers at most 8 searches a second per source and tracks at most 256
   sources (chorus's numbers, not the specification's): a spoofed `ssdp:all` draws six
   datagrams per renderer, and a full table refuses rather than grows.

5. **Identity is UUID version 5.** `udn(namespace, server_id, target)` is UUIDv5 over
   `<server id>/<target name>`, with target names `room:<id>`, `group:<id>` and `live:<member
   ids sorted by byte order, each once, joined by +>`. The function sorts, so two orderings of
   one member set are one UDN and a live group that re-forms is the same device. The namespace
   is one random version 4 UUID fixed in the code (`77370a15-ddff-429d-b8dd-619ee6830a62`,
   drawn on 2026-10-03); two servers on one network stay apart through the server id in the
   name. SHA-1 is written here (it guards nothing; version 5 is defined over it) and held to
   RFC 3174's vectors and RFC 9562's.

6. **One table.** `description::{AVTRANSPORT, RENDERING_CONTROL, CONNECTION_MANAGER}` list each
   service's actions, arguments and state variables once. The SCPD text is written from them,
   `soap::validate` checks a request against them (unknown action 401, missing input 402), the
   state machines answer in their order, and the tests read the SCPD *text* back with the
   control point's parser and hold it to the rules (every related variable exists, inputs
   before outputs, LastChange the only evented variable of AVTransport and RenderingControl).
   Offered: every required action, plus AVTransport's SetNextAVTransportURI, Pause and
   GetCurrentTransportActions and RenderingControl's GetMute, SetMute, GetVolume and SetVolume.
   Seek modes: the required TRACK_NR, and REL_TIME.

7. **What is refused**, each by a named code or status:
   - Record, SetRecordQualityMode, SetPlayMode, PrepareForConnection, ConnectionComplete, the
     decibel and loudness actions: absent from the SCPDs, 401 if called;
   - InstanceID other than 0: 718 on AVTransport, 702 on RenderingControl; ConnectionID other
     than 0: 706;
   - Play with a Speed other than `1`: 717; with no media: 701; Seek units other than REL_TIME
     and TRACK_NR: 710; a bad or out-of-range target, Next with no next URI, Previous: 711;
   - a Channel other than Master, a volume outside 0 to 100, a mute that is not a boolean: 402
     (RenderingControl:1's action tables list only 402, 501 and 702; it has no code for a
     channel the device lacks);
   - a URI that is not `http://` or `https://`: 716, before anything is stored;
   - a DOCTYPE in a control request: no action is parsed (the server answers 400); in metadata:
     "no metadata", and the action still succeeds;
   - SUBSCRIBE with SID and NT or CALLBACK: 400; without a valid callback or with the wrong NT,
     and an unknown SID: 412; a full subscription table (16 per service, chorus's number): 503.

8. **The security rules.**
   - *CallStranger* (CVE-2020-12695): `gena::callback_allowed` applies UDA 2.0 section 4.1
     ("a delivery URL not on the same network segment ... shall not be accepted"), P6's stricter
     form (the address must lie in a configured list of household subnets, by default the
     server's own, so a private address on some other network is refused too) and one rule more:
     **the callback host must be the address the SUBSCRIBE came from**. Only `http://`; never a
     link-local, multicast, broadcast or unspecified address; loopback only when a test allows
     it. **Host names are refused**: resolving one is a network act a pure crate does not
     perform, the answer could change between the check and each delivery, and control points
     send their own address. The RFC 1918 ranges UDA 2.0 lists are not hard-coded in the rule:
     the configured subnets are the stricter statement, and `gena::is_private_v4` is offered so
     the server can warn about a subnet outside them.
   - *DOCTYPE*: point 2.
   - The album art address in metadata is kept as text and never fetched.

9. **The Sink list, and the honest `audio/mp4`.** `http-get:*:<mime>:*` for audio/mpeg,
   audio/flac, audio/x-flac, audio/ogg, application/ogg, audio/wav, audio/x-wav, audio/wave,
   audio/L16, audio/mp4, audio/x-m4a and audio/m4a. No AAC type under any name (not audio/aac,
   audio/aacp, audio/x-aac or audio/vnd.dlna.adts) and no `DLNA.ORG_PN`; a test asserts both.
   **`audio/mp4` is listed although chorus has no AAC decoder, and this must be said plainly:**
   MP4 is a container and `audio/mp4` is the only name ALAC has, while most files under that
   name hold AAC. The list promises the container. What is inside is decided at play time from
   the file: ALAC plays; AAC is refused by name when the stream is opened, and the transport
   goes to STOPPED with TransportStatus ERROR_OCCURRED. The alternative, leaving `audio/mp4`
   out, would make ALAC unplayable from any server that does not transcode. A control point may
   therefore offer an AAC file and see it fail; that is the cost, and it is visible.

10. **Volume.** Master only, 0 to 100, step 1, in a static SCPD. Linear to chorus's
    thousandths: `v * 10` one way, `(t + 5) / 10` back (nearest, halves up), so every UPnP step
    round-trips exactly. A room's limit does not shrink the range (that would make the SCPD
    per-renderer and change it, and so the CONFIGID, whenever the limit changes): SetVolume
    above the limit succeeds, the server applies it through the control plane, which clamps,
    and reports the value that holds; GetVolume and the event say that real value. An action
    therefore never changes the variables itself, only the report does, and a change from any
    other controller is evented the same way. SelectPreset `FactoryDefaults` succeeds and
    changes nothing (a preset that moved a room's volume by surprise would be a loudness jump).

11. **Event moderation.** `lastchange::Moderator`, one per service instance: at most one
    LastChange per 200 ms (AVTransport:1 and RenderingControl:1, section 2.3, table 2), changes
    coalesced to the latest value per variable and channel, the first change after a quiet
    period sent at once, and the four position variables refused at the queue's door so no path
    can event them (AVTransport:1 section 2.3.1). The state machine records by comparing every
    evented variable before and after each input, so nothing changes unevented. Metadata is
    escaped twice on its way out (once as a `val` attribute, once as the property's text) and a
    test unescapes it twice back to the bytes the control point sent.

12. **The state machine and the gapless inference.** `AvTransport` takes actions (returning out
    arguments and `Effect`s for the server: Load, Start, Pause, Resume, Stop, SeekTo, QueueNext,
    ClearNext, SkipToNext) and reports from the player (`media_opened`, `playing`,
    `track_boundary`, `next_failed`, `ended`, `failed`, `position`), and never waits: an action
    has 30 seconds (UDA 1.1 section 3.2.1) and a fetch can take longer, so failures arrive as
    reports. States: NO_MEDIA_PRESENT, STOPPED, TRANSITIONING, PLAYING, PAUSED_PLAYBACK.
    At the handover AVTransport:1 section 2.4.2.3 (and AVTransport:3 section 5.4.3.3, nearly
    word for word) says AVTransportURI takes NextAVTransportURI's value, the metadata likewise,
    and NextAVTransportURI becomes empty. **Neither says what TransportState is at the boundary
    or which events fire. chorus keeps PLAYING, with no TRANSITIONING and no STOPPED, and that
    is an inference:** the action exists "to provide a seamless transition", the
    specification's own test for TRANSITIONING is a "noticable amount of time" before the media
    is heard, and a control point shown STOPPED there would take the track for ended and push
    the next one itself. All the variables move in one LastChange. If the next URI was good but
    not ready in time, there is a real wait, and the machine says so with TRANSITIONING. A next
    URI that fails keeps the state until the current one ends, then STOPPED with
    ERROR_OCCURRED (section 2.4.2.3). The next URI can be replaced or cleared up to the
    handover.
    Where the specification allows a choice or is silent, and what was chosen: Stop with no
    media succeeds quietly (the specification allows a 701; control points send Stop before
    every SetAVTransportURI); SetAVTransportURI with an empty URI clears the media and with a
    new URI clears the queued next one; Next with a queued next URI goes to it (the
    specification does not connect Next to the next URI) and Previous is always 711; Seek is
    also allowed while paused; Play while TRANSITIONING succeeds.
    Reports carry an **epoch**, a counter that goes up when what is loaded or whether it runs
    is decided anew; a report from an older epoch is ignored, so a late "ended" from the
    player's thread cannot stop a track the control point has since started.

13. **Fixtures.** `fixtures/upnp` is Rust-only by declaration. 109 vectors were typed from the
    specifications' templates (by a script of literal strings, not by the crate): the SSDP
    messages byte for byte, searches with their expected reading and responses, SOAP requests
    in several dialects with their parsed fields, responses and faults, SUBSCRIBE variants with
    their status, the callback rule's cases, NOTIFY bodies, LastChange documents and DIDL-Lite
    samples (one with DLNA fields, one hostile). Four are generated by `make upnp-vectors` (the
    description and the SCPDs) and a test holds them current. One test walks every kind and
    fails if a file in the directory is read by none of it.

## Options not chosen

| Option | Licence | Why not |
|---|---|---|
| rupnp 3.0.0 (with ssdp-client 2.1.0) | MIT/Apache-2.0 (crates.io API, read 2026-10-03 by the research) | A control point library, not a device; async, on hyper and tokio, in a server with no runtime and a fixed, graded thread population. As a test control point it would hide the wire details the tests exist to check |
| dlna-dmr 0.1.3 | MIT (P6, crates.io API read 2026-09-30) | A renderer framework with its own HTTP server and device model; chorus needs sixteen or more root devices on one port that follow the control plane's rooms and groups, live groups appearing and leaving, and its own event and fetch rules. P6's U2 was declined for this |
| pupnp (libupnp) | BSD-3-Clause (P6, GitHub licence API read 2026-09-30) | A C library with its own threads and callbacks inside the server; the device model would still have to be mapped onto chorus's targets. P6's U3 |
| quick-xml 0.42.0 | MIT (crates.io API, read 2026-10-03 by the research) | A general streaming XML library (one required dependency, memchr) for three tiny dialects; the untrusted-input limits and the refusal of DOCTYPE would be configuration and review of someone else's parser rather than the absence of the code. P6 named it; the envelope settled on no XML crate |
| roxmltree 0.21.1 | MIT OR Apache-2.0 (crates.io API, read 2026-10-03 by the research) | A read-only tree with namespaces (one dependency, memchr): the nearest fit, and the fallback if the reader here proves too small. Not needed: the tree here is about 100 lines on the pull reader |
| A per-install random namespace for UDNs | | Needs state that a wiped state directory loses, and then every control point sees new devices; the server id in the name does the same job |
| A lower `maximum` in the SCPD for a volume limit | | Per-renderer SCPDs that change with the limit: a description change, so byebye, a new CONFIGID and lost subscriptions |
| Leaving `audio/mp4` out of the Sink list | | ALAC would be unplayable from servers that do not transcode (point 9) |

## Consequences and open points

- Track R owns the sockets and must: answer 400 for `SoapError`, 415 for a content type that is
  not `text/xml`, and 500 with `soap::build_fault` for a `UpnpError`; flush the SUBSCRIBE
  response before sending `Subscriptions::initial`; call `RenderingControl::report` after
  applying a volume or mute effect; tag the player's reports with `AvTransport::epoch`; drop a
  renderer's subscriptions when it byebyes.
- `audio-path.conf` is untouched: no unit on the audio path depends on this crate yet. When
  chorus-server gains the dependency, the completeness rule will ask for
  `crates/upnp/src/lib.rs` under `[excluded]` (control code: no PCM, no timestamp on the wire,
  reads no clock), as it did for `crates/mqtt`.
- Interop with real control points is goal 17's. The places most likely to move there: the
  Sink list's spellings, whether a control point wants `DLNA.ORG_PN` (to be brought to the
  owner, not added silently), Stop with no media, and the PLAYING-at-the-boundary inference.
- IPv6 discovery (UDA 1.1 annex A) is not built; the callback rule and the subnet type already
  take IPv6 addresses.
