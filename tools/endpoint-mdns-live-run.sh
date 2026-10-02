#!/usr/bin/env bash
# Goal 14's live half: the C endpoint finds a real server over real multicast.
#
# The BYTES of discovery need none of this and are graded in the gate, with no
# network at all, against the committed packets both implementations read
# (`make firmware-check`, firmware/tests/test_discovery.c over
# fixtures/discovery). This is the additional check: a real `chorus-server
# --advertise` and the endpoint's host binary (`chorus-endpoint-session
# --discover`, the pure browse of firmware/src/discovery.c over a POSIX socket)
# exchanging real datagrams on one link, the way tools/mdns-live-run.sh does it
# for the Linux client.
#
# Whether multicast reaches a container and crosses a network's VLANs is an
# open question in any deployment, which is why the endpoint has two fallbacks.
# So this either passes or refuses BY NAME, and whichever happened is what is
# reported: a live exchange that could not run is never one that passed, and it
# is not in `make gate`, which stays deterministic.
#
#   ./tools/endpoint-mdns-live-run.sh          # or: make verify-endpoint-mdns

source "$(dirname "$0")/lib.sh"

build_once

CRITERION="a C endpoint with no configured server address discovers the server by DNS-SD on a real link, and finds it again by itself"

say "chorus: the C endpoint's live multicast exchange"
say "  criterion: $CRITERION"
say "  group:     224.0.0.251, UDP port 5353 (RFC 6762 sections 2 and 3)"
say "  service:   _chorus-audio._tcp.local."

require_multicast "$CRITERION"

ENDPOINT="$REPO_ROOT/firmware/build/chorus-endpoint-session"
if [ "${CHORUS_SKIP_BUILD:-0}" != "1" ]; then
    make -s -f "$REPO_ROOT/firmware/Makefile" "$ENDPOINT"
fi
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

OUT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/chorus-endpoint-mdns-live.XXXXXX")"
AUDIO="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
mkdir -p "$OUT_DIR/server-identity"

SERVER=""
ENDPOINT_PID=""
trap 'kill_quietly "$ENDPOINT_PID"; kill_quietly "$SERVER"; rm -rf "$OUT_DIR"' EXIT

start_server() {
    "$BIN_DIR/chorus-server" \
        --identity-dir "$OUT_DIR/server-identity" \
        --listen "127.0.0.1:$AUDIO" \
        --source tone \
        --serve-forever \
        --advertise \
        --instance chorus-live-endpoint \
        "${CONTRACT_ARGS[@]}" >>"$OUT_DIR/server.log" 2>&1 &
    SERVER=$!
}

field() {
    sed -n "s/.*[[:space:]]$1=\\([^ ]*\\).*/\\1/p" "$2" | tail -n 1
}

start_server
sleep 2
if ! grep -q 'advertising instances=' "$OUT_DIR/server.log"; then
    sed 's/^/    /' "$OUT_DIR/server.log"
    missing_prerequisite \
        "$CRITERION" \
        "a server able to advertise on this link; it refused: $(grep -m1 'could not advertise' "$OUT_DIR/server.log" || echo 'see the log above')" \
        "run this where UDP port 5353 can be bound and the group 224.0.0.251 joined"
fi

# --- 1. no address at all: discovery is the only way there ---------------------

say ""
say "chorus: 1 of 4, an endpoint with an empty store and no address"
set +e
"$ENDPOINT" --no-server --discover --store "$OUT_DIR/board" --run-seconds 3 \
    --log "$OUT_DIR/found.log" >"$OUT_DIR/found.summary" 2>&1
STATUS=$?
set -e
sed 's/^/    /' "$OUT_DIR/found.summary"
check "the-endpoint-found-the-server-on-the-link" \
    "$(grep -q "server-located how=mdns address=127.0.0.1:$AUDIO instance=chorus-live-endpoint._chorus-audio._tcp.local." \
        "$OUT_DIR/found.summary" && echo 1 || echo 0)" \
    "$(grep 'server-located' "$OUT_DIR/found.summary" | head -n 1)"
check "it-went-on-to-join-and-play" \
    "$([ "$STATUS" -eq 0 ] && [ "$(field handshakes "$OUT_DIR/found.summary")" -ge 1 ] \
        && [ "$(field chunks "$OUT_DIR/found.summary")" -gt 0 ] && echo 1 || echo 0)" \
    "exit $STATUS, $(field handshakes "$OUT_DIR/found.summary") handshake(s), $(field chunks "$OUT_DIR/found.summary") chunks from the server it found"
check "and-kept-where-it-found-it" \
    "$([ "$(cat "$OUT_DIR/board/server_addr" 2>/dev/null)" = "127.0.0.1:$AUDIO" ] && echo 1 || echo 0)" \
    "the store's last server is $(cat "$OUT_DIR/board/server_addr" 2>/dev/null || echo none)"

# --- 2. the advertiser gone: the fallbacks, in order ---------------------------

say ""
say "chorus: 2 of 4, nothing advertising: the last good server, then the static address"
kill_quietly "$SERVER"
SERVER=""
sleep 1
set +e
"$ENDPOINT" --no-server --discover --discover-ms 400 --store "$OUT_DIR/board" --run-seconds 1 \
    --log "$OUT_DIR/lastgood.log" >"$OUT_DIR/lastgood.summary" 2>&1
set -e
check "with-nothing-advertising-it-goes-back-to-the-last-server" \
    "$(grep -q "server-located how=last-good address=127.0.0.1:$AUDIO because=discovery returned nothing in 400 ms" \
        "$OUT_DIR/lastgood.summary" && echo 1 || echo 0)" \
    "$(grep 'server-located' "$OUT_DIR/lastgood.summary" | head -n 1)"
set +e
"$ENDPOINT" --server 127.0.0.1:1 --discover --discover-ms 400 --store "$OUT_DIR/fresh" \
    --run-seconds 1 --log "$OUT_DIR/static.log" >"$OUT_DIR/static.summary" 2>&1
set -e
check "a-board-that-has-met-no-server-takes-the-static-address" \
    "$(grep -q 'server-located how=static-fallback address=127.0.0.1:1 because=discovery returned nothing in 400 ms' \
        "$OUT_DIR/static.summary" && echo 1 || echo 0)" \
    "$(grep 'server-located' "$OUT_DIR/static.summary" | head -n 1)"

# --- 3. none of the three --------------------------------------------------------

say ""
say "chorus: 3 of 4, no advertiser, no last server, no address"
set +e
"$ENDPOINT" --no-server --discover --discover-ms 400 --store "$OUT_DIR/fresh-too" \
    --run-seconds 1 >"$OUT_DIR/nowhere.summary" 2>&1
STATUS=$?
set -e
sed 's/^/    /' "$OUT_DIR/nowhere.summary"
check "with-none-of-the-three-it-says-so-and-stops" \
    "$([ "$STATUS" -eq 7 ] && grep -q 'server-located how=nowhere because=discovery returned nothing in 400 ms' \
        "$OUT_DIR/nowhere.summary" && echo 1 || echo 0)" \
    "exit $STATUS: it reports no server rather than one it did not find"

# --- 4. the server appears after the endpoint started --------------------------

say ""
say "chorus: 4 of 4, an endpoint that started before its server finds it by itself"
set +e
"$ENDPOINT" --server 127.0.0.1:1 --discover --discover-ms 400 --store "$OUT_DIR/early" \
    --run-seconds 14 --log "$OUT_DIR/early.log" >"$OUT_DIR/early.summary" 2>&1 &
ENDPOINT_PID=$!
set -e
sleep 2
start_server
set +e
wait "$ENDPOINT_PID"
STATUS=$?
set -e
ENDPOINT_PID=""
sed 's/^/    /' "$OUT_DIR/early.summary"
check "it-started-on-the-fallback" \
    "$(grep -q 'server-located how=static-fallback address=127.0.0.1:1' "$OUT_DIR/early.summary" \
        && echo 1 || echo 0)" \
    "$(grep 'server-located' "$OUT_DIR/early.summary" | head -n 1)"
check "it-asked-again-and-found-the-server" \
    "$(grep -q "event=server-relocated detail=\"server=127.0.0.1:$AUDIO\"" "$OUT_DIR/early.log" \
        && echo 1 || echo 0)" \
    "$(grep -o 'event=server-relocated detail="[^"]*"' "$OUT_DIR/early.log" | head -n 1)"
check "and-played-with-no-human-action" \
    "$([ "$STATUS" -eq 0 ] && [ "$(field chunks "$OUT_DIR/early.summary")" -gt 0 ] && echo 1 || echo 0)" \
    "exit $STATUS, $(field chunks "$OUT_DIR/early.summary") chunks from a server that did not exist when it started"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: a real C endpoint found a real server over real multicast, fell back in order without one, and found it again by itself"
    exit 0
fi
say "chorus: $FAILURES live endpoint-discovery checks did not hold"
exit 1
