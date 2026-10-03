# 0130: a Soloist receiver is supervised by chorus-soloistd over one directory of FIFOs and Unix sockets, with the protocol, the API model, RFC 6455 framing, the expiry arithmetic and the pool in one pure crate, three libc calls in one listed module, and a fake Soloist that no image can carry

- Status: accepted (goal 17, 2026-10-03)
- Decided by: the owner for what is built (P7 Option C: Soloist receivers in containers of
  their own, supervised by chorus, the binary and key the owner's; K65, K66, K78, K80); the goal
  (program section 21) inside the coordinator's goal-17 design envelope (sections 0, 1, 2.1 to
  2.5), track `chorus-g17/soloistd`, for everything else. Every number below that is not cited
  is chorus's own choice and is said to be
- Implemented in: `crates/soloist` (new, package `chorus-soloist`: `api`, `ws`, `protocol`,
  `build`, `pool`, `keydir`), `crates/soloistd` (new, `chorus-soloistd`: `args`, `log`,
  `pipewire`, `supervisor`, `sys`, `wsclient`; the example `chorus-fake-soloist`),
  `crates/soloist-fake` (new, a library), `fixtures/soloist` (37 vectors),
  `crates/soloist/tests/fixtures.rs`, `crates/soloistd/tests/supervisor.rs`, `docs/soloist.md`.
  The server side (FIFO readers, the manager, the `soloist:` source, `chorusctl soloist`) and
  the `chorus-soloist` image are later tracks of the same goal and have their own records

## Context

P7 settled that Spotify Connect reaches chorus through Spotify's own receiver, Soloist: a
proprietary binary with a per-developer API key and a 90-day build lifetime, which chorus may
not ship. The envelope settled the shape: a fixed pool of receiver containers, each with
PipeWire, WirePlumber, one Soloist and a supervisor; one shared directory between them and
chorus-server and no network path; a newline-delimited JSON protocol on a Unix socket; PCM
through a FIFO written by a PipeWire pipe-tunnel sink.

This record is the half that needs no chorus-server: what the two sides agree on (a pure
library), the supervisor, and the fake that stands in for Soloist in every test, since
"never download or run Soloist" is a rule of the program.

## What was read

All on 2026-10-03.

- Soloist's documentation under https://developer.spotify.com/documentation/soloist : the
  command-line reference, the WebSocket API reference, the getting-started tutorial, the
  overview, the downloads and updates page (through goal 17's research digest, which quotes
  them verbatim, and P7).
- Issue reports of the public `spotify/soloist` repository, numbers 1, 2, 5, 6, 8, 10, 12, 13
  (through the same digest): LEADs only, used for the `--version` shapes and the float32 output.
- RFC 6455, The WebSocket Protocol, https://www.rfc-editor.org/rfc/rfc6455 (sections 1.3, 4.1,
  4.2, 5.1 to 5.7, 7.4, 8.1). RFC 3174 (SHA-1), https://www.rfc-editor.org/rfc/rfc3174 . RFC
  4648 (base64), https://www.rfc-editor.org/rfc/rfc4648 .
- PipeWire's pipe-tunnel module page, https://docs.pipewire.org/page_module_pipe_tunnel.html ,
  and goal 17's PipeWire probe (PipeWire 1.4.2 and WirePlumber 0.5.8, both MIT in the parts
  used; their shipped configuration files, no source).
- Howard Hinnant, "chrono-Compatible Low-Level Date Algorithms",
  https://howardhinnant.github.io/date_algorithms.html (public domain): the days-from-civil
  arithmetic.
- POSIX.1-2017 `mkfifo`, `kill`, `signal`; Linux `signal(7)`, `pipe(7)`, `flock(2)`.
- chorus's own code: `crates/control/src/json.rs`, `crates/upnp/src/uuid.rs` (`Target`, `sha1`),
  `crates/server/src/source.rs` (the FIFO open), `tools/conventions/*`, `deploy/Dockerfile`,
  `tools/image.sh`, `tools/release.sh`.
- No GPL or LGPL source was opened. No Soloist binary or archive was downloaded or run.

## Decision

1. **One pure crate, `chorus-soloist`, holds everything both sides must agree on.** The
   WebSocket API model, RFC 6455 framing, the supervisor protocol, the `--version` parser and
   expiry arithmetic, the receiver pool and the key-to-directory mapping: text and bytes in,
   text and bytes out, `#![forbid(unsafe_code)]`, no I/O and no clock read ("now" and the
   monotonic time are arguments). The server will call the same functions the supervisor and
   the fake are tested with, against the same fixtures.

2. **JSON is `chorus_control::json`.** `chorus-soloist` depends on `chorus-control` for the
   workspace's hand-written reader and canonical writer. Why not a minimal reader of its own:
   the server already links `chorus-control` and will hold these values, a second reader would
   be a second JSON dialect to keep equal to the first, and `chorus-control` has no
   dependencies, so the sidecar pays compile time only. What it costs: the supervisor's build
   compiles the control catalog it never calls, and the reader's strictness applies to
   Soloist's frames (a frame with a duplicate key is dropped with a log line, not relayed).

3. **SHA-1 and base64 are private to `chorus_soloist::ws`** (a gray-zone call, working
   agreement 3). The handshake's `Sec-WebSocket-Accept` needs both. SHA-1 exists in
   `chorus_upnp::uuid`; base64 existed nowhere in library code when this was written, and the
   OpenHome track is adding one to `crates/upnp` in parallel. Depending on the UPnP crate for
   a hash would tie the sidecar to the renderer's protocol crate, and editing `crates/upnp`
   was another track's. So `ws` carries about 60 lines of SHA-1 and 20 of base64, written from
   RFC 3174 and RFC 4648 and held to their test vectors. Once both tracks are merged, one small
   shared place for the two (a `chorus-hash` crate, or `chorus-control`) is the obvious
   follow-up; it is not done here.

4. **The WebSocket client is hand-written, blocking, with a writer thread.** No async runtime
   and no crate (the workspace's posture; brief section 4.7). The pure half is `ws`: the
   request, the response check (status 101, `Upgrade`, `Connection`, `Sec-WebSocket-Accept`,
   and no extension or subprotocol since none is offered), masked client frames, fragment
   reassembly with control frames between fragments, and a bound on a message (1 MiB, chorus's
   choice) checked on the declared length before any payload is buffered. It is tested on the
   RFC's own examples: section 1.3's accept value and every frame of section 5.7.

5. **The supervisor is one deciding thread fed by readers.** The main thread takes messages
   from a channel with a 25 ms timeout and owns every decision; the acceptor, the connection
   reader, the child monitors, the output readers and the WebSocket client each read one thing
   and send what they read. Timers (the backoff, SIGTERM to SIGKILL, the expiry check, its own
   SIGTERM) need no thread. A write to the server has a 2 s timeout, after which the
   connection is dropped: a server that does not read cannot stall a receiver.

6. **`flock` is std's; `mkfifo`, SIGTERM and the supervisor's own signal handling are three
   libc calls in one listed module**, `crates/soloistd/src/sys.rs` (a new line in conventions
   rule 2's list). The alternative, the `mkfifo` and `kill` programs as child processes, needs
   no `unsafe`, and was not chosen because it cannot do the third thing: catch SIGTERM. A
   supervisor that is PID 1 of its container is not delivered a SIGTERM it does not handle, so
   a container stop would end in SIGKILL for everything after the timeout; one that is not
   PID 1 would die at once and orphan Soloist. Either way Soloist would never get the normal
   shutdown its documentation describes (the one that removes `ws.addr` and `ws.port`), and
   whether a stale `soloist.pid` then blocks the next start is not documented. With a handler
   needed anyway, `mkfifo` and `kill` are two more declarations of the same kind, and the
   supervisor no longer depends on which programs its image carries nor forks to send a
   signal. The module is 3 `extern` declarations and 3 `unsafe` blocks; the handler stores one
   atomic. `SIGKILL` is std's `Child::kill`. A process id is signalled only under the lock the
   monitor reaps under, so it cannot have been reused.

7. **Exit code 10 is `expired` and stays so; everything else is retried.** Only `restart`
   clears `expired` (an `assign` of the same target does not, so a reconnecting server cannot
   turn it into a restart loop). Other exits, a missing `ws.port`, a missing key file and a
   missing binary are retried with a delay doubled from 1 s to 60 s (chorus's choice). An
   unparseable `--version` is "expiry unknown", never "expired": Soloist's own exit code is the
   authority.

8. **The receiver pool is pure and ranks by kind, then age.** Rooms, then saved groups (both
   in the caller's order), then live groups oldest first; a target never moves between
   receivers; a dissolved live group keeps its receiver while it is busy and for the grace
   period after it is dissolved and idle, but ranks below every listed target, so a new target
   that needs a receiver takes a lingering one's at once. A property-style test (300 seeded
   histories, no external crate) holds every stated invariant.

9. **The fake Soloist is a library, run as an example.** `crates/soloist-fake` has no binary
   target and `chorus-fake-soloist` is an example of `crates/soloistd`: `cargo build --bins`,
   which the Dockerfile, `tools/image.sh` and `tools/release.sh` use, builds neither, so the
   fake cannot reach an image or a release even by a later `COPY target/release/*`. As a
   library it can also be an example of `crates/server` when the server track wants the same
   fake. It is built from the documentation only and its header says which behaviours are
   documented and which are assumptions.

10. **Announcements pause a Soloist source, never duck it** (P7's overlap clause): recorded
    here as a rule for goal 20's announcement path; nothing is built for it in this change.

## Options not chosen

- **A WebSocket or JSON crate** (tungstenite, serde_json): each would be the workspace's first
  of its kind, for one loopback connection and a dozen message shapes (rule 13's question, "why
  not build it": it is about 400 lines, and it is tested on the RFC's examples).
- **The fake as a `[[bin]]` of a test-support crate**: `--workspace --bins` would build it into
  `target/release`, one `COPY` away from an image.
- **`mkfifo` and `kill` as child programs**: decision 6.
- **Restarting Soloist when its WebSocket drops**: a reconnect under the same bound is tried
  first, because a restart interrupts what is playing.
- **Keeping events for a server that is not connected**: the server gets `hello`, `build`,
  `status` and a fresh `auth_state` on every connection, and asks for `get_state` itself.

## Consequences and open points

- Conventions rule 2's list gains `crates/soloistd/src/sys.rs`; rule 9 gains `fixtures/soloist`
  (Rust-only by declaration).
- `deploy/Dockerfile`'s `cargo build --workspace --bins` now also builds `chorus-soloistd`; it
  copies nothing new into the runtime stage. The `chorus-soloist` image is its own track.
- The gate runs no PipeWire (conventions rule 10): `--pipewire auto` is covered by unit tests
  of the configuration text and by one run by hand recorded in `docs/soloist.md`.
- Everything `docs/soloist.md` lists under "What is assumed" waits on the owner's build: the
  `--version` format, the shutdown signal, the output format, the endpoint files' format.
- The integration tests add about 1 s of wall clock to the fast tier when run alone on the
  development host (15 tests in parallel; every wait is bounded, three are fixed: 300 ms,
  300 ms and 400 ms).
