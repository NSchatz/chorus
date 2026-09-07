#!/usr/bin/env bash
# AC-2's fallback half, end to end and all the way to PLAYING.
#
# "WHEN an endpoint starts with no configured server address THE SYSTEM SHALL
# discover one by mDNS and SHALL fall back to a static address when discovery
# returns nothing."
#
# The spec grades that half like this: "an endpoint started with no server
# address and a discovery attempt that returns nothing connects to the
# configured static address AND PLAYS". The word that costs something is
# "plays". `cargo test -p chorus-discovery --test dnssd_vectors` grades the
# DECISION - the resolver answers `Located::Fallback` - and tools/mdns-live-run.sh
# grades the decision again against a real link, but neither of them plays a
# frame: the live one uses a device that does not exist on purpose, because what
# it is grading is the multicast exchange.
#
# So this is the sentence run to its end. A real `chorus-server` at a static
# address, nothing at all advertising on the link, and a real `chorus-client`
# started with `--discover`: it must browse, find nothing, fall back to the
# address it was configured with, open a session against THAT address, and
# advance a played-frame counter.
#
# What this needs and what it does not:
#
# - It needs a playback device that OPENS. The ALSA `null` device is enough,
#   because what is graded is whether frames were played and against which
#   address, not the value of any delay the device reports.
# - It needs NO multicast. The fallback is what happens when the browse answers
#   nothing, and a browse that cannot run at all is a DIFFERENT antecedent from
#   the one the criterion states - so this refuses by name in that case rather
#   than passing on it.
#
#   CHORUS_CLIENT_DEVICE=null ./tools/discovery-fallback-run.sh
#   # or: make verify-discovery-fallback

source "$(dirname "$0")/lib.sh"

build_once

CRITERION="an endpoint started with no server address, whose discovery attempt returns nothing, connects to the configured static address and plays"
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

OUT_DIR="${TMPDIR:-/tmp}/chorus-discovery-fallback"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"
DEVICE="$(audio_device)"
CONTRACT_ARGS="$(server_contract_args)"
AUDIO="$(free_port)"
WINDOW_MS=400
RUN_SECONDS=6

SERVER=""
trap 'kill_quietly "$SERVER"' EXIT

say ""
say "chorus: the static fallback, all the way to playing"
say "  criterion: $CRITERION"

# A real server, and deliberately NO --advertise: there is nothing on this link
# for a browse to find, which is the criterion's antecedent.
"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$AUDIO" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    --serve-forever \
    $CONTRACT_ARGS >"$OUT_DIR/server.log" 2>&1 &
SERVER=$!
WAITED=0
while [ "$WAITED" -lt 100 ]; do
    if grep -q 'listening on=' "$OUT_DIR/server.log" 2>/dev/null; then
        break
    fi
    sleep 0.2
    WAITED=$(( WAITED + 1 ))
done
if ! grep -q 'listening on=' "$OUT_DIR/server.log" 2>/dev/null; then
    say "FAIL the server never came up; it said:"
    sed 's/^/    /' "$OUT_DIR/server.log"
    exit 1
fi
check "nothing-is-advertising-on-this-link" \
    "$(grep -q 'advertising instances=' "$OUT_DIR/server.log" && echo 0 || echo 1)" \
    "the server was started without --advertise, so a browse has nothing to find"

# --discover with a static address: browse first, fall back second. This is the
# endpoint the criterion describes.
set +e
OUT="$("$BIN_DIR/chorus-client" \
    --server "127.0.0.1:$AUDIO" \
    --discover --discover-ms "$WINDOW_MS" \
    --device "$DEVICE" \
    --run-seconds "$RUN_SECONDS" \
    --delay-log "$OUT_DIR/endpoint.log" 2>&1)"
STATUS=$?
set -e
printf '%s\n' "$OUT" | sed 's/^/    /'

# A browse that could not run at all is not "discovery returned nothing". The
# criterion's antecedent was not reached, so this refuses by name rather than
# reporting the criterion satisfied by an easier case.
if printf '%s' "$OUT" | grep -q 'because=could not run at all'; then
    missing_prerequisite \
        "$CRITERION" \
        "a link on which an mDNS browse can run at all; it reported: $(printf '%s' "$OUT" | grep -o 'because=could not run at all[^ ]*.*' | head -n 1)" \
        "run this where UDP port 5353 can be bound and the group 224.0.0.251 joined, so that a browse can return nothing rather than fail"
fi

# `|| true` throughout: lib.sh runs with `pipefail`, and every one of these is a
# grep that is ALLOWED to find nothing - finding nothing is what the check below
# it reports as a failure, in a sentence, rather than what kills the script.
LOCATED="$(printf '%s' "$OUT" | grep 'server-located' | head -n 1 || true)"

check "the-browse-ran-and-returned-nothing" \
    "$(printf '%s' "$OUT" | grep -q "because=returned nothing in $WINDOW_MS ms" && echo 1 || echo 0)" \
    "${LOCATED:-no server-located line at all}"
check "it-did-not-discover-a-server" \
    "$(printf '%s' "$OUT" | grep -q 'how=mdns' && echo 0 || echo 1)" \
    "no 'how=mdns' line: nothing was found, so the fallback is a fallback"
check "it-fell-back-to-the-address-it-was-configured-with" \
    "$(printf '%s' "$OUT" | grep -q "server-located how=static-fallback address=127.0.0.1:$AUDIO" \
        && echo 1 || echo 0)" \
    "the configured static address is 127.0.0.1:$AUDIO"

# "and plays". Read off the session summary the endpoint printed on its way out
# and off the delay log it wrote, and tied to the address it fell back to, so a
# run that played against something else cannot satisfy this.
SESSION="$(printf '%s' "$OUT" | grep 'session n=1 ' | head -n 1 || true)"
FRAMES="$(printf '%s' "$SESSION" | grep -o ' frames_played=[0-9]*' | head -n 1 | cut -d= -f2 || true)"
check "the-session-it-opened-was-against-the-fallback-address" \
    "$(printf '%s' "$SESSION" | grep -q "server=127.0.0.1:$AUDIO played=1" && echo 1 || echo 0)" \
    "${SESSION:-no session summary at all}"
check "it-played" \
    "$([ "${FRAMES:-0}" -gt 0 ] && echo 1 || echo 0)" \
    "$FRAMES frames reached the device on that session"
CHUNKS="$(printf '%s' "$OUT" | grep -o 'chunks_played=[0-9]*' | head -n 1 | cut -d= -f2 || true)"
CHUNKS_OK=0
if [ "${CHUNKS:-0}" -gt 0 ]; then
    CHUNKS_OK=1
fi
check "it-reported-chunks-reaching-the-device-as-well-as-frames" \
    "$CHUNKS_OK" \
    "chunks_played=${CHUNKS:-none reported}"
ORDERLY=0
if [ "$STATUS" -eq 0 ] && printf '%s' "$OUT" | grep -q 'stop=run-length-reached'; then
    ORDERLY=1
fi
check "it-stopped-because-the-run-was-over-and-not-because-anything-failed" \
    "$ORDERLY" \
    "exit $STATUS, $(printf '%s' "$OUT" | grep -o 'stop=[a-z-]*' | head -n 1)"

# And the other process agrees that something connected to it and was served.
check "the-server-saw-the-endpoint-connect" \
    "$(grep -q 'client connected' "$OUT_DIR/server.log" && echo 1 || echo 0)" \
    "$(grep -c 'client connected' "$OUT_DIR/server.log") connection(s) on the static address"

# The contrast that makes the checks above about the FALLBACK route and not
# about --server on its own: the same client, same server, no --discover.
set +e
CONFIGURED="$("$BIN_DIR/chorus-client" \
    --server "127.0.0.1:$AUDIO" \
    --device "$DEVICE" \
    --run-seconds 3 \
    --delay-log "$OUT_DIR/configured.log" 2>&1)"
set -e
CONTRAST=0
if printf '%s' "$CONFIGURED" | grep -q 'server-located how=configured' \
    && ! printf '%s' "$CONFIGURED" | grep -q 'how=static-fallback'; then
    CONTRAST=1
fi
check "without---discover-the-same-address-is-reported-as-configured-and-not-as-a-fallback" \
    "$CONTRAST" \
    "$(printf '%s' "$CONFIGURED" | grep 'server-located' | head -n 1)"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: a browse returned nothing, the endpoint fell back to its static address and played $FRAMES frames"
    exit 0
fi
say "chorus: $FAILURES fallback checks did not hold"
exit 1
