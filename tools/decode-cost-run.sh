#!/usr/bin/env bash
# What FLAC and Opus decoding cost on the ESP32-S3 itself (chorus goal 6's
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

CRITERION="the FLAC and Opus decode cost is measured on the ESP32-S3 itself, from a decode that matches its reference, rather than inferred from the host's"
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
    if printf '%s' "$reply" | grep -q 'decode_matches=no'; then
        bench_finish FAIL "a decode did not match its reference; no figure is published"
    fi
    bench_finish MEASURED "FLAC $(decode_figure "$reply" flac-s16-stereo-44k1 cpu_fraction) and Opus $(decode_figure "$reply" opus-tv10-celt-stereo cpu_fraction) of one core for real-time playback, on the chip"
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
report_and_publish
