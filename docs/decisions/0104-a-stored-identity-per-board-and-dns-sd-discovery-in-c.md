# 0104: a board keeps a random id, its Noise key and its server's pin in a key-value store, finds the server by a DNS-SD browse written in C over the Rust crate's vectors, and falls back to the last server it shook hands with, then to a static address that is not loopback

- Status: accepted (goal 14, 2026-10-02)
- Decided by: the goal-14 design envelope, sections 1 and 5 (track `chorus-g14/identity-discovery`,
  PR #104); K27 (no hardware address in anything published), K91, K92; audit A-10's
  server-address part; BRIEF.md section 3.1 and CLAUDE.md rules 3 and 5
- Implemented in: `firmware/src/identity.c`, `firmware/src/discovery.c`, `firmware/src/store.c`
  with their headers under `firmware/include/chorus/`; `firmware/src/session.c` (the `store` and
  `relocate` seams of `chorus_session_config_t`); `firmware/main/esp_identity.c`,
  `esp_discovery.c`, `esp_store.c` and two calls in `app_main.c`; the host session binary
  (`firmware/tests/main_session.c`: `--store`, `--discover`, `--discover-ms`, `--no-server`) with
  `firmware/tests/file_store.c` and `posix_discovery.c`; held by `firmware/tests/test_identity.c`,
  `test_discovery.c`, `identity-session.sh` (all in `make firmware-check`) and
  `tools/endpoint-mdns-live-run.sh` (`make verify-endpoint-mdns`, outside the gate)

## Context

Until this change a board had no identity and no way to find its server. `app_main.c` set neither
an id nor a key path, so every board presented the id `chorus-endpoint` and a Noise key made
fresh at each boot: the server adopted the first boot and refused the second with `key_changed`,
and two boards were one id. The server address was `server_address = 127.0.0.1:4010` from
`endpoint.conf`, which no board can reach anything at (audit A-10). Goal 14's line D is
auto-adoption; this is its firmware half: a board that stays the speaker it was adopted as, and
finds the server by itself.

## Decision

**The id is random, not the MAC.** `chorus-` and twelve lower-case hex digits, six bytes from
`chorus_noise_system_random` (PSA) at the first boot, kept in the store under `id`. An id is
written to the server's log and state and to pasted bench output; a hardware address there is
identity the owner did not choose to publish (K27). Forty-eight random bits: for 100 speakers
in one house the chance that any two share an id is about 100^2 / 2^49, under 2 in 10^11, and a
collision is not silent (the second board's key is not the pinned one, so it is refused by
name). The id satisfies the control catalog's identifier rule (at most 32 characters), so one id
serves both planes.

**A value the store did not keep is not used.** A key or id that was made and not written would
be a different one at the next boot, which is the refusal this change exists to end. So a failed
write is `not-saved`, the session ends `identity-unusable`, and the board opens no session
(`app_main.c` puts the output stage back to high impedance). An unreadable store is not an empty
one (a new identity over a store that only failed to answer turns one speaker into two) and a
stored value that is not an identity is refused and never overwritten: only the owner replaces an
identity. The same holds for the pins: unreadable pins are refused, because an endpoint that
forgot its pins would adopt whichever server answered first.

**The pins moved, their meaning did not.** The pin logic left `session.c` for `identity.c` as
pure functions (`chorus_pins_parse`, `chorus_pins_render`, `chorus_pins_check`) used by both
media: with a store the text lives under `server_pins`, without one in the file `--server-pins`
names, in the same adoption-store text form. A changed server key is still `key_changed`, sent
and surfaced, the pin untouched and the run ended; goal 6's `session-outage.sh` grades that for
the files unchanged, and `identity-session.sh` grades it for the store. An id met twice in the
text is now refused (it was read as two pins before).

**The session takes a store, optionally.** `chorus_session_config_t.store`: with it the key, the
pins and (when `endpoint_id` is empty) the id are the store's and the file paths are not read;
without it the run is byte for byte what it was. The host binary keeps `--key` and
`--server-pins`, and gains `--store <dir>` (one file per key), so a test reboots an endpoint by
starting the process again.

**Discovery is written here, in C, small.** A PTR query for `_chorus-audio._tcp.local.` and a
resolver for PTR, SRV, TXT and A answers with name compression: about 700 lines, no heap, no
clock (CLAUDE.md rule 3: small and instructive). It is held to the SAME packets as the Rust
crate: `fixtures/discovery` is now a shared directory (`check-shared-fixtures.sh`), and
`test_discovery.c` walks it, producing every query vector byte for byte and resolving every
response vector to its `.expected`. Not chosen: `espressif/mdns` from the component registry. It
is a full responder with a cache and its own task, a second implementation that no shared vector
would hold to the Rust one, and a registry pin to carry; the endpoint advertises nothing and
needs one question answered.

**The browse.** The query carries the unicast-response bit (RFC 6762 section 5.4), from an
ephemeral port, so nothing binds 5353 or joins the group and the answer comes back to the
asking socket, as the Linux client's browse does. It is sent again halfway through the window if
nothing has answered. The first instance that resolves to an address is taken. A packet with one
malformed record resolves to nothing; pointers may only point backwards and at most 64 are
followed; a datagram that is not an advertisement of the service is passed over (port 5353 is a
shared channel), and 32 datagrams end a browse on a link that never falls silent.

**The fallback order** (the envelope's): the server discovery found; else the store's last good
server (`server_addr`, written by the session when a handshake completes with a pinned server,
and only when it differs from the value held, so a board whose server stays put writes no flash
for it); else `server_address` when it is not loopback; else nothing, said by name with what each
of the three lacked. On the board "nothing" does not stop the endpoint: the session keeps its
backoff loop and asks again.

**Finding the server again.** `chorus_session_config_t.relocate` is asked after every third
connection attempt in a row that reached nothing, and a browse that FINDS a server moves the
session to it (`server-relocated`). A board that powered up before its server (every power cut:
the board boots in a second, the server does not) or whose server took a new address finds it
without a power cycle. No fallback is consulted there: the session already has an address.

**What is in the gate and what is not.** The vectors, the browse over a scripted link, the
fallback order and the identity over a fake store are in `make firmware-check`, with a real
`chorus-server` on loopback for `identity-session.sh` (no multicast). The exchange over real
multicast is `make verify-endpoint-mdns`, beside `make verify-mdns`: it passes where the link
carries multicast and refuses by name (`MISSING PREREQUISITE`, exit 3) where it does not, and
`tools/unrun-checks-are-visibly-unrun.sh` holds it to that refusal.

## ASSUMED values

None is measured on a board; each is marked ASSUMED where it is defined.

- `CHORUS_DISCOVERY_DEFAULT_WINDOW_MS` 1500: RFC 6762 section 6 delays a shared-record response
  by 20 to 120 ms; the margin over that is a guess.
- `CHORUS_SESSION_RELOCATE_AFTER_FAILURES` 3 attempts before the session asks discovery again.
- `CHORUS_DISCOVERY_MAX_DATAGRAM` 1472, `CHORUS_DISCOVERY_MAX_PACKETS` 32,
  `CHORUS_DISCOVERY_SLICE_MS` 50, one re-query at half the window: bounds chosen, not measured.
- `CHORUS_PINS_MAX` 8 (unchanged from goal 6) and a pin set bounded by one store value (1024
  bytes: eight servers with ids of up to about 40 bytes).

## Deviations from the envelope

- `store.h` is the envelope's text after `clang-format` (one comment's alignment changes): the
  verbatim bytes fail conventions rule 5, and the formatted bytes are the same for whichever
  track commits them.
- This branch carries `store.c`, `tests/fake_store.{c,h}` and a minimal
  `firmware/main/esp_store.{c,h}` (track `provisioning` owns them and had not merged). Whichever
  is on main at the second merge is kept. `chorus_esp_identity_load` calls
  `chorus_esp_store_init()` itself (a second call is free), so the image is correct whether or
  not `app_main` also calls it; no `chorus_esp_store_init()` line was added to `app_main.c`.
- One store key beyond the envelope's list: `server_addr`, the fallback the envelope names.
- The two `app_main.c` calls sit after the console attach and the fault watch, just before the
  session task, not directly after the link bring-up: a browse takes up to its window and the
  amplifier is live by then, so the fault watch runs first. The order between them and against
  the other tracks' calls is the envelope's.
- `session.c` gains the `relocate` seam, which the envelope does not name.
- IPv4 only. The Rust resolver dials the first address of either family; the C one takes the
  first A record and reads an AAAA without dialling it, because the C session's `host:port`
  takes no bracketed IPv6. No committed vector carries an AAAA.

## Consequences and follow-ups

- Discovery is unauthenticated, as multicast DNS is. Trust is still the pin: a responder that
  answers with another server under a NEW id is adopted on first use, as any first server is, and
  one that answers under a pinned id with another key ends the run with `key_changed` until the
  owner acts. Whether an adopted speaker should refuse servers it has not pinned is the adoption
  design's question, not settled here.
- `esp_hal.c`'s radio bring-up erases NVS when it reports no free pages or a new version. That
  predates this change and would erase the identity with the radio's calibration; the store's own
  init refuses instead. For the integration track.
- The browse window and the relocate cadence are compile-time constants, not `endpoint.conf`
  keys.
- The board bindings (NVS, lwIP) are not host-gradable and not claimed; the owner's bench session
  is what shows a board keeping its id across a power cycle and finding a server on the house
  network. Nothing here is timing evidence.

## What was read

- RFC 6762 (multicast DNS), sections 5.4 and 6, https://www.rfc-editor.org/rfc/rfc6762.txt, read
  2026-10-02; sections 2, 3, 11 and 18.1 and RFC 6763 sections 4.1, 5, 6.1 and 6.4 as quoted in
  this repository's `crates/discovery/src/{wire,dnssd,net}.rs` and `fixtures/discovery/*.params`
  (chorus's own code, read in full).
- ESP-IDF v6.1 (Apache-2.0) at the pinned tag, read 2026-10-02:
  `docs/en/api-reference/storage/nvs_flash.rst` lines 56, 105 and 280 (a new value is appended and
  the old one then marked erased; power may be cut at any point without losing data other than
  the pair being written), `components/nvs_flash/include/nvs.h` lines 29-60 (error codes, the
  15-character key), `components/lwip/port/include/lwipopts.h` lines 474 and 1008 (IGMP and
  `SO_RCVTIMEO` are on).
- chorus's own `firmware/src/session.c`, `noise.c`, `docs/protocol.md` ("Adoption: trust on first
  use"), `docs/audit/2026-09-audit.md` finding A-10, and the goal-14 envelope and survey.
- No GPL source and no reciprocally licensed design was opened.
