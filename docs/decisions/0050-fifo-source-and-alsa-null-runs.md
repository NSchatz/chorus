# 0050: a FIFO source is held open and fills silence, and the ALSA null runs are a labelled host step in the gate

- Status: accepted (goal 7, 2026-09-30)
- Decided by: the goal (audit B-2's proposed fix; goal 7 item 3, brief section 11)
- Implemented in: `crates/server/src/source.rs` (`FifoSource`), `crates/server/tests/fifo_source.rs`,
  `crates/client-linux/src/logcheck.rs` and `src/bin/delaylog_check.rs`
  (`--device-reports-no-delay`), `tools/lib.sh` (`use_rootless_alsa`), `tools/ten-minute-run.sh`
  (`CHORUS_TEN_MINUTE_NULL`), `tools/alsa-null-run.sh`, `tools/gate.sh` (step `alsa-null`)

## Context

BRIEF.md section 2.1 names a FIFO as a development input. The server read every non-generated
source with `File::open`, so a FIFO with no writer blocked the supervisor inside the open, and a
player that closed its end made `read` return 0, which ended every client's stream (audit B-2).

The ALSA device checks refused in this container because it has no system libasound. Goal 5
installed alsa-lib 1.2.16.1 rootless from conda-forge at `/cache/opt/chorus-alsa`, but nothing in
the repository found it, and the ten-minute run refuses the ALSA `null` device by name because
`null` reports a delay of zero for ever.

## What was read

All read 2026-09-30.

- fifo(7), <https://man7.org/linux/man-pages/man7/fifo.7.html>: "Under Linux, opening a FIFO for
  read and write will succeed both in blocking and nonblocking mode."
- `O_NONBLOCK` in the permissively licensed `libc` crate for Linux on x86_64 and aarch64, 2048:
  <https://docs.rs/libc/latest/x86_64-unknown-linux-gnu/libc/constant.O_NONBLOCK.html> and the
  aarch64 page.
- The installed package record `/cache/opt/chorus-alsa/conda-meta/alsa-lib-1.2.16.1-h7cc23a3_1.json`
  (URL `https://conda.anaconda.org/conda-forge/linux-64/alsa-lib-1.2.16.1-h7cc23a3_1.conda`,
  sha256 `a35bddac04be093769e81814465a537961c6ed0f8d3cc23d6dce6ecdfaf71821`).

## Decision

1. `--source fifo:<path>`, or any path that is a named pipe, is a `FifoSource`: opened read-write
   and non-blocking, so the open never waits for a player and a closed writer is never end of
   file. Every read hands back exactly one chunk: the whole frames the pipe holds, then silence
   (all-zero bytes, silence in every supported format) for the rest if the pipe has not filled
   the chunk within half a chunk period. A partial frame waits for its remaining bytes. It never
   returns 0, so it never ends a stream. A plain file keeps its end-of-stream behaviour, and
   `fifo:` on a plain file is refused by name. The flag is only built for the targets whose value
   was checked; any other target gets a refusal, not a guessed constant. The pipe is polled at
   1 ms rather than with `poll(2)`, because this crate denies unsafe code.
2. The runners find libasound the way the gate finds ccache: a system library first, else
   `CHORUS_ALSA_PREFIX` (default `/cache/opt/chorus-alsa`) on `LD_LIBRARY_PATH`. The refusal when
   neither exists names the pinned micromamba install command.
3. `make gate` gains a step `alsa-null` (`make verify-alsa-null`): `tools/stream-end-and-loss.sh` and the
   restart storm on `null`. Under CI with no libasound it prints SKIPPED with the reason, as the
   identity scan does. The ten-minute run is not in it.
4. The ten-minute run gets an opt-in `null` form (`CHORUS_TEN_MINUTE_NULL=1`,
   `make ten-minute-run-null`). It grades what `null` can answer: 600 s graded, zero underruns, no
   rate change, occupancy under the ceiling. `chorus-delaylog-check --device-reports-no-delay` moves
   the delay-bounds finding to a `NOT GRADED` line (never a pass). In its place goes a check that
   can fail: every sample's delay and both extremes are zero, so a real card's log cannot be graded
   this way. Without the variable, `null` is refused exactly as before. The run is labelled host /
   ALSA null and does not pass the ten-minute criterion.

## Consequences

- A player that pauses or changes track leaves every endpoint on one stream with silence between
  tracks. A pipe source never sends `stream_end`, so `--serve-forever` and serve-once behave the
  same until the process is stopped.
- The runners that grade a reported delay (`delay-log-shape.sh`, the real ten-minute run,
  `overflow-run.sh`) and the host contract (needs `RLIMIT_RTPRIO` above zero) still refuse here by
  name. `null` cannot answer them.
- `chorus-server --help` no longer advertises `-` (stdin) as a source. It was never implemented,
  since `File::open("-")` opens a file named `-`.
