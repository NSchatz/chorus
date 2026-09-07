#!/usr/bin/env bash
# AC-3, the restart storm, over real sockets against real processes.
#
# "WHEN every endpoint reconnects at once after a server restart THE SYSTEM
# SHALL return all of them to playback with no operator action."
#
# Nothing here is a mock, and it follows firmware/tests/session-outage.sh, which
# already does this for one endpoint: `chorus-server` is started for real, FOUR
# `chorus-client` endpoints join it over loopback TCP and play, the server is
# killed with SIGKILL, a NEW server process is started on the same addresses,
# and every one of the four has to come back by itself. The only thing that
# touches any endpoint is the passage of time.
#
# The second half of the criterion is the one that is easy to lose: "Zone names,
# group membership, volume and mute as they stood before the kill are what the
# endpoints come back to; a restart that returns them to defaults fails this."
# So the zones are deliberately moved off their defaults before the kill, and
# what the new server serves is compared against what was set, field by field.
#
# The names set below are deliberately NOT tidy, and that is load-bearing.
# `is_display_name` in crates/control/src/catalog.rs admits any printable
# character in a name, so a person using the shipped UI's rename box can type
# one - and the persisted state file's own comment character is '#'. A grader
# that only ever sets "The Kitchen" cannot see a name that does not come back,
# which is exactly how a '#' in a name reached this branch unnoticed. Each name
# here carries at least one character of the persisted format's own syntax, one
# of them BEGINS with the comment character, and a check below refuses to pass
# if a later edit tidies them.
#
# Needs a playback device that opens; the ALSA `null` device is enough, because
# what is graded is whether a played-frame counter advances again and not the
# value of any reported delay. Refuses by name where there is not even one.
#
#   CHORUS_CLIENT_DEVICE=null ./tools/restart-storm-run.sh   # or: make verify-restart-storm

source "$(dirname "$0")/lib.sh"

build_once

CRITERION="every endpoint reconnects at once after a server restart and all of them return to playback with no operator action, at the zone names, groups, volumes and mutes they had before it"
require_audio_device "$CRITERION"

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

ENDPOINTS=4
OUT_DIR="${TMPDIR:-/tmp}/chorus-restart-storm"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"
DEVICE="$(audio_device)"
CONTRACT_ARGS="$(server_contract_args)"
AUDIO="$(free_port)"
CONTROL="$(free_port)"
STATE_FILE="$OUT_DIR/zones.state"

SERVER_PID=""
ENDPOINT_PIDS=()

start_server() {
    local log="$1"
    "$BIN_DIR/chorus-server" \
        --listen "127.0.0.1:$AUDIO" \
        --source tone \
        --rate "$(conf sample_rate_hz)" \
        --channels "$(conf channels)" \
        --format "$(conf sample_format)" \
        --chunk-us "$(conf chunk_us)" \
        --rttime-us "$(conf rttime_us)" \
        --rt-priority "$(conf rt_priority)" \
        --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
        --serve-forever \
        --max-clients 8 \
        --control-listen "127.0.0.1:$CONTROL" \
        --control-workers 16 \
        --state-file "$STATE_FILE" \
        --zone kitchen --zone study --zone hall --zone porch \
        $CONTRACT_ARGS >"$log" 2>&1 &
    SERVER_PID=$!
    local waited=0
    while [ "$waited" -lt 100 ]; do
        if grep -q 'listening on=' "$log" 2>/dev/null; then
            return 0
        fi
        sleep 0.2
        waited=$(( waited + 1 ))
    done
    say "FAIL the server never came up; it said:"
    sed 's/^/    /' "$log"
    exit 1
}

kill_server_hard() {
    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill -9 "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    SERVER_PID=""
}

stop_everything() {
    local pid
    for pid in "${ENDPOINT_PIDS[@]:-}"; do
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
    ENDPOINT_PIDS=()
    kill_server_hard
}
trap stop_everything EXIT

command() {
    python3 - "$CONTROL" "$1" <<'PY'
import socket, sys
port, body = int(sys.argv[1]), sys.argv[2].encode()
s = socket.create_connection(("127.0.0.1", port))
s.sendall(
    b"POST /api/command HTTP/1.1\r\nHost: chorus\r\nContent-Type: application/json\r\n"
    + b"Content-Length: " + str(len(body)).encode() + b"\r\nConnection: close\r\n\r\n" + body
)
answer = b""
while True:
    chunk = s.recv(65536)
    if not chunk:
        break
    answer += chunk
sys.stdout.write(answer.decode("utf-8", "replace"))
PY
}

fetch_state() {
    python3 - "$CONTROL" <<'PY'
import socket, sys
s = socket.create_connection(("127.0.0.1", int(sys.argv[1])))
s.sendall(b"GET /api/state HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n")
answer = b""
while True:
    chunk = s.recv(65536)
    if not chunk:
        break
    answer += chunk
sys.stdout.write(answer.decode("utf-8", "replace").split("\r\n\r\n", 1)[-1].strip())
PY
}

# --- a server, four endpoints, and a house that is not at its defaults --------

say ""
say "chorus: a server, $ENDPOINTS endpoints, and zone state that is not the default"
start_server "$OUT_DIR/server-1.log"

for SET in \
    '{"v":1,"t":"name","zone":"kitchen","name":"The Kitchen #1"}' \
    '{"v":1,"t":"name","zone":"study","name":"#2 The Study"}' \
    '{"v":1,"t":"name","zone":"hall","name":"Hall \\ Landing"}' \
    '{"v":1,"t":"name","zone":"porch","name":"Porch [north] = 1"}' \
    '{"v":1,"t":"volume","zone":"kitchen","volume":0.375}' \
    '{"v":1,"t":"volume","zone":"study","volume":0.125}' \
    '{"v":1,"t":"mute","zone":"hall","muted":true}' \
    '{"v":1,"t":"group","zone":"kitchen","group":"downstairs"}' \
    '{"v":1,"t":"group","zone":"study","group":"downstairs"}'
do
    ANSWER="$(command "$SET")"
    if ! printf '%s' "$ANSWER" | grep -q '200 OK'; then
        say "FAIL the setup command $SET was refused: $(printf '%s' "$ANSWER" | head -n 1)"
        exit 1
    fi
done
for ZONE in kitchen study hall porch; do
    # Long enough to cover the whole run: the endpoints stop themselves at the
    # end of it, and everything below is read out of what they printed on their
    # way out rather than out of a process that was killed mid-sentence.
    "$BIN_DIR/chorus-client" \
        --server "127.0.0.1:$AUDIO" \
        --control "127.0.0.1:$CONTROL" \
        --zone "$ZONE" \
        --endpoint "endpoint-$ZONE" \
        --rejoin \
        --run-seconds 34 \
        --device "$DEVICE" \
        --delay-log "$OUT_DIR/endpoint-$ZONE.log" \
        --sync-interval-ms "$(sync_conf sync_interval_ms)" \
        >"$OUT_DIR/endpoint-$ZONE.out" 2>&1 &
    ENDPOINT_PIDS+=("$!")
done

# Every one of them has to be PLAYING before the kill, or the storm is not a
# storm.
WAITED=0
while [ "$WAITED" -lt 120 ]; do
    ATTACHED=0
    for ZONE in kitchen study hall porch; do
        if grep -q 'chunks_played=' "$OUT_DIR/endpoint-$ZONE.log" 2>/dev/null \
            || grep -q 'kind=start-fill' "$OUT_DIR/endpoint-$ZONE.log" 2>/dev/null; then
            ATTACHED=$(( ATTACHED + 1 ))
        fi
    done
    if [ "$ATTACHED" -ge "$ENDPOINTS" ]; then
        break
    fi
    sleep 0.5
    WAITED=$(( WAITED + 1 ))
done
check "all-four-endpoints-were-attached-and-playing-before-the-kill" \
    "$([ "${ATTACHED:-0}" -ge "$ENDPOINTS" ] && echo 1 || echo 0)" \
    "$ATTACHED of $ENDPOINTS had begun playing"
CLIENTS="$(grep -c 'client connected' "$OUT_DIR/server-1.log" || true)"
check "the-server-saw-all-four-connect" \
    "$([ "${CLIENTS:-0}" -ge "$ENDPOINTS" ] && echo 1 || echo 0)" \
    "$CLIENTS client connections on the first server"

sleep 3

# Taken now, with everything attached, so that what is compared after the
# restart is the state as it stood at the instant the server was killed.
BEFORE="$(fetch_state)"
say "chorus: before the kill, the server says:"
printf '%s\n' "$BEFORE" | sed 's/^/    /'

# --- the kill, and a new process on the same addresses ------------------------

say ""
say "chorus: killing the server with SIGKILL and starting a NEW process on the same addresses"
kill_server_hard
sleep 2
start_server "$OUT_DIR/server-2.log"
say "chorus: a new server is up; nothing will be said to any endpoint from here on"

# Every endpoint runs to the end of its own configured length and stops itself,
# so what is read below is what each of them printed on its way out. Killing
# them here instead would cut off the session that is the whole point.
say "chorus: waiting for every endpoint to finish its run"
for PID in "${ENDPOINT_PIDS[@]}"; do
    wait "$PID" 2>/dev/null || true
done
ENDPOINT_PIDS=()

AFTER="$(fetch_state)"
say "chorus: after the restart, the new server says:"
printf '%s\n' "$AFTER" | sed 's/^/    /'

stop_everything
sleep 1

# --- every endpoint back to advancing its played-frame counter ----------------

say ""
for ZONE in kitchen study hall porch; do
    OUT="$OUT_DIR/endpoint-$ZONE.out"
    SESSIONS="$(grep -c 'session n=' "$OUT" || true)"
    PLAYED_AFTER="$(awk '/session n=/ { n=$0 } END { print n }' "$OUT" \
        | grep -o 'total_frames_played=[0-9]*' | cut -d= -f2 || true)"
    FIRST_PLAYED="$(grep -m1 'session n=1 ' "$OUT" | grep -o 'frames_played=[0-9]*' \
        | head -n 1 | cut -d= -f2 || true)"
    LATER_PLAYED="$(grep 'session n=' "$OUT" | grep -v 'session n=1 ' \
        | grep -o ' frames_played=[0-9]*' | cut -d= -f2 | awk '{ s += $1 } END { print s+0 }')"
    check "$ZONE-rejoined-with-nothing-telling-it-to" \
        "$([ "${SESSIONS:-0}" -ge 2 ] && echo 1 || echo 0)" \
        "$SESSIONS sessions; the second one is against a process that did not exist when it started"
    check "$ZONE-played-before-the-kill" \
        "$([ "${FIRST_PLAYED:-0}" -gt 0 ] && echo 1 || echo 0)" \
        "$FIRST_PLAYED frames on the first session"
    check "$ZONE-is-advancing-its-played-frame-counter-again" \
        "$([ "${LATER_PLAYED:-0}" -gt 0 ] && echo 1 || echo 0)" \
        "$LATER_PLAYED frames played after the restart, on $(( SESSIONS - 1 )) later session(s)"
    check "$ZONE-needed-no-operator-action" \
        "$(grep -q 'reason=connection-lost' "$OUT" && echo 1 || echo 0)" \
        "it lost the server and came back by itself: $(grep -o 'stop=[a-z-]*' "$OUT" | tr '\n' ' ')"
done

CLIENTS_2="$(grep -c 'client connected' "$OUT_DIR/server-2.log" || true)"
check "the-new-server-saw-all-four-come-back" \
    "$([ "${CLIENTS_2:-0}" -ge "$ENDPOINTS" ] && echo 1 || echo 0)" \
    "$CLIENTS_2 client connections on the second server, which nobody told them about"

# --- and they came back to the state they left, not to defaults ---------------

say ""

# The bytes the server must serve, hand-written rather than derived from what it
# said: a grader that builds its expectation with the same encoder it is
# grading agrees with itself. A backslash is written '\\' here because that is
# how the state message encodes one.
EXPECTED=(
    '"id":"kitchen","name":"The Kitchen #1","group":"downstairs","volume":0.375,"muted":false'
    '"id":"study","name":"#2 The Study","group":"downstairs","volume":0.125,"muted":false'
    '"id":"hall","name":"Hall \\ Landing","group":"hall","volume":1.000,"muted":true'
    '"id":"porch","name":"Porch [north] = 1","group":"porch","volume":1.000,"muted":false'
)

# A guard on this grader, not on the server: if a later edit tidies the names
# back to ones that carry none of the persisted format's own syntax, this fails
# instead of quietly grading a case that cannot break. Every character listed
# here is one `is_display_name` admits and one the state file gives a meaning.
AWKWARD=1
MISSING=""
for CH in '#' '\' '[' ']' '='; do
    if ! printf '%s\n' "${EXPECTED[@]}" | grep -qF -- "$CH"; then
        AWKWARD=0
        MISSING="$MISSING $CH"
    fi
done
if ! printf '%s\n' "${EXPECTED[@]}" | grep -qF -- '"name":"#'; then
    AWKWARD=0
    MISSING="$MISSING a-name-that-begins-with-the-comment-character"
fi
check "the-names-this-grader-sets-carry-the-punctuation-the-catalog-admits" "$AWKWARD" \
    "a name here carries each of # \\ [ ] = and one begins with '#'${MISSING:+; MISSING:$MISSING}"

for FIELD in "${EXPECTED[@]}"; do
    ZONE="$(printf '%s' "$FIELD" | sed 's/^"id":"\([a-z]*\)".*/\1/')"
    check "$ZONE-was-holding-that-name-before-the-kill" \
        "$(printf '%s' "$BEFORE" | grep -qF "$FIELD" && echo 1 || echo 0)" \
        "the server accepted and held it: $FIELD"
    check "$ZONE-came-back-to-what-it-was-and-not-to-defaults" \
        "$(printf '%s' "$AFTER" | grep -qF "$FIELD" && echo 1 || echo 0)" \
        "$FIELD"
done

# The strongest form of it: what a subscriber is served after the restart is the
# same zone state it was served before, field for field. Only the serial and the
# presence differ, because those are facts about now.
BEFORE_ZONES="$(printf '%s' "$BEFORE" | sed 's/"serial":[0-9]*//; s/"present":\[[^]]*\]//g')"
AFTER_ZONES="$(printf '%s' "$AFTER" | sed 's/"serial":[0-9]*//; s/"present":\[[^]]*\]//g')"
check "the-whole-zone-state-survived-the-restart" \
    "$([ "$BEFORE_ZONES" = "$AFTER_ZONES" ] && echo 1 || echo 0)" \
    "every field but the serial and which endpoints are attached is byte-identical"

# And it survived because it was PERSISTED, not because the process never died.
check "the-state-was-read-back-from-the-file-and-not-reconfigured" \
    "$(grep -q 'state=reloaded' "$OUT_DIR/server-2.log" && echo 1 || echo 0)" \
    "$(grep -o 'control listening on=[^ ]* workers=[0-9]* zones=[0-9]* state=[a-z]*' "$OUT_DIR/server-2.log" | head -n 1)"
check "the-first-server-had-nothing-to-read-back" \
    "$(grep -q 'state=configured' "$OUT_DIR/server-1.log" && echo 1 || echo 0)" \
    "$(grep -o 'state=[a-z]*' "$OUT_DIR/server-1.log" | head -n 1)"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: $ENDPOINTS endpoints came back to playback by themselves, at the state they left"
    exit 0
fi
say "chorus: $FAILURES restart-storm checks did not hold"
exit 1
