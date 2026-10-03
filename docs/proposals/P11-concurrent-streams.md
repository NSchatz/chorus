# P11: Concurrent streams

- Decisions: K76
- Status: PROPOSED (chorus goal 17, 2026-10-03)
- If deferred: -
- Builds on: goal 16 (§20, the players' pool and the UPnP renderers this measures), goal 17 (§21 item 4, "measure CPU and memory per receiver and decoder stream, and propose the limit"; item 1, the Soloist receivers of P7; item 5, the homelab receivers PR that carries the limits), P7 (the pool of 16 receivers and its `ASSUMED` per-receiver cost), P9 (the settled decoders)

## Question

How many independent streams may a chorus server play at once, and how many receivers may it
keep, on the homelab's class of host? K76: "Research and propose: goal 1 proposes the
independent-stream limit from CPU/memory measurements of receivers (Soloist, UPnP renderer) and
decoders on the homelab's class of host. Not chosen as final: one per room; up to 4." I11 moved
the proposal to this goal, where the things to measure exist.

An independent stream is one thing playing that nothing else is playing: one player thread
decoding one URL, or one receiver's audio, cut into one stream slot. A group of five rooms
playing one album is one stream.

## Constraints that bind every option

- **K75.** The house is sized to "up to 8 rooms (about 20 speakers)". Eight rooms can play at
  most eight different things.
- **K59.** Every room, every saved group and every live group is a cast target in each
  protocol. Targets outnumber rooms; streams cannot.
- **P7.** A pool of 16 Soloist receivers ("8 rooms, K75, plus saved and live groups; `ASSUMED`
  counts"), each with its own PipeWire and WirePlumber, and per-receiver limits of
  `mem_limit: 192m`, `cpus: 0.25`, `pids_limit: 64`, all `ASSUMED` "until P11 measures".
- **The thread contract** (`crates/server/src/main.rs`, ADR 0012, ADR 0124). Every thread
  exists before the scheduling report and none is made after it, so the number of players
  (`--players`, at most `MAX_PLAYERS` = 16), stream slots (`--slots`, at most 32), endpoint
  sessions (`--max-clients`) and receiver readers is fixed when the server starts. A limit is
  therefore a start-up number whatever else it is: a server cannot grow a seventeenth player
  because the host looks idle.
- **What happens at the limit today.** With every player in use a renderer answers
  `SetAVTransportURI` and `Play` with UPnP error 501 and the log says `no free player`
  (`docs/upnp.md`). Nothing is degraded for the streams already playing.
- **The homelab's limits.** The chorus-server service of homelab PR #237 (open, head
  `46a8214`, read 2026-10-03) runs with `cpus: 1.0`, `mem_limit: 256m`, `pids_limit: 128`, a
  real-time priority ceiling of 20 and 64 MiB of lockable memory, sized for a server with no
  players and no renderers ("the audio path is a handful of threads"). Every service there
  carries `mem_limit`, `pids_limit` and `cpus`; chorus proposes changes to them by PR and
  never merges one (K28).
- **The host.** Intel Xeon E5-2680 v4, shared with every other homelab service and, during
  these measurements, with other tenants' builds (one-minute load average 29 to
  65 on four visible CPUs). Every CPU figure below is an upper bound.
- **Rule 8.** The choice is argued on what chorus needs, not on what this container holds.
- **No Soloist here.** Soloist is proprietary; chorus never downloads or runs it. Its own cost
  can only be measured by the owner, on the owner's build.

## What was measured

All from `docs/measurements/concurrent-streams-host.md` (`Source: host`, builds `891c5b7` and,
for the receivers, Opus and ALAC, `856411c`; 2026-10-03): the release server with 16 rooms, 16 connected endpoints, 16 players and 16
renderers, K = 1 to 16 streams of each settled format played by a scripted control point,
sampled per thread over 60 s. CPU is a percentage of one core of this host under load.

| What | CPU, % of one core | Memory |
|---|---|---|
| A decoder alone, per stream (`--probe-media`) | WAV 0.08 to 0.10, FLAC 0.22, MP3 0.29, Vorbis 0.30, Opus 0.62, ALAC 0.60 | not measured alone |
| One stream at the server's rate (48 kHz WAV), its player thread | 0.4 to 0.5 | 0.5 to 0.8 MB |
| One Opus stream (48 kHz, no resampling), its player thread | TO FILL | TO FILL MB |
| One stream at 44.1 kHz, its player thread: fetch, decode, resample, write | WAV 3.9 to 4.0, FLAC 3.8 to 3.9, MP3 4.3 to 4.5, Vorbis 4.6 to 4.8, ALAC TO FILL | 0.7 to 1.7 MB; Vorbis 2.3 to 2.6 MB; ALAC TO FILL MB |
| The resampler's share of that | about 3.5 | - |
| The audio thread | 0.7 with nothing playing, about 0.15 more per stream, 2.7 to 3.2 at 16 | - |
| One connected endpoint (its two client threads), playing or silent | about 0.5 (7.5 to 8.7 for 16) | - |
| The renderers' threads, 16 renderers, idle; with one control point subscribed to all | 0.56; 0.58 | 0.9 MB for all 16; 0.2 MB more |
| 16 idle players | 0.3 for all 16 | 2.3 MB for all 16 |
| The whole server, 16 rooms, 16 endpoints, nothing playing | 9.5 to 10.5 | 20 MB resident |
| The whole server, 8 streams at 44.1 kHz | 41.5 (FLAC) to 49.1 (Vorbis) | 27 to 40 MB resident |
| The whole server, 16 streams at 44.1 kHz | 73.2 (WAV) to 87.3 (Vorbis) | 33 to 58 MB resident |
| PipeWire and WirePlumber per Soloist receiver (the probe) | 0.20 to 0.25 playing, under a clock tick idle | 19 MB resident, 3.8 MB unshareable |
| A playing Soloist receiver in chorus-server (its `soloist-reader` thread: FIFO read, resample 44.1 to 48 kHz, port) | TO FILL; TO FILL on a 44.1 kHz server, where nothing is resampled | TO FILL MB |
| An idle receiver in chorus-server; the one `soloist-manager` thread | TO FILL per reader; TO FILL | 17 more threads' stacks if memory is locked |
| `chorus-soloistd`, one per receiver (supervising the fake Soloist) | TO FILL | TO FILL MB resident, TO FILL MB proportional; TO FILL threads |
| Soloist itself | not measurable here; P7's `ASSUMED` 1 to 3 while playing | P7's `ASSUMED` 50 to 100 MB |

Three things the numbers say that the planning figures did not:

1. **The resampler, not the decoder, is the cost of a stream.** The planning estimate was
   "about 3 % of one core worst case" per stream with a wire encoder; without any encoder a
   44.1 kHz stream measures 3.8 to 4.8 %, nine tenths of it the 44.1 to 48 kHz resampler. The
   decoders alone are within the planning proxy's range (0.1 to 0.6 %). A receiver's audio
   arrives at 44.1 kHz too, and a playing Soloist receiver measures TO FILL % on its reader
   thread, a WAV stream's cost. This is a cost driver, not something this proposal fixes: the
   follow-up is named under Open inputs.
2. **Everything is linear and nothing saturates** up to the code's ceiling of 16 players.
3. **Memory is not the constraint unlocked, and may be the constraint locked.** The server's
   resident set never passed 58 MB. But a deployment locks its memory, which makes every
   mapping resident, and the 71-thread server maps 211 MB with access (mostly thread stacks; 151 MB even with neither players nor renderers),
   against 64 MiB of lockable memory and `mem_limit: 256m` in homelab PR #237. That is read
   from `/proc/<pid>/maps`, not measured under a lock (Open inputs).

## What a house costs

The house of K75 is 8 rooms and about 20 speakers. From the table, as upper bounds on this
host under load, the worst format (Vorbis at 44.1 kHz) taken for every stream, with 4 more
endpoints than the 16 measured (0.55 % each):

| Streams playing at once | chorus-server, % of one core | Share of the `cpus: 1.0` of PR #237 |
|---|---|---|
| 0 | about 13 | one eighth |
| 4 | about 30 | under one third |
| 8 (every room its own stream) | about 51 | one half |
| 16 (the code's ceiling) | about 90 | nine tenths |

A 48 kHz PCM stream, which needs no resampling, costs about a ninth of a resampled one, and an
Opus stream (TO FILL %) about a quarter.

A room plays one source at a time (K78), so a stream is a player's or a receiver's, never
both: eight rooms hear at most eight of them together, and a playing receiver costs what a
playing file does (TO FILL % against 3.9 to 4.8 %). The table therefore holds for any mix.
The receivers add, beside the streams, 16 idle reader threads and the manager: TO FILL %
of one core in all.

Threads: 71 at 16 endpoints, 16 players and 4 renderer workers; the same 71 for 20 endpoints
and 8 players; 17 more for 16 receivers (`N + 1`): 88, against `pids_limit: 128`. Mapped with
access, which is what a locked process would hold: 151 MB at 47 threads, 210 MB at 71,
TO FILL MB at the 64 threads of the receivers' run; about 2.2 to 3 MB a thread, so about
TO FILL MB for the 88 threads of the recommended deployment (an extrapolation).

The receiver containers, with P7's pool of 16: PipeWire and WirePlumber take 16 x 19 MB = about
300 MB resident and, if all 16 played, about 4 % of one core; `chorus-soloistd` 16 x TO FILL MB
and TO FILL % each; Soloist itself is `ASSUMED` at
0.8 to 1.6 GB and 16 to 48 % of one core for 16 instances playing (P7's figures, unmeasured).
Only as many receivers as there are rooms can be heard at once.

## Options

**Option A: one stream per room.** The limit is the number of rooms (8, K75): `--players 8`,
eight slots for the players beside the line-ins' and receivers' slots, and a receiver pool of
16 as P7 has it. Every room can always play its own thing; nothing is ever refused for lack of
a player unless more targets than rooms are asked to play different things at once, which
eight rooms cannot hear. The cap needs no explaining: it is the house.

**Option B: a fixed cap below the rooms** (K76's "up to 4": `--players 4`, a pool of 8
receivers). Half the worst-case CPU and fewer threads. A fifth room that wants its own radio
station gets error 501 while the host idles.

**Option C: the ceiling the code has, everywhere** (`--players 16`, 16 receivers, all allowed
to play at once). The numbers the measurement ran at. Sixteen streams cannot be heard in eight
rooms, so the second eight buy only the cost of being ready: threads, memory, and a worst case
nobody can reach by listening.

**Option D: no fixed cap, admission by measured headroom.** The server reads its own CPU
use and refuses a new stream when the host is short. It still needs a start-up ceiling (the
thread contract), so it is A or C plus a runtime rule; a refusal would depend on what other
tenants did in the last minute, which a household cannot predict or reproduce, and the rule
would need its own measurement of a quantity (headroom on a shared host) this report shows
moving by the minute.

## Comparison

| Criterion | A: one per room (8) | B: fixed cap of 4 | C: the ceiling (16) | D: admission by headroom |
|---|---|---|---|---|
| Every room can play its own thing (K75) | Yes | No: the fifth is refused (501) | Yes | Not promised |
| Worst case, chorus-server CPU, % of one core (upper bound) | about 51 | about 30 | about 90 | as its ceiling |
| Margin under `cpus: 1.0` | 2 times | 3 times | 1.1 times | as its ceiling, less the rule's own error |
| Player threads kept ready | 8 | 4 | 16 | as its ceiling |
| A refusal is predictable | Yes: the ninth different stream, which 8 rooms cannot ask for | Yes | Yes | No: depends on the host's other tenants |
| New code | None (flags) | None (flags) | None (flags) | A CPU reading on the control path, its tests and its own measurement |
| Receiver pool (P7) | 16 targets, at most 8 heard | 8 | 16 | 16 |

## Recommendation

**Recommendation:** Option A, one independent stream per room: 8 players for 8 rooms (`--players 8`), because every figure is linear, eight rooms cannot hear a ninth stream, and the worst case measured (every room its own resampled Vorbis stream, 20 speakers) is about half of one core as an upper bound on a loaded host.

In numbers, all PROPOSED:

- **Independent streams: 8** (the number of rooms, K75), as the deployment's `--players`, with
  `--slots` covering the players, the line-ins and the receivers. The code's ceiling
  (`MAX_PLAYERS` 16) stays as it is: a house configured with more rooms raises the flag.
- **UPnP renderers: one per target, no cap.** Sixteen idle renderers cost 0.56 % of one core
  and under 1 MB between them; a renderer costs a stream only while it plays.
- **Soloist receivers: a pool of 16** (`--soloist-receivers 16`, P7's count: targets, K59),
  of which at most 8 are heard at once. An idle receiver costs chorus-server TO FILL % of one
  core and a thread; a playing one is one of the 8 streams.
- **chorus-server's container** (the homelab receivers PR carries these, PROPOSED):
  `cpus: 2.0`, `mem_limit: 512m`, `ulimits.memlock` 384 MiB (402653184), `pids_limit: 192`.
  - CPU: the worst case measured is about TO FILL % of one core (8 resampled Vorbis
    streams, 20 speakers, 16 receivers, renderers), an upper bound on a loaded host. That is
    TO FILL times inside `cpus: 1.0` and TO FILL times inside `cpus: 2.0`; chorus
    proposes 2.0 because the bound is of a host that was not quiet and a throttled audio
    thread is an audible fault, not a slow page.
  - Locked memory: PR #237 grants 64 MiB and the server locks everything it maps. The
    mappings with access are 151 MB with no players or renderers, 210 MB with them, and about
    TO FILL MB with the receivers' 17 threads: every one of them above 64 MiB.
    384 MiB is that figure with a margin of about a half; `mem_limit: 512m` sits above it so
    the lock limit, which the server reports by name, is met before the kernel's. Both are
    read from `/proc/<pid>/maps` on an unlocked process, so both are PROPOSED pending the
    locked measurement under Open inputs. Unlocked, the resident set never passed 58 MB.
  - `pids_limit`: 88 threads; 192 leaves room for a `--max-clients` above 20.
- **Each receiver container** (P7's `ASSUMED` limits, kept): `mem_limit: 192m`, `cpus: 0.25`,
  `pids_limit: 64`. Measured of it: PipeWire and WirePlumber, 19 MB resident and 0.25 % of
  one core playing, 6 threads with a stand-in client; `chorus-soloistd`, TO FILL MB,
  TO FILL % and TO FILL threads. That is about TO FILL MB and 0.3 % of one
  core measured. Soloist's own half is `ASSUMED` from P7 (50 to 100 MB, 1 to 3 % of one core
  playing): at its top the container holds about TO FILL MB of its 192 and about 3.3 % of
  one core of its 25. The owner's measurement of one Soloist instance (Open inputs) is what
  makes these final; a figure above about 150 MB would raise `mem_limit`, nothing else.
- **The pool's total**: 16 receiver containers at about TO FILL MB measured plus
  Soloist's `ASSUMED` 50 to 100 MB each is 1.2 to 2.0 GB resident, 3.1 GB if every container
  reached its limit.

What the owner gives up: sixteen simultaneous different streams (which eight rooms cannot
play) and a server that sizes itself. What it costs: nothing to build; a handful of numbers
in two compose files. What would change it: a room list with more than 8 rooms (the limits scale with the
rooms: about 5 % of one core and one player thread a room); DSP (goal 12's room
correction) or a coded wire format costing far more per stream than the 0.15 % the audio
thread shows today; a measured Soloist instance well above P7's `ASSUMED` 100 MB; a host
quieter or busier than this one by a factor of two.

**The limits, in one sentence:** 8 independent streams (one per room, `--players 8`), a pool of 16 Soloist receivers of which at most 8 are heard at once, one UPnP renderer per target with no cap; chorus-server at `cpus: 2.0`, `mem_limit: 512m`, 384 MiB of lockable memory and `pids_limit: 192`; each receiver container at `mem_limit: 192m`, `cpus: 0.25` and `pids_limit: 64`.

## If the owner defers

The brief gives P11 no deferral cell (§5's table: `-`). Until the owner decides, chorus ships
what goal 16 built: the players and renderers are off unless their flags are given, `--players`
is whatever the deployment's command line says up to 16, and the homelab receivers PR of this
goal states its numbers as PROPOSED with this document as their source. Nothing is capped
below the code's ceilings and nothing is promised about them.

## Open inputs

- **Soloist's own resident set, CPU and thread count per instance**, idle and playing, and its
  cache's growth: not measurable here (no binary, no key; never downloaded). An item for the
  owner's queue, filed by the goal's coordinator: on the homelab host, with the owner's build,
  `ps -o rss,nlwp,time` of one instance idle and after ten minutes of playing, and of four at
  once. P7's `ASSUMED` 50 to 100 MB and 1 % to 3 % of a core stand until then, and the
  receiver containers' limits are sized on them. The fake Soloist of the tests is not
  Soloist and none of its cost is reported here.
- **The server under locked memory.** A deployment locks its memory (`mlockall`); this
  container cannot (8 MiB granted). The report gives the size of what would be locked as read
  from the mappings, not a measured locked resident set. The homelab PR's 64 MiB of lockable
  memory must be checked against it on a host that grants the limit: an owner step, or a
  chorus measurement in a container started with the `ulimits` the compose file gives. Until
  then a server started as PR #237 starts it, with more threads than v0.1.0's, is expected
  to refuse the contract (exit 3, "locking memory was denied") or to need
  `--allow-unlocked-memory`: an inference from the mappings, to be confirmed there.
- **The resampler** (a named follow-up, not fixed here): about 3.5 % of one core per 44.1 kHz
  stream or playing receiver, nine tenths of a stream's cost. Its quality is measured
  (`docs/measurements/resampler-quality.md`: 96 zero crossings a side, `f64` accumulation);
  a cheaper filter, vector arithmetic, or running the server at 44.1 kHz would each move the
  limit's CPU side by a factor of several. Whichever is tried is measured with this harness
  and against that report before it changes anything here.
- **Thread stacks** (a named follow-up): most of what a locked server would hold is the
  default 2 MiB stack of each of its 88 threads. Sized stacks would bring the locked set near
  the resident set measured here; until that is built and measured, the memory limits above
  are sized for the stacks as they are.
- **Wire encoders and DSP.** The server sends PCM and applied no EQ or room correction in
  these runs. Goal 12's DSP and any coded wire format add to the audio thread and are
  measured where they are built.
- **A quiet host.** The same harness on the homelab host itself (`make concurrent-streams`)
  would replace upper bounds with figures; it needs no device and changes nothing.
- **Saved-group and live-group counts** (they size the receiver pool, not the stream limit):
  `ASSUMED` in P7; the room-list item in the owner's queue.

## Sources

- `docs/measurements/concurrent-streams-host.md` and its raw files under
  `docs/measurements/raw/concurrent-streams-host-2026-10-03/` (this goal; builds `891c5b7` and `856411c`).
- The PipeWire probe of goal 17 (2026-10-03): `/cache/tmp/chorus-g17/pw-probe/`, index
  `SHA256SUMS` (sha256 `4b517e0c89657288aaafce72af1bc8b578e4e922b05832a1ce46054e880285ac`);
  its per-receiver table is copied into the report above.
- `.claude/goals/2026-09-chorus-research/research-casting-decoders.md` section 5 (2026-09-29):
  the planning-time ffmpeg proxy figures and proposed limits this replaces.
- `docs/proposals/P7-spotify-soloist.md` (the pool, the `ASSUMED` per-receiver cost and limits).
- `NSchatz/homelab` PR #237 at head `46a8214` (`gh pr diff 237 -R NSchatz/homelab`, read
  2026-10-03): the chorus-server service's `cpus`, `mem_limit`, `pids_limit` and `ulimits`.
- `.claude/goals/2026-09-chorus.md`: K59, K66, K75, K76, I11, §5's table, §21.
- `docs/upnp.md`, `docs/decoders.md`, `docs/decisions/0124-the-media-player-engine.md`,
  `crates/server/src/main.rs` (the thread population), `crates/server/src/player.rs`
  (`MAX_PLAYERS`), `crates/server/src/config.rs` (`MAX_SLOTS`), `crates/hostctl` (`lock_memory`).

## What was read

- Rules: `/cache/tmp/chorus-g17/agent-rules.md`, `CLAUDE.md`, `docs/conventions.md`.
- The goal's design and survey notes (`/cache/tmp/chorus-g17/design.md` section 6,
  `research/survey.md` sections F and J, `research/pipewire.md`), 2026-10-03.
- chorus's own files named in Sources, and `tools/house-soak-run.sh`, `tools/lib.sh`,
  `tools/decode-fixtures/generate.py`, `crates/server/tests/upnp_control_point.rs`,
  `crates/server/tests/common/mod.rs`, `crates/upnp/src/client.rs`,
  `docs/measurements/README.md`, `codec-decode-cost-host.md`, `gapless-join-host.md`,
  `resampler-quality.md`, `house-soak-8-rooms.md`, 2026-10-03.
- For the receivers: `docs/soloist.md`, `crates/server/tests/soloist_receivers.rs`, the two
  examples of `crates/server`, the header of `crates/soloist-fake/src/lib.rs`,
  `docs/telemetry.md` (the receivers' counters), 2026-10-03.
- `NSchatz/homelab` PR #237's diff (read-only), 2026-10-03.
- The reference programs (ffmpeg, LAME, FLAC) were run as programs to make the test signals;
  none of their source was opened. No GPL or LGPL source file was opened. No web page was
  read. No Soloist binary was downloaded or run.
