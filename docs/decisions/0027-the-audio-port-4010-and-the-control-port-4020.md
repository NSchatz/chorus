# 0027: the audio port is 4010 and the control port is 4020

- Status: accepted (recorded 2026-09-30, goal 4; the decision predates the record)
- Made in: SOUND-2 (`257d739`, 2026-08-23) for 4010, the first server and client defaults;
  PRODUCT-6 (`d567f06`, 2026-09-07) for 4020, the control page's example and the discovery
  fixtures' `ctl=4020`
- Recorded because: audit B-8 (`docs/audit/2026-09-audit.md`), K48
- Implemented in: `crates/server/src/config.rs:109`, `crates/client-linux/src/config.rs:137`,
  `firmware/config/endpoint.conf:67`, `deploy/Dockerfile:55,61`, `deploy/run-server.sh:50-51`,
  `docs/control-page.md:147`, `fixtures/discovery/`

## Context

The audio stream (TCP, `crates/server`, BRIEF.md section 5.2's "one multiplexed TCP connection
per device") and the control plane (HTTP, ADR 0026) each need a port. No record said why 4010
was picked in SOUND-2; the commit that introduced it gives no reason. This record states what
the numbers are, what they are not, and how they may change.

## Decision

- **4010/tcp is the audio port.** It is the default of the server's `--listen`
  (`127.0.0.1:4010`, `crates/server/src/config.rs:109`), of the Linux client's `--server`
  (`crates/client-linux/src/config.rs:137`) and of the endpoint firmware's `server_address`
  (`firmware/config/endpoint.conf:67`), and what the image exposes and listens on
  (`deploy/Dockerfile:55,61`).
- **4020/tcp is the control port by convention.** The server has no control-listen default
  (`control_listen: None`, `crates/server/src/config.rs:127`): the control plane is off unless an
  address is given. The image and the run script give `4020` (`deploy/Dockerfile:61`,
  `deploy/run-server.sh:51`), and so do the docs' examples.
- **Both are configurable, and discovery carries the real ones.** The server advertises the port
  it actually listens on in the audio service's SRV record and the control port in its TXT record
  as `ctl=<port>` (`crates/server/src/main.rs:355-375`), so a client that discovers never relies
  on either number. The numbers matter only for the static fallback and for firewall rules.
- **Neither is registered with IANA,** and chorus does not seek a registration: chorus is a
  private household system, not a protocol offered to others.

## Consequences

- Both numbers sit in the IANA "User Ports" range (1024-49151, RFC 6335 section 6) and both are
  assigned to other services in the IANA registry: 4010/tcp and udp to the service name
  `samsung-unidex`, 4020/tcp and udp to `trap` (read 2026-09-30). chorus uses them without a
  registration, on a local network. If a host already runs either service, the operator moves
  chorus with `--listen`, `--control-listen`, `CHORUS_PORT` or `CHORUS_CONTROL_PORT`; nothing in
  the protocol depends on the number.
- A change of the default is cheap in code (the five places above plus the fixtures that pin
  `ctl=4020`), but it moves every deployed endpoint that relies on the static fallback, so it is a
  decision with its own record rather than a quiet edit.
- The host-networked deploy (K34) puts both ports directly on the host, so they must be free
  there; the deploy PR checks that.

## Alternatives not chosen

- **A number from the Dynamic Ports range (49152-65535)**: never assigned by IANA, but the
  operating system hands those out as ephemeral source ports, so a fixed listener there can
  collide with an outgoing connection's port.
- **An IANA registration** (RFC 6335 section 8.1): the right path for a protocol others will
  speak; out of scope for a private system, and an outward-facing act the program does not take.
- **Changing to unassigned numbers now**: a registry "unassigned" state can change later, the
  current numbers work, and discovery already makes the number irrelevant where it works.
  Kept for now; a later record can move them if a real clash appears.

## What was read

- IANA, "Service Name and Transport Protocol Port Number Registry", the CSV export:
  https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.csv
  (read 2026-09-30; the rows for 4010 and 4020).
- RFC 6335, IANA procedures for service names and port numbers:
  https://www.rfc-editor.org/rfc/rfc6335.txt (read 2026-09-30), section 6 (the three ranges)
  and section 8.1.
- The code as built: the files under "Implemented in" above, `crates/server/src/main.rs`, and
  the commits `257d739` and `d567f06` that introduced the numbers, all read 2026-09-30.
