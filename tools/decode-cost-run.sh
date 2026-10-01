#!/usr/bin/env bash
# What FLAC and Opus decoding and the DSP chain cost on the ESP32-S3 itself (chorus goal 6's
# follow-up, goal 7's deferred S3 decode-cost measurement; goal 8's console,
# audit A-13).
#
# Goal 6 measured the decoders on the host (docs/measurements/
# codec-decode-cost-host.md, source host) and could not measure them where they
# run. This asks the endpoint's serial console for `decode-cost`: the image
# carries one FLAC and one Opus stream from fixtures/codec, decodes each chunk
# by chunk through its own codec seam, times the decode calls on its monotonic
# clock and replies with frames per second and the fraction of one core real
# time playback would take (firmware/src/decode_cost.c). A figure is only
# published for a decode that hashed to the fixture's decode_fnv1a64, so it is
# always about a decode that produced the right audio. The reply also names the
# console task's least free stack, since the decoders run on it.
#
# The DSP chain beside them (goal 12): the same session then asks for
# `dsp-cost`, which runs the endpoint's DSP chain (firmware/src/dsp.c, the
# chain the playout path runs) in four configurations (flat; all-on: tone,
# loudness, speech, night and eight room-EQ filters; the LFE member of a 2.1
# set; the two-way split) over one second of a generated 48 kHz stereo signal
# in the playout path's 32-frame calls, times the chain calls on the same
# monotonic clock, and replies per configuration with frames per second and
# the fraction of one core (firmware/src/dsp_cost.c). Its figure is published
# only for a chain whose output sums to the checksum the host computed for the
# same input, within a tolerance (libm may round a last place differently on
# the chip; bit_exact says whether it did). The figures go in this same
# report, so S6 is still one command: a decode and a chain both run on this
# chip's one audio path, and the playout budget needs both from one image.
#
# NOT PASSED ANYWHERE IN THIS REPOSITORY, and nothing is graded against a bound:
# the result is MEASURED, a record for the playout and DSP budgets (whether a
# decode fits beside them is those goals' question). Taken with the stream
# stopped: the console task shares the chip with the session, and a busy session
# would be measured as decoder cost.
#
# NEEDS AN ENVIRONMENT.
#   - an ESP32-S3 endpoint flashed with an image built from this checkout, its
#     serial console on CHORUS_ESP32S3_PORT (flashing is the owner's act)
#
# THE BENCH REPORT (K45). The run writes docs/measurements/
# embedded5-decode-cost-<date>.md with the console's reply hashed, and with
# CHORUS_BENCH_PR=1 commits it on bench/<date>-embedded5-decode-cost and opens
# the PR. `--report-from <run directory>` runs only that half.
#
#   CHORUS_ESP32S3_PORT=/dev/ttyACM0 ./tools/decode-cost-run.sh
#   ./tools/decode-cost-run.sh --report-from <run directory>

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"

CRITERION="the FLAC and Opus decode cost and the DSP chain's cost are measured on the ESP32-S3 itself, from a decode and a chain output that match their references, rather than inferred from the host's"
TOPIC=embedded5-decode-cost

# One `name:key=value,...` group per fixture in the reply; print one field.
decode_figure() {
    local reply="$1" name="$2" key="$3"
    printf '%s\n' "$reply" | tr ' ' '\n' | sed -n "s/^$name://p" | tr ',' '\n' | sed -n "s/^$key=//p" | head -n 1
}

report_and_publish() {
    local raw="$BENCH_RUN_DIR/raw" reply stack name codec
    reply="$(sed -n 's/^\(decode-cost .*\)$/\1/p' "$raw/console.txt" 2>/dev/null | head -n 1)"
    stack="$(printf '%s\n' "$reply" | tr ' ' '\n' | sed -n 's/^stack_free_bytes=//p' | head -n 1)"
    if [ -z "$reply" ]; then
        bench_field flac_frames_per_s "no reply"
        bench_field flac_cpu_fraction "no reply"
        bench_field opus_frames_per_s "no reply"
        bench_field opus_cpu_fraction "no reply"
        bench_finish FAIL "the console gave no decode-cost reply: $(head -n 1 "$raw/console.txt" 2>/dev/null)"
    fi
    for name in flac-s16-stereo-44k1 opus-tv10-celt-stereo; do
        codec="$(decode_figure "$reply" "$name" codec)"
        bench_field "${codec:-missing}_frames_per_s" "$(decode_figure "$reply" "$name" frames_per_s) ($name)"
        bench_field "${codec:-missing}_cpu_fraction" "$(decode_figure "$reply" "$name" cpu_fraction) ($name)"
        bench_field "${codec:-missing}_decode_matches" "$(decode_figure "$reply" "$name" decode_matches)"
    done
    bench_field console_stack_free_bytes "${stack:-not reported} (of 16384, firmware/main/console_esp.c)"

    # The DSP chain: one `name:key=value,...` group per configuration, the
    # same shape (firmware/src/console.c's dsp_cost).
    local dsp dsp_stack config key
    dsp="$(sed -n 's/^\(dsp-cost .*\)$/\1/p' "$raw/dsp-console.txt" 2>/dev/null | head -n 1)"
    for config in flat all-on sub two-way; do
        key="dsp_$(printf '%s' "$config" | tr '-' '_')"
        if [ -n "$dsp" ]; then
            bench_field "${key}_frames_per_s" "$(decode_figure "$dsp" "$config" frames_per_s) (outputs $(decode_figure "$dsp" "$config" outputs))"
            bench_field "${key}_cpu_fraction" "$(decode_figure "$dsp" "$config" cpu_fraction)"
            bench_field "${key}_output_matches" "$(decode_figure "$dsp" "$config" output_matches) (deviation $(decode_figure "$dsp" "$config" deviation), bit exact $(decode_figure "$dsp" "$config" bit_exact))"
        else
            bench_field "${key}_frames_per_s" "no reply"
            bench_field "${key}_cpu_fraction" "no reply"
        fi
    done
    dsp_stack="$(printf '%s\n' "$dsp" | tr ' ' '\n' | sed -n 's/^stack_free_bytes=//p' | head -n 1)"
    bench_field dsp_console_stack_free_bytes "${dsp_stack:-not reported} (after dsp-cost; of 16384)"

    if printf '%s' "$reply" | grep -q 'decode_matches=no'; then
        bench_finish FAIL "a decode did not match its reference; no figure is published"
    fi
    if [ -z "$dsp" ]; then
        bench_finish FAIL "the console gave no dsp-cost reply: $(grep -v '^#' "$raw/dsp-console.txt" 2>/dev/null | head -n 1)"
    fi
    if printf '%s' "$dsp" | grep -q 'output_matches=no'; then
        bench_finish FAIL "a DSP chain's output did not match the host's checksum; no figure is published"
    fi
    bench_finish MEASURED "FLAC $(decode_figure "$reply" flac-s16-stereo-44k1 cpu_fraction) and Opus $(decode_figure "$reply" opus-tv10-celt-stereo cpu_fraction) of one core for real-time playback, the DSP chain $(decode_figure "$dsp" flat cpu_fraction) flat and $(decode_figure "$dsp" all-on cpu_fraction) all-on, on the chip"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" "$TOPIC"
    report_and_publish
fi

say "chorus: the ESP32-S3's decode cost, over its serial console"
say "  endpoint: ${CHORUS_ESP32S3_PORT:-<unset>}"

require_esp32s3_endpoint "$CRITERION"
require_endpoint_console "$CRITERION"

bench_begin "$TOPIC" "$CRITERION"
bench_device "ESP32-S3 endpoint on its serial console (image from this checkout; stream stopped)"
bench_reproduce "CHORUS_ESP32S3_PORT=<port> tools/decode-cost-run.sh"
OUT_DIR="$BENCH_RUN_DIR/raw"
mkdir -p "$OUT_DIR"

say "chorus: asking for decode-cost (the decode takes a few seconds on the chip)"
endpoint_console decode-cost 60 > "$OUT_DIR/console.txt" || printf 'no reply\n' > "$OUT_DIR/console.txt"
cat "$OUT_DIR/console.txt"
say "chorus: asking for dsp-cost (four configurations of the chain, a few seconds on the chip)"
endpoint_console dsp-cost 120 > "$OUT_DIR/dsp-console.txt" || printf 'no reply\n' > "$OUT_DIR/dsp-console.txt"
cat "$OUT_DIR/dsp-console.txt"
report_and_publish
