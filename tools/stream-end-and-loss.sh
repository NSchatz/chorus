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

source "$(dirname "$0")/lib.sh"

CRITERION="a clean end is signalled in band and exits zero; a lost server is reported as the other case and exits non-zero; neither drain counts as underruns"

build_once
require_audio_device "$CRITERION"

DEVICE="$(audio_device)"
CONTRACT_ARGS="$(server_contract_args)"
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

# --- a source that ends cleanly ---------------------------------------------
say "chorus: a source that ends cleanly"
PORT="$(free_port)"
LOG="${TMPDIR:-/tmp}/chorus-clean-end.log"
# 2010 ms of tone at 20 ms a chunk is 100 whole chunks and a 10 ms final one.
"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --tone-ms 2010 \
    --chunk-us "$(conf chunk_us)" \
    $CONTRACT_ARGS ${CHORUS_SERVER_EXTRA_ARGS:-} > "${LOG}.server" 2>&1 &
SERVER_PID=$!
sleep 1

set +e
CLIENT_OUT="$("$BIN_DIR/chorus-client" \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --delay-log "$LOG" 2>&1)"
CLIENT_STATUS=$?
set -e
wait "$SERVER_PID" 2>/dev/null || true
SERVER_STATUS=$?
printf '%s\n' "$CLIENT_OUT"
cat "${LOG}.server"

check "clean-end-client-exits-zero" \
    "$([ "$CLIENT_STATUS" -eq 0 ] && echo 1 || echo 0)" "exit $CLIENT_STATUS"
check "clean-end-server-says-it-ended-cleanly" \
    "$(grep -q 'ended_cleanly=1' "${LOG}.server" && echo 1 || echo 0)" \
    "$(grep 'stream done' "${LOG}.server" | head -n 1)"
check "clean-end-is-reported-as-the-in-band-signal" \
    "$(printf '%s' "$CLIENT_OUT" | grep -q 'reason=end-of-stream' && echo 1 || echo 0)" \
    "$(printf '%s' "$CLIENT_OUT" | grep 'stopped reason' | head -n 1)"
check "clean-end-says-the-signal-was-in-band" \
    "$(printf '%s' "$CLIENT_OUT" | grep -q 'in-band end-of-stream signal' && echo 1 || echo 0)" \
    "$(printf '%s' "$CLIENT_OUT" | grep 'in-band' | head -n 1)"
check "clean-end-played-audio" \
    "$(printf '%s' "$CLIENT_OUT" | grep -q 'played=1' && echo 1 || echo 0)" \
    "$(printf '%s' "$CLIENT_OUT" | grep 'stopped reason' | head -n 1)"
check "clean-end-counts-no-underruns-for-the-drain" \
    "$(printf '%s' "$CLIENT_OUT" | grep -q 'underruns=0' && echo 1 || echo 0)" \
    "$(printf '%s' "$CLIENT_OUT" | grep 'underruns=' | head -n 1)"

# Every frame the server sent reached the device, short final chunk included.
SENT="$(sed -n 's/.*frames_sent=\([0-9]*\).*/\1/p' "${LOG}.server" | tail -n 1)"
WRITTEN="$(sed -n 's/.*frames_written=\([0-9]*\).*/\1/p' "$LOG" | tail -n 1)"
check "clean-end-plays-out-everything-including-the-short-final-chunk" \
    "$([ "${SENT:-0}" = "${WRITTEN:-1}" ] && echo 1 || echo 0)" \
    "the server sent ${SENT:-?} frames and the client wrote ${WRITTEN:-?}"

# --- a server killed mid-run ------------------------------------------------
say ""
say "chorus: a server killed mid-run"
PORT="$(free_port)"
LOSS_LOG="${TMPDIR:-/tmp}/chorus-server-loss.log"
"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --chunk-us "$(conf chunk_us)" \
    $CONTRACT_ARGS ${CHORUS_SERVER_EXTRA_ARGS:-} > "${LOSS_LOG}.server" 2>&1 &
SERVER_PID=$!
sleep 1

( sleep 4; kill -9 "$SERVER_PID" 2>/dev/null || true ) &
KILLER_PID=$!

set +e
LOSS_OUT="$("$BIN_DIR/chorus-client" \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --delay-log "$LOSS_LOG" \
    --run-seconds 30 2>&1)"
LOSS_STATUS=$?
set -e
wait "$KILLER_PID" 2>/dev/null || true
printf '%s\n' "$LOSS_OUT"

check "lost-server-exits-non-zero" \
    "$([ "$LOSS_STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $LOSS_STATUS"
check "lost-server-is-reported-as-the-other-case" \
    "$(printf '%s' "$LOSS_OUT" | grep -q 'reason=connection-lost' && echo 1 || echo 0)" \
    "$(printf '%s' "$LOSS_OUT" | grep 'stopped reason' | head -n 1)"
check "lost-server-says-there-was-no-end-of-stream-signal" \
    "$(printf '%s' "$LOSS_OUT" | grep -q 'no end-of-stream signal' && echo 1 || echo 0)" \
    "$(printf '%s' "$LOSS_OUT" | grep 'no end-of-stream' | head -n 1)"
check "lost-server-played-what-it-held" \
    "$(printf '%s' "$LOSS_OUT" | grep -q 'played=1' && echo 1 || echo 0)" \
    "$(printf '%s' "$LOSS_OUT" | grep 'stopped reason' | head -n 1)"
check "lost-server-counts-no-underruns-for-the-deliberate-play-out" \
    "$(printf '%s' "$LOSS_OUT" | grep -q 'underruns=0' && echo 1 || echo 0)" \
    "$(printf '%s' "$LOSS_OUT" | grep 'underruns=' | head -n 1)"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: the two ends are told apart by the signal, and neither drain is an underrun"
    exit 0
fi
say "chorus: $FAILURES checks failed"
exit 1
