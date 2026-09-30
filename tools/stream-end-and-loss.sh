#!/usr/bin/env bash
# The two ways a stream stops, told apart by the signal and never by the
# timing.
#
# Verifies, through the real binaries on a real socket and a real ALSA device:
#
#   - a source that ends cleanly makes the server send an end-of-stream signal
#     IN BAND, as data on the connection, after the final chunk and before the
#     close; the client plays out what it holds including a short final chunk,
#     exits zero, and counts no underruns for the drain;
#   - a server killed mid-run makes the client report the unannounced case,
#     distinguish it from the first, play out what it already holds, and exit
#     non-zero, again counting no underruns for the drain.
#
# Prerequisite: an ALSA playback device that can be opened. The ALSA `null`
# device is enough here, because nothing below is graded on the delay it
# reports.
#
#   ./tools/stream-end-and-loss.sh
#
# THE BENCH REPORT (K45). On a real device (CHORUS_CLIENT_DEVICE is not the
# ALSA `null` device) the run ends by writing
# docs/measurements/sound2-stream-end-and-loss-<date>.md with both runs' client
# output, server output and delay logs and the exit statuses hashed
# (tools/bench/lib.sh), and with CHORUS_BENCH_PR=1 commits that on
# bench/<date>-sound2-stream-end-and-loss and opens the PR. A failing run
# writes its report too (Result: FAIL) and still exits non-zero. The `null`
# runs (`make verify-null-device`, `make verify-alsa-null`) write no bench
# report: they are host evidence, not hardware. The report half alone:
#   ./tools/stream-end-and-loss.sh --report-from <run directory>

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"
set -- "${BENCH_ARGS[@]+"${BENCH_ARGS[@]}"}"

CRITERION="a clean end is signalled in band and exits zero; a lost server is reported as the other case and exits non-zero; neither drain counts as underruns"
TOPIC=sound2-stream-end-and-loss
FAILURES=0

check() {
    local name="$1" ok="$2" detail="$3"
    if [ "$ok" = "1" ]; then
        say "pass $name: $detail"
    else
        say "FAIL $name: $detail"
        FAILURES=$(( FAILURES + 1 ))
    fi
}

# One key of a run's status file.
run_status() {
    sed -n "s/^$2 = //p" "$1" 2>/dev/null | head -n 1
}

# Grade both runs from their directory: clean-end.{client,server,log},
# lost-server.{client,server,log} and run-status.txt. Prints every check and
# returns non-zero when any failed, as the inline grading always did.
grade() {
    local dir="$1" client_out status sent written
    FAILURES=0

    client_out="$(cat "$dir/clean-end.client" 2>/dev/null || true)"
    status="$(run_status "$dir/run-status.txt" clean_end_client_exit)"
    status="${status:-255}"
    say "chorus: grading the clean end"
    check "clean-end-client-exits-zero" \
        "$([ "$status" -eq 0 ] && echo 1 || echo 0)" "exit $status"
    check "clean-end-server-says-it-ended-cleanly" \
        "$(grep -q 'ended_cleanly=1' "$dir/clean-end.server" 2>/dev/null && echo 1 || echo 0)" \
        "$(grep 'stream done' "$dir/clean-end.server" 2>/dev/null | head -n 1)"
    check "clean-end-is-reported-as-the-in-band-signal" \
        "$(printf '%s' "$client_out" | grep -q 'reason=end-of-stream' && echo 1 || echo 0)" \
        "$(printf '%s' "$client_out" | grep 'stopped reason' | head -n 1)"
    check "clean-end-says-the-signal-was-in-band" \
        "$(printf '%s' "$client_out" | grep -q 'in-band end-of-stream signal' && echo 1 || echo 0)" \
        "$(printf '%s' "$client_out" | grep 'in-band' | head -n 1)"
    check "clean-end-played-audio" \
        "$(printf '%s' "$client_out" | grep -q 'played=1' && echo 1 || echo 0)" \
        "$(printf '%s' "$client_out" | grep 'stopped reason' | head -n 1)"
    check "clean-end-counts-no-underruns-for-the-drain" \
        "$(printf '%s' "$client_out" | grep -q 'underruns=0' && echo 1 || echo 0)" \
        "$(printf '%s' "$client_out" | grep 'underruns=' | head -n 1)"

    # Every frame the server sent reached the device, short final chunk included.
    sent="$(sed -n 's/.*frames_sent=\([0-9]*\).*/\1/p' "$dir/clean-end.server" 2>/dev/null | tail -n 1)"
    written="$(sed -n 's/.*frames_written=\([0-9]*\).*/\1/p' "$dir/clean-end.log" 2>/dev/null | tail -n 1)"
    check "clean-end-plays-out-everything-including-the-short-final-chunk" \
        "$([ "${sent:-0}" = "${written:-1}" ] && echo 1 || echo 0)" \
        "the server sent ${sent:-?} frames and the client wrote ${written:-?}"

    client_out="$(cat "$dir/lost-server.client" 2>/dev/null || true)"
    status="$(run_status "$dir/run-status.txt" lost_server_client_exit)"
    status="${status:-0}"
    say ""
    say "chorus: grading the lost server"
    check "lost-server-exits-non-zero" \
        "$([ "$status" -ne 0 ] && echo 1 || echo 0)" "exit $status"
    check "lost-server-is-reported-as-the-other-case" \
        "$(printf '%s' "$client_out" | grep -q 'reason=connection-lost' && echo 1 || echo 0)" \
        "$(printf '%s' "$client_out" | grep 'stopped reason' | head -n 1)"
    check "lost-server-says-there-was-no-end-of-stream-signal" \
        "$(printf '%s' "$client_out" | grep -q 'no end-of-stream signal' && echo 1 || echo 0)" \
        "$(printf '%s' "$client_out" | grep 'no end-of-stream' | head -n 1)"
    check "lost-server-played-what-it-held" \
        "$(printf '%s' "$client_out" | grep -q 'played=1' && echo 1 || echo 0)" \
        "$(printf '%s' "$client_out" | grep 'stopped reason' | head -n 1)"
    check "lost-server-counts-no-underruns-for-the-deliberate-play-out" \
        "$(printf '%s' "$client_out" | grep -q 'underruns=0' && echo 1 || echo 0)" \
        "$(printf '%s' "$client_out" | grep 'underruns=' | head -n 1)"

    say ""
    if [ "$FAILURES" -eq 0 ]; then
        say "chorus: the two ends are told apart by the signal, and neither drain is an underrun"
        return 0
    fi
    say "chorus: $FAILURES checks failed"
    return 1
}

# The report and PR half: the raw files in, the bench report out.
report_and_publish() {
    local raw="$BENCH_RUN_DIR/raw" out="$BENCH_RUN_DIR/analysis/grade.txt" ok=1
    bench_analysis grade grade "$raw" && ok=0
    bench_field checks_failed "$(grep -c '^FAIL ' "$out" || true)"
    bench_field clean_end_exit "$(run_status "$raw/run-status.txt" clean_end_client_exit)"
    bench_field lost_server_exit "$(run_status "$raw/run-status.txt" lost_server_client_exit)"
    bench_field frames_sent "$(sed -n 's/.*frames_sent=\([0-9]*\).*/\1/p' "$raw/clean-end.server" 2>/dev/null | tail -n 1)"
    bench_field frames_written "$(sed -n 's/.*frames_written=\([0-9]*\).*/\1/p' "$raw/clean-end.log" 2>/dev/null | tail -n 1)"
    local summary
    summary="$([ "$ok" = 0 ] && echo "a clean end and a lost server were told apart by the signal, and neither drain was an underrun" || echo "$(grep -c '^FAIL ' "$out" || true) stream-end or server-loss checks failed")"
    [ "$ok" = 0 ] && bench_finish PASS "$summary"
    bench_finish FAIL "$summary"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" "$TOPIC"
    report_and_publish
fi

build_once
require_audio_device "$CRITERION"

DEVICE="$(audio_device)"
BENCH_ON=0
if [ "$DEVICE" != null ]; then
    # A device that is not ALSA `null` is real hardware: this run is a bench run.
    BENCH_ON=1
    bench_begin "$TOPIC" "$CRITERION"
    bench_device "playback: ALSA $DEVICE"
    bench_reproduce "CHORUS_CLIENT_DEVICE=$DEVICE tools/stream-end-and-loss.sh"
    RUN_DIR="$BENCH_RUN_DIR/raw"
else
    RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/chorus-stream-end-and-loss-XXXXXX")"
fi
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
read -r -a SERVER_EXTRA_ARGS <<< "${CHORUS_SERVER_EXTRA_ARGS:-}"

# --- a source that ends cleanly ---------------------------------------------
say "chorus: a source that ends cleanly"
PORT="$(free_port)"
# 2010 ms of tone at 20 ms a chunk is 100 whole chunks and a 10 ms final one.
"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --tone-ms 2010 \
    --chunk-us "$(conf chunk_us)" \
    "${CONTRACT_ARGS[@]}" "${SERVER_EXTRA_ARGS[@]}" > "$RUN_DIR/clean-end.server" 2>&1 &
SERVER_PID=$!
sleep 1

set +e
"$BIN_DIR/chorus-client" \
    --ephemeral-identity \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --delay-log "$RUN_DIR/clean-end.log" > "$RUN_DIR/clean-end.client" 2>&1
CLIENT_STATUS=$?
set -e
wait "$SERVER_PID" 2>/dev/null || true
cat "$RUN_DIR/clean-end.client" "$RUN_DIR/clean-end.server"

# --- a server killed mid-run ------------------------------------------------
say ""
say "chorus: a server killed mid-run"
PORT="$(free_port)"
"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --chunk-us "$(conf chunk_us)" \
    "${CONTRACT_ARGS[@]}" "${SERVER_EXTRA_ARGS[@]}" > "$RUN_DIR/lost-server.server" 2>&1 &
SERVER_PID=$!
sleep 1

( sleep 4; kill -9 "$SERVER_PID" 2>/dev/null || true ) &
KILLER_PID=$!

set +e
"$BIN_DIR/chorus-client" \
    --ephemeral-identity \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --delay-log "$RUN_DIR/lost-server.log" \
    --run-seconds 30 > "$RUN_DIR/lost-server.client" 2>&1
LOSS_STATUS=$?
set -e
wait "$KILLER_PID" 2>/dev/null || true
cat "$RUN_DIR/lost-server.client"

{
    printf 'clean_end_client_exit = %s\n' "$CLIENT_STATUS"
    printf 'lost_server_client_exit = %s\n' "$LOSS_STATUS"
} > "$RUN_DIR/run-status.txt"

say ""
if [ "$BENCH_ON" = 1 ]; then
    report_and_publish
fi
grade "$RUN_DIR"
