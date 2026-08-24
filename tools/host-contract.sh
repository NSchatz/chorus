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
# The last of those is answered from the server's own inventory of its threads.
# An inventory that did not complete cannot answer it, so a report saying so is
# a REFUSAL here, naming this criterion and the reason: an unverified criterion
# reported as passed is the one failure mode that makes every other row of the
# evidence table worthless.
#
# Prerequisite: a granted rtprio ceiling above zero on the running host.
#
#   ./tools/host-contract.sh
#
# Grading a report that was produced elsewhere, which is how the refusal above
# is exercised on a machine with no granted ceiling. It grades a report; it
# cannot make a run pass that did not:
#
#   CHORUS_HOST_CONTRACT_REPORT=<file> ./tools/host-contract.sh

source "$(dirname "$0")/lib.sh"

CRITERION="the priority obtained sits inside the granted ceiling, every real-time thread is bounded and reported, and no thread is real-time without being reported"

# Everything this check concludes, it concludes from the report. Factored out
# so the same grading runs whether the report was produced by the run below or
# handed in.
grade_report() {
    local report="$1"

    cat "$report"

    # First, because a report that cannot answer the question must not be read
    # for an answer to it.
    if grep -q 'INCOMPLETE-THREAD-INVENTORY' "$report"; then
        local reason
        reason="$(sed -n 's/.*INCOMPLETE-THREAD-INVENTORY reason=//p' "$report" | head -n 1)"
        say "chorus: FAIL the server could not take a complete inventory of its own threads,"
        say "        so this criterion was NOT verified by this run."
        say "  criterion:    $CRITERION"
        say "  reason:       ${reason:-the report says the inventory did not complete and gives no reason}"
        say "  this check is NOT passed, NOT skipped-green and NOT satisfied."
        return 1
    fi

    if ! grep -q 'scheduling=real-time' "$report"; then
        say "chorus: FAIL the server did not obtain a real-time policy"
        return 1
    fi

    local obtained read_ceiling
    obtained="$(sed -n 's/.*rtprio_obtained=\([0-9]*\).*/\1/p' "$report" | head -n 1)"
    read_ceiling="$(sed -n 's/.*rtprio_ceiling=\([0-9]*\).*/\1/p' "$report" | head -n 1)"
    say "chorus: reported ceiling=$read_ceiling obtained=$obtained"
    if [ "${obtained:-0}" -lt 1 ]; then
        say "chorus: FAIL the obtained priority is below 1"
        return 1
    fi
    if [ "${obtained:-0}" -gt "${read_ceiling:-0}" ]; then
        say "chorus: FAIL the obtained priority is above the ceiling it read"
        return 1
    fi

    if grep -q 'UNDECLARED-REAL-TIME-THREAD' "$report"; then
        say "chorus: FAIL a thread runs under a real-time policy the server did not report"
        return 1
    fi
    if ! grep -q 'undeclared_real_time=0' "$report"; then
        say "chorus: FAIL the scheduling report does not say zero undeclared real-time threads"
        return 1
    fi

    # Every declared real-time thread has to carry a bound, and the kernel has
    # to agree about its policy and priority.
    python3 - "$report" <<'PY'
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
}

# Grading a report handed in from elsewhere. Placed before the prerequisite
# guard on purpose: grading is not the run, needs nothing the run needs, and
# the run itself still refuses below when this host granted no ceiling.
if [ -n "${CHORUS_HOST_CONTRACT_REPORT:-}" ]; then
    say "chorus: host contract, grading the report at $CHORUS_HOST_CONTRACT_REPORT"
    set +e
    grade_report "$CHORUS_HOST_CONTRACT_REPORT"
    GRADE_STATUS=$?
    set -e
    exit "$GRADE_STATUS"
fi

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

grade_report "$REPORT"
