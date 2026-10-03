# Spotify Soloist receivers: the sidecar, its protocol and its fake

Spotify Soloist is Spotify's own Spotify Connect receiver program for Linux, proprietary, with a
90-day build lifetime and a per-developer API key. chorus never downloads, ships, links or runs
it in its own builds and tests: the owner installs the binary and the key on the host, and chorus
supervises it (proposal P7, Option C). This page is what goal 17's track S1 built: the supervisor
`chorus-soloistd`, the pure library `chorus-soloist` both sides share, and the fake Soloist the
tests run. The server side (the FIFO readers, the pool's manager, the `soloist:` source,
`chorusctl soloist`) is a later change and has its own section in this page when it lands.

Decision record: [0130](decisions/0130-the-soloist-receiver-supervisor.md).

## Words

- A **receiver** is one Soloist instance with everything it needs: one container holding
  PipeWire, WirePlumber, one Soloist process and `chorus-soloistd`. Receivers are numbered from
  0; the id is `r<i>`. (P7 calls this a "slot"; in chorus's code a slot is a stream slot.)
- A **target** is a room, a saved group or a live group. Its key is the UPnP renderer's:
  `room:<id>`, `group:<id>`, `live:<member ids, sorted, joined by +>`.
- The **receiver directory** is the one directory chorus-server and the receiver containers
  share (`--soloist-dir` on both sides). Nothing else connects them: no network path.

## The receiver directory

For receiver `i`:

| File | What | Made by |
|---|---|---|
| `r<i>.lock` | held with an exclusive `flock` for the supervisor's lifetime; how identical containers tell themselves apart | `chorus-soloistd` |
| `r<i>.pcm` | the PCM FIFO, mode 0660: interleaved little-endian float32, 44100 Hz, 2 channels | `chorus-soloistd` (before PipeWire starts); written by PipeWire's sink `chorus-r<i>` |
| `r<i>.sock` | a Unix stream socket, mode 0660, the supervisor protocol below; one connection at a time, a new one replaces the old | `chorus-soloistd` |

The names and the format are constants of `chorus_soloist` (`lock_file_name`, `pcm_file_name`,
`socket_file_name`, `node_name`, `PCM_RATE`, `PCM_CHANNELS`, `PCM_FRAME_BYTES`,
`PIPEWIRE_FORMAT`).

The FIFO carries audio only while Soloist plays: an idle sink writes nothing, so its bytes are
not a liveness signal. A full FIFO drops audio and never stalls Soloist (the sink's
`tunnel.may-pause` stays at its default, false for a sink: "A paused stream will consume no CPU
and will resume when the fifo becomes readable or writable again" is the behaviour chorus does
not want; https://docs.pipewire.org/page_module_pipe_tunnel.html, read 2026-10-03). A reader
that attaches late finds the oldest audio the pipe could hold, so the server drains every FIFO
always and discards what no group listens to.

## `chorus-soloistd`

```
chorus-soloistd --soloist-dir DIR --api-key-file FILE --state-dir DIR --cache-dir DIR [options]
```

| Flag | Default | Meaning |
|---|---|---|
| `--soloist-dir DIR` | required | the receiver directory |
| `--receivers N` | 1 | the pool's size; the lowest free index below N is claimed |
| `--receiver I` | none | claim exactly this index |
| `--soloist-bin PATH` | `/opt/soloist/soloist` | the owner's Soloist executable |
| `--api-key-file FILE` | required | the API key; read at each start of Soloist, never logged |
| `--state-dir DIR` | required | Soloist data directories, one per target, live under it |
| `--cache-dir DIR` | required | Soloist cache directories, one per target, live under it |
| `--cache-size MB` | 256 (`ASSUMED`, P7) | Soloist's `--cache-size`: 0 or at least 100 |
| `--pipewire auto\|none` | `auto` | run PipeWire and WirePlumber, or neither (tests) |
| `--pipewire-bin`, `--wireplumber-bin` | `pipewire`, `wireplumber` | the two programs |
| `--pipewire-runtime-dir DIR` | `/run/chorus-soloist` | where the configuration and PipeWire's socket go |
| `--wireplumber-config-dir DIR` | `/usr/share/wireplumber` | WirePlumber's stock configuration |
| `--ws-timeout-ms` | 20000 | how long Soloist has to offer its WebSocket |
| `--stop-timeout-ms` | 5000 | SIGTERM to SIGKILL |
| `--backoff-min-ms`, `--backoff-max-ms` | 1000, 60000 | the retry delay after a failure: doubled each time, capped |
| `--expiry-check-secs` | 86400 | how often the build's expiry is looked at |

The defaults in the last four rows are chorus's own choices, not measured values.

Exit codes: 0 stopped by SIGTERM or SIGINT; 2 a usage error; 3 no receiver index is free (or the
one named is held); 4 the receiver directory cannot be used (the lock file, the FIFO or the
socket); 5 PipeWire could not be started.

What it does:

1. Claims its index, makes the FIFO.
2. With `--pipewire auto`: writes the configuration below and runs PipeWire, then WirePlumber.
   If either exits, both are started again after the backoff, and so is Soloist.
3. Reads `soloist --version` (at start, on `restart`, and at every expiry check) and logs
   `warning: Soloist build expires in N days` from 14 days before expiry, or
   `warning: Soloist build expired`.
4. Listens on `r<i>.sock` and follows the server.
5. On `assign`, runs Soloist with exactly these arguments (Soloist's command-line reference,
   https://developer.spotify.com/documentation/soloist/reference/command-line, read 2026-10-03):

   ```
   --device-name <name> --data-dir <state-dir>/<directory name of the key>
   --cache-dir <cache-dir>/<directory name of the key> --cache-size <MB>
   --pipewire-device chorus-r<i> --initial-volume 100 --ws 127.0.0.1:0 --api-key <key>
   ```

   The directory name of a key (`chorus_soloist::keydir::dir_name`): `room:kitchen` is
   `room-kitchen`, `group:downstairs` is `group-downstairs`, `live:den+kitchen` is
   `live-den_kitchen`. The mapping is injective, so a target always gets the same directory and
   with it the same Spotify Connect identity and stored session ("Use the same data directory
   across restarts to keep the same device identity and stored Spotify Connect session",
   https://developer.spotify.com/documentation/soloist/tutorials/getting-started, read
   2026-10-03).
6. Waits for `ws.port` in the data directory (stale `ws.port` and `ws.addr` are removed before
   the start), connects to `ws://<ws.addr or 127.0.0.1>:<ws.port>/` with its own RFC 6455
   client, and relays every JSON text frame as an `event`. No `ws.port`, or no successful
   handshake, within `--ws-timeout-ms` is a fault: Soloist "starts without the WebSocket API and
   logs a warning" when it cannot bind, and a receiver chorus cannot talk to is stopped and
   retried. A connection that drops later is connected again under the same bound.
7. When Soloist exits by itself: code 10 is `expired`, and nothing starts it again until
   `restart` ("Spotify Soloist build expired. Install a newer build before starting Spotify
   Soloist again."); anything else is `failed` and retried after the backoff.
8. Stops Soloist with SIGTERM, then SIGKILL after `--stop-timeout-ms`: on `release`, on an
   `assign` of another target or name, and on its own SIGTERM (a container stop).

It reaps its own children (PipeWire, WirePlumber, Soloist) and no others: as PID 1 of a
container it does not adopt and reap processes those leave behind, so the receiver container
runs it under an init (compose `init: true`) or as PID 1 knowing that; the image track decides
which. It handles SIGTERM and SIGINT itself either way.

The API key: Soloist accepts it only as a command-line argument (`-k, --api-key KEY`: "Treat
this value as a secret"; the reference lists no key file and no environment variable, and says
"Spotify Soloist does not currently read a configuration file"), so it is visible in `/proc/<pid>/cmdline` inside the
receiver container, which holds no other user. Everything the supervisor writes goes through
one redaction by value: its own log lines (the command line it logs reads
`--api-key [redacted]`), Soloist's output passed through, and every protocol message.

## The supervisor protocol

On `r<i>.sock`: newline-delimited JSON objects, UTF-8, at most 65536 bytes a line (the line
feed included), both ways. Every message has `"t"`. `chorus_soloist::protocol` encodes and
decodes both directions (`FromSupervisor`, `ToSupervisor`, `LineBuffer`); the examples below are
the fixtures `fixtures/soloist/protocol-*.line`, byte for byte. A kind a reader does not know is
skipped, so either side can gain a message.

Supervisor to server:

```
{"t":"hello","v":1,"receiver":3,"supervisor":"0.1.0"}
{"t":"build","present":true,"version":"Soloist 1.3.8.96, build 20260930, Linux/aarch64","build_epoch":1790726400,"expires_epoch":1798502400}
{"t":"status","state":"running","target":"live:den+kitchen","name":"Kitchen + Den","detail":"","generation":12}
{"t":"event","generation":12,"event":{"type":"playback_changed","status":"paused"}}
```

- `hello` is first on every connection, then `build`, then `status`. If Soloist is running, the
  supervisor also asks it for `auth_state` again, so a server that reconnects gets one.
- `build`: `present` is whether an executable exists at `--soloist-bin`; `version` is the first
  line of `soloist --version`, at most 200 characters; `expires_epoch` is `build_epoch` plus 90
  days. Both are `null` when the version output names no build time: "expiry unknown", never
  "expired". Sent again whenever it changes.
- `status`: `state` is `idle` (no target), `starting` (Soloist is being started, stopped for a
  new target, or its WebSocket is not connected yet), `running` (Soloist is up and its WebSocket
  is connected), `expired` (exit code 10), `failed` (a failure, with the retry in `detail`:
  `Soloist exited 1; retry in 4 s (attempt 3)`), `no-binary`. Sent on every change. A status
  whose `detail` starts `command dropped` or `assign refused` is a remark about one message and
  changes nothing else.
- `event`: one JSON text frame of Soloist, as Soloist sent it (re-serialised compactly, members
  in their order, numbers as their digits). Parse `event` with
  `chorus_soloist::api::event_from_value`. Events that arrive while no server is connected are
  not kept.

Server to supervisor:

```
{"t":"assign","generation":12,"target":"live:den+kitchen","name":"Kitchen + Den"}
{"t":"release","generation":13}
{"t":"command","generation":12,"command":{"type":"command","command":"play","uri":"spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"}}
{"t":"restart"}
```

- `assign`: run Soloist for this target under this device name. The same target and name as
  now changes only the generation (Soloist carries on; an `expired` or `failed` receiver stays
  so). Another target or name stops Soloist, then starts it. A target that is not a key, or an
  empty name, is refused with a remark.
- `release`: stop Soloist, go `idle`.
- `command`: a Soloist command object, sent as is (`chorus_soloist::api::Command::to_value`
  builds one). Dropped with a remark when Soloist is not `running` or the generation is not the
  present one.
- `restart`: read the build again, clear `expired` and `failed`, start again. This is what
  `chorusctl soloist restart` will send after the owner replaces the binary.

`generation` is the server's counter. `assign` and `release` set it; every `status` and `event`
echoes it, so the server drops what belongs to a target it has since replaced.

## The Soloist WebSocket API model

`chorus_soloist::api` is Soloist's WebSocket API reference
(https://developer.spotify.com/documentation/soloist/reference/websocket-api, read 2026-10-03):
`Command` builds the documented command objects, and `parse_event` reads events tolerantly
(every member but `type` is optional; an unknown event type is `Event::Other`). The page's own
examples are the fixtures `fixtures/soloist/command-*.json`, `event-*.json` and `entity.json`.
The API has no request ids, and an `error` carries only a message, of which one string is
documented ("command requires authentication"): nothing matches on message text.
`position.timestamp_ms` is Soloist's wall clock and never reaches the audio path.

## Build expiry

"The output includes the Spotify Soloist version, build timestamp, build identifiers, platform,
and architecture" is all the command-line reference says about `--version`: no example. The
shapes users transcribed in issue reports of the public `spotify/soloist` repository (issues 1,
8, 10 and 13, read 2026-10-03; LEADs, not the literal output) all carry a 10-digit Unix time or
a calendar date, so `chorus_soloist::build::parse_version` looks for a run of exactly 10 digits
that is a time from 2020 to 2099, else a `YYYYMMDD` or `YYYY-MM-DD` date (midnight UTC of that
day, the earliest the build can have been made, so a warning is never late), and otherwise
reports "unknown". The four shapes and a garbage text are fixtures
(`fixtures/soloist/version-*.txt`). `Expiry::at` turns a build time and "now" into `Unknown`,
`Fine`, `Warning` (14 days or fewer left) or `Expired`. The 90 days are P7's verified claim
("Builds expire 90 days after their build date", `docs/proposals/P7-spotify-soloist.md`,
found unchanged on 2026-10-03 on the downloads and updates page,
https://developer.spotify.com/documentation/soloist/reference/downloads-and-updates, and the
overview: Soloist "exits with code `10` when the build has expired"); the 14 days are P7's
choice.

## PipeWire

`--pipewire auto` writes, under `--pipewire-runtime-dir` (mode 0700, also `XDG_RUNTIME_DIR` for
PipeWire, WirePlumber and Soloist):

- `conf/pipewire.conf`: a complete daemon configuration with no ALSA, Bluetooth, D-Bus or
  real-time module, a 44100 Hz clock and a timer driver;
- `conf/pipewire.conf.d/10-chorus-receiver.conf`: the sink, `libpipewire-module-pipe-tunnel`
  with `tunnel.mode = sink`, `pipe.filename` the FIFO, `audio.format = F32LE`,
  `audio.rate = 44100`, `audio.channels = 2`, `node.name = chorus-r<i>`;
- `conf/client.conf`: for Soloist's own PipeWire context (no D-Bus, no mlock);
- `wireplumber/wireplumber.conf.d/10-chorus-receiver.conf`: a profile `chorus-receiver` with
  the linking policy only. WirePlumber is run as `wireplumber -p chorus-receiver` with
  `WIREPLUMBER_CONFIG_DIR=<that directory>:<--wireplumber-config-dir>`.

A session manager is needed: without one a client is never linked to the sink and the FIFO
stays empty (goal 17's PipeWire probe; its report is the measurement track's).

Run here by hand on 2026-10-03 (not part of the gate, which runs no PipeWire: conventions rule
10): `chorus-soloistd --pipewire auto` at this change, with PipeWire 1.4.2 and WirePlumber 0.5.8
from Debian 13's packages extracted rootless, and `pw-cat -p --target chorus-r0 --rate 44100
--channels 2 --format f32 --raw -` playing a 3 s float32 ramp: the FIFO delivered 2048 zero
frames, then all 132300 frames bit for bit, then 820 zero frames; SIGTERM ended the supervisor
with exit code 0 and no process left. This shows the configuration and the supervision work; it
is not a timing measurement.

## The fake Soloist

`crates/soloist-fake` is a library with no binary target; the program the tests run is the
example `chorus-fake-soloist` of `crates/soloistd`. `cargo build --bins` (which
`deploy/Dockerfile`, `tools/image.sh` and `tools/release.sh` use) never builds an example or
this library's code into a program, so no image and no release can carry the fake. Its header
lists, item by item, what is documented behaviour (the options, the exit codes 0, 1 and 10, the
data directory's files, the stored session, the WebSocket API's commands, events and errors,
the bind failure) and what is a named assumption. It writes the FIFO itself, standing in for
the sink, with a signal in which every frame names its URI and its index
(`chorus_soloist_fake::expected_pcm`, `frame_of`), and a test plays the Spotify app through a
control socket: `login`, `logout`, `play <uri>`, `pause`, `resume`, `volume <n>`, `drop-ws`,
`exit <code>`.

`crates/soloistd/tests/supervisor.rs` runs the real `chorus-soloistd` against it: 15 tests,
about 1 s in all on the development host when run alone (they run in parallel; the two fixed
waits are 300 ms and 400 ms, every other wait ends when the awaited message arrives).

## What is assumed, pending the owner's build

Soloist's documentation does not state these; each is a named assumption here and an
owner-build question. Where the code depends on one, it fails safe.

| Not stated | Assumed | If wrong |
|---|---|---|
| the `--version` format | a 10-digit Unix time or a `YYYYMMDD` / `YYYY-MM-DD` date appears in it | the build time reads as unknown: no warning, and exit code 10 still gives `expired` |
| which signal is a normal shutdown | SIGTERM | Soloist is killed after `--stop-timeout-ms`; whether a stale `soloist.pid` then blocks the next start is itself not stated |
| the output sample format | float32, 44.1 kHz, stereo (LEADs in issues 2 and 6) | nothing breaks: the sink converts whatever Soloist plays to the FIFO's format |
| the format of `ws.port` and `ws.addr` | a number, an address, with optional surrounding whitespace | no connection within the bound: `failed`, with the reason in `detail` |
| the WebSocket path, ping and close behaviour | path `/`, no subprotocol; pings are answered, a close is answered | the handshake fails: `failed` |
| whether a running Soloist exits at the moment of expiry | not relied on | either way exit code 10 is `expired` |
| Soloist's PipeWire node properties | not relied on: routing is by `--pipewire-device` | none |
| the cache size, 256 MB | P7's `ASSUMED` | a flag |

## Sources

Soloist's documentation, all read 2026-10-03: the command-line reference, the WebSocket API
reference, the getting-started tutorial, the overview and the downloads and updates pages under
https://developer.spotify.com/documentation/soloist. RFC 6455
(https://www.rfc-editor.org/rfc/rfc6455), RFC 3174 and RFC 4648, read 2026-10-03. PipeWire's
pipe-tunnel module page (https://docs.pipewire.org/page_module_pipe_tunnel.html, read
2026-10-03). No Soloist binary or archive was downloaded or run.
