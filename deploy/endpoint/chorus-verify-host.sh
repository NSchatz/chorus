#!/usr/bin/env bash
# chorus-verify-host: `make verify-host` on an installed Linux endpoint.
#
# Installed as /usr/bin/chorus-verify-host by the chorus-endpoint package. It
# runs the repository's own two checks, byte for byte the scripts `make
# verify-host` runs (tools/host-contract.sh and tools/spin-test.sh, copied with
# tools/lib.sh and config/verification.conf under /usr/lib/chorus/verify-host),
# against the probes the package installs under /usr/lib/chorus/probe instead
# of a cargo build:
#
#   - the host contract: chorus-server takes its real-time contract on this
#     kernel, and the report is graded against /proc (the priority inside the
#     granted ceiling, every real-time thread bounded and reported);
#   - the spin test: chorus-rt-spin spins under SCHED_FIFO past its CPU-time
#     bound, which has to FIRE within a second while a normal-priority
#     heartbeat keeps going.
#
# Both need a granted rtprio ceiling. A login shell has none (RLIMIT_RTPRIO 0
# on Debian), so each refuses by name (exit 3) unless it is run with the
# service's limits, for example:
#
#   sudo systemd-run --pty --wait --collect -p DynamicUser=yes \
#       -p LimitRTPRIO=20 -p LimitRTTIME=200ms -p LimitMEMLOCK=64M \
#       /usr/bin/chorus-verify-host
#
# Both run even when the first fails, as `make verify-host` would report both;
# the exit status is the first non-zero one. python3 is needed (the graders and
# the heartbeat are Python).

set -u
# CHORUS_ENDPOINT_ROOT: an unpacked package tree instead of /, for the
# package's own check (tools/endpoint-package.sh).
PREFIX="${CHORUS_ENDPOINT_ROOT:-}"
LIB="$PREFIX/usr/lib/chorus/verify-host"
export CHORUS_SKIP_BUILD=1
export CHORUS_BIN_DIR="$PREFIX/usr/lib/chorus/probe"

rc=0
for check in host-contract spin-test; do
    echo "chorus-verify-host: $check"
    bash "$LIB/tools/$check.sh"
    status=$?
    echo "chorus-verify-host: $check exited $status"
    if [ "$rc" -eq 0 ] && [ "$status" -ne 0 ]; then
        rc=$status
    fi
done
exit "$rc"
