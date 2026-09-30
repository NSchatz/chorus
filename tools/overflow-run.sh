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
#
# THE BENCH REPORT (K45). The device this needs is one that paces, which is
# hardware (or a loopback), so every run that gets past the guard is a bench
# run: it ends by writing docs/measurements/sound2-overflow-<date>.md with the
# delay log, the client's output and the run's status hashed
# (tools/bench/lib.sh), and with CHORUS_BENCH_PR=1 commits that on
# bench/<date>-sound2-overflow and opens the PR. A failing run writes its
# report too (Result: FAIL) and still exits non-zero. The report half alone:
#   ./tools/overflow-run.sh --report-from <run directory>

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"
set -- "${BENCH_ARGS[@]+"${BENCH_ARGS[@]}"}"

CRITERION="the behaviour at both configured bounds, and that no rate change happens in response"
TOPIC=sound2-overflow

# One key of the run's status file, raw/run-status.txt.
run_status() {
    sed -n "s/^$1 = //p" "$BENCH_RUN_DIR/raw/run-status.txt" 2>/dev/null | head -n 1
}

# Grade one run from its raw files. Prints what it found and returns non-zero
# on the first failure, as the inline grading always did.
# shellcheck disable=SC2329 # run by bench_analysis, which shellcheck cannot follow
grade() {
    local log="$BENCH_RUN_DIR/raw/overflow-run.log" stdout="$BENCH_RUN_DIR/raw/client.stdout"
    local client_status crossings overflow
    client_status="$(run_status client_exit)"
    if [ "${client_status:-255}" -ne 0 ]; then
        say "chorus: FAIL the client exited ${client_status:-without a recorded status}"
        return 1
    fi
    if ! grep -q '^event .*kind=bound-crossing .*bound=maximum' "$log"; then
        say "chorus: FAIL the crossing at the maximum was never reported"
        return 1
    fi
    crossings="$(grep -c '^event .*kind=bound-crossing' "$log" || true)"
    say "chorus: $crossings crossings reported"
    overflow="$(sed -n 's/.*discarded_overflow=\([0-9]*\).*/\1/p' "$stdout" | tail -n 1)"
    if [ -z "$overflow" ] || [ "$overflow" -eq 0 ]; then
        say "chorus: FAIL nothing was discarded at the ceiling"
        return 1
    fi
    say "chorus: $overflow chunks discarded at the ceiling"
    if [ "$(run_status config_sha256_before)" != "$(run_status config_sha256_after)" ] \
        || [ -z "$(run_status config_sha256_before)" ]; then
        say "chorus: FAIL the configuration on disk changed during the run"
        return 1
    fi
    say "chorus: the configuration on disk is unchanged"
    "$BIN_DIR/chorus-delaylog-check" "$log" --require-no-rate-change
}

# The report and PR half: the raw files in, the bench report out.
report_and_publish() {
    local log="$BENCH_RUN_DIR/raw/overflow-run.log" ok=1
    bench_analysis grade grade && ok=0
    bench_field client_exit "$(run_status client_exit)"
    bench_field skew_ppm "$(run_status skew_ppm)"
    bench_field crossings "$(grep -c '^event .*kind=bound-crossing' "$log" 2>/dev/null || echo 0)"
    bench_field discarded_overflow "$(sed -n 's/.*discarded_overflow=\([0-9]*\).*/\1/p' \
        "$BENCH_RUN_DIR/raw/client.stdout" 2>/dev/null | tail -n 1)"
    bench_field config_unchanged "$([ -n "$(run_status config_sha256_before)" ] \
        && [ "$(run_status config_sha256_before)" = "$(run_status config_sha256_after)" ] && echo yes || echo no)"
    bench_field delaylog "$([ "$ok" = 0 ] && echo graded clean || echo failed its grading)"
    local summary
    summary="the over-rate run $([ "$ok" = 0 ] && echo "crossed the maximum, discarded at the ceiling and changed no rate" || echo "failed its grading") ($(grep -c '^event .*kind=bound-crossing' "$log" 2>/dev/null || echo 0) crossings at $(run_status skew_ppm) ppm)"
    [ "$ok" = 0 ] && bench_finish PASS "$summary"
    bench_finish FAIL "$summary"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" "$TOPIC"
    report_and_publish
fi

LOG="${1:-}"

build_once
require_pacing_audio_device "$CRITERION"
# A device that paces is real hardware (or a loopback): this run is a bench run.
bench_begin "$TOPIC" "$CRITERION"
bench_device "playback: ALSA $(audio_device)"
bench_reproduce "CHORUS_CLIENT_DEVICE=$(audio_device) tools/overflow-run.sh"
LOG="${LOG:-$BENCH_RUN_DIR/raw/overflow-run.log}"
DEVICE="$(audio_device)"
PORT="$(free_port)"
SKEW="$(conf ac9_rate_skew_ppm)"
SPAN_US=$(( $(conf max_us) - $(conf min_us) ))
SECONDS_TO_CROSS=$(( SPAN_US / SKEW ))
RUN_SECONDS=$(( SECONDS_TO_CROSS * 2 + 30 ))
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
read -r -a SERVER_EXTRA_ARGS <<< "${CHORUS_SERVER_EXTRA_ARGS:-}"

CONFIG_BEFORE="$(sha256sum "$REPO_ROOT/config/verification.conf" | cut -d' ' -f1)"

say "chorus: over-rate run"
say "  device:     $DEVICE"
say "  skew:       ${SKEW} ppm"
say "  span:       ${SPAN_US} us, crossed in ${SECONDS_TO_CROSS}s"
say "  run:        ${RUN_SECONDS}s"
say "  log:        $LOG"

"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    --rate-skew-ppm "$SKEW" \
    "${CONTRACT_ARGS[@]}" "${SERVER_EXTRA_ARGS[@]}" &
SERVER_PID=$!
trap 'kill_quietly "$SERVER_PID"' EXIT

sleep 1

set +e
"$BIN_DIR/chorus-client" \
    --ephemeral-identity \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --min-us "$(conf min_us)" \
    --max-us "$(conf max_us)" \
    --start-fill-us "$(conf start_fill_us)" \
    --device-target-us "$(conf device_target_us)" \
    --overflow-skew-ppm "$SKEW" \
    --delay-log "$LOG" \
    --run-seconds "$RUN_SECONDS" | tee "$BENCH_RUN_DIR/raw/client.stdout"
CLIENT_STATUS="${PIPESTATUS[0]}"
set -e
say "chorus: client exited $CLIENT_STATUS"

CONFIG_AFTER="$(sha256sum "$REPO_ROOT/config/verification.conf" | cut -d' ' -f1)"
if [ -f "$LOG" ] && [ "$LOG" != "$BENCH_RUN_DIR/raw/overflow-run.log" ]; then
    cp "$LOG" "$BENCH_RUN_DIR/raw/overflow-run.log"
fi
{
    printf 'client_exit = %s\n' "$CLIENT_STATUS"
    printf 'skew_ppm = %s\n' "$SKEW"
    printf 'config_sha256_before = %s\n' "$CONFIG_BEFORE"
    printf 'config_sha256_after = %s\n' "$CONFIG_AFTER"
} > "$BENCH_RUN_DIR/raw/run-status.txt"
report_and_publish
