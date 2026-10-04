# 0114: chorusctl is a std-only noun/verb client of the control API that sends the catalog's own bytes, prints the server's own JSON, and maps what came back to five exit codes

- Status: accepted (goal 15, 2026-10-03)
- Decided by: the goal (program section 19 item 3, done-when line D; K46) inside the
  coordinator's goal-15 design envelope (section 3), track `chorus-g15/ctl`; every default below
  not cited is ASSUMED
- Implemented in: `crates/ctl` (new: `src/grammar.rs` the one table, `src/parse.rs`,
  `src/client.rs`, `src/render.rs`, `src/lib.rs` with `run` and `run_with`, `src/main.rs`;
  `tests/commands.rs`, `tests/output.rs`, `tests/exit_codes.rs`, `tests/docs.rs`,
  `tests/support/mod.rs`); `crates/server/tests/chorusctl.rs` and the `chorus-ctl`
  dev-dependency of `crates/server`; the root `Cargo.toml` and `Cargo.lock` (one more member).
  The page for a person is `docs/chorusctl.md`

## Context

K46 asks for `chorusctl`, a command line over the control API, and the goal's line D asks that
it cover rooms, groups, volume, inputs, endpoints and updates with `--json` and exit codes. The
control API exists and is pinned: `GET /api/state` answers the state message, `POST /api/command`
takes one catalog message and answers the new state or the server's `error` or `refused`
(`docs/control-plane.md`), and `fixtures/control` holds 118 vectors of those bytes. The catalog,
its encoder and a JSON reader and writer are a library with no dependency (`chorus-control`).
Every chorus binary parses its own arguments by hand and documents its exit codes as a contract
(`crates/server/src/main.rs`, `crates/client-linux/src/main.rs`), and the server's HTTP is
hand-written over `std::net`. Since goal 14 the catalog can start a firmware transfer
(`firmware_install`), which the server guards (ADR 0110, conventions rule 20).

## What was read

All read 2026-10-03: the program's section 19 and K46, K93, I13;
[`.claude/goals/2026-09-chorus-research/research-pwa-conventions.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus-research/research-pwa-conventions.md) section 5; the goal-15
design envelope (section 3 and "Rules every track follows") and code survey (sections A, D, E,
F); `CLAUDE.md`; `docs/conventions.md` (rules 2, 9, 12, 13, 15, 16, 18, 19, 20, 21);
`docs/control-plane.md` (the routes, the refusals, "Firmware: staged images and explicit
installs"); `docs/firmware-updates.md`; ADRs 0016, 0110 and 0112; `crates/control/src/catalog.rs`
(`Command`, `encode`, `Volume`), `json.rs`, `rooms.rs` (`Source`, `InputId`);
`crates/server/src/control.rs` (the router, `REQUEST_DEADLINE`, the POST rules), `health.rs` (the
existing hand-written HTTP client); `crates/server/tests/common/mod.rs`;
`crates/control/tests/vectors/mod.rs`; the vectors under `fixtures/control` and
`fixtures/control/v2`; `tools/image.sh`, `tools/release.sh`, `deploy/Dockerfile`;
`tools/conventions/check-adrs.sh`, `check-provenance.sh`, `check-flash-guard.sh`. No GPL source
and no reciprocally licensed design file was opened; nothing outside this repository was read,
so nothing is cited by URL.

## Decision

1. **One crate, one dependency.** `crates/ctl` is the package `chorus-ctl` with the binary
   `chorusctl` and a library that holds everything: `main.rs` is the arguments in and the
   exit code out. It depends on `chorus-control` alone and on no third-party crate.
2. **The bytes sent are the catalog's by construction.** A mutating verb becomes a typed
   `chorus_control::Command`, and what goes on the socket is `Command::encode()`. chorusctl has
   no JSON of its own to drift. `tests/commands.rs` takes each case's arguments from a vector's
   `.fields`, runs chorusctl against a fake server, and compares the body that arrived with the
   vector's `.json`: 23 cases over 22 vectors, and a second test fails if a message type the
   table says a verb sends has no such case.
3. **Grammar: `chorusctl [flags] <noun> <verb> [operands]`, six nouns, from one table.**
   `grammar.rs` lists every noun, verb, flag and exit code once; the parser looks commands up in
   it, `--help` (global and per noun) is printed from it, and the tests walk it (every verb's
   example parses and sends what its row says). The nouns are the six areas of line D:
   - `rooms`: `list`, `show`, `name` (the wire word `zone` stays on the wire; a person says room);
   - `groups`: `list`, `join`, `leave`, `save`, `delete`, `take`. `join` and `leave` send `join`
     and `ungroup`; the v1 `group` command (name the group id yourself) is not offered, because
     `join` reaches every state `group` reaches and the server names live groups;
   - `volume`: `get`, `set`, `step`, `mute`, `unmute`, `limit`. A room is the operand, a group is
     `--group <group>`: the flag is explicit, so one command line always means one message and
     chorusctl never reads the state to guess whether a word is a room or a group;
   - `inputs`: `list`, `select`. `select <input> <target>` is `take` with the source
     `line-in:<input>`: the catalog has no other command that points a room at an input;
   - `endpoints`: `list`, `show`, `name`, `room` (`--none` is the wire's `null`), `forget`: the
     adopted speakers (`speaker_*`) and the attached endpoints with their link;
   - `updates`: `list` (staged images), `status` (each speaker's firmware), `install`
     (`<speaker>` or `--all`, `--force`), `cancel`, `rescan`.
   Flags are `--flag value` or `--flag=value` anywhere on the line; a word that starts with a
   dash and a digit is an operand (a negative step), and a bare `--` ends the flags. An unknown
   flag, noun or verb is a usage error naming the closest valid one by edit distance; a flag that
   does not apply to the verb is refused, never ignored.
4. **`--server host:port`, `CHORUS_SERVER` as its default, and no built-in address.** The control
   plane is opt-in with no default port, so a default here would be an address nobody chose.
   `http://host:port/` is reduced to `host:port`; any other scheme or a path is a usage error.
   `--timeout` (1 to 60 s, default 5 s, the server's own `REQUEST_DEADLINE`) bounds the whole
   exchange on the monotonic clock.
5. **Hand-written HTTP over `std::net::TcpStream`.** One request per connection, as the server
   serves it (`Connection: close`, a `Content-Length`): connect, write, read to the end, hold
   the body to the declared length, at most 8 MiB. No `Origin` header, which is what the server's
   same-origin rule expects of a client that is not a browser.
6. **`--json` is the server's own JSON.** A read verb prints the part of the state it is about
   (the `zones` array, one zone, one group, `inputs`, `firmware.images`), cut out and written
   back with the catalog's canonical writer, which keeps member order and every number's digits;
   `tests/output.rs` holds each single-value part to being a substring of the state vector. Three
   read verbs wrap parts in an object of chorusctl's (`groups list`, `endpoints list`,
   `endpoints show`), each documented. A mutating verb prints the whole state message as
   received. An error is one JSON object on stderr. The human form (tables, `key: value` lines)
   is not a contract.
7. **Exit codes: 0 ok, 1 usage, 2 unreachable, 3 refused, 4 not-found.** They are derived from
   what came back, not from the HTTP status alone: a `state` with 200 is ok; a body that is the
   catalog's `error` or `refused` is refused whatever the status (400, 426, 503); anything else
   (no connection, a timeout, not HTTP, a short body, a body that is not a catalog message) is
   unreachable. The fifth code is the one the envelope left open: a read verb that names one
   thing (`rooms show`, `volume get`, `endpoints show`) and finds the state without it. The
   server answered and refused nothing, so neither 2 nor 3 is true of it, and a script needs to
   tell "no such room" from "the server is down". A usage error never opens a connection
   (`tests/exit_codes.rs` checks the listener saw none).
8. **`updates install` sends `firmware_install` and nothing else.** Whether an image goes to a
   speaker stays the server's decision and the server's guard (ADR 0110). chorusctl does not
   read or set the owner-at-bench variable, has no flag that reaches around the server, and is
   not a guard reader or a flashing tool in conventions rule 20's sense: it writes to no
   device. A refusal is exit 3 with the server's words.
9. **The real-server test lives in `crates/server/tests`.** Cargo gives a binary's path
   (`CARGO_BIN_EXE_chorus-server`) only to that package's own integration tests, so
   `crates/server/tests/chorusctl.rs` takes `chorus-ctl` as a dev-dependency and calls
   `chorus_ctl::run_with`: all six nouns against the real binary, a real adopted session for
   `endpoints`, refusals as 3, a stopped server as 2. The cycle is dev-only, the kind
   `chorus-control` already has with `chorus-server`.
10. **chorusctl does not ship in the server image or the release in this change.** The image is
    one static binary with an entrypoint and a healthcheck, and its test and the release notes
    name that binary (`tools/image.sh`, `tools/release.sh`); chorusctl is an operator's tool that
    runs on any machine that can reach the control port, built with
    `cargo build --release -p chorus-ctl`. Shipping it (in the image for `docker exec`, or as a
    release artifact) is a packaging decision with its own test and is listed below. The
    Dockerfile's build context copies `crates/` whole and builds `--workspace --bins`, so the new
    member compiles there without a change.

## Not chosen

- **An argument-parsing crate, an HTTP crate or a JSON crate.** BRIEF 3.2's question (why not
  build it) has a short answer here: the whole client is one request and one response of a
  protocol whose server side is already hand-written in this tree, the grammar is a table of 27
  verbs, and the JSON reader and canonical writer the byte-equality rests on already exist in
  `chorus-control`. A general serialiser would have to be held to the catalog's byte rules by
  the same vectors anyway.
- **Guessing room or group from the state** for `volume`: a second request, a race with a
  rename, and a command line whose meaning depends on the server's state.
- **Folding not-found into 3**: it would say the server refused something it never saw.
- **A raw escape hatch** (`chorusctl send '<json>'`) and the catalog's other commands (bond,
  quiet hours, alarms, sleep, autoplay, sound, bass management, room EQ, A/V trim): outside
  line D's six areas. The table takes a noun per area when a goal asks for one.
- **Following `GET /api/events`**: a watch verb is a long-lived stream with its own exit rules;
  not asked for.

## ASSUMED

The 5 s default timeout and its 1 to 60 s range; the 8 MiB response ceiling; the human table
columns; `CHORUS_SERVER` as the variable's name (the envelope's); the verbs chosen where the
envelope offered alternatives (`join`/`leave`, `select`, `list` and `status` as two verbs).

## Follow-ups

- Packaging: chorusctl in the server image and as a release artifact, with the image test
  running its `--help`.
- The catalog's remaining commands as nouns (`alarms`, `sound`, `bonds`), and a `watch` verb over
  the event stream, when a goal asks for them.
