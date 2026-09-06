#!/usr/bin/env bash
# AC-1, both halves, with real processes on real sockets.
#
# "WHEN a zone is grouped, ungrouped, volume-changed or muted THE SYSTEM SHALL
# apply it to every affected endpoint and SHALL fan the resulting state out to
# all subscribers."
#
# The criterion is graded in two halves and this runs both:
#
#   Applied-at-the-endpoint is read off the samples a modelled sink ACCEPTED,
#   never off a status line. That is `cargo test -p chorus-client-linux --test
#   zone_apply`, which this script runs; the model lives under tests/ because
#   crates/client-linux/src/sink.rs forbids a modelled sink reachable from the
#   binary, and it is the only place the bytes that would reach a DAC can be
#   inspected. This script also runs REAL chorus-client endpoints against a
#   REAL server, which is what makes the group half end to end: an endpoint is
#   grouped elsewhere and comes back playing the other stream, on its own.
#
#   Fanned-out is read off at least two real subscribers on real sockets
#   against a real chorus-server process, each receiving the post-command
#   state, INCLUDING the subscriber that did not issue the command.
#
# Needs a playback device that opens. The ALSA `null` device is enough - what
# is graded here is which stream an endpoint is attached to and what the server
# told every subscriber, not the value of any reported delay - and this refuses
# by name where there is not even one of those.
#
#   CHORUS_CLIENT_DEVICE=null ./tools/control-plane-run.sh   # or: make verify-control

source "$(dirname "$0")/lib.sh"

build_once

CRITERION="a zone that is grouped, ungrouped, volume-changed or muted is applied to every affected endpoint and the resulting state is fanned out to all subscribers"
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

OUT_DIR="${TMPDIR:-/tmp}/chorus-control-plane"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"
DEVICE="$(audio_device)"
CONTRACT_ARGS="$(server_contract_args)"

PIDS=()
stop_everything() {
    local pid
    for pid in "${PIDS[@]:-}"; do
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
    PIDS=()
}
trap stop_everything EXIT

# --- the samples, off a modelled sink ----------------------------------------
#
# Run first, because it is the half that cannot be satisfied by any report the
# client makes about itself, and because a failure here makes everything below
# meaningless.

say ""
say "chorus: AC-1 half one, what the device ACCEPTED"
set +e
SAMPLES_OUT="$(cd "$REPO_ROOT" && cargo test --quiet -p chorus-client-linux --test zone_apply 2>&1)"
SAMPLES_STATUS=$?
set -e
printf '%s\n' "$SAMPLES_OUT" | sed 's/^/    /'
check "accepted-samples-carry-the-commanded-volume-mute-and-group" \
    "$([ "$SAMPLES_STATUS" -eq 0 ] && echo 1 || echo 0)" \
    "cargo test -p chorus-client-linux --test zone_apply exited $SAMPLES_STATUS"

# --- the fanout, over real sockets -------------------------------------------

say ""
say "chorus: AC-1 half two, what every subscriber was told"

AUDIO_A="$(free_port)"
AUDIO_B="$(free_port)"
CONTROL="$(free_port)"
STATE_FILE="$OUT_DIR/zones.state"

start_server() {
    local listen="$1"
    local control="$2"
    local log="$3"
    shift 3
    "$BIN_DIR/chorus-server" \
        --listen "127.0.0.1:$listen" \
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
        $control \
        $CONTRACT_ARGS "$@" >"$log" 2>&1 &
    PIDS+=("$!")
}

# The control server, which also serves group "downstairs" its audio, and a
# second audio server which is group "upstairs". Two streams, distinguishable
# because they are two processes with two timelines and two sequence runs.
start_server "$AUDIO_A" \
    "--control-listen 127.0.0.1:$CONTROL --state-file $STATE_FILE \
     --zone kitchen --zone study \
     --group-audio downstairs=127.0.0.1:$AUDIO_A --group-audio upstairs=127.0.0.1:$AUDIO_B" \
    "$OUT_DIR/server-a.log"
start_server "$AUDIO_B" "" "$OUT_DIR/server-b.log"
sleep 2

check "the-control-channel-came-up" \
    "$(grep -q "control listening on=" "$OUT_DIR/server-a.log" && echo 1 || echo 0)" \
    "$(grep 'control listening on=' "$OUT_DIR/server-a.log" | head -n 1)"

# Two REAL subscribers, on real sockets, each holding an event stream open and
# writing every state message it is sent to a file. Written with python3's
# socket module rather than curl so that what is exercised is a socket and an
# HTTP request, with nothing in between that might be doing its own buffering.
subscriber() {
    local name="$1"
    python3 - "$CONTROL" "$OUT_DIR/$name.events" <<'PY' &
import socket, sys
port, path = int(sys.argv[1]), sys.argv[2]
s = socket.create_connection(("127.0.0.1", port))
s.sendall(b"GET /api/events HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n")
with open(path, "w", buffering=1) as out:
    held = b""
    while True:
        chunk = s.recv(65536)
        if not chunk:
            break
        held += chunk
        while b"\n" in held:
            line, held = held.split(b"\n", 1)
            line = line.decode("utf-8", "replace").strip()
            if line.startswith("data: "):
                out.write(line[len("data: "):] + "\n")
PY
    PIDS+=("$!")
}

subscriber "watcher-one"
subscriber "watcher-two"
sleep 1

check "two-real-subscribers-attached-and-were-sent-the-state" \
    "$([ -s "$OUT_DIR/watcher-one.events" ] && [ -s "$OUT_DIR/watcher-two.events" ] \
        && echo 1 || echo 0)" \
    "watcher-one has $(wc -l < "$OUT_DIR/watcher-one.events" | tr -d ' ') message(s), watcher-two has $(wc -l < "$OUT_DIR/watcher-two.events" | tr -d ' ')"

# The commanding subscriber is a THIRD connection, so that neither watcher
# issued anything: the criterion is explicit that the state has to reach the
# subscriber that did not send the command.
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

for BODY in \
    '{"v":1,"t":"name","zone":"kitchen","name":"The Kitchen"}' \
    '{"v":1,"t":"volume","zone":"kitchen","volume":0.375}' \
    '{"v":1,"t":"mute","zone":"study","muted":true}' \
    '{"v":1,"t":"group","zone":"kitchen","group":"upstairs"}' \
    '{"v":1,"t":"ungroup","zone":"kitchen"}' \
    '{"v":1,"t":"group","zone":"kitchen","group":"downstairs"}'
do
    ANSWER="$(command "$BODY")"
    check "command-applied $(printf '%s' "$BODY" | sed 's/.*"t":"\([a-z]*\)".*/\1/')" \
        "$(printf '%s' "$ANSWER" | grep -q '200 OK' && echo 1 || echo 0)" \
        "$(printf '%s' "$ANSWER" | head -n 1 | tr -d '\r')"
    sleep 0.3
done
sleep 1

for WATCHER in watcher-one watcher-two; do
    EVENTS="$OUT_DIR/$WATCHER.events"
    LAST="$(tail -n 1 "$EVENTS")"
    check "$WATCHER-saw-the-name" \
        "$(grep -q '"name":"The Kitchen"' "$EVENTS" && echo 1 || echo 0)" \
        "$(grep -c '"name":"The Kitchen"' "$EVENTS") of $(wc -l < "$EVENTS" | tr -d ' ') messages carry it"
    check "$WATCHER-saw-the-volume" \
        "$(grep -q '"volume":0.375' "$EVENTS" && echo 1 || echo 0)" \
        "$(grep -c '"volume":0.375' "$EVENTS") messages carry it"
    check "$WATCHER-saw-the-mute" \
        "$(grep -q '"id":"study","name":"study","group":"study","volume":1.000,"muted":true' "$EVENTS" \
            && echo 1 || echo 0)" \
        "the study is muted in $(grep -c '"muted":true' "$EVENTS") messages"
    check "$WATCHER-saw-the-group-and-the-ungroup" \
        "$(grep -q '"group":"upstairs"' "$EVENTS" && grep -q '"group":"kitchen"' "$EVENTS" \
            && echo 1 || echo 0)" \
        "grouped in $(grep -c '"group":"upstairs"' "$EVENTS"), alone in $(grep -c '"group":"kitchen"' "$EVENTS")"
    check "$WATCHER-holds-the-post-command-state" \
        "$(printf '%s' "$LAST" | grep -q '"group":"downstairs"' && echo 1 || echo 0)" \
        "$(printf '%s' "$LAST" | cut -c1-120)"
done

# Both watchers were sent the SAME messages, which is what "fan out" means: two
# subscribers holding two different accounts of one server would be worse than
# one subscriber.
check "both-subscribers-hold-the-same-final-state" \
    "$([ "$(tail -n 1 "$OUT_DIR/watcher-one.events")" = "$(tail -n 1 "$OUT_DIR/watcher-two.events")" ] \
        && echo 1 || echo 0)" \
    "the last message each holds is byte-identical"

# --- a real endpoint, moved between groups ------------------------------------
#
# The end-to-end half of the group change: a REAL chorus-client, attached to a
# zone, is put into a group whose stream is the OTHER server, and it has to come
# back playing that one with nothing telling it to.

say ""
say "chorus: AC-1, a real endpoint following its zone between two streams"

"$BIN_DIR/chorus-client" \
    --server "127.0.0.1:$AUDIO_A" \
    --control "127.0.0.1:$CONTROL" \
    --zone kitchen \
    --endpoint endpoint-a \
    --rejoin \
    --run-seconds 14 \
    --device "$DEVICE" \
    --delay-log "$OUT_DIR/endpoint-a.log" \
    --sync-interval-ms "$(sync_conf sync_interval_ms)" \
    >"$OUT_DIR/endpoint-a.out" 2>&1 &
ENDPOINT=$!
PIDS+=("$ENDPOINT")
sleep 4

BEFORE="$(grep -c "server=127.0.0.1:$AUDIO_A" "$OUT_DIR/endpoint-a.out" || true)"
say "chorus: putting the kitchen into the group whose stream is 127.0.0.1:$AUDIO_B"
command '{"v":1,"t":"group","zone":"kitchen","group":"upstairs"}' >/dev/null
sleep 6

wait "$ENDPOINT" 2>/dev/null || true
cat "$OUT_DIR/endpoint-a.out" | sed 's/^/    /'

check "the-endpoint-attached-to-its-zone" \
    "$(grep -q 'control attached=' "$OUT_DIR/endpoint-a.out" && echo 1 || echo 0)" \
    "$(grep 'control attached=' "$OUT_DIR/endpoint-a.out" | head -n 1)"
check "the-endpoint-started-on-its-groups-stream" \
    "$([ "${BEFORE:-0}" -ge 1 ] && echo 1 || echo 0)" \
    "$BEFORE session line(s) name 127.0.0.1:$AUDIO_A before the group change"
PLAYED_B="$(grep -c "session n=.* server=127.0.0.1:$AUDIO_B played=1" "$OUT_DIR/endpoint-a.out" || true)"
check "the-endpoint-followed-its-zone-to-the-other-stream-and-played" \
    "$([ "${PLAYED_B:-0}" -ge 1 ] && echo 1 || echo 0)" \
    "$(grep "server=127.0.0.1:$AUDIO_B" "$OUT_DIR/endpoint-a.out" | tail -n 1)"
check "nothing-was-said-to-the-endpoint-itself" \
    "$(grep -q 'moves=1' "$OUT_DIR/endpoint-a.out" && echo 1 || echo 0)" \
    "the endpoint moved because the SERVER's state said so: $(grep -o 'moves=[0-9]*' "$OUT_DIR/endpoint-a.out" | tail -n 1)"
check "the-server-recorded-the-endpoint-as-present" \
    "$(grep -q '"present":\["endpoint-a"\]' "$OUT_DIR/watcher-one.events" && echo 1 || echo 0)" \
    "$(grep -c '"present":\["endpoint-a"\]' "$OUT_DIR/watcher-one.events") state messages name it"

# --- the fanout's own bound, reported ------------------------------------------
#
# AC-11 asks that what a dropped subscriber lost is counted AND reported. The
# count is read off the running server here, over the same socket everything
# else used, rather than out of a log line printed at exit.

say ""
CONTROL_REPORT="$(python3 - "$CONTROL" <<'PY'
import socket, sys
s = socket.create_connection(("127.0.0.1", int(sys.argv[1])))
s.sendall(b"GET /api/report HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n")
answer = b""
while True:
    chunk = s.recv(65536)
    if not chunk:
        break
    answer += chunk
sys.stdout.write(answer.decode("utf-8", "replace").split("\r\n\r\n", 1)[-1].strip())
PY
)"
say "chorus: the control plane's own report: $CONTROL_REPORT"
check "the-control-plane-reports-what-it-did" \
    "$(printf '%s' "$CONTROL_REPORT" | grep -q 'control applied=[1-9]' && echo 1 || echo 0)" \
    "$CONTROL_REPORT"
check "the-fanouts-bound-and-its-drops-are-reported" \
    "$(printf '%s' "$CONTROL_REPORT" | grep -q 'queue_limit=[0-9]* dropped_subscribers=[0-9]* dropped_messages=[0-9]*' \
        && echo 1 || echo 0)" \
    "$CONTROL_REPORT"

stop_everything
sleep 1

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: a zone change reaches every affected endpoint and every subscriber"
    exit 0
fi
say "chorus: $FAILURES control-plane checks did not hold"
exit 1
