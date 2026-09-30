#!/usr/bin/env bash
# AC-1 and AC-3: the hardware-attended run, and THE TWO CRITERIA IT GRADES ARE
# NOT PASSED.
#
# AC-1. WHEN an ESP32-S3 endpoint plays a grouped stream alongside a Linux
# endpoint THE SYSTEM SHALL hold inter-device error within the bound SYNC-4
# met.
#
# AC-3. WHEN the I2S slot width is 24 bits THE SYSTEM SHALL configure an MCLK
# multiple divisible by three so the bit-clock division stays integral. Graded
# by MEASURING THE PRODUCED SAMPLE RATE and not by reading the configuration
# back, which is the roadmap's explicit instruction for this assertion: a
# configuration read back agrees with itself whatever the hardware does.
#
# Neither is claimed anywhere in this repository. Both need an ESP32-S3 with a
# TAS5825M-class amplifier and a real loudspeaker, a Linux endpoint to play the
# same grouped stream, and the RIG-3 capture rig. This machine has none of
# those and this pipeline has no route to acquire them. On a machine without
# them this script exits non-zero naming the missing prerequisite and the
# criterion it was verifying, and reports nothing as passed, skipped-green or
# satisfied. docs/verification-record.md quotes the refusal and says plainly
# that the criteria are unmet.
#
# The bound AC-1 names is SYNC-4's own first criterion: median inter-device
# error below 0.5 ms over at least an hour of one grouped stream, as measured
# by the RIG-3 harness, with no hard resync after the first minute. SYNC-4's
# AC-1 is ITSELF blocked on the same hardware. This phase does not claim it and
# does not unblock it.
#
# NEEDS AN ENVIRONMENT.
#   - an ESP32-S3 endpoint with the amplifier wired per firmware/config/
#     endpoint.conf and a real loudspeaker on it (CHORUS_ESP32S3_PORT)
#   - the TAS5825M register map read off the datasheet and written into
#     firmware/config/endpoint.conf, which this phase declares UNKNOWN
#   - a Linux endpoint running chorus-client to play the same grouped stream
#     (CHORUS_SECOND_ENDPOINT)
#   - both endpoints' line outputs wired into the L and R inputs of one audio
#     interface, visible to ALSA as a capture device (CHORUS_CAPTURE_DEVICE)
#
# The grouped stream IS the rig's chirp: the server runs `--source chirp`,
# built from config/measure.conf, because the lag analyser refuses anything
# without the chirp in it (audit A-7), and the capture tool records with
# `--record-only` and emits nothing of its own. The chirp leaves through a real
# amplifier into a real loudspeaker. The server holds it to the amplitude
# ceiling in config/measure.conf before it binds a socket; do not raise that
# ceiling to make a quiet capture louder, move the interface's input gain
# instead. This script adds no gain control and writes no amplifier register.
# The endpoint's own analog gain ceiling is in firmware/config/endpoint.conf and
# the endpoint refuses to enable output above it.
#
# THE BENCH REPORT (K45). The run ends by writing
# docs/measurements/embedded5-endpoint-rig-<date>.md with every capture and the
# Linux endpoint's delay log hashed (tools/bench/lib.sh), and with
# CHORUS_BENCH_PR=1 it commits that on bench/<date>-embedded5-endpoint-rig and
# opens the PR. `--report-from <run directory>` runs only that half. AC-3's
# analysis, the sample rate the endpoint actually produced, recovered from the
# capture, does not exist yet: chorus-measure has no such analyser, so the
# report hashes the captures for it and says INCOMPLETE rather than PASS.
#
#   CHORUS_ESP32S3_PORT=/dev/ttyACM0 CHORUS_SECOND_ENDPOINT=user@endpoint-b \
#   CHORUS_CAPTURE_DEVICE=hw:1,0 \
#       ./tools/endpoint-rig-run.sh          # or: make verify-endpoint-rig
#   ./tools/endpoint-rig-run.sh --report-from <run directory>

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"

CRITERION="an ESP32-S3 endpoint playing a grouped stream alongside a Linux endpoint holds inter-device error within the bound SYNC-4 met, and the sample rate the 24-bit I2S configuration actually produces is measured rather than read back"

build_once

# The report and PR half. AC-1 is graded from the captures exactly as SYNC-4's
# hour is; AC-3 has no analyser yet, so the best this report can say is
# INCOMPLETE (or FAIL when AC-1 fails).
report_and_publish() {
    local bound median
    bound="$(transport_conf wired_bound_us)"
    bench_lag_captures endpoint-rig
    median="$(printf '%s' "$BENCH_LAG_MEDIANS" | bench_median_abs)"
    if [ -f "$BENCH_RUN_DIR/raw/endpoint-rig-linux.log" ]; then
        bench_analysis delaylog-linux "$BIN_DIR/chorus-delaylog-check" \
            "$BENCH_RUN_DIR/raw/endpoint-rig-linux.log" --require-zero-underruns || true
    fi
    bench_field captures_analysed "$BENCH_LAG_RESOLVED of $BENCH_LAG_TOTAL resolved"
    bench_field median_of_medians_us "${median:-none}"
    bench_field bound_us "$bound (config/transport.conf wired_bound_us)"
    bench_field produced_rate "not analysed: chorus-measure has no produced-sample-rate analyser yet; the captures are hashed for it (AC-3)"
    local summary="AC-1: median of $BENCH_LAG_RESOLVED capture medians ${median} us against the ${bound} us bound; AC-3: produced sample rate not analysed (no analyser yet)"
    if [ "$BENCH_LAG_TOTAL" -gt 0 ] && [ "$BENCH_LAG_RESOLVED" = "$BENCH_LAG_TOTAL" ] \
        && awk -v m="$median" -v b="$bound" 'BEGIN { exit !(m < b) }'; then
        bench_finish INCOMPLETE "$summary"
    fi
    bench_finish FAIL "$summary"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" embedded5-endpoint-rig
    report_and_publish
fi

RUN_SECONDS="$(sync_conf sync_hour_run_seconds)"
CAPTURE_SECONDS="$(sync_conf sync_hour_capture_seconds)"
SETTLE_SECONDS="$(sync_conf sync_hour_settle_seconds)"
CAPTURES="$(sync_conf sync_hour_captures)"
CAPTURE_DEVICE="$(capture_device)"
SAMPLE_RATE="$(endpoint_conf i2s_sample_rate_hz)"
SLOT_WIDTH="$(endpoint_conf i2s_slot_bit_width)"
MCLK_MULTIPLE="$(endpoint_conf i2s_mclk_multiple)"

GRADED_SECONDS=$((RUN_SECONDS - SETTLE_SECONDS))
CAPTURE_SPACING=$((GRADED_SECONDS / CAPTURES))

say "chorus: the ESP32-S3 endpoint beside a Linux endpoint, on the rig"
say "  criterion:       AC-1 and AC-3, the two criteria of this phase that need hardware"
say "  run:             ${RUN_SECONDS}s, of which the first ${SETTLE_SECONDS}s is acquisition"
say "  captures:        ${CAPTURES} of ${CAPTURE_SECONDS}s, one every ${CAPTURE_SPACING}s"
say "  endpoint:        ${CHORUS_ESP32S3_PORT:-<unset>}"
say "  linux endpoint:  ${CHORUS_SECOND_ENDPOINT:-<unset>}"
say "  capture:         $CAPTURE_DEVICE"
say "  i2s:             ${SAMPLE_RATE} Hz, ${SLOT_WIDTH}-bit slots, MCLK x${MCLK_MULTIPLE}"

# Every prerequisite is checked before anything is started, so a run that
# cannot be graded emits nothing at all.
require_esp32s3_endpoint "$CRITERION"
require_amplifier_registers "$CRITERION"
require_second_endpoint "$CRITERION"
require_capture_device "$CRITERION"

bench_begin embedded5-endpoint-rig "$CRITERION"
bench_device "capture interface: ALSA $CAPTURE_DEVICE (L = ESP32-S3 endpoint, R = Linux endpoint)"
bench_device "ESP32-S3 endpoint on its serial console, I2S ${SAMPLE_RATE} Hz, ${SLOT_WIDTH}-bit slots, MCLK x${MCLK_MULTIPLE} (firmware/config/endpoint.conf)"
bench_device "Linux endpoint: chorus-client over ssh, ALSA ${CHORUS_SECOND_DEVICE:-default} (address not recorded, K27)"
bench_reproduce "tools/endpoint-rig-run.sh"
OUT_DIR="$BENCH_RUN_DIR/raw"
LOG_DIR="$BENCH_RUN_DIR/raw"

PORT="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
read -r -a SERVER_EXTRA_ARGS <<< "${CHORUS_SERVER_EXTRA_ARGS:-}"
mkdir -p "$LOG_DIR"

say "chorus: starting the server; both endpoints join the SAME stream"
"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "0.0.0.0:$PORT" \
    --source chirp \
    --measure-config "$REPO_ROOT/config/measure.conf" \
    --rate "$SAMPLE_RATE" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    --rttime-us "$(conf rttime_us)" \
    --rt-priority "$(conf rt_priority)" \
    --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
    "${CONTRACT_ARGS[@]}" "${SERVER_EXTRA_ARGS[@]}" &
SERVER_PID=$!
trap 'kill_quietly "$SERVER_PID"' EXIT

sleep 1

SERVER_HOST="${CHORUS_SERVER_HOST:-$(hostname)}"

say "chorus: pointing the ESP32-S3 endpoint at ${SERVER_HOST}:${PORT}"
# The endpoint takes its server address over the serial console. Everything
# else it needs is in the image, from firmware/config/endpoint.conf.
printf 'server %s:%s\n' "$SERVER_HOST" "$PORT" > "$CHORUS_ESP32S3_PORT"

say "chorus: starting the Linux endpoint on $CHORUS_SECOND_ENDPOINT"
REMOTE_COMMAND="chorus-client --ephemeral-identity --endpoint-id endpoint-rig-linux --server ${SERVER_HOST}:${PORT} \
--device ${CHORUS_SECOND_DEVICE:-default} \
--delay-log endpoint-rig-linux.log \
--min-us $(conf min_us) --max-us $(conf max_us) \
--start-fill-us $(conf start_fill_us) --device-target-us $(conf device_target_us) \
--sync-interval-ms $(sync_conf sync_interval_ms) \
--filter-window $(sync_conf filter_window) \
--smoothing-alpha $(sync_conf smoothing_alpha) \
--hard-resync-threshold-us $(sync_conf hard_resync_threshold_us) \
--max-correction-ppm $(sync_conf max_correction_ppm) \
--staleness-limit-ms $(sync_conf staleness_limit_ms) \
--max-rtt-us $(sync_conf max_rtt_us) \
--playout-latency-us $(sync_conf playout_latency_us) \
--mute-us $(sync_conf mute_us) \
--run-seconds $RUN_SECONDS"
# shellcheck disable=SC2029 # the remote command line is built here on purpose, from this checkout's config
ssh "$CHORUS_SECOND_ENDPOINT" "$REMOTE_COMMAND" &
LINUX_PID=$!
trap 'kill_quietly "$LINUX_PID"; kill_quietly "$SERVER_PID"' EXIT

# AC-3 is graded from the CAPTURE and not from the configuration: the sample
# rate the endpoint's I2S actually produces is recovered from the recorded
# audio, so an MCLK multiple that makes the rate imprecise shows up as a rate
# that is not the one that was asked for. A configuration read back agrees with
# itself whatever the hardware does, which is why the roadmap says to measure.
say "chorus: capturing both line outputs, $CAPTURES times, starting at t=${SETTLE_SECONDS}s"
RUN_STARTED=$SECONDS
CAPTURE_INDEX=1
while [ "$CAPTURE_INDEX" -le "$CAPTURES" ]; do
    DUE=$((SETTLE_SECONDS + (CAPTURE_INDEX - 1) * CAPTURE_SPACING))
    while [ $((SECONDS - RUN_STARTED)) -lt "$DUE" ]; do
        sleep 1
    done
    WAV="$OUT_DIR/capture-$CAPTURE_INDEX.wav"
    say "chorus: capture $CAPTURE_INDEX of $CAPTURES at t=$((SECONDS - RUN_STARTED))s"
    "$BIN_DIR/chorus-measure-capture" \
        --record-only \
        --capture-device "$CAPTURE_DEVICE" \
        --seconds "$CAPTURE_SECONDS" \
        --out "$WAV"
    CAPTURE_INDEX=$((CAPTURE_INDEX + 1))
done

set +e
wait "$LINUX_PID"
LINUX_STATUS=$?
set -e
kill_quietly "$SERVER_PID"
say "chorus: the Linux endpoint exited $LINUX_STATUS"
scp -q "$CHORUS_SECOND_ENDPOINT:endpoint-rig-linux.log" "$LOG_DIR/endpoint-rig-linux.log" \
    || say "chorus: the Linux endpoint's delay log could not be fetched; the report says so"

say "chorus: analysing the captures and writing the bench report"
report_and_publish
