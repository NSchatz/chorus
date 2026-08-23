#!/usr/bin/env bash
# Run the chorus server container with the contract it needs.
#
# This file is the scheduling contract. The image cannot grant itself a
# real-time priority ceiling or a locked-memory allowance; only the run
# command can, and Docker's own reference documents the spelling: `--ulimit`
# takes `<type>=<soft limit>[:<hard limit>]` and lists `rtprio` as "Maximum
# real-time scheduling priority" and `memlock` as "Maximum locked-in-memory
# address space".
#
# What each grant is for, and what it costs the host:
#
#   --ulimit rtprio=<n>
#       The ceiling on the static priority the server may take. It is a
#       CEILING, not a grant of that priority: the server reads it and asks for
#       no more. Keep it well below 99 so that nothing here can outrank a
#       kernel thread.
#
#   --ulimit memlock=<bytes>
#       How much memory the server may lock. `capabilities(7)` names the
#       alternative, CAP_IPC_LOCK; a limit is the smaller hammer and is what
#       this uses.
#
#   --ulimit rttime is NOT set here.
#       The CPU-time bound is applied by the process itself, per real-time
#       thread, before that thread does any audio work, because it has to be
#       in force from the first instruction rather than from whenever the
#       process got round to it.
#
# The safety property this exists for, in the manual's own words: "A
# nonblocking infinite loop in a thread scheduled under the SCHED_FIFO,
# SCHED_RR, or SCHED_DEADLINE policy can potentially block all other threads
# from accessing the CPU forever." This host runs other things. The bound is
# what stops a bug here from being an outage there.

set -euo pipefail

IMAGE="${CHORUS_IMAGE:-chorus-server:dev}"
NAME="${CHORUS_CONTAINER:-chorus-server}"
PORT="${CHORUS_PORT:-4010}"

# The ceiling this container is granted. 20 is comfortably above an ordinary
# thread and far below anything the kernel runs.
RTPRIO="${CHORUS_RTPRIO:-20}"

# 64 MiB, which is what the server asks to lock by default.
MEMLOCK="${CHORUS_MEMLOCK:-67108864}"

# The per-thread CPU-time bound, in microseconds, applied by the server itself.
RTTIME_US="${CHORUS_RTTIME_US:-200000}"

exec docker run \
    --rm \
    --name "$NAME" \
    --publish "$PORT:4010" \
    --ulimit "rtprio=$RTPRIO" \
    --ulimit "memlock=$MEMLOCK" \
    --cpus "${CHORUS_CPUS:-1.0}" \
    "$IMAGE" \
    --listen "0.0.0.0:4010" \
    --source "${CHORUS_SOURCE:-tone}" \
    --rt-priority "$RTPRIO" \
    --rttime-us "$RTTIME_US" \
    --memlock-wanted-bytes "$MEMLOCK" \
    --serve-forever \
    "$@"
