# 0025: multicast DNS and DNS-SD are hand-written, with no external crate

- Status: accepted (recorded 2026-09-30, goal 4; the decision predates the record)
- Made in: PRODUCT-6 (`d567f06`, 2026-09-07), which added `crates/discovery`
- Recorded because: audit B-8 (`docs/audit/2026-09-audit.md`), K48; BRIEF.md section 3.2 names
  mDNS as a gray-zone "build or vendor" call and asks that each such call be logged, and section
  5.8 says whether mDNS is hand-written or minimally vendored "is a per-item call for the
  decision log"
- Implemented in: `crates/discovery` (`wire.rs`, `dnssd.rs`, `net.rs`), `fixtures/discovery/`,
  `crates/discovery/tests/dnssd_vectors.rs`, `docs/control-plane.md` ("Discovery")

## Context

The server has to tell endpoints where it is, and BRIEF.md section 5.8 recommends "mDNS with
fallback". mDNS is a gray-zone item under BRIEF.md section 3.2: small enough to build, common
enough to vendor. The workspace had no third-party crate when discovery was built, and still has
none (`Cargo.lock` lists only the workspace's own packages; see the correction in ADR 0002).

What chorus needs from mDNS is narrow: one server advertises two services, and one endpoint
browses for one of them and resolves it to a host and port. The phase's acceptance is graded on
bytes: the query an endpoint sends and the response it reads are committed fixtures.

## Decision

chorus writes its own mDNS and DNS-SD, in `crates/discovery`, with an empty `[dependencies]`
table (`crates/discovery/Cargo.toml:10`), and implements only what the narrow need above uses:

- `wire.rs` encodes and decodes DNS messages. It reads name compression (bounded: pointers only
  point backwards and the number followed is capped) and never writes it, so one message has
  exactly one spelling and a golden vector is a contract (`crates/discovery/src/wire.rs:9-21`).
- `dnssd.rs` is RFC 6763 section 4.1's PTR, SRV and TXT shape and the resolver over it, on the
  RFC 6762 port and group (`crates/discovery/src/dnssd.rs:32-51`): services
  `_chorus-audio._tcp.local.` and `_chorus-ctl._tcp.local.`, a 120 s record TTL (RFC 6762
  section 10).
- `net.rs` is the two sockets: the advertiser binds UDP 5353 and joins 224.0.0.251
  (`crates/discovery/src/net.rs:66-76`); the browser sends a query with the unicast-response bit,
  so it never binds 5353 itself (`docs/control-plane.md`, "Discovery").
- Deliberately not implemented: probing and conflict resolution (RFC 6762 section 8),
  known-answer suppression, a cache, and duplicate-question suppression
  (`crates/discovery/src/lib.rs:18-24`). A static server address is the fallback for every case
  where discovery does not work, and an endpoint with neither exits `7` naming what it lacked.

## Consequences

- The packets are the contract: `fixtures/discovery/` pins the query and the responses byte for
  byte, and a second implementation (the C endpoint, when it discovers) is graded against the
  packets rather than against this code, as `fixtures/protocol/` does for the audio wire.
- chorus is not a general-purpose responder. On a link with another responder advertising the
  same instance name, nothing detects the conflict.
- The standard library's `UdpSocket` cannot set `SO_REUSEADDR` or `SO_REUSEPORT` before `bind`,
  so the advertiser takes UDP 5353 exclusively, where RFC 6762 section 15.1 says every mDNS
  implementation SHOULD use those options so all can bind 5353. On a host where another responder
  (an avahi reflector) already holds 5353, the server cannot advertise and says so (exit `9`,
  `crates/server/src/main.rs:28-30`). That is audit L-3, assigned to goal 4's deploy work; its fix
  (setting the option through the platform's socket call, or a small crate) is a change to this
  record's scope and gets its own record if it adds a dependency.
- The ESP32-S3 endpoint does not discover yet; it joins the configured `server_address`
  (`firmware/config/endpoint.conf:67`).

## Alternatives not chosen

- **A Rust mDNS crate** (a responder and browser library): more complete (probing, caching,
  reuse options) but a dependency for a feature chorus uses a small part of, and it would not by
  itself produce the fixed byte spelling the fixtures need. Rejected under BRIEF.md section 3.2
  ("prefer building when it is small and instructive").
- **Talking to the host's avahi daemon** (D-Bus or its client library): ties the server to one
  host daemon, does not exist in the ESP32 endpoint, and puts discovery outside what the tests can
  grade.
- **No discovery, static addresses only**: the fallback exists anyway, but BRIEF.md section 5.8
  asks for discovery with fallback, not instead of it.

## What was read

- RFC 6762, Multicast DNS: https://www.rfc-editor.org/rfc/rfc6762.txt (read 2026-09-30),
  sections 2, 3, 8, 10, 11 and 15.1 (the `SO_REUSEPORT`/`SO_REUSEADDR` SHOULD).
- RFC 6763, DNS-Based Service Discovery: https://www.rfc-editor.org/rfc/rfc6763.txt (read
  2026-09-30), sections 4.1, 5, 6 and 7 (service names) and 16 (IANA manages service names).
- The code as built: `crates/discovery/src/lib.rs`, `dnssd.rs`, `wire.rs`, `net.rs`,
  `crates/discovery/Cargo.toml`, `crates/server/src/main.rs`, `docs/control-plane.md`, and
  audit findings B-8 and L-3 (`docs/audit/2026-09-audit.md`), all read 2026-09-30.
- No mDNS implementation's source was opened for this record.
