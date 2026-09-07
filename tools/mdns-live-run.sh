#!/usr/bin/env bash
# AC-2's live half: a real advertiser and a real browse, over real multicast.
#
# "WHEN an endpoint starts with no configured server address THE SYSTEM SHALL
# discover one by mDNS and SHALL fall back to a static address when discovery
# returns nothing."
#
# The BYTES half of that criterion needs none of this and is graded with no
# network at all, against committed packets, by `cargo test -p chorus-discovery
# --test dnssd_vectors`. This is the additional check: an advertiser and a
# resolver on one link, exchanging real datagrams.
#
# The roadmap anticipated that it might not run: "whether multicast reaches a
# container and crosses this network's VLANs is an open question, and the
# fallback makes the phase gradeable either way." So this either passes or
# refuses BY NAME, and whichever happened is recorded. A live exchange that
# could not run is never reported as one that passed.
#
#   ./tools/mdns-live-run.sh          # or: make verify-mdns

source "$(dirname "$0")/lib.sh"

build_once

CRITERION="an endpoint with no configured server address discovers one by mDNS on a real link"

say "chorus: the live multicast exchange"
say "  criterion: $CRITERION"
say "  group:     224.0.0.251, UDP port 5353 (RFC 6762 sections 2 and 3)"
say "  services:  _chorus-audio._tcp.local. and _chorus-ctl._tcp.local."

require_multicast "$CRITERION"

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

OUT_DIR="${TMPDIR:-/tmp}/chorus-mdns-live"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"
AUDIO="$(free_port)"
CONTROL="$(free_port)"
CONTRACT_ARGS="$(server_contract_args)"

SERVER=""
trap 'kill_quietly "$SERVER"' EXIT

"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$AUDIO" \
    --source tone \
    --serve-forever \
    --control-listen "127.0.0.1:$CONTROL" \
    --zone kitchen \
    --advertise \
    --instance chorus-live \
    $CONTRACT_ARGS >"$OUT_DIR/server.log" 2>&1 &
SERVER=$!
sleep 2

if ! grep -q 'advertising instances=' "$OUT_DIR/server.log"; then
    sed 's/^/    /' "$OUT_DIR/server.log"
    missing_prerequisite \
        "$CRITERION" \
        "a server able to advertise on this link; it refused: $(grep -m1 'could not advertise' "$OUT_DIR/server.log" || echo 'see the log above')" \
        "run this where UDP port 5353 can be bound and the group 224.0.0.251 joined"
fi
check "the-server-advertises-both-services" \
    "$(grep -q '_chorus-audio._tcp.local' "$OUT_DIR/server.log" \
        && grep -q '_chorus-ctl._tcp.local' "$OUT_DIR/server.log" && echo 1 || echo 0)" \
    "$(grep 'advertising instances=' "$OUT_DIR/server.log" | head -n 1)"

# An endpoint started with NO static address at all: --no-server takes the
# default away, so if discovery returns nothing this run has nowhere to go and
# exits 7 saying so. That is the whole point of the check.
set +e
OUT="$("$BIN_DIR/chorus-client" \
    --no-server --discover \
    --device chorus-no-such-device \
    --delay-log "$OUT_DIR/unused.log" 2>&1)"
STATUS=$?
set -e
printf '%s\n' "$OUT" | sed 's/^/    /'

check "the-endpoint-found-a-server-on-the-link" \
    "$(printf '%s' "$OUT" | grep -q 'server-located how=mdns' && echo 1 || echo 0)" \
    "$(printf '%s' "$OUT" | grep 'server-located' | head -n 1)"
check "what-it-found-is-the-server-that-is-running" \
    "$(printf '%s' "$OUT" | grep -q "server-located how=mdns address=[^ ]*:$AUDIO" && echo 1 || echo 0)" \
    "the advertised port is $AUDIO, which is what this server bound"
check "the-endpoint-did-not-fall-back" \
    "$(printf '%s' "$OUT" | grep -q 'how=static-fallback' && echo 0 || echo 1)" \
    "discovery answered, so the fallback was not taken"
# It got as far as the device, which is the next thing after finding a server
# and is the only reason this run exits non-zero.
check "it-went-on-to-use-what-it-found" \
    "$([ "$STATUS" -eq 4 ] && echo 1 || echo 0)" \
    "exit $STATUS: it reached the audio device, which does not exist here, having found a server"

# And the other direction: with the advertiser gone, the same endpoint falls
# back to the static address rather than reporting a server it did not find.
kill_quietly "$SERVER"
SERVER=""
sleep 1
set +e
OUT="$("$BIN_DIR/chorus-client" \
    --server 127.0.0.1:1 --discover --discover-ms 400 \
    --device chorus-no-such-device \
    --delay-log "$OUT_DIR/unused.log" 2>&1)"
set -e
check "with-nothing-advertising-it-falls-back-to-the-static-address" \
    "$(printf '%s' "$OUT" | grep -q 'server-located how=static-fallback address=127.0.0.1:1' \
        && echo 1 || echo 0)" \
    "$(printf '%s' "$OUT" | grep 'server-located' | head -n 1)"
check "and-the-fallback-says-why-it-was-taken" \
    "$(printf '%s' "$OUT" | grep -q 'because=returned nothing' && echo 1 || echo 0)" \
    "$(printf '%s' "$OUT" | grep 'because=' | head -n 1)"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: a real endpoint found a real server over real multicast, and fell back without one"
    exit 0
fi
say "chorus: $FAILURES live-discovery checks did not hold"
exit 1
