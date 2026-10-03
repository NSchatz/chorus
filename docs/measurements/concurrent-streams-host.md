# What concurrent streams and receivers cost chorus-server, on the production host class

Date: 2026-10-03
Source: host
Build measured: `891c5b760bd9aea30b5ece2046ab4f6f2bbc06a8`
Build note: two builds, both commits of `main`. The decoders alone, the idle servers and the
stream rows for `wav48`, `wav`, `flac`, `mp3` and `vorbis` were measured on `891c5b7`, the
commit named above. The Soloist receivers' section and the stream row for `opus`
were measured later the same day on `856411c64f9d4876e9deb348bd39043bdcda4892` (the merge of
the receivers' server side, PR #132): the release build was made in this report's branch at a
commit whose `crates/`, `Cargo.toml`, `Cargo.lock`, `config/`, `third_party/` and
`rust-toolchain.toml` are those of `856411c` (`git diff 856411c <that commit>` over them is
empty; each run's `params` names the commit and the server binary's sha256). Each table says
which build its rows are from where both appear.
Timing evidence: none. These are CPU-time and memory figures of processes on a shared
development container; nothing here says when a sample reaches a speaker, and only a `hardware`
report is timing evidence (BRIEF.md section 3.1 rule 3). They are the measured numbers proposal
P11 (`docs/proposals/P11-concurrent-streams.md`, decision K76) rests on, and they replace the
planning-time ffmpeg proxy figures of `research-casting-decoders.md` section 5.

**Shared host, upper bound.** The container runs on the homelab's class of host, shared with other
tenants, and the one-minute load average stood between 17 and 65 over four
visible CPUs during every window below (each window's own figure is in its table). CPU figures
are therefore upper bounds for this processor: a busy neighbour costs cache and clock frequency,
never the reverse. Memory figures do not depend on the load.

## The machine

```
cpu        Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz, 2 sockets x 14 cores x 2 threads (lscpu),
           56 CPUs in /proc/cpuinfo
container  nproc 4; cgroup cpu.max "2800000 100000"; memory.max 51539607552
kernel     Linux 6.12.111+deb13-amd64 x86_64; CLK_TCK 100
compiler   rustc 1.98.1 (48a229cea 2026-09-01), the release profile, --locked
build      891c5b760bd9aea30b5ece2046ab4f6f2bbc06a8 (origin/main), no change under crates/, Cargo.toml,
           Cargo.lock, config/ or third_party/; chorus-server sha256
           e4c531bac27fe2e173e0343e6035047fd8e58a30c493a23752d2d708bbafcfff
build 2    856411c64f9d4876e9deb348bd39043bdcda4892 (main), the same toolchain and profile; chorus-server
           sha256 `02bf71923cbd7a349ba5def298b6b75beff96bd4dcc0f61bfcebdb164ec7258a`
limits     RLIMIT_RTPRIO 0 and RLIMIT_MEMLOCK 8 MiB, so the server ran with
           --allow-non-realtime --allow-unlocked-memory: the audio thread was an ordinary
           thread and no memory was locked (see "What this does not show")
```

Every run recorded these itself (`params` and `lscpu.txt` in each run directory).

## Command

```
cargo build --release --locked -p chorus-server -p chorus-client-linux      # at the commit above
CHORUS_SKIP_BUILD=1 CHORUS_STREAMS_MODE=decoders make concurrent-streams
CHORUS_SKIP_BUILD=1 CHORUS_STREAMS_MODE=idle make concurrent-streams
CHORUS_SKIP_BUILD=1 CHORUS_STREAMS_FORMATS="wav48" make concurrent-streams   # then "wav flac", "mp3 vorbis", "opus alac"
# the second build, at 856411c's crates, with the test examples (the supervisor and the fake Soloist):
cargo build --release --locked -p chorus-server -p chorus-client-linux --bins --examples
CHORUS_SKIP_BUILD=1 CHORUS_STREAMS_MODE=receivers CHORUS_STREAMS_KS=1,4,8,16 CHORUS_STREAMS_WINDOW=45 CHORUS_STREAMS_SETTLE=6 make concurrent-streams
CHORUS_SKIP_BUILD=1 CHORUS_STREAMS_FORMATS="opus alac" CHORUS_STREAMS_KS=1,4,8,16 CHORUS_STREAMS_SETTLE=6 make concurrent-streams
python3 tools/concurrent-streams/harness.py summary <each run's windows.jsonl and probe-media.tsv>
```

each under the heavy locks (no build or gate of chorus ran beside a window; other tenants' work
did), each run under twenty minutes. The harness is `tools/concurrent-streams-run.sh` and
`tools/concurrent-streams/harness.py`, which are the method; the last command printed every
table below. The harness is not a gate step and grades nothing.

## Method

**What runs.** The release `chorus-server` with sixteen rooms, `--slots 16 --max-clients 16
--players 16 --upnp` (four UPnP workers, eight control workers: 71 threads), 48 kHz 16-bit
stereo in 20 ms chunks (`config/verification.conf`), its renderers on the loopback seams
`crates/server/tests/upnp_control_point.rs` uses. Sixteen `chorus-client` processes on the ALSA
`null` device, one per room, each holding a session. A local HTTP media server (another process,
Range honoured). A scripted UPnP control point (`SetAVTransportURI`, `Play`) starts the same file
on K room renderers at once, each with its own URL, so K player threads each fetch, decode,
resample to 48 kHz and write their own player port, the audio thread cuts K slots, and K sessions
carry K different streams. K = 0 is the same server with every room attached and playing
nothing (a session in a room that plays nothing is still sent chunks, of silence).

**The windows.** For each format a fresh server; a window at K = 0; then for K = 1, 2, 4, 8 and
16 (the players' ceiling, `MAX_PLAYERS`): start K streams, wait until every renderer says
PLAYING, settle 10 s, sample `/proc` at both ends of 60 s, stop. A window counts as steady when
every renderer is still PLAYING at its end and its `RelTime` advanced by the window's length
within 3 s; a window that is not says so in the table.

**CPU.** Per thread, the scheduler's own run time (`/proc/<pid>/task/<tid>/schedstat`, first
field, nanoseconds) at both ends of the window, over the window's length on `CLOCK_MONOTONIC`,
as a percentage of one core. `utime + stime` from `/proc/<pid>/task/<tid>/stat` was read at the
same instants (10 ms ticks: one tick is 0.017 % over 60 s) and is kept in the raw files; the
process total is given both ways. Threads are told apart by the roles the server prints in its
scheduling report (`thread role=player-3 tid=...`): the server does not name its threads for the
kernel, and nothing in the server was changed for this measurement.

**Memory.** `VmRSS`, `RssAnon`, `VmHWM`, `VmSize` from `/proc/<pid>/status` and `Pss` from
`/proc/<pid>/smaps_rollup` at the window's end. "Per stream" is the growth of `RssAnon` against
the same server's K = 0, divided by K. Each server ran its K in rising order, so a figure at K
includes whatever the allocator kept from the smaller K before it.

**The test signals.** One 130 s stereo signal at 44.1 kHz, 16 bits (a 440 Hz and a 660 Hz sine,
each mixed with its own seeded pink noise, from the pinned ffmpeg's `sine` and `anoisesrc`
sources), encoded once per settled input format by the pinned reference programs of
`make decode-fixtures` (`fixtures/README.md` has the pins: ffmpeg 9.0.2, LAME 4.0, FLAC 1.5.0,
libvorbis 1.3.7, libopus 1.6.1), at the upper end of what a library holds (`ASSUMED`), so decode
cost is not understated. Pink noise does not compress, so the lossless files are large for
their kind: an upper bound again.

| file | what | bytes | kbit/s | sha256 |
|---|---|---|---|---|
| `wav48.wav` (`wav48`) | WAV, 16-bit stereo 48 kHz (the server's own rate: no decoder work, no resampling) | 24960044 | 1536 | `11ab6013c9d5336986e198933f2f79a95898216439c16749a29a062ede5c843d` |
| `wav.wav` (`wav`) | WAV, 16-bit stereo 44.1 kHz | 22932044 | 1411 | `cb8d6cecad6e286d81c9c869cb68f9c0bc234a10686c014aa33f4a6f79aff028` |
| `flac.flac` (`flac`) | FLAC level 5, 16-bit stereo 44.1 kHz | 17663781 | 1087 | `f362b991f7610a7aa35ab2c3044a6038cca10d7bce794e3c6db137a26c0ddd3c` |
| `mp3.mp3` (`mp3`) | MP3 320 kbit/s CBR, stereo 44.1 kHz | 5202546 | 320 | `14f6d7faf1a6c8382db10e04b07424e6a5df2ffc9cd9edd5b94d6b5fe7abd0b0` |
| `vorbis.ogg` (`vorbis`) | Ogg Vorbis quality 6, stereo 44.1 kHz | 3272597 | 201 | `8d20560143b5fc7eca8067291911463966a06e2c22caf5edce0f42b262a7919d` |
| `opus.opus` (`opus`) | Ogg Opus 128 kbit/s, stereo 48 kHz | 2014848 | 124 | `a67723324e2e5d1a656640d46c21a4cf722b7ef37ac8be12db974cbe0adb9277` |
| `alac.m4a` (`alac`) | ALAC in MP4 (moov first), 16-bit stereo 44.1 kHz | 17845857 | 1098 | `98d905cff5d19ee21aab4e103de1fdec4121af42067bde2e647075076bbb4aaf` |

`wav48` is the same signal at the server's own rate: it needs no decoder work and no resampler,
so it shows what a stream costs before either. Opus decodes at 48 kHz and needs no resampler
either. Every other file is 44.1 kHz and goes through `chorus_decode::Resampler`
(`docs/measurements/resampler-quality.md`).

## Result: the decoders alone

`chorus-server --probe-media <file>` decodes a file to its end and starts nothing else (no
fetch, no resampler, no stream). The child's `utime + stime` from `wait4`, three runs, per second
of audio:

| format | runs | median | lowest | highest |
|---|---|---|---|---|
| wav48 | 3 | 0.103 | 0.089 | 0.116 |
| wav | 3 | 0.083 | 0.082 | 0.097 |
| flac | 3 | 0.218 | 0.164 | 0.234 |
| mp3 | 3 | 0.290 | 0.284 | 0.301 |
| vorbis | 3 | 0.301 | 0.222 | 0.354 |
| opus | 3 | 0.615 | 0.600 | 0.620 |
| alac | 3 | 0.600 | 0.523 | 0.691 |

## Result: K independent streams through the whole server

Sixteen rooms, sixteen connected endpoints, K streams. Every table is one row per format (one
server per format) and one column per K. The `opus` row is from the second build
(`856411c`), run without K = 2; every other row is from `891c5b7`. **ALAC through the whole
server is not measured**: its first run was not granted the locks, its second ended before its
first window on a busy control plane's 503 (the control point now tries again), and its third
was not granted the locks either. Its decoder alone is in the table above. The second build's server
also holds the code of PR #132, switched off here (no `--soloist-receivers`).

Player threads, % of one core per playing stream (the mean of the K busiest player threads):

| format | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| wav48 | 0.48 | 0.47 | 0.46 | 0.42 | 0.40 |
| wav | 3.96 | 3.90 | 3.87 | 3.94 | 3.87 |
| flac | 3.82 | 3.87 | 3.92 | 3.81 | 3.89 |
| mp3 | 4.46 | 4.29 | 4.28 | 4.27 | 4.27 |
| vorbis | 4.49 | 4.45 | 4.32 | 4.76 | 4.72 |
| opus | 1.52 | - | 1.32 | 1.35 | 1.36 |

The idlest and the busiest of those K threads in each window, % of one core:

| format | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| wav48 | 0.48 to 0.48 | 0.46 to 0.48 | 0.45 to 0.47 | 0.40 to 0.44 | 0.38 to 0.43 |
| wav | 3.96 to 3.96 | 3.87 to 3.93 | 3.81 to 3.92 | 3.77 to 4.22 | 3.75 to 3.99 |
| flac | 3.82 to 3.82 | 3.80 to 3.93 | 3.88 to 3.99 | 3.63 to 4.00 | 3.74 to 4.06 |
| mp3 | 4.46 to 4.46 | 4.28 to 4.31 | 4.16 to 4.50 | 4.21 to 4.32 | 4.18 to 4.35 |
| vorbis | 4.49 to 4.49 | 4.43 to 4.47 | 4.20 to 4.44 | 4.65 to 4.87 | 4.56 to 4.90 |
| opus | 1.52 to 1.52 | - | 1.29 to 1.35 | 1.27 to 1.41 | 1.32 to 1.39 |

The audio thread, % of one core:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 0.69 | 0.88 | 1.05 | 1.45 | 1.98 | 2.99 |
| wav | 0.65 | 0.80 | 0.88 | 1.16 | 1.69 | 2.69 |
| flac | 0.66 | 0.80 | 0.92 | 1.21 | 1.71 | 2.79 |
| mp3 | 0.66 | 0.88 | 0.97 | 1.25 | 1.80 | 2.92 |
| vorbis | 0.70 | 0.86 | 0.99 | 1.26 | 2.05 | 3.22 |
| opus | 0.53 | 0.66 | - | 1.11 | 1.84 | 3.13 |

The client threads (two per endpoint slot, 16 endpoints connected), % of one core, sum:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 7.59 | 7.93 | 7.98 | 7.53 | 7.77 | 7.98 |
| wav | 8.46 | 8.44 | 8.15 | 8.13 | 7.97 | 7.71 |
| flac | 8.24 | 8.08 | 8.16 | 8.03 | 8.19 | 8.04 |
| mp3 | 8.73 | 8.07 | 8.07 | 8.11 | 8.02 | 8.05 |
| vorbis | 8.29 | 8.37 | 8.05 | 8.22 | 7.74 | 7.48 |
| opus | 9.68 | 9.77 | - | 8.82 | 8.98 | 9.38 |

The renderers' threads (`upnp-*`), % of one core, sum:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 0.55 | 0.59 | 0.60 | 0.62 | 0.61 | 0.63 |
| wav | 0.52 | 0.55 | 0.54 | 0.55 | 0.58 | 0.62 |
| flac | 0.52 | 0.54 | 0.55 | 0.55 | 0.58 | 0.64 |
| mp3 | 0.52 | 0.59 | 0.56 | 0.59 | 0.60 | 0.65 |
| vorbis | 0.53 | 0.58 | 0.56 | 0.56 | 0.64 | 0.68 |
| opus | 0.75 | 0.78 | - | 0.69 | 0.72 | 0.78 |

Every other thread, % of one core, sum:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 0.35 | 0.36 | 0.36 | 0.37 | 0.36 | 0.35 |
| wav | 0.34 | 0.34 | 0.32 | 0.32 | 0.32 | 0.32 |
| flac | 0.34 | 0.33 | 0.34 | 0.33 | 0.33 | 0.33 |
| mp3 | 0.33 | 0.35 | 0.33 | 0.31 | 0.32 | 0.33 |
| vorbis | 0.35 | 0.32 | 0.32 | 0.32 | 0.35 | 0.35 |
| opus | 0.41 | 0.41 | - | 0.36 | 0.37 | 0.36 |

The whole process, % of one core:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 9.49 | 10.54 | 11.20 | 12.05 | 14.20 | 18.35 |
| wav | 10.26 | 14.36 | 17.94 | 25.86 | 42.19 | 73.21 |
| flac | 10.04 | 13.82 | 17.96 | 26.01 | 41.47 | 74.06 |
| mp3 | 10.54 | 14.65 | 18.79 | 27.61 | 45.03 | 80.35 |
| vorbis | 10.18 | 14.91 | 19.09 | 27.87 | 49.07 | 87.29 |
| opus | 11.78 | 13.52 | - | 16.54 | 22.89 | 35.35 |

The whole process by clock ticks (utime + stime), % of one core:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 9.48 | 10.55 | 11.20 | 12.06 | 14.18 | 18.35 |
| wav | 10.27 | 14.36 | 17.95 | 25.88 | 42.19 | 73.21 |
| flac | 10.03 | 13.82 | 17.96 | 26.00 | 41.48 | 74.07 |
| mp3 | 10.53 | 14.65 | 18.80 | 27.60 | 45.03 | 80.33 |
| vorbis | 10.18 | 14.91 | 19.08 | 27.86 | 49.07 | 87.28 |
| opus | 11.78 | 13.51 | - | 16.55 | 22.88 | 35.33 |

VmRSS at the window's end, kB:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 20340 | 21728 | 22648 | 23636 | 25440 | 29112 |
| wav | 20456 | 21916 | 23048 | 27900 | 29316 | 33428 |
| flac | 20468 | 22128 | 23360 | 24964 | 28224 | 34692 |
| mp3 | 20552 | 22392 | 23548 | 24792 | 27380 | 32532 |
| vorbis | 20396 | 23716 | 26476 | 30932 | 39856 | 58156 |
| opus | 21824 | 23604 | - | 25724 | 28476 | 33528 |

RssAnon at the window's end, kB:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 14940 | 15624 | 16544 | 17532 | 19336 | 23008 |
| wav | 15000 | 15820 | 16952 | 21804 | 23220 | 27332 |
| flac | 15028 | 16048 | 17280 | 18884 | 22144 | 28612 |
| mp3 | 15148 | 16284 | 17440 | 18684 | 21272 | 26424 |
| vorbis | 15012 | 17372 | 20132 | 24588 | 33512 | 51812 |
| opus | 16004 | 16824 | - | 18944 | 21696 | 26748 |

Pss (smaps_rollup) at the window's end, kB:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 18136 | 19526 | 20448 | 21434 | 23240 | 26912 |
| wav | 18255 | 19715 | 20847 | 25698 | 27119 | 31228 |
| flac | 18283 | 19944 | 21175 | 22779 | 26039 | 32508 |
| mp3 | 18344 | 20182 | 21339 | 22583 | 25171 | 30323 |
| vorbis | 18234 | 21556 | 24316 | 28774 | 37689 | 55995 |
| opus | 19671 | 21451 | - | 23570 | 26322 | 31376 |

VmHWM at the window's end, kB:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 20340 | 21728 | 22648 | 23636 | 25440 | 29112 |
| wav | 20456 | 21916 | 23048 | 27900 | 29316 | 33428 |
| flac | 20468 | 22128 | 23360 | 24964 | 28224 | 34692 |
| mp3 | 20552 | 22392 | 23548 | 24792 | 27380 | 32532 |
| vorbis | 20396 | 23716 | 26476 | 30932 | 39856 | 58156 |
| opus | 21824 | 23604 | - | 25724 | 28476 | 33528 |

VmSize at the window's end, kB:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 4785972 | 4785980 | 4785980 | 4785980 | 4785980 | 4785980 |
| wav | 4785972 | 4785980 | 4785980 | 4785980 | 4785980 | 4785980 |
| flac | 4785972 | 4785980 | 4785980 | 4785980 | 4785980 | 4785980 |
| mp3 | 4785972 | 4785980 | 4785980 | 4785980 | 4785980 | 4785980 |
| vorbis | 4785972 | 4785980 | 4785980 | 4785980 | 4785980 | 4785980 |
| opus | 4786520 | 4786528 | - | 4786528 | 4786528 | 4786528 |

Mapped with any access (what locking memory would make resident), kB:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 210688 | 210836 | 211036 | 211260 | 211540 | 212168 |
| wav | 210764 | 211084 | 211512 | 211940 | 212816 | 214916 |
| flac | 210804 | 211336 | 211924 | 212880 | 214864 | 218716 |
| mp3 | 210924 | 211580 | 212012 | 212496 | 213576 | 215796 |
| vorbis | 210796 | 212848 | 215028 | 219064 | 227040 | 243568 |
| opus | 212152 | 212464 | - | 213284 | 214464 | 216612 |

Threads:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 71 | 71 | 71 | 71 | 71 | 71 |
| wav | 71 | 71 | 71 | 71 | 71 | 71 |
| flac | 71 | 71 | 71 | 71 | 71 | 71 |
| mp3 | 71 | 71 | 71 | 71 | 71 | 71 |
| vorbis | 71 | 71 | 71 | 71 | 71 | 71 |
| opus | 71 | 71 | - | 71 | 71 | 71 |

Load average (1 min) at the window's start and end:

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | 46.10 to 45.66 | 45.66 to 38.15 | 32.48 to 46.54 | 44.97 to 46.49 | 46.80 to 43.27 | 42.04 to 37.49 |
| wav | 39.35 to 33.94 | 34.39 to 37.87 | 41.71 to 36.71 | 38.33 to 36.98 | 36.76 to 31.41 | 32.01 to 35.48 |
| flac | 36.21 to 38.45 | 38.53 to 34.03 | 42.14 to 41.26 | 39.35 to 41.04 | 42.35 to 35.74 | 38.17 to 42.61 |
| mp3 | 42.77 to 44.28 | 43.93 to 54.41 | 54.74 to 50.34 | 43.72 to 40.99 | 41.09 to 33.22 | 36.63 to 44.51 |
| vorbis | 39.44 to 41.18 | 40.54 to 38.46 | 32.92 to 38.23 | 36.61 to 29.15 | 33.92 to 49.45 | 56.84 to 51.32 |
| opus | 17.99 to 17.05 | 17.13 to 17.46 | - | 18.20 to 17.56 | 17.23 to 16.96 | 18.36 to 20.20 |

Steady (every renderer PLAYING at the end and its position advanced by the window, within 3 s):

| format | K = 0 | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|---|
| wav48 | yes | yes | yes | yes | yes | yes |
| wav | yes | yes | yes | yes | yes | yes |
| flac | yes | yes | yes | yes | yes | yes |
| mp3 | yes | yes | yes | yes | yes | yes |
| vorbis | yes | yes | yes | yes | yes | yes |
| opus | yes | yes | - | yes | yes | yes |

RssAnon growth per playing stream against the same server's K = 0, kB:

| format | K = 1 | K = 2 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| wav48 | 684 | 802 | 648 | 550 | 504 |
| wav | 820 | 976 | 1701 | 1028 | 771 |
| flac | 1020 | 1126 | 964 | 890 | 849 |
| mp3 | 1136 | 1146 | 884 | 766 | 705 |
| vorbis | 2360 | 2560 | 2394 | 2312 | 2300 |
| opus | 820 | - | 735 | 712 | 672 |

What the tables say:

- **A stream at the server's own rate is cheap; a resampled one is not.** A 48 kHz WAV costs
  its player thread 0.4 to 0.5 % of one core; the same signal at 44.1 kHz costs 3.8 to 4.0 %.
  The difference, about 3.5 % of one core, is the resampler (96 zero crossings a side, `f64`
  accumulation: `docs/measurements/resampler-quality.md`). The decoders add what the first
  table says they do alone: FLAC nothing visible, MP3 about 0.4, Vorbis 0.4 to 0.8. ALAC is not measured here; its decoder alone costs
  what Opus's does, so beside the resampler it should sit near Vorbis (an estimate). Opus decodes at 48 kHz, so it pays its decoder (the dearest of the six) and no
  resampler: 1.3 to 1.5 % of one core a stream. Most
  music is 44.1 kHz and the server runs at 48 kHz, so the resampled figure is the one a house
  pays.
- **It is linear.** Per stream the figure is the same from K = 1 to K = 16, and the K threads
  of a window lie within a few tenths of each other; nothing saturates at sixteen streams.
- **Endpoints cost more than idle streams.** Sixteen sessions cost 7.5 to 8.7 % of one core
  between them (about 0.5 % each) whether their rooms play or not, because a session is
  sent chunks (of silence) either way.
- **The audio thread** grows by about 0.15 % of one core per stream, from 0.7 % with none to
  about 3 % with sixteen.
- **Memory is small.** About 20 MB resident with nothing playing; 0.5 to 1.7 MB of anonymous
  memory per stream for every format but Vorbis, which takes about 2.4 MB; 58 MB resident
  at the most (sixteen Vorbis streams).

## Result: the renderers and the players when nothing plays

The same server with no endpoint connected, three ways, and the last also with one control
point subscribed to all three services of all sixteen renderers (48 subscriptions, events
received on a loopback callback). 60 s windows.
The table is the second idle run of the day (`idle2`), made after the sampler learned to read
the mappings; the first (`idle`, in the raw files) gave the same CPU and memory within about a
tenth of a percent of one core and 0.1 MB. Its `params` names the harness's commit as `head`; the
server is the same binary (the same `server_sha256`). The last column grows by about 2.2 to
2.6 MB a thread, a thread's stack.

| configuration | window | threads | upnp threads | player threads | audio | process | VmRSS | RssAnon | Pss | Pss_Anon | VmSize | mapped with any access | load 1 min |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 16 rooms, 16 slots, control plane; no players, no renderers | idle-no-control-point | 47 | 0.000 | 0.000 | 0.454 | 0.865 | 12276 | 7136 | 10056 | 7136 | 3157408 | 150720 | 61.34 to 65.13 |
| the same with --players 16 | idle-no-control-point | 63 | 0.000 | 0.333 | 0.442 | 1.157 | 14624 | 9596 | 12436 | 9596 | 4245172 | 191952 | 65.13 to 53.57 |
| the same with --players 16 --upnp (16 renderers) | idle-no-control-point | 71 | 0.569 | 0.310 | 0.426 | 1.674 | 15380 | 9992 | 13160 | 9992 | 4785960 | 209536 | 53.57 to 55.62 |
| the same with --players 16 --upnp (16 renderers) | idle-one-subscribed-control-point | 71 | 0.543 | 0.325 | 0.415 | 1.632 | 15604 | 10216 | 13409 | 10216 | 4785960 | 209536 | 60.17 to 48.94 |

## Result: PipeWire and WirePlumber per Soloist receiver

From the PipeWire probe of the same day on the same host (PipeWire 1.4.2 and WirePlumber 0.5.8
from Debian trixie, one PipeWire and one WirePlumber per receiver, each with a pipe-tunnel sink
read by a prompt reader; a `pw-cat` client stands in for Soloist). CPU from `utime + stime` of
each process (10 ms ticks), 120 s windows for one receiver and 60 s for four and eight; load
average 33 to 64 throughout.

| receivers | state | PipeWire CPU, % of one core, mean per receiver / sum | WirePlumber CPU | PipeWire RSS / Pss / Pss_Anon, kB, mean | WirePlumber RSS / Pss / Pss_Anon, kB, mean |
|---|---|---|---|---|---|
| 1 | idle | 0.000 | 0.000 | 7448 / 2337 / 1384 | 10568 / 4359 / 1844 |
| 1 | playing | 0.242 | 0.000 | 8340 / 3689 / 1924 | 10644 / 6625 / 1856 |
| 4 | idle | 0.000 / 0.000 | 0.000 | 7453 / 2029 / 1383 | 10517 / 3192 / 1849 |
| 4 | playing | 0.204 / 0.817 | 0.000 | 8356 / 2452 / 1924 | 10590 / 3154 / 1858 |
| 8 | idle | 0.000 / 0.000 | 0.000 | 7536 / 1738 / 1385 | 10539 / 2545 / 1851 |
| 8 | playing | 0.196 / 1.566 | 0.000 | 8416 / 2228 / 1925 | 10612 / 2527 / 1860 |

"0.000" is under one clock tick in the window, not zero. Per receiver: about 0.2 to 0.25 % of
one core while playing, about 19 MB of resident set (8.4 MB PipeWire, 10.6 MB WirePlumber), of
which about 3.8 MB cannot be shared between receivers (`Pss_Anon`); six threads with the
stand-in client. Linear from one to eight. The stand-in client itself (0.11 to 0.13 % of one
core, 7.6 to 7.8 MB) is not a figure for Soloist.

## Result: the Soloist receivers on chorus's side (reader threads, manager, supervisor)

Build `856411c`. The server with `--soloist-receivers 16` and no players or renderers, sixteen
rooms and sixteen connected endpoints; sixteen real `chorus-soloistd` supervisors with
`--pipewire none`, each running the tests' fake Soloist (`crates/soloist-fake`, through the
examples `server-test-soloistd` and `server-test-fake-soloist` of `crates/server`, as
`crates/server/tests/soloist_receivers.rs` starts them). Every receiver is assigned to its
room and logged in. "The Spotify app" (the fake's test control socket) plays on K receivers;
each takes its own room (K78), so K reader threads each read their FIFO's float32 44.1 kHz
stereo, convert it to the server's rate and write their port, and K rooms hear K receivers.
45 s windows after a 6 s settle; a window is steady when every playing receiver's port played
the window's frames within 3 % (`chorus_soloist_frames_played_total`).

Two servers: one at 48 kHz, the rate a deployment runs at, where every reader resamples; one at
44.1 kHz, the FIFO's own rate, where none does. The difference is the resampler again.

**The fake Soloist is not Soloist.** It writes a ramp into the FIFO itself, standing in for
Soloist, PipeWire and the sink together; its CPU and memory were not sampled and nothing here
is a figure for Soloist or for PipeWire. `chorus-soloistd` is the real supervisor, but it
supervised the fake: what it costs relaying a real Soloist's events is the same code on other
input.

Reader threads (`soloist-reader-<i>`), % of one core per playing receiver (the mean of the K busiest; at K = 0 the mean of all 16, idle):

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 0.405 | 4.49 | 4.30 | 5.12 | 5.05 |
| 44100 Hz | 0.409 | 0.47 | 0.48 | 0.48 | 0.49 |

The idlest and the busiest of those K reader threads:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | - | 4.49 to 4.49 | 4.15 to 4.38 | 4.99 to 5.26 | 4.80 to 5.34 |
| 44100 Hz | - | 0.47 to 0.47 | 0.47 to 0.49 | 0.46 to 0.49 | 0.47 to 0.51 |

All 16 reader threads, % of one core, sum:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 6.48 | 9.69 | 21.40 | 44.13 | 80.87 |
| 44100 Hz | 6.54 | 6.62 | 6.98 | 7.12 | 7.84 |

The `soloist-manager` thread, % of one core:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 0.850 | 0.899 | 0.979 | 1.319 | 1.599 |
| 44100 Hz | 0.935 | 1.055 | 1.200 | 1.349 | 1.825 |

The audio thread, % of one core:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 0.82 | 0.98 | 1.40 | 2.29 | 3.37 |
| 44100 Hz | 0.74 | 0.88 | 1.36 | 2.05 | 3.22 |

The client threads (16 endpoints), % of one core, sum:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 9.48 | 8.80 | 8.69 | 9.55 | 9.25 |
| 44100 Hz | 9.41 | 9.54 | 9.52 | 9.33 | 9.50 |

The whole server process, % of one core:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 18.02 | 20.71 | 32.81 | 57.71 | 95.47 |
| 44100 Hz | 18.03 | 18.49 | 19.46 | 20.25 | 22.79 |

Server VmRSS at the window's end, kB:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 23736 | 26276 | 28608 | 32056 | 39612 |
| 44100 Hz | 22192 | 22724 | 24284 | 26308 | 30188 |

Server RssAnon at the window's end, kB:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 17832 | 20372 | 22704 | 26152 | 33708 |
| 44100 Hz | 16244 | 16776 | 18336 | 20360 | 24240 |

Server RssAnon growth per playing receiver against K = 0, kB:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | - | 2540 | 1218 | 1040 | 992 |
| 44100 Hz | - | 532 | 523 | 514 | 500 |

Server threads:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 64 | 64 | 64 | 64 | 64 |
| 44100 Hz | 64 | 64 | 64 | 64 | 64 |

Server mappings with any access, kB:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 197760 | 198348 | 200784 | 203304 | 209560 |
| 44100 Hz | 192912 | 193160 | 193888 | 194976 | 196696 |

One `chorus-soloistd` whose receiver plays, % of one core, mean:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | - | 0.368 | 0.357 | 0.392 | 0.367 |
| 44100 Hz | - | 0.387 | 0.387 | 0.381 | 0.392 |

One `chorus-soloistd` whose receiver is idle, % of one core, mean:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 0.392 | 0.354 | 0.351 | 0.391 | - |
| 44100 Hz | 0.385 | 0.386 | 0.386 | 0.380 | - |

One `chorus-soloistd`, VmRSS kB, mean of all 16:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 2694 | 2695 | 2697 | 2701 | 2708 |
| 44100 Hz | 2682 | 2682 | 2685 | 2689 | 2697 |

One `chorus-soloistd`, RssAnon kB, mean of all 16:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 275 | 276 | 278 | 282 | 289 |
| 44100 Hz | 274 | 275 | 277 | 282 | 290 |

One `chorus-soloistd`, Pss kB, mean of all 16:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 333 | 333 | 335 | 340 | 346 |
| 44100 Hz | 331 | 332 | 334 | 338 | 347 |

One `chorus-soloistd`, Pss_Anon kB, mean of all 16:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 275 | 276 | 278 | 282 | 289 |
| 44100 Hz | 274 | 275 | 277 | 282 | 290 |

One `chorus-soloistd`, threads:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 8 | 8 | 8 | 8 | 8 |
| 44100 Hz | 8 | 8 | 8 | 8 | 8 |

Underruns counted by the receivers' ports in the window, all receivers:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 0 | 0 | 0 | 0 | 5 |
| 44100 Hz | 0 | 1 | 2 | 2 | 32 |

Frames dropped by the receivers' ports in the window, all receivers:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 0 | 0 | 0 | 0 | 0 |
| 44100 Hz | 0 | 0 | 0 | 0 | 0 |

Load average (1 min) at the window's start and end:

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | 19.60 to 20.49 | 19.94 to 21.20 | 20.67 to 19.61 | 18.67 to 18.78 | 18.51 to 18.51 |
| 44100 Hz | 17.86 to 19.90 | 20.63 to 21.11 | 20.49 to 20.04 | 20.10 to 19.89 | 19.57 to 18.56 |

Steady (every playing receiver's port played the window's frames within 3 %):

| server | K = 0 | K = 1 | K = 4 | K = 8 | K = 16 |
|---|---|---|---|---|---|
| 48000 Hz | yes | yes | NO | NO | yes |
| 44100 Hz | yes | NO | NO | NO | yes |

What the tables say:

- **A playing receiver costs chorus-server what a 44.1 kHz file does**: 4.3 to 5.1 % of one core
  on its reader thread at 48 kHz, against 0.5 % at 44.1 kHz where nothing is resampled.
  The resampler is about 4.3 % of one core per playing receiver.
- **An idle receiver is not free**: 0.38 to 0.43 % of one core per idle reader thread (it polls an
  empty FIFO), so sixteen idle receivers cost 6.5 % between them, and the one manager
  thread 0.85 to 1.82 %, rising with what plays.
- **The "steady" row says NO in 5 of the 8 playing windows, and that is not
  explained.** The ports' `frames_played` counters advanced by 86 to 99 %
  of the window's frames, the same shortfall for every receiver of a window, with no frame
  dropped and 42 underruns counted in all 8 windows. Either the fake Soloist
  delivered less than real time in those windows or the counters are published in steps; this
  run cannot tell which. The CPU figures are per receiver as it ran: if the fake ran slow, a
  full-rate receiver costs more by up to that ratio (at most 10 % more at 48 kHz).
- **Memory**: about 1.0 to 2.5 kB of anonymous memory per playing receiver in the server; the
  `N + 1` threads add their stacks to what locking memory would hold (the mappings row).
- **`chorus-soloistd`** is small: about 2.6 MB resident (0.3 MB proportional,
  0.3 MB unshareable), 8 threads, 0.34 to 0.40 % of one core, playing or idle.

## What this does not show

- **Soloist's own resident set and CPU.** Soloist is proprietary; no binary and no key exist
  here and chorus never downloads or runs one. Its cost per instance is the owner's to measure
  on the owner's build (an item in the owner's queue), and stays `ASSUMED` until then.
- **Wire encoders.** The server sends PCM to every endpoint today. FLAC or Opus on the wire
  (ADR 0044) would add an encode per stream, which nothing here measures.
- **DSP beyond what this server applied.** No room had an EQ, a loudness curve, a room
  correction filter, a bonded set or a volume other than the default; the audio thread's
  figure is the cut and the fanout alone.
- **The real-time contract and locked memory.** This container grants no real-time priority and
  8 MiB of locked memory, so the audio thread ran as an ordinary thread and `mlockall` was not
  called. A deployment calls `mlockall(MCL_CURRENT | MCL_FUTURE)` (`crates/hostctl`), which makes
  every mapped page resident: there the resident set approaches `VmSize` less its unreadable
  guard pages, not the `VmRSS` above, and it counts against the locked-memory limit. The
  `VmSize` rows are given for that reason; what the locked process really holds is not measured.
- **A network.** Everything is loopback: no TLS to a media server, no loss, no slow source.
  Sixteen endpoints on ALSA `null`; a house of twenty speakers has more sessions than this.
- **Groups.** Every stream played in one room. A stream shared by a group costs one player and
  one slot whatever the group's size, and one session per endpoint, as measured.
- **A quiet host.** Every figure was taken beside other tenants' load; an idle host would show
  less CPU, by an amount this report cannot give.
- **Timing of any kind.** Whether a player thread kept its ring filled is visible only as
  "steady"; underruns heard by a room, the audio thread's tick against its deadline and
  everything about playout are other reports' subjects.

## Raw files

The files the tables are made from are committed beside this report, under
`docs/measurements/raw/concurrent-streams-host-2026-10-03/`, with their own `SHA256SUMS`:
each run's `windows.jsonl` (one reduced window a line), `params` and `media.tsv`, the decoder
run's `probe-media.tsv`, and the PipeWire probe's `res-summary.txt`. `receivers-*` and
`opus-alac-*` are the second build's (the `opus-alac` run holds Opus's windows only).

| file | bytes | sha256 |
|---|---|---|
| decoders-params | 704 | `9c7960db47964c22623168c044accf47c6382ee38cba716993bf5e43975be626` |
| decoders-probe-media.tsv | 3989 | `0e767fa143401bcf60677e48913450e78974e700b4a72b65e7513ddec7a67004` |
| idle-params | 700 | `ccd9f0749da8e1bc6f0b624e22c85fab78ecf1bd1810c129ecfec549a59ac4a6` |
| idle-windows.jsonl | 6103 | `cdd634df328b9f7bb59380ec3c1cbfee23c119355f64668afa920a017f5a3423` |
| idle2-params | 700 | `dc4b8f5e7f0442bcf9a3eb57e07378b699870f05899b9e39b036b88e67ef17a3` |
| idle2-windows.jsonl | 6291 | `a8461be1afeec3e685f4a9be10fd858a20195b1f0ec9b53d01a36478b104b95b` |
| media.tsv | 995 | `cdc47d5d73f070a008514484a36f578fd72fd2a6dbac281c374e7ce59fc3a14b` |
| mp3-vorbis-params | 678 | `71dfb3a3b78ab9ff5371d0a48a7423025664e8dd903621bb7d33d278b432faf2` |
| mp3-vorbis-windows.jsonl | 22323 | `02e1da2b5da4fce5f1fcf96071452f5cf4def7dbe18112a7738cd37c5cccb760` |
| opus-alac-params | 625 | `cdf316a2fd4acd3224c83096857b46aa9a5e9f4a398e4f8f6b70a3c6ea5c5cf6` |
| opus-alac-windows.jsonl | 9363 | `6c5cdc33527e80e5025bc80a6e848c3256a1e3b3ae72448278f777679bf9ffaf` |
| pipewire-res-summary.txt | 3507 | `256dfe44515c4956e82407128bfacb9126e1578a673771af0a457ef7d6debb2f` |
| receivers-params | 720 | `d59ea15c5735e02b1dfdfcd0fff2efcb269030bd3be25b6a58e2194a3af2a98f` |
| receivers-windows.jsonl | 38073 | `15e745a45e9d5538a82f04e8136df9eb1310cd9b61aa45bac379b64e6e778ab1` |
| wav-flac-params | 676 | `ffdbfc1b0e0a6e192ce22a9af199b14e3b8a9e277dd9f17f9d52840d7ce042d6` |
| wav-flac-windows.jsonl | 22325 | `b34b45b237807ee0e3ba7279b5810b4a3634d20849d95bc6848a367d2c0b8e91` |
| wav48-params | 673 | `fb02fa3778d4c36c29ff564b805583e918393d84a292a29e8564e2ff4e187791` |
| wav48-windows.jsonl | 11126 | `3463011936edf290d821ac5d2dd3ed150883dbe16c7dda8a085a1581b8657ed2` |

The full run directories (every `/proc` sample per thread, the servers' and endpoints' logs,
the control plane's report at each window's end) are kept at `/cache/tmp/chorus-g17/p11/`, not
committed, indexed by `/cache/tmp/chorus-g17/p11/SHA256SUMS` (sha256 `d8bc4bb0db961d55d5295b0a7ac0904d4d726aaab3189bf3ecf6d92b2368960c`). The
PipeWire probe's files are at `/cache/tmp/chorus-g17/pw-probe/`, indexed by its `SHA256SUMS`
(104 files; sha256 of the index
`4b517e0c89657288aaafce72af1bc8b578e4e922b05832a1ce46054e880285ac`); its method and its other
findings (pacing, cadence, latency) are in the goal's research note, not here.
