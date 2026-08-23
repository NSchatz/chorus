#!/usr/bin/env bash
# The spin test: prove the real-time CPU-time bound FIRES, rather than merely
# being configured.
#
# `sched(7)`: "A nonblocking infinite loop in a thread scheduled under the
# SCHED_FIFO, SCHED_RR, or SCHED_DEADLINE policy can potentially block all
# other threads from accessing the CPU forever." This runs exactly that loop,
# with the documented safeguard in place, beside a normal-priority heartbeat.
#
# Verifies, against the heartbeat trace and the process exit:
#   - the spinning process is terminated by its CPU-time limit within one
#     second of the limit expiring;
#   - a normal-priority process started before the spin shows no gap greater
#     than one second in its progress, from the start of the spin until that
#     termination.
#
# A test that only reads the configured limit back does not satisfy this. The
# bound has to fire.
#
# Prerequisite: a granted rtprio ceiling above zero on the running host.
#
#   ./tools/spin-test.sh

source "$(dirname "$0")/lib.sh"

CRITERION="a real-time thread that runs without yielding past its CPU-time limit is terminated by that limit within one second, and a normal-priority process beside it keeps making progress"

build_once
require_rtprio "$CRITERION"

RTTIME_US="$(conf rttime_us)"
RT_PRIORITY="$(conf rt_priority)"
TRACE="${TMPDIR:-/tmp}/chorus-heartbeat.trace"
: > "$TRACE"

say "chorus: spin test"
say "  rttime_us:   $RTTIME_US"
say "  rt_priority: $RT_PRIORITY"
say "  heartbeat:   $TRACE"

# A normal-priority heartbeat, started BEFORE the spin, writing a monotonic
# timestamp every 100 ms. If the real-time thread starves the host, this trace
# has a hole in it.
python3 - "$TRACE" <<'PY' &
import sys, time
path = sys.argv[1]
with open(path, "a", buffering=1) as f:
    while True:
        f.write("%.6f\n" % time.monotonic())
        time.sleep(0.1)
PY
HEARTBEAT_PID=$!
trap 'kill_quietly "$HEARTBEAT_PID"' EXIT

sleep 1
SPIN_STARTED="$(python3 -c 'import time; print("%.6f" % time.monotonic())')"

set +e
"$BIN_DIR/chorus-rt-spin" --rttime-us "$RTTIME_US" --rt-priority "$RT_PRIORITY"
SPIN_STATUS=$?
set -e
SPIN_ENDED="$(python3 -c 'import time; print("%.6f" % time.monotonic())')"

sleep 0.5
kill_quietly "$HEARTBEAT_PID"
trap - EXIT

say "chorus: the spin exited with status $SPIN_STATUS"

# 128 + 24 is SIGXCPU as a shell exit status. 137 is SIGKILL, which is the
# hard-limit path and is also the bound firing.
if [ "$SPIN_STATUS" -eq 3 ]; then
    say "chorus: FAIL the spin never took a real-time policy"
    exit 3
fi
if [ "$SPIN_STATUS" -eq 1 ]; then
    say "chorus: FAIL the CPU-time bound did not fire; the spin outlived it"
    exit 1
fi
if [ "$SPIN_STATUS" -ne 152 ] && [ "$SPIN_STATUS" -ne 137 ]; then
    say "chorus: FAIL the spin exited $SPIN_STATUS, which is neither SIGXCPU (152) nor SIGKILL (137)"
    exit 1
fi
say "chorus: the bound fired: the process was terminated by a signal, not by returning"

python3 - "$TRACE" "$SPIN_STARTED" "$SPIN_ENDED" "$RTTIME_US" <<'PY'
import sys
trace, started, ended, rttime_us = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]), int(sys.argv[4])
stamps = [float(line) for line in open(trace) if line.strip()]
during = [t for t in stamps if started - 0.2 <= t <= ended + 0.2]
if len(during) < 2:
    print("FAIL the heartbeat produced %d stamps during the spin" % len(during))
    sys.exit(1)
gaps = [b - a for a, b in zip(during, during[1:])]
worst = max(gaps)
print("heartbeat: %d stamps during the spin, widest gap %.3f s" % (len(during), worst))
if worst > 1.0:
    print("FAIL a normal-priority process lost more than one second to the spin")
    sys.exit(1)
spin_seconds = ended - started
print("spin: ran %.3f s against a %d us bound" % (spin_seconds, rttime_us))
if spin_seconds > rttime_us / 1e6 + 1.0:
    print("FAIL the bound did not fire within one second of expiring")
    sys.exit(1)
print("pass the bound fired and the host kept making progress")
PY
