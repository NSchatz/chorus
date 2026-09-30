#!/usr/bin/env bash
# A one-minute run, graded the way the ten-minute run is graded, to check the
# shape of the record and the delay it carries rather than the length of the
# run.
#
# Verifies: the client records, at an interval no coarser than once per second
# and in a form a script can parse without the client running, the
# device-reported delay, the buffer occupancy, a value from its own monotonic
# timeline, and whether each sample is inside the graded interval; and records
# the configured bounds and start fill in the same file. Also verifies that
# output is withheld until the configured start fill and that the log names the
# fill it started at, and that the three bound relations hold. It then grades
# the log with `chorus-delaylog-check --min-graded-seconds 30`, which asserts
# the reported delay is inside the configured bounds for the whole graded
# interval.
#
# Prerequisite: an ALSA playback device that REPORTS A DELAY - a real card or a
# snd-aloop loopback. The ALSA `null` device is not one: it accepts every frame
# instantly and reports a delay of zero, which the grading above could only
# fail. That is why the guard below is require_pacing_audio_device and not
# require_audio_device, and it is stricter than "any usable ALSA playback
# device" sounds.
#
# On a device that merely opens, `null` included, run
# tools/start-fill-and-log-shape.sh instead: it grades the start fill, the
# log's shape and the bound relations, and grades nothing that rests on the
# delay a device reports.
#
#   ./tools/delay-log-shape.sh [output-log]
#
# THE BENCH REPORT (K45). The device this needs is one that paces, which is
# hardware (or a loopback), so every run that gets past the guard is a bench
# run: it ends by writing docs/measurements/sound2-delay-log-shape-<date>.md
# with the delay log and the client's exit status hashed (tools/bench/lib.sh),
# and with CHORUS_BENCH_PR=1 commits that on bench/<date>-sound2-delay-log-shape
# and opens the PR. A failing run writes its report too (Result: FAIL) and
# still exits non-zero. The report half alone, from a run directory:
#   ./tools/delay-log-shape.sh --report-from <run directory>

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"
set -- "${BENCH_ARGS[@]+"${BENCH_ARGS[@]}"}"

CRITERION="the delay log's shape and the delay it carries: at least 60 samples in a minute, four columns each, the bounds and start fill, and the reported delay inside those bounds for the whole graded interval"
TOPIC=sound2-delay-log-shape

# Grade one run: <log> <client exit status>. Prints what it found and returns
# non-zero on the first failure, as the inline grading always did.
grade() {
    local log="$1" client_status="$2"
    if [ "$client_status" -ne 0 ]; then
        say "chorus: FAIL the client exited $client_status"
        return 1
    fi
    local samples bad
    samples="$(grep -c '^sample ' "$log" || true)"
    say "chorus: $samples samples in the log"
    if [ "$samples" -lt 60 ]; then
        say "chorus: FAIL a one-minute run has to leave at least 60 samples, it left $samples"
        return 1
    fi
    if ! grep -q '^config .*min_us=.*max_us=.*start_fill_us=' "$log"; then
        say "chorus: FAIL the log has no config record carrying the bounds and start fill"
        return 1
    fi
    if ! grep -q '^event .*kind=start-fill' "$log"; then
        say "chorus: FAIL the log does not name the fill it started at"
        return 1
    fi
    bad="$(grep '^sample ' "$log" \
        | grep -vc 'mono_us=.*delay_us=.*occupancy_us=.*graded=' || true)"
    if [ "$bad" -ne 0 ]; then
        say "chorus: FAIL $bad samples are missing one of the four columns"
        return 1
    fi
    "$BIN_DIR/chorus-delaylog-check" "$log" --min-graded-seconds 30
}

# The report and PR half: the raw files in, the bench report out.
report_and_publish() {
    local log="$BENCH_RUN_DIR/raw/delay-log-shape.log" status ok=1
    status="$(sed -n 's/^client_exit = //p' "$BENCH_RUN_DIR/raw/run-status.txt" 2>/dev/null | head -n 1)"
    bench_analysis grade grade "$log" "${status:-255}" && ok=0
    bench_field samples "$(grep -c '^sample ' "$log" 2>/dev/null || echo 0)"
    bench_field client_exit "${status:-not recorded}"
    bench_field graded_seconds "$(bench_delaylog_value "$log" graded_span_us \
        | awk '/^[0-9]+$/ { printf "%.1f", $1 / 1e6; next } { print "none" }')"
    bench_field delay_min_us "$(bench_delaylog_value "$log" delay_min_us)"
    bench_field delay_max_us "$(bench_delaylog_value "$log" delay_max_us)"
    bench_field delaylog "$([ "$ok" = 0 ] && echo graded clean || echo failed its grading)"
    local summary
    summary="the one-minute delay log $([ "$ok" = 0 ] && echo graded clean || echo failed its grading) (delay $(bench_delaylog_value "$log" delay_min_us) to $(bench_delaylog_value "$log" delay_max_us) us over at least 30 s graded)"
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
bench_reproduce "CHORUS_CLIENT_DEVICE=$(audio_device) tools/delay-log-shape.sh"
LOG="${LOG:-$BENCH_RUN_DIR/raw/delay-log-shape.log}"
DEVICE="$(audio_device)"
PORT="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
read -r -a SERVER_EXTRA_ARGS <<< "${CHORUS_SERVER_EXTRA_ARGS:-}"

say "chorus: one-minute delay-log shape run"
say "  device: $DEVICE"
say "  log:    $LOG"

"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
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
    --overflow-skew-ppm "$(conf ac9_rate_skew_ppm)" \
    --delay-log "$LOG" \
    --run-seconds 60
CLIENT_STATUS=$?
set -e
say "chorus: client exited $CLIENT_STATUS"

if [ -f "$LOG" ] && [ "$LOG" != "$BENCH_RUN_DIR/raw/delay-log-shape.log" ]; then
    cp "$LOG" "$BENCH_RUN_DIR/raw/delay-log-shape.log"
fi
printf 'client_exit = %s\n' "$CLIENT_STATUS" > "$BENCH_RUN_DIR/raw/run-status.txt"
report_and_publish
