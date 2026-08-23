#!/usr/bin/env bash
# The over-rate run: feed the client faster than its device consumes, until the
# buffer reaches its ceiling.
#
# Verifies: the drift toward a bound is reported; the crossing at the maximum
# is reported once per crossing; whole arriving chunks are discarded rather
# than enqueued while occupancy is at or past the maximum; the overflow count
# is its own; playback continues; the configuration on disk is unchanged; no
# bound moved; and the played-out sample count still matches the device's
# nominal rate, so nothing resampled or retimed anything.
#
# The rate difference applied is config/verification.conf's ac9_rate_skew_ppm,
# which is the same number the client's own bound relations are checked
# against. 240000 us of span at 2000 ppm crosses in 120 s, inside one
# ten-minute run.
#
# Prerequisite: a usable ALSA playback device.
#
#   ./tools/overflow-run.sh [output-log]

source "$(dirname "$0")/lib.sh"

CRITERION="the behaviour at both configured bounds, and that no rate change happens in response"
LOG="${1:-${TMPDIR:-/tmp}/chorus-overflow-run.log}"

build_once
require_pacing_audio_device "$CRITERION"

DEVICE="$(audio_device)"
PORT="$(free_port)"
SKEW="$(conf ac9_rate_skew_ppm)"
SPAN_US=$(( $(conf max_us) - $(conf min_us) ))
SECONDS_TO_CROSS=$(( SPAN_US / SKEW ))
RUN_SECONDS=$(( SECONDS_TO_CROSS * 2 + 30 ))
CONTRACT_ARGS="$(server_contract_args)"

CONFIG_BEFORE="$(sha256sum "$REPO_ROOT/config/verification.conf" | cut -d' ' -f1)"

say "chorus: over-rate run"
say "  device:     $DEVICE"
say "  skew:       ${SKEW} ppm"
say "  span:       ${SPAN_US} us, crossed in ${SECONDS_TO_CROSS}s"
say "  run:        ${RUN_SECONDS}s"
say "  log:        $LOG"

"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    --rate-skew-ppm "$SKEW" \
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
    --overflow-skew-ppm "$SKEW" \
    --delay-log "$LOG" \
    --run-seconds "$RUN_SECONDS" | tee "${LOG}.stdout"
CLIENT_STATUS="${PIPESTATUS[0]}"
set -e
say "chorus: client exited $CLIENT_STATUS"
[ "$CLIENT_STATUS" -eq 0 ] || exit "$CLIENT_STATUS"

if ! grep -q '^event .*kind=bound-crossing .*bound=maximum' "$LOG"; then
    say "chorus: FAIL the crossing at the maximum was never reported"
    exit 1
fi
CROSSINGS="$(grep -c '^event .*kind=bound-crossing' "$LOG" || true)"
say "chorus: $CROSSINGS crossings reported"

OVERFLOW="$(sed -n 's/.*discarded_overflow=\([0-9]*\).*/\1/p' "${LOG}.stdout" | tail -n 1)"
if [ -z "$OVERFLOW" ] || [ "$OVERFLOW" -eq 0 ]; then
    say "chorus: FAIL nothing was discarded at the ceiling"
    exit 1
fi
say "chorus: $OVERFLOW chunks discarded at the ceiling"

CONFIG_AFTER="$(sha256sum "$REPO_ROOT/config/verification.conf" | cut -d' ' -f1)"
if [ "$CONFIG_BEFORE" != "$CONFIG_AFTER" ]; then
    say "chorus: FAIL the configuration on disk changed during the run"
    exit 1
fi
say "chorus: the configuration on disk is unchanged"

"$BIN_DIR/chorus-delaylog-check" "$LOG" --require-no-rate-change
