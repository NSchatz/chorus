#!/usr/bin/env bash
# A one-minute run, to check the shape of the record rather than the length of
# the run.
#
# Verifies: the client records, at an interval no coarser than once per second
# and in a form a script can parse without the client running, the
# device-reported delay, the buffer occupancy, a value from its own monotonic
# timeline, and whether each sample is inside the graded interval; and records
# the configured bounds and start fill in the same file. Also verifies that
# output is withheld until the configured start fill and that the log names the
# fill it started at, and that the three bound relations hold.
#
# Prerequisite: a usable ALSA playback device, including a loopback or the
# ALSA null device.
#
#   ./tools/delay-log-shape.sh [output-log]

source "$(dirname "$0")/lib.sh"

CRITERION="the delay log's shape: at least 60 samples in a minute, four columns each, plus the bounds and start fill"
LOG="${1:-${TMPDIR:-/tmp}/chorus-delay-log-shape.log}"

build_once
require_pacing_audio_device "$CRITERION"

DEVICE="$(audio_device)"
PORT="$(free_port)"
CONTRACT_ARGS="$(server_contract_args)"

say "chorus: one-minute delay-log shape run"
say "  device: $DEVICE"
say "  log:    $LOG"

"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    $CONTRACT_ARGS ${CHORUS_SERVER_EXTRA_ARGS:-} &
SERVER_PID=$!
trap 'kill_quietly "$SERVER_PID"' EXIT

sleep 1

set +e
"$BIN_DIR/chorus-client" \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --min-us "$(conf min_us)" \
    --max-us "$(conf max_us)" \
    --start-fill-us "$(conf start_fill_us)" \
    --device-target-us "$(conf device_target_us)" \
    --overflow-skew-ppm "$(conf ac9_rate_skew_ppm)" \
    --delay-log "$LOG" \
    --run-seconds 60
CLIENT_STATUS=$?
set -e
say "chorus: client exited $CLIENT_STATUS"
[ "$CLIENT_STATUS" -eq 0 ] || exit "$CLIENT_STATUS"

SAMPLES="$(grep -c '^sample ' "$LOG" || true)"
say "chorus: $SAMPLES samples in the log"
if [ "$SAMPLES" -lt 60 ]; then
    say "chorus: FAIL a one-minute run has to leave at least 60 samples, it left $SAMPLES"
    exit 1
fi
if ! grep -q '^config .*min_us=.*max_us=.*start_fill_us=' "$LOG"; then
    say "chorus: FAIL the log has no config record carrying the bounds and start fill"
    exit 1
fi
if ! grep -q '^event .*kind=start-fill' "$LOG"; then
    say "chorus: FAIL the log does not name the fill it started at"
    exit 1
fi
BAD="$(grep '^sample ' "$LOG" \
    | grep -vc 'mono_us=.*delay_us=.*occupancy_us=.*graded=' || true)"
if [ "$BAD" -ne 0 ]; then
    say "chorus: FAIL $BAD samples are missing one of the four columns"
    exit 1
fi

"$BIN_DIR/chorus-delaylog-check" "$LOG" --min-graded-seconds 30
