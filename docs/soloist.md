# Spotify Soloist receivers: the sidecar, its protocol and its fake

Spotify Soloist is Spotify's own Spotify Connect receiver program for Linux, proprietary, with a
90-day build lifetime and a per-developer API key. chorus never downloads, ships, links or runs
it in its own builds and tests: the owner installs the binary and the key on the host, and chorus
supervises it (proposal P7, Option C). This page is both halves: the supervisor
`chorus-soloistd`, the pure library `chorus-soloist` both sides share and the fake Soloist the
tests run; and, from "The server side" on, what `chorus-server` does with the receivers: the
flags, the `soloist:` source, take the room, volume, the alarm source and the expiry warning.

Decision records: [0130](decisions/0130-the-soloist-receiver-supervisor.md) (the supervisor),
[0132](decisions/0132-the-soloist-receivers-in-the-server.md) (the server side) and
[0131](decisions/0131-the-chorus-soloist-image.md) (the `chorus-soloist` image, its listings check
and the deploy files; "The image" and "Running the receivers" below).

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
| `--health-check` | | with `--soloist-dir` (and `--pipewire`, `--pipewire-runtime-dir` when not the defaults): the container's probe, below |

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
runs it under an init (`init: true` in `deploy/soloist/compose.yaml`, ADR 0131: what Soloist
forks is not documented). It handles SIGTERM and SIGINT itself either way.

`chorus-soloistd --health-check --soloist-dir DIR` is the probe a container healthcheck runs. It
starts nothing and **connects to nothing**: the receiver's socket takes one connection and a new
one replaces the old, so a probe that connected would drop chorus-server each time it ran.
After claiming its receiver the supervisor writes the index to the file `receiver` in its
runtime directory (private to the container); the probe reads it and exits 0 when `r<i>.lock`
is held by someone (an exclusive lock taken from the probe would block), `r<i>.sock` is a
socket, `r<i>.pcm` is a FIFO and, with `--pipewire auto`, PipeWire's socket exists; otherwise 1
with the reason on one line. It says the supervisor is alive and holds its receiver. It does
not say Soloist is logged in or playing: that is the `status` and the events chorus-server
gets.

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

Since ADR 0131 the full gate runs this path on the image's own PipeWire (`make soloist-image`,
"The image" below). Before that, run here by hand on 2026-10-03: `chorus-soloistd --pipewire auto` at this change, with PipeWire 1.4.2 and WirePlumber 0.5.8
from Debian 13's packages extracted rootless, and `pw-cat -p --target chorus-r0 --rate 44100
--channels 2 --format f32 --raw -` playing a 3 s float32 ramp: the FIFO delivered 2048 zero
frames, then all 132300 frames bit for bit, then 820 zero frames; SIGTERM ended the supervisor
with exit code 0 and no process left. This shows the configuration and the supervision work; it
is not a timing measurement.

## The image

`make soloist-image` (`tools/soloist-image.sh`) builds `target/image/chorus-soloist-oci.tar`, an
OCI image layout of one receiver, with no container daemon (ADR 0131 says why this way):

| In the image | From |
|---|---|
| Debian 13 (trixie), `docker.io/library/debian:trixie-20260918-slim` pinned by digest | the registry, once, then the cache |
| 60 Debian packages: PipeWire 1.4.2, WirePlumber 0.5.8 and what they depend on, unpacked with `dpkg-deb -x` | snapshot.debian.org at `20260918T000000Z`, each file held to the sha256 in `deploy/soloist/debian-packages.pins` |
| `/usr/local/bin/chorus-soloistd`, a static binary (the entrypoint) | this workspace |
| `/etc/passwd` and `/etc/group` with the user `chorus`, uid and gid 65532 (chorus-server's, so both can use the receiver directory) | the base's files plus one line each |
| `/etc/ld.so.cache` | the image's own `ldconfig`, run by the build |
| `/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md`, `/usr/share/doc/<package>/copyright`, `/var/lib/dpkg/status.d/<package>` | `deploy/soloist/THIRD-PARTY-NOTICES.md` plus the package table; the packages themselves; each package's control file |

**No Soloist file is in the image**, and none is in the repository or a release: `make
soloist-lists` prints what chorus ships and fails on any such file (conventions rule 24).
PipeWire's and WirePlumber's configuration is not in the image either: `chorus-soloistd` writes
it at start ("PipeWire" above).

Mount points and what the run provides (`deploy/soloist/compose.yaml`):

| Path | What | In the reference compose file |
|---|---|---|
| `/run/chorus/soloist` | the receiver directory, shared with chorus-server | a host directory owned by 65532, bound into both |
| `/opt/soloist` | the owner's Soloist: the executable `/opt/soloist/soloist` | a host directory, read-only |
| `/run/secrets/soloist_api_key` | the owner's API key | a host file, read-only, readable by 65532 |
| `/var/lib/chorus-soloist` | state: Soloist's data directory per target (the Connect identity and session) | a host directory; worth backing up |
| `/var/cache/chorus-soloist` | Soloist's cache per target | a host directory outside any backed-up path |
| `/run/chorus-soloist` | the supervisor's runtime directory: the written configuration, PipeWire's socket, the `receiver` index file | a tmpfs owned by 65532 |

The image's command is `--soloist-dir /run/chorus/soloist --api-key-file
/run/secrets/soloist_api_key --state-dir /var/lib/chorus-soloist --cache-dir
/var/cache/chorus-soloist`; the compose file repeats it and adds `--receivers`. The root file
system can be read-only. The image is amd64 only, as the server image is.

What the image test proves, and what it does not. `make soloist-image` unpacks the tarball
(umoci, rootless) and, with no container (the gate has no user namespace and no chroot), runs
the unpacked files by path: the image's programs are started through the image's own loader
over the image's own libraries, and PipeWire's module directories are named by environment
variables where a container would use the compiled-in paths. It shows:

- the entrypoint, the command and the user of the image's configuration; `chorus-soloistd` is a
  static position-independent executable and runs; `/opt/soloist` is an empty directory;
- every pinned package's copyright file and control record is present and the notices name each
  package and its source;
- every `NEEDED` library of every program and library the packages brought is in the image's
  loader cache or the file's own `RUNPATH` (253 files): the loader finds them with no maintainer
  script having run, and nothing needs a file of an excluded package;
- the image's `pipewire` and `wireplumber` are the pinned versions and run;
- **the real PCM path**: the image's `chorus-soloistd --pipewire auto` writes the configuration,
  starts and supervises the image's PipeWire and WirePlumber; the image's own `pw-cat` plays a
  2 s float32 signal into the sink `chorus-r0`; the FIFO `r0.pcm` must deliver frames of that
  signal only, bit for bit and in order (a quantum lost to a busy host is counted and reported,
  and less than half arriving fails);
- **the supervisor's half**, on the same running supervisor: `--health-check` healthy; a
  connection to `r0.sock` as chorus-server makes it gets `hello`, `build` and `status idle`;
  `assign` starts the fake Soloist (built outside the image, given by `--soloist-bin`, standing
  in for the owner's binary) and gives `status running`; the fake's login and play come back as
  relayed events; `release` gives `idle`; SIGTERM ends the supervisor with exit code 0 and no
  child left; `--health-check` then says unhealthy; the API key is in no log line.

It does not show: Soloist itself (never downloaded or run here), so not that Soloist finds the
libraries it needs in this image, nor that Soloist's PipeWire client links to the sink the way
`pw-cat --target` does (the fake Soloist has no PipeWire client: it writes a FIFO itself, which
is off in this test, so the audio in the FIFO is `pw-cat`'s, never the fake's); the image inside
a container runtime (the read-only root, the tmpfs, the dropped capabilities, the limits, the
macvlan network, the healthcheck as Docker runs it); anything about timing (conventions rule
11); arm64.

## Running the receivers

`deploy/soloist/compose.yaml` is the reference: one service, `deploy.replicas` identical
containers on an external macvlan network (each receiver needs UDP 5353 of its own network
namespace, P7), every container claiming the lowest free receiver index. chorus-server's
half is the override `deploy/soloist/server.compose.yaml`, added to `deploy/compose.yaml` only on
a host that runs the receivers: the same receiver directory mounted, and `--slots`,
`--soloist-dir` and `--soloist-receivers` on the command ("The server side" below; the other
flags keep their defaults). Nothing in
this repository deploys anything; the homelab's stack is a PR in the owner's homelab repository.

The owner's steps, on the host that runs both:

1. **The network.** Create a macvlan network on the LAN interface with a range of addresses
   reserved for the receivers, named `chorus-soloist-lan` (or set `CHORUS_SOLOIST_NETWORK`).
   The interface, subnet and range are the host's and stay out of this repository.
2. **The directories**, all owned by uid and gid 65532: under `/srv/chorus/soloist` (or
   `CHORUS_SOLOIST_HOST_DIR`) `receivers` (mode 0770), `bin`, `secrets` (mode 0700) and
   `state`; and the cache, `/var/cache/chorus-soloist` (or `CHORUS_SOLOIST_CACHE_DIR`), outside
   whatever the host backs up (up to 256 MB per target, `ASSUMED`, P7). `state` holds each
   target's Spotify Connect identity and session and is worth backing up.
3. **Place the binary**: Spotify's Soloist build for the host's architecture as
   `bin/soloist`, executable. chorus never downloads it.
4. **Place the key**: the Soloist API key as the single line of `secrets/api_key`, mode 0400,
   owner 65532. It is read at each start of Soloist and never logged.
5. **Start** the receivers with the image pinned by digest (`CHORUS_SOLOIST_IMAGE`: the
   manifest digest `make soloist-image` prints, which the release notes repeat), and
   chorus-server with the override (`docker compose -f deploy/compose.yaml -f
   deploy/soloist/server.compose.yaml up -d`), with the same `CHORUS_SOLOIST_RECEIVERS`.
6. **Every 90 days at the latest, replace the binary**: put the new build at `bin/soloist`,
   then `chorusctl soloist restart` (restarting the receiver containers does the same). From 14 days before expiry the supervisor logs
   `warning: Soloist build expires in N days`; an expired build exits with code 10 and the
   receiver stays `expired` until the restart.

The limits per receiver in the compose file (`mem_limit: 192m`, `cpus: 0.25`,
`pids_limit: 64`) are P7's and `ASSUMED`: Soloist's own cost cannot be measured without the
owner's build. The PipeWire half is measured: about 19 MB RSS, about 0.25 % of one core and 6
threads for PipeWire and WirePlumber with one client playing (goal 17's PipeWire probe,
PipeWire 1.4.2 and WirePlumber 0.5.8, 2026-10-03, `Source: host`; the measurement track's
report carries it).

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
about 1 s in all on the development host when run alone (they run in parallel; the three
fixed waits are 300 ms, 300 ms and 400 ms, every other wait ends when the awaited message arrives).

## The server side

```
chorus-server ... --control-listen ADDR --slots S --soloist-dir DIR --soloist-receivers N
```

| Flag | Default | Meaning |
|---|---|---|
| `--soloist-receivers N` | 0 | how many receivers, `r0` to `r<N-1>`, at most 32 (`ASSUMED`: twice P7's default pool of 16; P11, accepted by the owner on 2026-10-05, measured what a host carries and keeps the pool at 16). 0, or the flag absent: no Soloist code runs and no thread of it exists |
| `--soloist-dir DIR` | required with N above 0 | the receiver directory, the same one the receiver containers are given |
| `--soloist-grace SECONDS` | 60 (`ASSUMED`, P7) | how long a dissolved, idle live group keeps its receiver |
| `--soloist-volume chorus\|receiver` | `chorus` | which gain stage a Spotify volume drives; "Volume" below |
| `--soloist-alarms` | off | let an alarm play a stored Spotify URI; "The alarm source" below |

The receivers need the control plane and at least one stream slot. A receiver directory the
server cannot list at start is exit code 11, before any thread exists. A receiver whose
container has not started is not an error: its FIFO and socket are looked for again every
250 ms for as long as the server runs. The other flags are refused when `--soloist-receivers` is
absent and are inert beside an explicit `--soloist-receivers 0`, so a deployment switches the
receivers off by changing one number.

### Threads

`N + 1`, made at start and never after, registered by role like every thread of the server
(`docs/control-plane.md`, "The thread population"): `soloist-reader-<i>` per receiver and one
`soloist-manager`. They exist whether or not any receiver container is running.

### The reader and the port

`soloist-reader-<i>` (`crates/server/src/soloistreader.rs`) is the one reader of `r<i>.pcm`. It
opens it the way the FIFO source opens a pipe (read-write and non-blocking: no writer is not a
hang, a writer that closes is not an end) and reads it always, played or not, so the pipe never
fills. It converts the FIFO's float32 44.1 kHz stereo to the server's channel count with the
media player's remix and to the server's rate with its resampler (`chorus_decode`), and writes
the receiver's port (`crates/server/src/soloistport.rs`), a ring the audio thread takes one
chunk from each tick without waiting.

The port's rules, and where each number comes from:

- **While no group plays the receiver, what is read is discarded** (and counted). Selecting and
  deselecting both empty the ring, so a group that takes a receiver hears only what arrived
  after it did.
- **A full ring drops and counts**: the line-in port's rule. It holds one second (`ASSUMED`, the
  size of the other ports).
- **A fill target of 120 ms.** The port plays nothing until it holds 120 ms, and fills to it
  again after it ran dry. This is chorus's choice from goal 17's PipeWire probe (PipeWire 1.4.2
  and WirePlumber 0.5.8 on the development host, 2026-10-03; `Source: host`, its report is the
  goal's measurement track's): the sink writes the FIFO one whole quantum at a time, at most
  2048 frames (46.4 ms), and on that host, loaded and without real-time scheduling, the largest
  gap between two writes was 98 ms. 120 ms is that gap plus one default 20 ms chunk. It is the
  "extra pipe buffer" P7 expected chorus to absorb; it delays what a room hears and is not a
  sync error, because every room of the group plays the same chunk at the same instant.
- **Running dry is silence, on time, and counted**: the missing part of a chunk is padded, and a
  dry spell shorter than 500 ms (`ASSUMED`) is an underrun. A longer one is the music having
  stopped (an idle sink writes nothing at all), and the next stream starts on an empty ring.
- The probe also saw whole quanta lost on the loaded host. Nothing can put those back; the
  frames either side are played in order, and the gap is an underrun in the counters.

The counters are in `GET /metrics` (`docs/telemetry.md`).

**Not built: rate matching.** The receiver's audio is clocked by the receiver container's
PipeWire graph, which is driven by a timer on the host's clock (the probe measured 44100.07
frames a second, 2 parts in a million fast), and chorus's timeline is the same host's monotonic
clock, so the two do not drift apart as a DAC and a server would. What difference there is ends
as a dropped write or an underrun after hours, both counted. If the owner's build shows more,
the fix is the line-in's (a rate-matched resampler), not a larger ring.

### The source and take the room (K78)

A group that plays receiver `i` has the source `soloist:r<i>` (`docs/control-plane.md`). No
command names it: the manager is the only thing that gives a group that source, and a client's
`take` naming one is refused by name.

`soloist-manager` (`crates/server/src/soloist.rs`) connects to every `r<i>.sock`, runs the pool
(`chorus_soloist::pool`) over the targets of the control state, and sends each supervisor the
`assign` or `release` it has not been sent. Then:

- **A target's device plays** (its receiver reports `playing`) **and the target's group does
  not play it**: first every OTHER receiver that a room of the target is hearing is sent `pause`
  then `deactivate`, then the target's rooms are made one group playing the receiver, with the
  catalog's own `take`. So playing on a saved group's device moves the member rooms into the
  group, and a member room's own device, if it was playing, is paused and deactivated before
  the take; and playing on a room's own device while the room is in a group takes the room out
  of the group. Two Spotify sources are never mixed in a room: the displaced receiver is told
  first, and the room's slot changes input at a chunk boundary, from one receiver's port to the
  other's (proposal P7, the Developer Policy's overlap clause).
- This rule is read literally in both directions, as the goal's design states it: when a room's
  own device takes the room out of a group, the group's device is one "a room of the target is
  hearing", so it is paused too, and the rooms left in the group go quiet until somebody plays
  on the group's device again. (If the same account moved its playback from the group's device
  to the room's, Spotify itself stops the group's device; chorus doing so as well changes
  nothing. With two accounts it is a choice, recorded in the decision record as one to revisit
  on the owner's build.)
- **A group stops playing a receiver, whoever made it** (it took another source, an alarm rang,
  a UPnP cast took the room, the group dissolved): the receiver is sent `pause`, and
  `deactivate` if it was the active device, so the Spotify app shows the truth and nothing
  plays unheard.
- After the manager's own `pause` a receiver that still says `playing` is left alone for 3 s
  (`ASSUMED`), so a report already on its way does not take the room back.

What is playing goes into the group's now-playing record: the item's name, its creators joined
by `, `, its parent's name as the album, its `large` cover (else the first), its duration, and
`playing` or `paused`, with `via` `spotify`. `position.timestamp_ms` is never read.

`chorusctl soloist pause|resume|next|previous <room or group>` (the catalog's `playback`) are
forwarded to the receiver the group plays as `pause`, `play`, `skip_next` and `skip_prev`.

### Volume (K77, K81), and why there are two mappings

Which gain stage Soloist's volume drives is not documented (`--initial-volume`: "If omitted,
Spotify Soloist uses the audio system's current/default volume", the command-line reference,
read 2026-10-03), and only the owner's build can show whether the audio in the FIFO has already
been scaled by it. P7 therefore asked for both mappings behind one switch:

- **`--soloist-volume chorus`** (the default) takes the audio to arrive untouched (Soloist is
  started with `--initial-volume 100`). A volume the Spotify app sets becomes the target's
  volume in chorus, through the catalog's own `volume` and `group_volume`, so every room's
  limit clamps it; and the target's volume, whoever changed it, is sent to the receiver with
  `set_volume`, so the app's slider shows what the rooms are at (and jumps back to the limit
  when a person pushed it past).
- **`--soloist-volume receiver`** takes Soloist's volume to act on its audio: it is the group
  gain, chorus leaves its own volumes alone (they are per-room trims on top), and chorus only
  clamps: a Spotify volume above the target's limit is set back to the limit.

The echo guard is one rule in both: a `volume_changed` equal to what the manager last sent is
its own echo and is not applied. If the default is wrong for the owner's build (the audio is
scaled by Soloist AND by chorus), music is quieter than the slider says and never louder: the
limits hold under either mapping.

### The alarm source, and why it ships off

`docs/inputs.md`, "The Spotify playlist source", has the behaviour and the fallback reasons.
The switch is off by default because of the Developer Policy's alarm clause (proposal P7,
"Alarms": the API allows the play, and the clause is the owner's to read in their own
dashboard before turning it on).

### Announcements pause, they do not duck (a rule for goal 20)

When the announcement path arrives (goal 20, K71), a room playing a `soloist:` source is
PAUSED through the API for the announcement and resumed after it, never ducked, and a switch
to or from a `soloist:` source cuts and never crossfades (the same overlap clause). Nothing of
it is built here; `Source::is_soloist` (`chorus_control::rooms`) is the query that path uses.

### When the build expires

Soloist builds expire 90 days after their build date. The server shows it four ways, from 14
days before:

- the state's `soloist` member: `warning` (`Soloist build expires in N days`, then `Soloist
  build expired`) and `build.expires_in_days` (`docs/control-plane.md`);
- the control page's warning line (`docs/control-page.md`);
- `chorus_soloist_build_expires_seconds` and `chorus_soloist_build_expired` in `GET /metrics`;
- a log line when the warning first holds and once a day after: `chorus-server: soloist
  warning: Soloist build expires in N days`.

`chorusctl soloist status` prints the warning, the build and every receiver. Updating is the
owner's action: replace the Soloist binary in the directory the receiver containers mount, then
`chorusctl soloist restart`, which has every supervisor read the binary again, clear `expired`
and start again.

### Tests

`crates/server/tests/soloist_receivers.rs` runs the real `chorus-server`, one real supervisor
per receiver (`--pipewire none`) and the fake Soloist under each, with the test as the Spotify
app. Both programs are examples of `crates/server` (`server-test-soloistd`,
`server-test-fake-soloist`), so nothing that builds an image or a release builds them. Its
servers run at 44.1 kHz, so the fake's signal is compared bit for bit; the 48 kHz path is the
same reader with the resampler in it, held to the resampler's own output by the reader's unit
tests. Sixteen tests, about 23 s when the binary runs alone on the development host (they run
in parallel; the four that wait out a real bound are the alarm timeout's 10 s, the 3.5 s that
shows a paused receiver is not played again, and two alarms' 12 s of modelled lead time).
`crates/server/tests/control_thread_population.rs` holds the thread counts with no receiver
container running.

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
| what Soloist needs from its host beyond PipeWire (shared libraries, CA certificates) | that a glibc program built for Linux runs on Debian 13 with the image's packages (a LEAD: issue 1's reporter ran it in a Debian trixie container) | Soloist does not start: `failed`, with its own message in the log; the missing package is a line in `deploy/soloist/debian-packages.pins` |
| which processes Soloist starts | not relied on: the receiver container runs an init | none |
| which gain stage Soloist's volume drives | the audio arrives unscaled (`--soloist-volume chorus`) | music is quieter than the slider says; `--soloist-volume receiver` is the other mapping |
| whether `play` on a device that is already playing sends `playback_changed` | not relied on: after `play` is accepted the manager asks for the state | none |
| whether a paused Soloist answers `pause` with an event | not relied on: the settle time is a bound, not a wait | a `playing` report is ignored for at most 3 s |
| Soloist's cadence into PipeWire | one quantum of at most 2048 frames at a time, as `pw-cat` did in the probe | more underruns in the counters; the fill target is one constant |
| whether two devices of one account can play at once | not relied on: a displaced device is paused by chorus either way | none |

## Sources

Soloist's documentation, all read 2026-10-03: the command-line reference, the WebSocket API
reference, the getting-started tutorial, the overview and the downloads and updates pages under
https://developer.spotify.com/documentation/soloist. RFC 6455
(https://www.rfc-editor.org/rfc/rfc6455), RFC 3174 and RFC 4648, read 2026-10-03. PipeWire's
pipe-tunnel module page (https://docs.pipewire.org/page_module_pipe_tunnel.html, read
2026-10-03). No Soloist binary or archive was downloaded or run.
