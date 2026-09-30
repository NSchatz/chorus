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
# The network is the HOST's (K34, B-15). The server advertises itself by
# multicast DNS and endpoints find it that way; BRIEF section 5.8 says a
# containerized server "should use host networking for multicast to work at
# all", and a bridge publishes ports but carries no multicast. With host
# networking there is nothing to publish: the server binds the host's ports.
#
# The control plane is on (the page, the API, zones and groups), and the zone
# state persists in a named volume, so a restart keeps every zone's name and
# volume.
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
AUDIO_PORT="${CHORUS_PORT:-4010}"
CONTROL_PORT="${CHORUS_CONTROL_PORT:-4020}"
STATE_VOLUME="${CHORUS_STATE_VOLUME:-chorus-state}"

# The ceiling this container is granted. 20 is comfortably above an ordinary
# thread and far below anything the kernel runs.
RTPRIO="${CHORUS_RTPRIO:-20}"

# 64 MiB, which is what the server asks to lock by default.
MEMLOCK="${CHORUS_MEMLOCK:-67108864}"

# The per-thread CPU-time bound, in microseconds, applied by the server itself.
RTTIME_US="${CHORUS_RTTIME_US:-200000}"

# --advertise is the default because discovery is how endpoints find the
# server; CHORUS_ADVERTISE=0 leaves it off for a host where another responder
# already holds UDP 5353 (audit L-3; the server refuses by name, exit 9, rather
# than run unadvertised). Endpoints then use their static fallback address.
ADVERTISE=(--advertise)
if [ "${CHORUS_ADVERTISE:-1}" = 0 ]; then
    ADVERTISE=()
fi

exec docker run \
    --rm \
    --name "$NAME" \
    --network host \
    --volume "$STATE_VOLUME:/var/lib/chorus" \
    --ulimit "rtprio=$RTPRIO" \
    --ulimit "memlock=$MEMLOCK" \
    --cpus "${CHORUS_CPUS:-1.0}" \
    "$IMAGE" \
    --listen "0.0.0.0:$AUDIO_PORT" \
    --control-listen "0.0.0.0:$CONTROL_PORT" \
    --state-file /var/lib/chorus/zones.state \
    "${ADVERTISE[@]}" \
    --source "${CHORUS_SOURCE:-tone}" \
    --rt-priority "$RTPRIO" \
    --rttime-us "$RTTIME_US" \
    --memlock-wanted-bytes "$MEMLOCK" \
    --serve-forever \
    "$@"
