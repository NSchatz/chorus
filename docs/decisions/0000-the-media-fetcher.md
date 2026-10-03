# 0000: media is fetched by a hand-written blocking HTTP/1.1 client under a resolved-address policy, with TLS from rustls and ring over the system's root bundle, ICY metadata taken out of the audio, and HLS only as packed MP3 with every other kind refused by name

- Status: proposed (goal 16, 2026-10-03)
- Decided by: the goal (program section 20) inside the coordinator's goal-16 design envelope
  (track `chorus-g16/fetch`); the policy's rules are the owner's (brief section 4.8, proposal
  P6 "Fetching", approved at Checkpoint K); AAC staying off is the owner's (P9). Every number
  below not cited is ASSUMED
- Implemented in: `crates/fetch` (new, package `chorus-fetch`): `url.rs` (the URL parser and
  reference resolution), `policy.rs` (`Policy`, the address rules, the own-address probe),
  `http.rs` (the client: connect, request, head, the three body framings, redirects, Range),
  `tls.rs` (the rustls configuration and handshake), `icy.rs` (the metadata filter),
  `hls/playlist.rs` (the playlist parser, variant choice, live entry point), `hls/mod.rs` (the
  segment stream and the reload schedule), `lib.rs` (`open`, `Stream`, `Opened`), `error.rs`
  (`FetchError`); `crates/fetch/tests/` (in-test servers on loopback: `http.rs`, `icy.rs`,
  `hls.rs`, `https.rs`, the playlists under `playlists/`). The owner's page is
  `docs/streams.md`

## Context

Goal 16 makes chorus-server play media it is given by URL: a UPnP control point's
`SetAVTransportURI`, and later stored alarm streams and Home Assistant's media URLs. Until now
the server only ever opened connections to peers it had adopted. Fetching a URL somebody else
chose is new, and it is the one place the brief's security section speaks to directly: "No
arbitrary URL fetch except the input paths the decisions name" (brief 4.8), which P6 turns into
a rule for the fetch itself: "Only `http` and `https`, no redirects to loopback, link-local or
the server's own services, bounded size and time."

What the fetcher has to carry, from the survey of what control points and radio stations send:
plain files with a length (seekable, for MP4 files whose index is at the end), chunked bodies,
endless radio streams with no length, SHOUTcast's non-HTTP status line and in-band metadata,
https (stored stream URLs are mostly https), and HLS playlists, which an owner will paste
because that is what many stations publish.

chorus-server has no async runtime and a fixed, graded thread population; its HTTP server and
its one HTTP client (`crates/ctl`) are written by hand over `std::net`. The fetcher is a
library a player thread calls; it starts no thread of its own.

## Decision

### 1. The client is written here, blocking, over `std::net`

`chorus_fetch::open(url, &policy)` returns a `Stream` that implements `Read`. One request per
connection (`Connection: close`). The request is `GET` with `Host`, `User-Agent:
chorus/<version>`, `Accept: */*`, `Accept-Encoding: identity`, `Icy-MetaData: 1` and, for a
seek, `Range: bytes=<n>-`. The response parser accepts `HTTP/1.x` and `ICY` status lines,
folded header lines and bare line feeds, skips interim (1xx) responses, and frames the body
as RFC 9112 section 6.3 says: chunked, else `Content-Length`, else until the connection
closes. Nothing buffers a body: the reader's pace is the pace, behind one 16 KiB socket buffer.
A body that ends before its length or inside a chunk is an error (`UnexpectedEof`), never a
quiet end.

Why written here (BRIEF 3.2: build what is small and instructive, vendor what is large and
undifferentiated): the part of HTTP a media fetch needs is small (the client is about 500
lines with its tests beside it), and the parts that matter to chorus are exactly the ones a
general client hides or forbids. The policy must see the resolved address and connect to that
same address at every hop; `ICY 200 OK` is not HTTP and a strict parser rejects it; the body
must be handed over as a stream with a seek that re-requests. The general clients (`ureq`,
`reqwest`, `hyper`) bring their own resolver hooks, redirect handling and, for two of the
three, an async runtime, which chorus-server does not have (ADR 0116's context); each would still need
the policy, ICY and HLS written around it. TLS is the opposite case: large, undifferentiated
and dangerous to write, so it is vendored (section 3).

Redirects: 301, 302, 303, 307 and 308 are followed up to `Policy::max_redirects`; each
`Location` is resolved against the URL that sent it (RFC 3986 section 5.2), parsed like any
URL (so a redirect to another scheme or to a URL with userinfo is refused), resolved and
checked like the first. One more redirect than the bound is `refused: more than N redirects`.
A redirect from https to http is followed: radio stations do it, and the content is audio the
owner asked for, not a credential (ASSUMED acceptable; nothing in the brief speaks to it).

Seeking: `Opened::seekable` is true when the response said `Accept-Ranges: bytes` and
declared a length (and is not an ICY-interleaved stream). `Stream::seek` only records the
target; the next read reaches it by reading up to 256 KiB forward on the open connection
(ASSUMED threshold) or by a new request with `Range`, which must come back 206 with a
`Content-Range` starting at the target. A server that ignores the range (200) is an error of
kind `Unsupported` at that read, not wrong bytes. A seek to or past the end reads as the end
without a request.

### 2. The fetch policy is a pure function over the resolved address

`policy::check_address(addr, &policy, &is_own)` decides about one `SocketAddr`. The client
resolves a name once, checks every address it resolved to, and connects only to addresses
that passed: the address checked is the address connected to, so a name that answers
differently a second time (DNS rebinding) is never asked a second time. A URL whose host is
written in an unusual numeric form is not parsed here at all: it goes to the resolver and its
answer is what is checked.

| Refusal (the words after `refused: `) | What | Source |
|---|---|---|
| `scheme <s>: only http and https are fetched` | any other scheme, in the URL, a redirect or a playlist entry | brief 4.8, P6 |
| `userinfo in the url: ...` | `user:password@` | this record: credentials do not belong in a URL chorus stores and logs; RFC 9110 section 4.2.4 deprecates it |
| `unspecified address <ip>` | `0.0.0.0/8`, `::` | RFC 1122 section 3.2.1.3; on Linux a connect to `0.0.0.0` reaches the local host (ASSUMED) |
| `loopback address <ip>` | `127.0.0.0/8`, `::1`, unless `allow_loopback` | P6 |
| `link-local address <ip>` | `169.254.0.0/16`, `fe80::/10` | P6; RFC 3927, RFC 4291 section 2.5.6 |
| `multicast address <ip>` | `224.0.0.0/4`, `ff00::/8` | this record: never a unicast HTTP server |
| `broadcast address <ip>` | `255.255.255.255` | this record, likewise |
| `the server's own port <p> at <ip>` | any address of this machine at a port in `denied_ports_on_self` | P6 ("the server's own services") |
| `more than <n> redirects` | the redirect bound | P6 ("bounded") |
| `response headers larger than <n> bytes` | the header bound | P6 ("bounded size") |

An IPv4-mapped IPv6 address (`::ffff:a.b.c.d`) is judged as the IPv4 address it reaches.
Private ranges (RFC 1918, IPv6 unique local) are allowed on purpose: control points serve
media from the household's own network, which is the feature. The class of attack the rules
answer is server-side request forgery: a renderer on the LAN made to read the host's
loopback services, a cloud-style metadata address, or chorus's own unauthenticated control
API (ADR 0027) and hand the result to whoever asked. No CVE is cited: none was read for this
record.

"An address of this machine" is found with no interface enumeration: an address is ours when
a UDP socket can be bound to it (`policy::is_own_address`). `std` offers no interface list
and `getifaddrs` would need `unsafe` and a new entry in the unsafe table. The probe is asked
only when the port is a denied one. Where the kernel allows binding to a foreign address
(`ip_nonlocal_bind`) every address looks like ours and the denied ports are refused
everywhere: it fails closed. Loopback always counts as this machine.

`Policy` carries the bounds; `Policy::default()` is the production policy with no ports
named. Its values are ASSUMED, not measured: 5 redirects, 10 s to connect, 15 s per socket
read or write and for a whole response head, 32 KiB of response headers. The server (track
R) names its own listeners in `denied_ports_on_self`. Tests set `allow_loopback`; nothing
else does.

### 3. HTTPS is rustls 0.23.45 with the ring provider; the roots are a PEM bundle on disk

`rustls = "=0.23.45"` with `default-features = false` and features `ring`, `std`, `tls12`.
The provider is passed to `ClientConfig::builder_with_provider` explicitly; nothing reads or
installs a process-wide default, so a second user of rustls in the process cannot change what
the fetcher does. TLS 1.2 and 1.3. ALPN offers `http/1.1` only. Host name verification is
rustls's and there is no switch, flag or policy field that turns verification off.

The handshake is run to its end inside `open`, so a certificate that is not accepted is
`FetchError::Tls` before any request is sent.

Roots: the PEM bundle at `Policy::ca_bundle`, else the file `SSL_CERT_FILE` names, else
`/etc/ssl/certs/ca-certificates.crt`, read with `rustls-pki-types`' PEM reader (a dependency
rustls already has). A bundle that cannot be read is `tls: ca bundle <path>: cannot be
read: ...`; one with no usable certificate is `tls: ca bundle <path>: holds no usable
certificate`. Both are raised before a socket is opened, per URL, and only for https: a
server without a bundle still plays http.

Fitness, option by option (rule 8: no argument below rests on what a development container
happens to have installed):

| Option | For chorus | Against |
|---|---|---|
| **rustls + ring (chosen)** | memory-safe TLS; 7 crates new to the build on Linux (rustls, ring, rustls-webpki, rustls-pki-types, untrusted, getrandom, once_cell), every licence already on the allowlist; ring's C and assembly build with a C compiler alone, which a static musl server binary can carry | a second C build in the server after libopus; ring's maintenance is by a small team (ASSUMED, its repository's status was not re-read) |
| rustls + aws-lc-rs (rustls's default provider) | post-quantum key exchange, a FIPS path | a much larger C library built with cmake inside the release image for two properties a radio client does not need; more build-time dependencies to pin and audit |
| rustls + `webpki-roots` | no file needed at run time | CDLA-Permissive-2.0 is not on the allowlist (an exception by ADR); the roots would age with the binary and be chorus's to update, where the image's bundle updates with the base image |
| `rustls-native-certs` for the roots | finds the bundle on more platforms | one more crate (plus `openssl-probe`) to do what one path and one environment variable do on the two platforms chorus-server runs on |
| native-tls / OpenSSL | none | a system library the static image does not have, C in the trust path |
| http only | no TLS code at all | stored stream URLs and most stations are https; the feature would refuse most of what it is for |

The cost stated plainly: certificate validity is checked against the system's wall clock,
inside rustls. A server whose clock is far wrong refuses https streams by a certificate date
error. That is correct behaviour and it is outside the audio path: no chorus code here reads
a settable clock, and nothing a certificate check decides reaches a timestamp.

### 4. ICY: the metadata is taken out of the audio, the title is kept

Every first request says `Icy-MetaData: 1`. When the response has `icy-metaint: N`, the
stream's `Read` removes one metadata block after every N bytes (a length byte L, then 16 x L
bytes) and keeps the latest `StreamTitle`, which `Stream::stream_title()` returns.
`icy-name`, `icy-br` and `icy-genre` are in `Opened::icy`. The filter is a pure state machine
(`icy::IcyFilter`) that works in place on the caller's buffer and is tested at every read
size, so a block split across reads and a zero-length block are ordinary cases. An
`icy-metaint` of 0 means no blocks; one above 1 MiB (ASSUMED bound) or not a number is
`malformed`. Range requests after a seek do not send the header, and an interleaved stream
is never seekable.

There is no standard for ICY. The header names come from the write-up cited below; the block
layout, the padding, the `StreamTitle='...';` syntax, the rule for a title that contains
`';`, and reading the text as UTF-8 else Latin-1 are ASSUMED from that page's examples and
common practice, and are marked so in `icy.rs`.

### 5. HLS: packed MP3 only, everything else refused by name

A response is a playlist when its content type is `application/vnd.apple.mpegurl`,
`audio/mpegurl` (RFC 8216 section 4), `application/x-mpegurl` or `audio/x-mpegurl`, or when
its body starts `#EXTM3U`. `open` then returns a `Stream` whose bytes are the playlist's
segments one after another.

- Master playlist (section 4.3.4.2): the variant with the highest `BANDWIDTH` whose `CODECS`
  are all MP3 (`mp4a.40.34`; also `mp4a.69`, `mp4a.6B`, ASSUMED) or absent. When none is
  playable and every variant carries AAC: `unsupported: hls: aac (<codec>)`.
- Media playlist: `EXT-X-TARGETDURATION`, `EXT-X-MEDIA-SEQUENCE`, `EXTINF`,
  `EXT-X-ENDLIST`; `EXT-X-DISCONTINUITY` is tolerated (the concatenation simply continues;
  an MP3 decoder resynchronises on the next frame header); `EXT-X-KEY:METHOD=NONE` is
  accepted. Unrecognised tags are ignored, as section 6.3.1 requires. What sections 4.1 to
  4.3 say must fail to parse fails: a byte order mark, two `EXT-X-VERSION` tags, master and
  media tags together, a repeated attribute, a missing `BANDWIDTH`, `EXTINF` or
  `EXT-X-TARGETDURATION`.
- Entry point (section 6.3.3): an ended playlist from its first segment; a live one from
  the latest segment that still has three target durations after its start.
- Reload (section 6.3.4), on `std::time::Instant`: after a load that found the playlist
  changed (the first load counts), no reload before one target duration has passed since
  that load began; after a load that found it unchanged, half a target duration. The next
  segment is the lowest media sequence number above the last one queued (section 6.3.5); a
  reader that fell behind the window resumes at the window's start. The stream ends at
  `EXT-X-ENDLIST`. ASSUMED bounds the RFC does not set: a playlist unchanged for six target
  durations ends the stream with a timeout; waits are never shorter than 250 ms; a playlist
  is at most 1 MiB.
- Segments (section 3.4): the ID3 tag at the start of each (the `PRIV` timestamp frame) is
  skipped by its declared length, so the concatenation is a clean MPEG audio stream. A
  segment with no ID3 tag is played all the same (the RFC's "SHOULD NOT play" is for
  clients that align renditions by the timestamp; chorus plays one rendition).
- Every playlist and segment URL is resolved against the playlist's URL and goes through
  the same request path, so the policy applies to each one.

Named refusals (`FetchError::Unsupported`, the words after `unsupported: `):

| Words | When |
|---|---|
| `hls: aac (<codec>)` | every variant's `CODECS` carries an `mp4a.40.x` other than `.34` |
| `hls: aac (adts segments)` | a segment starts with an ADTS sync word |
| `hls: mpeg-2 transport stream segments` | a segment starts with the 0x47 sync byte |
| `hls: fragmented mp4 segments (EXT-X-MAP)` / `hls: fragmented mp4 segments` | the tag, or a segment starting with an `ftyp`, `styp` or `moof` box |
| `hls: encrypted segments (EXT-X-KEY METHOD=<m>)` | any method but `NONE` |
| `hls: byte-range segments (EXT-X-BYTERANGE)` | the tag |
| `hls: ac-3 segments` | a segment starts with the AC-3 sync word |
| `hls: protocol version <n> (EXT-X-VERSION above 7)` | section 7: a client "MUST NOT attempt playback" of a version it does not support |
| `hls: an I-frame playlist (EXT-X-I-FRAMES-ONLY)` | the tag |
| `hls: no playable variant (codecs <list>)` | nothing playable and not all AAC (video, AC-3) |
| `hls: segments that are not packed mp3 audio` | anything else |
| `m3u: a plain playlist, not HLS ...` | an `#EXTM3U` list of stream URLs with no HLS tag |

Said plainly: **with AAC off, most real-world HLS radio is refused.** HLS audio in the wild
is overwhelmingly AAC, in ADTS, transport stream or fragmented MP4 segments. P9 accepted
that when it kept AAC out of the build. What this scope buys is that an HLS URL gets one
precise sentence instead of a decoder's confusion, and that the stations that do publish
MP3 over HLS play. The first segment is fetched and judged inside `open`, so the refusal
arrives where the URL was given; a later segment that turns out different stops the stream
with the same words, carried inside the `io::Error` (`FetchError::in_io`).

### 6. Limits and timeouts

| Bound | Value | Where it is set | Basis |
|---|---|---|---|
| Redirects | 5 | `Policy::default` | ASSUMED |
| Connect | 10 s per address, at most 3 addresses of a name | `Policy::default`, `http.rs` | ASSUMED |
| Socket read and write | 15 s | `Policy::default` | ASSUMED |
| A whole response head | the read timeout | `http.rs` | ASSUMED; stops a server that drips headers |
| Response headers | 32 KiB | `Policy::default` | ASSUMED |
| URL length | 4096 bytes | `url.rs` | ASSUMED |
| Chunk-size line, trailers | 256 bytes, 8 KiB | `http.rs` | ASSUMED |
| `icy-metaint` | 1 MiB | `icy.rs` | ASSUMED |
| Playlist | 1 MiB | `hls/mod.rs` | ASSUMED |
| ID3 at a segment's start | 1 MiB | `hls/mod.rs` | ASSUMED |
| Forward seek served by reading | 256 KiB | `lib.rs` | ASSUMED |

Not bounded here, and said so: the resolver. A name is resolved by the system's
`getaddrinfo` through `std`, which has no timeout of its own; a dead DNS server holds `open`
for the resolver's configured time. A body has no total size bound on purpose: a radio
stream is endless and the reader's pace bounds what is held.

## Not chosen

- **A general HTTP client crate** (`ureq`, `reqwest`, `hyper`): section 1.
- **aws-lc-rs, `webpki-roots`, `rustls-native-certs`, native-tls, http only**: section 3.
- **Keep-alive and a connection pool.** HLS fetches a segment every few seconds from one
  host, and a pool would save a handshake each time. It would also add connection reuse
  rules (a body not read to its end, a server that closes an idle connection under a
  request) to a client whose every request now has one simple lifetime. Revisit if a
  measurement shows the handshakes matter.
- **Sending `Range: bytes=0-` on the first request** to learn whether a server supports
  ranges. Some stream servers answer a range request oddly; `Accept-Ranges` on the plain
  answer is the declared signal.
- **Decompression** (`Content-Encoding: gzip`). The request says `identity`; audio is
  already compressed. A response that is encoded anyway is `unsupported: http: content
  coding <name>`.
- **Following a plain M3U or PLS** to the stream it names. A second kind of indirection
  with its own rules; the refusal tells the owner to use the stream's own URL. Cheap to add
  if owners meet it.
- **AES-128 HLS.** It needs AES-CBC and a key fetch for streams that are then, almost
  always, AAC anyway.
- **HTTP/2.** A media `GET` gains nothing from it; ALPN says `http/1.1`.
- **A test certificate generator crate (`rcgen`)** or committed test keys. The tests write
  their own X.509 with a 60-line DER writer and sign it with ring's Ed25519
  (`crates/fetch/tests/common/pki.rs`): no key in the repository, no new crate (ring is a
  direct dev-dependency at the version rustls already locks).
- **Enumerating interfaces** for "the server's own addresses": section 2.

## Consequences

- The server gains its second C build (ring) and its first TLS stack. The release image's
  musl build needs a C compiler for that target; that is track D's change to
  `tools/image.sh`, not this record's.
- `Cargo.lock` gains rustls, ring, rustls-webpki, rustls-pki-types, untrusted, getrandom,
  once_cell, and the platform-only crates ring and getrandom name for targets chorus does
  not build (wasi, windows-sys, windows-targets and its eight per-target crates). No licence
  exception; no crate in two versions (`tools/conventions/check-licence.sh`).
- `chorus-fetch` is not on the audio or timestamp path and is not in `audio-path.conf`: no
  unit listed there depends on it yet. When the server's player thread uses it (track R),
  that thread's unit decides whether it is listed or excluded, with its reason.
- The crate starts no thread. An HLS reload wait is a sleep inside `read`, on the player's
  thread, which is the thread that has nothing else to do until the next segment exists.

## What was read

All on 2026-10-03.

- The program brief, section 4.8 (`.claude/goals/2026-09-chorus.md`); proposal P6, "Fetching
  (4.8)" (`docs/proposals/P6-casting-receivers.md`); proposal P9 (AAC off); BRIEF.md
  section 3.2; the goal-16 design envelope and its research notes on decoders and streams.
- RFC 8216, HTTP Live Streaming, https://www.rfc-editor.org/rfc/rfc8216 : sections 3.4,
  4 (the content types), 4.1, 4.2, 4.3.1.1, 4.3.1.2, 4.3.2 (4.3.2.1, 4.3.2.2, 4.3.2.4,
  4.3.2.5), 4.3.3.1 to 4.3.3.4, 4.3.3.6, 4.3.4.2, 6.2.1, 6.3.1 to 6.3.5, 7 and the examples
  of section 8.
- RFC 9112, HTTP/1.1, https://www.rfc-editor.org/rfc/rfc9112 : sections 4, 5, 5.2, 6.3,
  7.1; RFC 9110, HTTP Semantics, https://www.rfc-editor.org/rfc/rfc9110 : sections 4.2.4,
  7.2, 14.1.2, 14.2, 14.4, 15.2, 15.4. Section numbers were checked against the tables of
  contents; the rules themselves are implemented as the tests state them, and where the
  text was not re-read line by line the code comment says ASSUMED.
- RFC 3986, URI Generic Syntax, https://www.rfc-editor.org/rfc/rfc3986 : sections 3, 3.1,
  3.5, 5.2, 5.4.1 (the resolution examples the tests use).
- RFC 5280 section 4.1 and RFC 8410 section 3, for the test certificates only.
- ICY: https://cast.readme.io/docs/icy (the header names; there is no standard).
- ID3v2.4.0 structure, https://id3.org/id3v2.4.0-structure : section 3.1 (the header, the
  synchsafe size, the footer flag).
- Apple, HLS Authoring Specification for Apple Devices, appendixes,
  https://developer.apple.com/documentation/http-live-streaming/hls-authoring-specification-for-apple-devices-appendixes
  : the CODECS table (`mp4a.40.34` is "MP3 audio").
- crates.io, https://crates.io/api/v1/crates/<name>/<version> : rustls 0.23.45
  (2026-09-14, Apache-2.0 OR ISC OR MIT; default features aws_lc_rs, logging,
  prefer-post-quantum, std, tls12), ring 0.17.14 (2025-03-11, Apache-2.0 AND ISC),
  rustls-webpki 0.103.15 (2026-08-21, ISC), rustls-pki-types 1.15.1 (2026-07-23, MIT OR
  Apache-2.0), untrusted 0.9.0 (2021-07-13, ISC), getrandom 0.2.17 (2026-01-11, MIT OR
  Apache-2.0), once_cell 1.21.4 (2026-03-12, MIT OR Apache-2.0); not chosen: aws-lc-rs
  1.18.1 (ISC AND (Apache-2.0 OR ISC)), webpki-roots 1.0.9 (CDLA-Permissive-2.0), rcgen
  0.14.10 (MIT OR Apache-2.0).
- rustls 0.23.45 API documentation, https://docs.rs/rustls/0.23.45/rustls/ (the builder
  with an explicit provider, `RootCertStore`, `StreamOwned`), and the public API of
  rustls-pki-types and ring as their crates document them. rustls, ring, rustls-webpki and
  their dependencies are permissively licensed; their source was not needed.
- distroless base image contents (ca-certificates),
  https://github.com/GoogleContainerTools/distroless/blob/main/base/README.md , as the
  design envelope's research read it.
- This repository: `crates/ctl/src/client.rs` and `crates/server/src/control.rs` (the house
  style for hand-written HTTP), `crates/audio-path` (what the path list requires), ADRs
  0026, 0027, 0039, 0044, 0116.

Clean-room: no GPL or LGPL source was opened (not FFmpeg, VLC, GStreamer, MPD, mpg123 or
any player's HLS or ICY code). curl, which is permissively licensed, was not read either.
