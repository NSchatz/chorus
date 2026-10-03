# chorusctl

`chorusctl` is the command line over the chorus control API (`docs/control-plane.md`): rooms,
groups, volume, inputs, endpoints and firmware updates, from a terminal or a script. It is a thin
client. It keeps no state, decides nothing the server decides, and every command it sends is one
message of the control catalog, byte for byte the vector under `fixtures/control` for the same
arguments (`crates/ctl/tests/commands.rs` holds it to that).

It is the crate `crates/ctl` (package `chorus-ctl`, binary `chorusctl`); the decision record is
`docs/decisions/0114-chorusctl.md`.

```sh
cargo build --release -p chorus-ctl        # target/release/chorusctl
export CHORUS_SERVER=127.0.0.1:8080        # the server's --control-listen address
chorusctl rooms list
chorusctl volume set kitchen 0.350
chorusctl --json groups list
```

## The server address

`--server <host:port>` is the address the server was given as `--control-listen`. When the flag
is absent the `CHORUS_SERVER` variable supplies it, and the flag wins when both are set. There is
no built-in default, because the control plane has no default port: with neither, `chorusctl`
exits 1 saying so. `http://host:port/` is accepted and read as `host:port`; anything else with a
scheme or a path is a usage error (there is no TLS and no path prefix to speak).

`chorusctl` talks to the control port directly: plain HTTP with no login and no token, because
the control plane itself has no authentication (`docs/control-plane.md`). From another machine it is
`chorusctl --server 192.0.2.10:8080 rooms list`, with the server's own address in place of the
documentation one.

`--timeout <seconds>` (1 to 60, default 5) bounds the whole exchange: connecting, sending and
reading the answer. The default is the server's own bound on a request (`REQUEST_DEADLINE` in
`crates/server/src/control.rs`).

## Grammar

```text
chorusctl [--server <host:port>] [--json] [--timeout <seconds>] <noun> <verb> [args]
```

- Flags are `--flag value` or `--flag=value` and may stand anywhere on the line.
- The first two words are the noun and the verb; the rest are the verb's operands.
- A word that starts with a dash and a digit is an operand, so `volume step kitchen -50` works.
  Any other operand that starts with a dash goes after a bare `--`:
  `chorusctl rooms name kitchen -- -1-`.
- A name with a space is one shell word: `chorusctl rooms name kitchen "The Kitchen"`.
- An unknown flag, noun or verb is a usage error that names the closest valid one. A flag that
  does not apply to the verb is a usage error too; nothing is silently ignored.
- `chorusctl --help` prints everything below; `chorusctl <noun> --help` (or
  `chorusctl help <noun>`) prints one noun's verbs with an example of each. Both come from the
  one table in `crates/ctl/src/grammar.rs`, which the parser reads too.

This is `chorusctl --help`, and a test (`crates/ctl/tests/docs.rs`) fails if this page and the
program disagree:

```text
usage: chorusctl [--server <host:port>] [--json] [--timeout <seconds>] <noun> <verb> [args]

chorusctl drives a chorus server through its control API.

rooms: the rooms (the catalog's zones)
  list                every room: group, volume, mute, limit, speakers present
  show <room>         one room in full
  name <room> <name>  give a room its display name

groups: what plays together: live groups and saved groups
  list                                  the groups that exist now, and the saved groups
  join <room> <target>                  put a room into the group a room or a group is in
  leave <room>                          take a room out of its group; it plays alone
  save <group> <name> <room> <room>...  save, or replace, a named group of two or more rooms
  delete <group>                        forget a saved group; its rooms stay where they are
  take <target> [--source <source>]     play in a room, a saved group or a live group, alone

volume: volume, mute and limit of a room, or the volume of a group
  get <room> | --group <group>             the volume now
  set (<room> | --group <group>) <volume>  set it: 0 to 1, at most three decimals (0.350)
  step (<room> | --group <group>) <step>   move it by signed thousandths, -1000 to 1000 (-50)
  mute <room>                              mute a room
  unmute <room>                            unmute a room
  limit <room> <limit>                     set a room's maximum volume, written as a volume

inputs: the line inputs endpoints offer
  list                     every input offered now, and the groups playing it
  select <input> <target>  play an input (<endpoint>/<input>) in a room or a group

endpoints: the adopted speakers and the endpoints attached to rooms
  list                              every speaker and endpoint, and any changed key
  show <id>                         one speaker or endpoint in full
  name <speaker> <name>             name an adopted speaker
  room <speaker> (<room> | --none)  assign a speaker to a room, or to none
  forget <speaker>                  forget a speaker and its pinned key; it is adopted afresh

updates: firmware images staged on the server, and installs
  list                                           the staged images and the server's verdict on each
  status                                         what each speaker runs, and its install's progress
  install (<speaker> | --all) <image> [--force]  install a staged image, if the server allows it
  cancel <speaker>                               abandon a speaker's install that is not yet verified
  rescan                                         have the server read its firmware directory again

flags:
  --server <host:port>  the server's control address (its --control-listen); default: $CHORUS_SERVER
  --json                print the server's own JSON; an error is one JSON object on stderr
  --timeout <seconds>   how long to wait for the server, 1 to 60; default 5
  --help                this text; after a noun, that noun's verbs (also -h, and 'help <noun>')
  --group <group>       volume get, set, step: act on a group instead of a room
  --source <source>     groups take: stream, none, line-in:<endpoint>/<input> or chime:<name>
  --none                endpoints room: take the speaker out of every room
  --all                 updates install: every present speaker of the image's board not running it
  --force               updates install: install even the version the speaker already runs

exit codes:
  0  ok: the command was applied, or the state was read and printed
  1  usage: the command line is not valid; nothing was sent
  2  unreachable: no answer from a chorus server: connect, timeout or a bad HTTP answer
  3  refused: the server answered and refused (its `error` or `refused` message)
  4  not-found: the server answered; what a read verb named is not in its state

The grammar, the --json shapes and the exit codes: docs/chorusctl.md
```

## Every noun and verb

"Sends" is the catalog message type, as `docs/control-plane.md` defines it. A verb that sends
nothing reads `GET /api/state`. The wire word for a room is `zone`.

### rooms

| Command | Sends | `--json` prints |
|---|---|---|
| `rooms list` | (reads) | the state's `zones` array |
| `rooms show <room>` | (reads) | that room's object from `zones` |
| `rooms name <room> <name>` | `name` | the state message |

### groups

| Command | Sends | `--json` prints |
|---|---|---|
| `groups list` | (reads) | `{"groups":[...],"saved_groups":[...]}`, both arrays as the state has them |
| `groups join <room> <target>` | `join` | the state message |
| `groups leave <room>` | `ungroup` | the state message |
| `groups save <group> <name> <room> <room>...` | `group_save` | the state message |
| `groups delete <group>` | `group_delete` | the state message |
| `groups take <target> [--source <source>]` | `take` | the state message |

A target is a room, a saved group or a group that exists now. A source is `stream`, `none`,
`line-in:<endpoint>/<input>` or `chime:<name>`; without `--source` the target keeps what its group
played.

### volume

| Command | Sends | `--json` prints |
|---|---|---|
| `volume get <room>` | (reads) | that room's object from `zones` |
| `volume get --group <group>` | (reads) | that group's object from `groups` |
| `volume set <room> <volume>` | `volume` | the state message |
| `volume set --group <group> <volume>` | `group_volume` | the state message |
| `volume step <room> <step>` | `volume_step` | the state message |
| `volume step --group <group> <step>` | `group_volume_step` | the state message |
| `volume mute <room>` | `mute` (`muted` true) | the state message |
| `volume unmute <room>` | `mute` (`muted` false) | the state message |
| `volume limit <room> <limit>` | `limit` | the state message |

A volume is the catalog's amplitude factor: 0 to 1 with at most three decimals (`0.350`, `1`,
`0`). `chorusctl` refuses anything else as a usage error instead of rounding it. A step is whole
thousandths, -1000 to 1000. A volume above a room's limit is not an error: the server clamps it,
and the line printed is what the room has now. Mute and limit are per room; the catalog has no
group mute.

### inputs

| Command | Sends | `--json` prints |
|---|---|---|
| `inputs list` | (reads) | the state's `inputs` array |
| `inputs select <input> <target>` | `take` with `source` `line-in:<input>` | the state message |

An input is `<endpoint>/<input>`, as `inputs list` prints it. `inputs select` is
`groups take <target> --source line-in:<input>` under the noun a person looks for it under.
`groups take <target> --source stream` goes back to the configured stream.

### endpoints

| Command | Sends | `--json` prints |
|---|---|---|
| `endpoints list` | (reads) | `{"endpoints":[...],"speakers":[...],"key_changes":[...]}` |
| `endpoints show <id>` | (reads) | `{"endpoint":<object or null>,"speaker":<object or null>}` |
| `endpoints name <speaker> <name>` | `speaker_name` | the state message |
| `endpoints room <speaker> <room>` | `speaker_room` | the state message |
| `endpoints room <speaker> --none` | `speaker_room` with `room` null | the state message |
| `endpoints forget <speaker>` | `speaker_forget` | the state message |

`speakers` are the adopted speakers, `endpoints` are the endpoints attached to rooms with their
link, and `key_changes` are speakers whose offered key differs from the pinned one. The server
leaves `speakers` and `key_changes` out of its state when they are empty; `endpoints list --json`
always has all three, as empty arrays then. `endpoints forget` is the catalog's only way past a
changed key: the speaker's next session is adopted afresh.

### updates

| Command | Sends | `--json` prints |
|---|---|---|
| `updates list` | (reads) | the state's `firmware.images` array (empty when there is none) |
| `updates status` | (reads) | the `speakers` objects that carry a `firmware` member |
| `updates install <speaker> <image> [--force]` | `firmware_install` | the state message |
| `updates install --all <image> [--force]` | `firmware_install` with `all` true | the state message |
| `updates cancel <speaker>` | `firmware_cancel` | the state message |
| `updates rescan` | `firmware_rescan` | the state message |

`updates install` sends the catalog's `firmware_install` to the server and does nothing else.
Whether an image goes to a speaker is the server's decision: it installs only a staged, verified
image of the speaker's board, and its sender refuses a transfer to a peer that is not loopback
(`owner-not-at-bench`) unless the owner, at the bench, started the server with
`CHORUS_OWNER_AT_BENCH` set to `1` (`docs/firmware-updates.md`, `docs/conventions.md` rule 20).
`chorusctl` never reads or sets `CHORUS_OWNER_AT_BENCH` and has no flag that reaches around the
server. An accepted `install` means the server took the request; `updates status` shows what
became of it (`STATE`, `REASON`, `RECEIVED`).

## Output

Without `--json`, a read verb prints a table or `key: value` lines, and a mutating verb prints the
part of the new state it changed (the room, the groups, the speaker, the images), or `ok` when
that part no longer holds what it named. That form is for a person and is not a contract: columns
may be added.

With `--json`, stdout is one line of JSON and it is the contract for scripts:

- A read verb prints the part of the state in the tables above. The part is cut out of the
  server's state message and written back as the server wrote it: the same member order and the
  same digits (`0.350` stays `0.350`, never `0.35`). Where the part is one value of the state it
  is a substring of what `GET /api/state` returned.
- A mutating verb prints the whole state message the server answered with, as received.
- The shapes follow the catalog (`docs/control-plane.md`): a new catalog member appears in
  `--json` output without a change here, so read by key and ignore members you do not know.

An error is never on stdout. Without `--json` it is two lines on stderr, what is wrong and what to
do. With `--json` it is one JSON object on stderr:

| Exit | Object |
|---|---|
| 1 | `{"error":"usage","exit":1,"detail":"...","hint":"..."}` |
| 2 | `{"error":"unreachable","exit":2,"detail":"...","server":"host:port"}` |
| 3 | `{"error":"refused","exit":3,"detail":"<the server's words>","status":400,"field":"zone"}` |
| 4 | `{"error":"not-found","exit":4,"detail":"..."}` |

For a refusal, `detail` and `field` are the server's own (`field` is empty when the server names
none) and `status` is the HTTP status it answered with.

## Exit codes

| Code | Name | Meaning |
|---|---|---|
| 0 | ok | the command was applied, or the state was read and printed |
| 1 | usage | the command line is not valid; nothing was sent |
| 2 | unreachable | no answer from a chorus server: connect, timeout or a bad HTTP answer |
| 3 | refused | the server answered and refused (its `error` or `refused` message) |
| 4 | not-found | the server answered; what a read verb named is not in its state |

- 1 also covers no server being named. A usage error never opens a connection.
- 2 covers a name that does not resolve, a refused connection, no answer within `--timeout`, an
  answer that is not HTTP, a body shorter than its `Content-Length`, and an HTTP answer whose
  body is not a control catalog message (a proxy's error page, some other service on that port).
- 3 is any answer that is the catalog's `error` or `refused`, whatever its HTTP status: a command
  the state refuses (400), a catalog version the server does not implement (426), a busy server
  (503). Nothing was applied.
- 4 is only for the read verbs that name one thing (`rooms show`, `volume get`,
  `endpoints show`): the server answered and its state has no such room, group, speaker or
  endpoint. A mutating verb that names something unknown is the server's refusal, exit 3.

## How it is tested

- `crates/ctl/tests/commands.rs`: every command sent is byte-equal to its vector under
  `fixtures/control` (the arguments come from the vector's `.fields`), and every message type the
  table says a verb sends has such a case.
- `crates/ctl/tests/output.rs`: the listings and the `--json` output, from the state vectors.
- `crates/ctl/tests/exit_codes.rs`: each exit code, against an in-process fake server on loopback
  replaying the vectors' bytes.
- `crates/server/tests/chorusctl.rs`: the real `chorus-server` binary driven through all six
  nouns.

```sh
cargo test -p chorus-ctl
cargo test -p chorus-server --test chorusctl
```
