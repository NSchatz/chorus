#!/usr/bin/env bash
# The device-backed measurement run: two endpoints play the rig's chirp as one
# grouped stream, both line outputs are captured together, and the capture is
# analysed.
#
# The chirp goes through the ENDPOINTS, not out of a local device: the server
# runs `--source chirp`, which builds the sweep from config/measure.conf through
# the same ChirpSpec the analyser is written against, endpoint A plays it here
# and endpoint B on CHORUS_SECOND_ENDPOINT, and the capture tool records with
# `--record-only` and emits nothing of its own. A chirp played on a local device
# measures the interface and no endpoint (audit A-3). The recording is taken
# after the endpoints have had the sync settle config/sync.conf declares, so it
# is of settled playout rather than of the acquisition transient.
#
# NEEDS AN ENVIRONMENT.
#   - a second wired Linux endpoint running chorus-client (CHORUS_SECOND_ENDPOINT)
#   - a local ALSA playback device that reports a delay, which endpoint A plays
#     on (CHORUS_CLIENT_DEVICE)
#   - both endpoints' line outputs wired into the L and R inputs of one audio
#     interface, visible to ALSA as a capture device (CHORUS_CAPTURE_DEVICE)
# On a machine without them this script exits non-zero naming the missing
# prerequisite and the criterion it was verifying, and reports nothing as passed
# or skipped-green.
#
# The chirp leaves through real amplifiers into real loudspeakers. The server
# holds it to the amplitude ceiling in config/measure.conf before it binds a
# socket, so a run asking for more than it permits refuses having emitted
# nothing. Do not raise that ceiling to make a quiet capture louder; move the
# interface's input gain instead.
#
#   CHORUS_SECOND_ENDPOINT=user@endpoint-b \
#   CHORUS_CAPTURE_DEVICE=hw:1,0 CHORUS_CLIENT_DEVICE=hw:0,0 \
#       ./tools/measure/capture-run.sh [seconds]

source "$(dirname "$0")/../lib.sh"

CRITERION="two endpoint line outputs captured together, cross-correlated into median, p95 and maximum inter-device lag at 10 us or better"

build_once

SECONDS_TO_CAPTURE="${1:-10}"
SETTLE_SECONDS="$(sync_conf sync_hour_settle_seconds)"
# The endpoints outlive the capture by a margin, so the recording never catches
# one of them stopping.
RUN_SECONDS=$((SETTLE_SECONDS + SECONDS_TO_CAPTURE + 30))
DEVICE="$(audio_device)"
CAPTURE_DEVICE="$(capture_device)"
OUT_DIR="${CHORUS_MEASURE_OUT:-${TMPDIR:-/tmp}}"
CAPTURE_FILE="$OUT_DIR/chorus-measure-capture.wav"

say "chorus: the device-backed measurement run, through two endpoints"
say "  capture:         ${SECONDS_TO_CAPTURE}s after a ${SETTLE_SECONDS}s settle"
say "  local device:    $DEVICE"
say "  capture device:  $CAPTURE_DEVICE"
say "  second endpoint: ${CHORUS_SECOND_ENDPOINT:-<unset>}"

# Every prerequisite is checked before anything is started, so a run that
# cannot be graded emits nothing at all.
require_second_endpoint "$CRITERION"
require_pacing_audio_device "$CRITERION"
require_capture_device "$CRITERION"

PORT="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
read -r -a SERVER_EXTRA_ARGS <<< "${CHORUS_SERVER_EXTRA_ARGS:-}"

say "chorus: starting the server on the rig's chirp; both endpoints join the SAME stream"
"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "0.0.0.0:$PORT" \
    --source chirp \
    --measure-config "$REPO_ROOT/config/measure.conf" \
    --rate "$(conf sample_rate_hz)" \
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

# The same committed constants tools/sync-hour-run.sh passes, from the same
# files, so the run that is measured is the one they describe.
client_args() {
    printf '%s' "--min-us $(conf min_us) --max-us $(conf max_us) \
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
}

say "chorus: starting endpoint A here"
read -r -a CLIENT_ARGS <<< "$(client_args)"
"$BIN_DIR/chorus-client" \
    --ephemeral-identity --endpoint-id capture-endpoint-a \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    "${CLIENT_ARGS[@]}" &
CLIENT_A_PID=$!
trap 'kill_quietly "$CLIENT_A_PID"; kill_quietly "$SERVER_PID"' EXIT

say "chorus: starting endpoint B on $CHORUS_SECOND_ENDPOINT"
SERVER_HOST="${CHORUS_SERVER_HOST:-$(hostname)}"
# shellcheck disable=SC2029 # the remote command line is built here on purpose, from this checkout's config
ssh "$CHORUS_SECOND_ENDPOINT" "chorus-client --ephemeral-identity --endpoint-id capture-endpoint-b --server ${SERVER_HOST}:${PORT} \
--device ${CHORUS_SECOND_DEVICE:-default} $(client_args)" &
CLIENT_B_PID=$!
trap 'kill_quietly "$CLIENT_B_PID"; kill_quietly "$CLIENT_A_PID"; kill_quietly "$SERVER_PID"' EXIT

say "chorus: letting both endpoints settle for ${SETTLE_SECONDS}s"
sleep "$SETTLE_SECONDS"

say "chorus: capturing both line outputs for $SECONDS_TO_CAPTURE s into $CAPTURE_FILE"
"$BIN_DIR/chorus-measure-capture" \
    --record-only \
    --capture-device "$CAPTURE_DEVICE" \
    --seconds "$SECONDS_TO_CAPTURE" \
    --out "$CAPTURE_FILE"

say "chorus: analysing the capture"
"$BIN_DIR/chorus-measure" lag "$CAPTURE_FILE" --label "device-run"

say "chorus: the run completed and its report is in docs/measurements/"
