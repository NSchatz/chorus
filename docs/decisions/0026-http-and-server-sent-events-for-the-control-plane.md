# 0026: the control plane is HTTP with server-sent events, not WebSocket

- Status: accepted (recorded 2026-09-30, goal 4; the decision predates the record)
- Made in: PRODUCT-6 (`d567f06`, 2026-09-07), which added the control plane
- Supersedes: BRIEF.md section 5.8's recommendation "WebSocket for UIs" (R10; the BRIEF edit is
  the lead's, goal 4)
- Recorded because: audit B-8 (`docs/audit/2026-09-audit.md`), K48; BRIEF.md section 3.2 lists
  WebSocket as a gray-zone call to be logged
- Implemented in: `crates/server/src/control.rs`, `crates/client-linux/src/control.rs`,
  `crates/server/src/ui/chorus.js`, `docs/control-plane.md` ("How the messages travel")

## Context

The control plane carries a small versioned JSON catalog (`docs/control-plane.md`,
`fixtures/control/`). Two kinds of peer use it: the control page in a browser, and endpoints and
shell verification scripts. Traffic is asymmetric: a command now and then from a client, and a
state message from the server to every subscriber after every change. BRIEF.md section 5.8
recommends "WebSocket for UIs". The workspace has no third-party crate, so either transport is
written by hand.

## Decision

One HTTP/1.1 port (`--control-listen`), every response `Connection: close`
(`docs/control-plane.md`, "How the messages travel"):

- `POST /api/command` carries one control message as `application/json`
  (`crates/server/src/control.rs:894`); `415` for another content type and `403` for a foreign
  `Origin` (`crates/server/src/control.rs:664-684`).
- `GET /api/events` is a server-sent event stream (`Content-Type: text/event-stream`), opening
  with the state as it stands and then one `data: <state message>` per change
  (`crates/server/src/control.rs:929-998`), with a `: keepalive` comment line every 15 s
  (`crates/server/src/control.rs:99-106`), the interval the HTML standard's authoring notes
  suggest against proxies that drop idle connections.
- The browser subscribes with `EventSource` (`crates/server/src/ui/chorus.js:732`); an endpoint
  and a shell script subscribe with a socket and a `GET` line
  (`crates/client-linux/src/control.rs:333-344`). So the UI and the verification scripts are
  the same subscriber (`crates/server/src/control.rs:22-34`).

## Consequences

- One code path serves both browsers and scripts; a check exercises exactly what the page uses.
- The server side is a response header and `data:` lines on a socket the fixed worker pool
  already holds: no handshake hashing, no framing, no masking.
- Each open stream holds one control worker for its lifetime. Audit B-5 found that eight
  subscribers lock out every command; the stopgap caps streams at workers minus one (PR #28),
  and goal 11 moves streams to one writer thread.
- Client-to-server traffic is a new HTTP request per command, which is fine at human command
  rates and would not be for a high-rate client stream (none is planned on this channel).
- A browser that limits HTTP/1.1 connections per server can run out with many tabs open, each
  holding an `EventSource`; the HTML standard notes this. The page is one tab in practice.
- BRIEF.md section 5.8's "WebSocket for UIs" is superseded by this record (R10).

## Alternatives not chosen

- **WebSocket (RFC 6455)**: full duplex, which the catalog does not need. Written by hand it
  needs the opening handshake (SHA-1 and base64 of the key, RFC 6455 section 4.2.2), frame
  parsing, and unmasking every client frame (section 5.3: a client MUST mask all frames), all
  code with no use here; and a shell script cannot speak it with a socket and a `GET` line, so
  the scripts would need a second, private subscriber protocol.
- **Polling `GET /api/state`**: simplest, but a change reaches subscribers only on the next
  poll, and the fanout's bounded queues and slow-subscriber handling
  (`crates/control/src/fanout.rs`) would have nothing to act on.
- **A raw TCP line protocol for endpoints and HTTP for the page**: two transports for one
  catalog, which is what the single-subscriber decision above avoids.

## What was read

- WHATWG HTML Living Standard, section 9.2 "Server-sent events" (the `text/event-stream` format,
  comment lines, `EventSource` reconnection, and the authoring notes on a comment every 15 s and
  on per-server connection limits):
  https://html.spec.whatwg.org/multipage/server-sent-events.html (read 2026-09-30).
- RFC 6455, The WebSocket Protocol: https://www.rfc-editor.org/rfc/rfc6455.txt (read
  2026-09-30), sections 1.3, 4.2.2, 5.1 and 5.3.
- The code as built: `crates/server/src/control.rs`, `crates/client-linux/src/control.rs`,
  `crates/server/src/ui/chorus.js`, `docs/control-plane.md`, and audit findings B-5, B-6 and B-8
  (`docs/audit/2026-09-audit.md`), all read 2026-09-30.
