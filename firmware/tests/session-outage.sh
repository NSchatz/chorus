#!/usr/bin/env bash
# AC-4, graded over a real socket against a real server process.
#
# "WHEN the server or the network disappears and returns THE SYSTEM SHALL
# rejoin and resume playback with no human action."
#
# Nothing here is a mock. `chorus-server` is started for real, the endpoint
# joins it over loopback TCP and decodes the committed protocol off the wire,
# the server is killed with SIGKILL, a NEW server process is started on the
# same port, and the endpoint has to come back by itself. No signal, no
# restart, no operator: the only thing that touches the endpoint is the passage
# of time.
#
# Three shapes, because they fail differently:
#
#   1. An outage SHORTER THAN ONE CHUNK. The chunk duration is a server option,
#      and this run uses the longest one a 48 kHz stereo 16-bit stream can
#      carry inside the protocol's 65535-byte payload (320 ms), so that a real
#      process restart genuinely fits inside one chunk. The outage is MEASURED
#      by the endpoint's own monotonic clock and compared against that
#      duration; it is not assumed.
#   2. An outage OF MINUTES. Real minutes of real wall clock against a real
#      dead server, because the property being graded is that there is no retry
#      budget to exhaust, and a modelled minute cannot exhaust one. The length
#      is committed in firmware/config/endpoint.conf as outage_minutes_seconds.
#   3. An outage long enough that the endpoint's connect attempts FAIL several
#      times before a new server process answers, so the path being graded is
#      the connect-refused loop and not only the peer-closed one, and the audio
#      resumes on a connection to a process that did not exist when the
#      endpoint started.
#
# Runs anywhere: no audio device, no privilege, loopback only.
#
#   bash firmware/tests/session-outage.sh      # or: make firmware-check

set -euo pipefail

FW="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$FW/../tools/lib.sh"

BUILD="${BUILD:-$FW/build}"
ENDPOINT="$BUILD/chorus-endpoint-session"
CONF="$FW/config/endpoint.conf"

endpoint_conf() {
    local key="$1"
    local value
    value="$(sed -n "s/^[[:space:]]*${key}[[:space:]]*=[[:space:]]*\\([^#]*\\).*/\\1/p" \
        "$CONF" | head -n 1 | tr -d '[:space:]')"
    if [ -z "$value" ]; then
        printf '%s has no %s\n' "$CONF" "$key" >&2
        exit 2
    fi
    printf '%s' "$value"
}

build_once

if [ ! -x "$ENDPOINT" ]; then
    say "FAIL $ENDPOINT has not been built"
    exit 1
fi

FAILURES=0
check() {
    local name="$1"
    local ok="$2"
    local detail="$3"
    if [ "$ok" = "1" ]; then
        say "pass $name: $detail"
    else
        say "FAIL $name: $detail"
        FAILURES=$(( FAILURES + 1 ))
    fi
}

SERVER_PID=""
start_server() {
    local port="$1"
    local chunk_us="$2"
    local rate="$3"
    local channels="$4"
    "$BIN_DIR/chorus-server" \
        --listen "127.0.0.1:$port" \
        --source tone \
        --rate "$rate" \
        --channels "$channels" \
        --format "$(conf sample_format)" \
        --chunk-us "$chunk_us" \
        --rttime-us "$(conf rttime_us)" \
        --rt-priority "$(conf rt_priority)" \
        --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
        $CONTRACT_ARGS >/dev/null 2>&1 &
    SERVER_PID=$!
}

kill_server_hard() {
    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill -9 "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    SERVER_PID=""
}

CONTRACT_ARGS="$(server_contract_args)"
OUT_DIR="${TMPDIR:-/tmp}"

field() {
    # One `key=value` field out of the endpoint's summary lines.
    sed -n "s/.*[[:space:]]$1=\\([^ ]*\\).*/\\1/p" "$2" | tail -n 1
}

# --- 1. an outage shorter than one chunk --------------------------------------

say ""
say "chorus: outage 1 of 3, shorter than one chunk"

CHUNK_US=320000
PORT="$(free_port)"
LOG="$OUT_DIR/chorus-endpoint-outage-short.log"
SUMMARY="$OUT_DIR/chorus-endpoint-outage-short.summary"

start_server "$PORT" "$CHUNK_US" 48000 2
sleep 1
"$ENDPOINT" --server "127.0.0.1:$PORT" --run-seconds 14 --log "$LOG" \
    >"$SUMMARY" 2>&1 &
ENDPOINT_PID=$!
trap 'kill -9 "$ENDPOINT_PID" 2>/dev/null || true; kill_server_hard' EXIT

sleep 4
say "chorus: killing the server (SIGKILL) and starting a new one on the same port"
kill_server_hard
start_server "$PORT" "$CHUNK_US" 48000 2

wait "$ENDPOINT_PID"
trap 'kill_server_hard' EXIT
kill_server_hard
cat "$SUMMARY"

REJOINS="$(field rejoins "$SUMMARY")"
PLAYED_ON="$(field connections_that_played "$SUMMARY")"
OUTAGE_NS="$(field longest_outage_ns "$SUMMARY")"
CHUNKS="$(field chunks "$SUMMARY")"
CHUNK_NS=$(( CHUNK_US * 1000 ))

check "short-outage-rejoined" \
    "$([ "${REJOINS:-0}" -ge 1 ] && echo 1 || echo 0)" \
    "the endpoint rejoined $REJOINS time(s) with nothing telling it to"
check "short-outage-resumed-playback" \
    "$([ "${PLAYED_ON:-0}" -ge 2 ] && echo 1 || echo 0)" \
    "audio played on $PLAYED_ON separate connections, so it resumed rather than merely reconnecting"
check "short-outage-was-shorter-than-one-chunk" \
    "$([ "${OUTAGE_NS:-0}" -gt 0 ] && [ "${OUTAGE_NS:-0}" -lt "$CHUNK_NS" ] && echo 1 || echo 0)" \
    "the measured outage was ${OUTAGE_NS} ns against one chunk of ${CHUNK_NS} ns"
check "short-outage-played-audio" \
    "$([ "${CHUNKS:-0}" -ge 2 ] && echo 1 || echo 0)" \
    "$CHUNKS chunks of real PCM came off the wire"
check "short-outage-needed-no-human" \
    "$(grep -c 'event=link-up' "$LOG" | awk '{print ($1 >= 2) ? 1 : 0}')" \
    "$(grep -c 'event=link-up' "$LOG") link-up events in the endpoint's own log"

# --- 2. an outage of minutes ---------------------------------------------------

OUTAGE_SECONDS="$(endpoint_conf outage_minutes_seconds)"
say ""
say "chorus: outage 2 of 3, an outage of minutes (${OUTAGE_SECONDS}s, which is real wall clock)"

PORT="$(free_port)"
LOG="$OUT_DIR/chorus-endpoint-outage-minutes.log"
SUMMARY="$OUT_DIR/chorus-endpoint-outage-minutes.summary"
RUN_SECONDS=$(( OUTAGE_SECONDS + 20 ))

start_server "$PORT" "$(conf chunk_us)" 48000 2
sleep 1
"$ENDPOINT" --server "127.0.0.1:$PORT" --run-seconds "$RUN_SECONDS" --log "$LOG" \
    >"$SUMMARY" 2>&1 &
ENDPOINT_PID=$!
trap 'kill -9 "$ENDPOINT_PID" 2>/dev/null || true; kill_server_hard' EXIT

sleep 5
say "chorus: killing the server and leaving it dead for ${OUTAGE_SECONDS}s"
kill_server_hard
sleep "$OUTAGE_SECONDS"
say "chorus: bringing a new server back"
start_server "$PORT" "$(conf chunk_us)" 48000 2

wait "$ENDPOINT_PID"
trap 'kill_server_hard' EXIT
kill_server_hard
cat "$SUMMARY"

REJOINS="$(field rejoins "$SUMMARY")"
PLAYED_ON="$(field connections_that_played "$SUMMARY")"
OUTAGE_NS="$(field longest_outage_ns "$SUMMARY")"
ATTEMPTS="$(field attempts "$SUMMARY")"
MINIMUM_NS=$(( OUTAGE_SECONDS * 1000000000 ))

check "minutes-outage-rejoined" \
    "$([ "${REJOINS:-0}" -ge 1 ] && echo 1 || echo 0)" \
    "the endpoint rejoined after ${OUTAGE_SECONDS}s of a dead server"
check "minutes-outage-resumed-playback" \
    "$([ "${PLAYED_ON:-0}" -ge 2 ] && echo 1 || echo 0)" \
    "audio played on $PLAYED_ON separate connections"
check "minutes-outage-really-lasted-minutes" \
    "$([ "${OUTAGE_NS:-0}" -ge "$MINIMUM_NS" ] && echo 1 || echo 0)" \
    "the endpoint measured its own outage at ${OUTAGE_NS} ns, at least ${MINIMUM_NS} ns"
check "minutes-outage-kept-trying-without-spinning" \
    "$([ "${ATTEMPTS:-0}" -ge 3 ] && [ "${ATTEMPTS:-0}" -le 2000 ] && echo 1 || echo 0)" \
    "$ATTEMPTS connection attempts over ${OUTAGE_SECONDS}s: more than a couple, and not a busy loop"

# --- 3. a server that returns on a new connection ------------------------------

say ""
say "chorus: outage 3 of 3, several failed attempts and then a new server process"

PORT="$(free_port)"
LOG="$OUT_DIR/chorus-endpoint-outage-new-connection.log"
SUMMARY="$OUT_DIR/chorus-endpoint-outage-new-connection.summary"

start_server "$PORT" "$(conf chunk_us)" 48000 2
sleep 1
"$ENDPOINT" --server "127.0.0.1:$PORT" --run-seconds 24 --log "$LOG" \
    >"$SUMMARY" 2>&1 &
ENDPOINT_PID=$!
trap 'kill -9 "$ENDPOINT_PID" 2>/dev/null || true; kill_server_hard' EXIT

sleep 3
say "chorus: killing the server and leaving nothing listening for 10s"
kill_server_hard
sleep 10
say "chorus: a NEW server process answers on the same address"
start_server "$PORT" "$(conf chunk_us)" 48000 2

wait "$ENDPOINT_PID"
trap 'kill_server_hard' EXIT
kill_server_hard
cat "$SUMMARY"

REJOINS="$(field rejoins "$SUMMARY")"
PLAYED_ON="$(field connections_that_played "$SUMMARY")"
ATTEMPTS="$(field attempts "$SUMMARY")"
EXCHANGES="$(field exchanges "$SUMMARY")"
FAILED_ATTEMPTS="$(grep -c 'event=connect-failed' "$LOG" || true)"

check "new-connection-rejoined" \
    "$([ "${REJOINS:-0}" -ge 1 ] && echo 1 || echo 0)" \
    "the endpoint joined the new process by itself"
check "new-connection-really-failed-first" \
    "$([ "${FAILED_ATTEMPTS:-0}" -ge 2 ] && echo 1 || echo 0)" \
    "$FAILED_ATTEMPTS connect attempts were refused before one succeeded, so the connect-refused path ran"
check "new-connection-resumed-playback" \
    "$([ "${PLAYED_ON:-0}" -ge 2 ] && echo 1 || echo 0)" \
    "audio played on $PLAYED_ON separate connections"
check "new-connection-re-established-the-exchange" \
    "$([ "${EXCHANGES:-0}" -ge 2 ] && echo 1 || echo 0)" \
    "$EXCHANGES time-sync exchanges completed across the two connections"
check "new-connection-attempts-are-bounded" \
    "$([ "${ATTEMPTS:-0}" -le 2000 ] && echo 1 || echo 0)" \
    "$ATTEMPTS attempts in total, which is a backoff and not a spin"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: the endpoint rejoins and resumes with no human action, over a real socket"
    exit 0
fi
say "chorus: $FAILURES session checks did not hold"
exit 1
