# 0086: the endpoint console's `dsp-cost` times the DSP chain on the chip in four configurations over a generated signal, publishes a figure only for output within tolerance of the host's checksum, and bench S6 records it in the decode-cost report

- Status: accepted (goal 12, 2026-10-01)
- Decided by: BRIEF.md section 3.1 rule 3 (no timing or headroom claim without a measurement
  report); ADR 0085's follow-up ("the chain's cost on the ESP32-S3 at 48 kHz stereo is
  unmeasured"); the decode-cost precedent of ADR 0060 (audit A-13) followed in shape; the
  placements below where the precedent left them open
- Implemented in: `firmware/include/chorus/dsp_cost.h`, `firmware/src/dsp_cost.c` (host build
  `LIB_SRC`, ESP-IDF component sources, `firmware/endpoint-units.conf`),
  `firmware/src/console.c` and `firmware/include/chorus/console.h` (`dsp-cost`),
  `firmware/main/console_esp.c` (the internal-RAM allocator and the help line);
  `tools/decode-cost-run.sh`, `tools/bench/topics.conf`, `tools/bench/e2e-test.sh`;
  `docs/bench-packet.md` S6, `docs/bench.md`; held by `firmware/tests/test_console.c`
  (`make -C firmware console`). The cross-checks: `firmware/tests/test_dsp.c` (the C room-EQ
  bounds against the wire's) and `crates/server/tests/room_eq_bounds_agree.rs` (the Rust bounds,
  and every fixture fit's `room_eq` command through the catalog)

## Context

ADR 0085 put the chain in the C endpoint's playout path, once per 32-frame call, with no figure
for what it costs on the ESP32-S3. Without one, nothing may say the chip has room for it beside
the decoders, the sync loop and the network. The decoders were in the same position after goal
6, and ADR 0060 solved it with a console command the owner's bench script drives: the image
carries its own input, times the work on its monotonic clock, checks the result against a
reference so a figure is only about correct output, and replies with frames per second and the
fraction of one core. The chain needed the same, plus a reference that survives the chip's libm.

## What was read

All read 2026-10-01: `BRIEF.md` section 3.1; ADRs 0060, 0082, 0083, 0085; `docs/dsp.md`,
`docs/bench-packet.md` (S5, S6), `docs/bench.md`; `firmware/src/decode_cost.c`,
`firmware/src/console.c`, `firmware/main/console_esp.c`, `firmware/main/app_main.c` (the chain's
boot allocation), `firmware/include/chorus/dsp.h`, `firmware/src/dsp.c` (which libm calls run per
frame: `log10f` and `expf` in the night compressor; `sin`, `cos`, `pow`, `log10`, `exp` in the
designs), `firmware/src/endpoint_dsp.c` (the 32-frame calls); `tools/decode-cost-run.sh`,
`tools/bench/{lib.sh,topics.conf,validate-report.sh,e2e-test.sh}`, `tools/lib.sh`
(`endpoint_console`); `crates/dsp/src/{roomfit.rs,settings.rs}`,
`crates/control/src/{catalog.rs,sound.rs}`, `firmware/include/chorus/protocol_v2.h`. The input
generator is xorshift32 (G. Marsaglia, "Xorshift RNGs", Journal of Statistical Software 8(14),
2003, <https://www.jstatsoft.org/article/view/v008i14>, the landing page read; the shift triple
13, 17, 5 is the paper's 32-bit example as recalled, not re-read: the input only has to be
deterministic and broadband, not a good generator, so nothing rests on the triple's quality). No
GPL source was opened.

## Decision

1. **The command.** `dsp-cost [flat|all-on|sub|two-way]`, one reply line in decode-cost's shape:
   `dsp-cost <name>:rate=48000,channels=2,outputs=<n>,frames=48000,elapsed_us=..,frames_per_s=..,
   cpu_fraction=..,deviation=..,output_matches=yes|no,bit_exact=yes|no ... stack_free_bytes=..`,
   or `error dsp-cost reason=<token>` (`usage`, `no-clock`, `no-such-config`, `chain-failed` with
   the library's or the allocator's reason, `output-differs`). Every value is runtime only.
2. **The configurations**, each a 48 kHz stereo (FL, FR) stream at room gain -12 dB and unity
   limit: `flat` (every stage bypassed, unbonded); `all-on` (bass +4, treble -3, loudness, night,
   speech and eight room-EQ filters shaped like a fit, unbonded); `sub` (the LFE member of a 2.1
   set: the LR4 low branch of the mains' sum at 80 Hz, one output); `two-way` (unbonded, the mono
   downmix split at 2 kHz, the tweeter trimmed and delayed). Each runs in a fresh chain over one
   second of signal.
3. **Timed as the playout path runs it.** The chain is called in 32-frame blocks, the playout
   path's `CHORUS_ENDPOINT_DSP_BLOCK_FRAMES`, so per-call overhead is counted as the endpoint pays
   it, and only the chain calls are timed: generating the input and summing the output are the
   harness's cost. The chain object (72680 bytes) is borrowed from an allocator the console is
   handed; the chip binds internal RAM, where `app_main` puts the playout path's own chain, so the
   figure is not an external-RAM figure. A board without the internal RAM for a second chain is
   refused by name (`no memory for a 72680 byte chain`), not measured somewhere slower.
4. **The input** is xorshift32 white noise, each sample a 24-bit integer over 2^24 in
   [-0.5, 0.5): integer arithmetic only, so it is the same bits on the chip and the host.
5. **The reference, with a tolerance.** decode-cost compares an exact FNV-1a hash, which works
   because the decoders are integer-exact. The chain is not: it calls libm per frame (the night
   compressor's `log10f` and `expf`) and in every design, and newlib on the chip need not round a
   last place as glibc does. So the output is summed, in double, into its energy and four
   projections onto pseudo-random +-1 sequences, and compared with the host's sums for the same
   input (`chorus_dsp_cost_reference`, committed in `dsp_cost.c`): the deviation is the energy's
   relative difference and each projection's difference over the reference's RMS norm, and the
   output matches at 1e-4 or less. A projection onto a random sign sequence measures the NORM of
   the error, so a narrow filter dropped shows at its own size even when the energy barely moves.
   The host test holds this both ways: the host reproduces the reference (bit exact on the
   machine that generated it; graded at the tolerance, so another glibc is not a red gate), and
   the all-on chain with its mildest filter dropped (420 Hz, +1.5 dB, Q 1) deviates 2.8e-2 and
   with night mode off 2.3, both refused. The exact FNV-1a hash is still compared and reported as
   `bit_exact`, information only.
6. **One report, one command.** `tools/decode-cost-run.sh` asks `decode-cost` and then
   `dsp-cost` on the same session, keeps both replies as raw files (`console.txt`,
   `dsp-console.txt`), and writes them into the same `embedded5-decode-cost-<date>` report, whose
   required fields gain `dsp_flat_cpu_fraction`, `dsp_all_on_cpu_fraction`,
   `dsp_sub_cpu_fraction` and `dsp_two_way_cpu_fraction`. A sibling script was not added: S6 is
   already "what the audio path costs on this chip, from this image", the playout budget needs
   the decoder and the chain figures from the same image and the same boot, and the owner keeps
   one command. The result is MEASURED only when every decode and every chain output matched;
   no `dsp-cost` reply or a mismatch is FAIL, with the decode figures still recorded. Nothing is
   graded against a bound.
7. **The cross-checks the earlier tracks left.** `CHORUS_DSP_ROOM_EQ_*` equal
   `CHORUS_V2_SOUND_EQ_*` (static asserts, and counted checks in `make -C firmware dsp`);
   `chorus_dsp::settings::ROOM_EQ_*` equal `chorus_control`'s `ROOM_EQ_*`, and every fixture room
   the fitter fits produces a `room_eq_command_json` the catalog decodes, validates and reads back
   as exactly the fitted filters. The Rust test lives in `crates/server/tests`, the one place that
   already has both crates (`chorus-server` depends on `chorus-control` and dev-depends on
   `chorus-dsp`), so neither library gains an edge to the other.

## Evidence

Host only, and host evidence is not timing evidence: `make -C firmware console` (63 checks) runs
the command against a fake clock that moves 1 ms per read (1500 calls, 1.5 s timed for 1 s of
audio, `cpu_fraction=1.5000`, asserted exactly as arithmetic) and once against the host's
monotonic clock, whose figures are printed and recorded nowhere. No check asserts a timing. The
chip's figures exist only once the owner runs S6; until then the chain's cost on the ESP32-S3 is
unmeasured and no headroom claim is made.

## Not chosen

- **An exact hash, as decode-cost.** A last-place libm difference would make the bench refuse a
  correct chain, and the owner would learn nothing about its cost.
- **Committing the reference output.** Four configurations of a second of stereo are about 1.5 MB
  of floats in the image; four projections and an energy are 48 bytes each and catch a stage
  dropped.
- **Energy or RMS alone.** A narrow cut moves the energy by far less than the tolerance on noise;
  the projections see it at its own size.
- **Timing the endpoint's own chain object.** It belongs to the playout task; borrowing it from the
  console would race the audio path. S6 is taken with no stream playing, and a second chain is
  the cleaner measurement.
- **A sibling script and topic.** See decision 6.

## ASSUMED values (not measured)

The 1e-4 tolerance; the -12 dB room gain the configurations run at; the all-on configuration's
settings and its eight filters (shaped like a fit, not taken from a room); the sub level -3 dB;
the two-way's 2 kHz, -3 dB tweeter trim and 100 us delay (the envelope's example until goals 24-25
design the drivers); one second of signal per configuration; the 120 s reply wait in the bench
script.

## Follow-ups

- The owner runs S6 (`CHORUS_ESP32S3_PORT=/dev/ttyACM0 CHORUS_BENCH_PR=1
  tools/decode-cost-run.sh`); its report is the first figure for the chain on the chip, and any
  headroom statement in `docs/dsp.md` cites it.
- If S6 says `no memory for a 72680 byte chain`, a board without PSRAM cannot hold two chains;
  then either the console borrows the playout path's chain while no stream plays, or the chain's
  delay pool shrinks.
- 96 kHz and multichannel (5.1 into a role) configurations, once an endpoint plays them.
