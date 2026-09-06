#!/usr/bin/env bash
# Run a command with UDP port 5353 already bound by something else.
#
# It exists so that `tools/unrun-checks-are-visibly-unrun.sh` can put
# `tools/mdns-live-run.sh` in an environment where its prerequisite is
# GENUINELY absent rather than merely declared absent. This container does carry
# multicast, so the only honest way to see the refusal is to make the port
# genuinely unavailable, which is exactly what a host already running a
# multicast DNS responder looks like.
#
#   ./tools/with-mdns-port-taken.sh <command> [args...]

set -uo pipefail

python3 - <<'PY' &
import socket, time
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.bind(("0.0.0.0", 5353))
time.sleep(120)
PY
HOLDER=$!
# Long enough for the bind above to have happened, and short enough not to be
# felt.
sleep 1

"$@"
STATUS=$?

kill "$HOLDER" 2>/dev/null || true
wait "$HOLDER" 2>/dev/null || true
exit "$STATUS"
