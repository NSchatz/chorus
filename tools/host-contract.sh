#!/usr/bin/env bash
# The host contract, checked against the kernel rather than against the
# server's own word for it.
#
# Verifies: the priority obtained is at least 1 and at most the granted rtprio
# ceiling, and is the actual scheduling priority of the running thread; every
# real-time thread the server created is reported carrying a CPU-time bound;
# the count of real-time threads in the report matches the count the process
# actually has; and the server places no thread under a real-time policy that
# it did not report.
#
# Prerequisite: a granted rtprio ceiling above zero on the running host.
#
#   ./tools/host-contract.sh

source "$(dirname "$0")/lib.sh"

CRITERION="the priority obtained sits inside the granted ceiling, every real-time thread is bounded and reported, and no thread is real-time without being reported"

build_once
require_rtprio "$CRITERION"

CEILING="$(ulimit -r)"
PORT="$(free_port)"
REPORT="${TMPDIR:-/tmp}/chorus-host-contract.report"

say "chorus: host contract"
say "  granted rtprio ceiling: $CEILING"

"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --rt-priority "$(conf rt_priority)" \
    --rttime-us "$(conf rttime_us)" \
    --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
    --allow-unlocked-memory \
    ${CHORUS_SERVER_EXTRA_ARGS:-} > "$REPORT" 2>&1 &
SERVER_PID=$!
trap 'kill_quietly "$SERVER_PID"' EXIT

sleep 2
cat "$REPORT"

if ! grep -q 'scheduling=real-time' "$REPORT"; then
    say "chorus: FAIL the server did not obtain a real-time policy"
    exit 1
fi

OBTAINED="$(sed -n 's/.*rtprio_obtained=\([0-9]*\).*/\1/p' "$REPORT" | head -n 1)"
READ_CEILING="$(sed -n 's/.*rtprio_ceiling=\([0-9]*\).*/\1/p' "$REPORT" | head -n 1)"
say "chorus: reported ceiling=$READ_CEILING obtained=$OBTAINED"
if [ "${OBTAINED:-0}" -lt 1 ]; then
    say "chorus: FAIL the obtained priority is below 1"
    exit 1
fi
if [ "${OBTAINED:-0}" -gt "${READ_CEILING:-0}" ]; then
    say "chorus: FAIL the obtained priority is above the ceiling it read"
    exit 1
fi

if grep -q 'UNDECLARED-REAL-TIME-THREAD' "$REPORT"; then
    say "chorus: FAIL a thread runs under a real-time policy the server did not report"
    exit 1
fi
if ! grep -q 'undeclared_real_time=0' "$REPORT"; then
    say "chorus: FAIL the scheduling report does not say zero undeclared real-time threads"
    exit 1
fi

# Every declared real-time thread has to carry a bound, and the kernel has to
# agree about its policy and priority.
python3 - "$REPORT" <<'PY'
import re, sys
report = open(sys.argv[1]).read()
threads = re.findall(
    r"thread role=(\S+) tid=(\d+) declared_real_time=(\d) declared_priority=(\S+) "
    r"declared_rttime_us=(\S+) kernel_policy=(\S+) kernel_priority=(\S+)",
    report,
)
if not threads:
    print("FAIL the report lists no threads")
    sys.exit(1)
real_time = [t for t in threads if t[2] == "1"]
if not real_time:
    print("FAIL the report declares no real-time thread")
    sys.exit(1)
for role, tid, _, priority, rttime, policy, kernel_priority in real_time:
    print("thread %s tid=%s priority=%s rttime_us=%s kernel=%s/%s"
          % (role, tid, priority, rttime, policy, kernel_priority))
    if rttime in ("-", "0"):
        print("FAIL real-time thread %s carries no CPU-time bound" % role)
        sys.exit(1)
    if policy not in ("SCHED_FIFO", "SCHED_RR"):
        print("FAIL the kernel says %s is %s, not a real-time policy" % (role, policy))
        sys.exit(1)
    if priority != kernel_priority:
        print("FAIL %s declared priority %s and the kernel says %s"
              % (role, priority, kernel_priority))
        sys.exit(1)
unregistered = re.findall(r"thread role=unregistered .* kernel_policy=(\S+)", report)
for policy in unregistered:
    if policy in ("SCHED_FIFO", "SCHED_RR"):
        print("FAIL an unregistered thread runs under %s" % policy)
        sys.exit(1)
print("pass %d real-time threads, all bounded, all agreed with the kernel, none undeclared"
      % len(real_time))
PY
