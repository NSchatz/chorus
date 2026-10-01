#!/usr/bin/env bash
# The ALSA `null` runs `make gate` carries: the real server and the real client
# binaries against the ALSA `null` device, host / ALSA null, not hardware, and
# the client's line-in probe on `null`'s capture side.
#
# What runs is the fastest subset that exercises the client's real ALSA path
# end to end: `tools/stream-end-and-loss.sh` (a clean end and a lost server,
# AC-20 and AC-21, a few seconds) and the restart storm (AC-3: four endpoints,
# the server SIGKILLed and replaced, all four back by themselves at the zone
# state they left, about 40 s). Left out for time, and run by hand with
# `make verify-null-device` and `make ten-minute-run-null`: the one-minute
# start-fill and log-shape run, and the ten-minute run. Nothing that
# grades a reported delay can run on `null`, which reports zero for ever, and
# the runners that need one keep refusing it by name.
#
# The client loads libasound.so.2 at run time. Where the system has none (a
# development container has no apt), the rootless conda-forge install at
# CHORUS_ALSA_PREFIX (default /cache/opt/chorus-alsa) is put on the loader's
# path, the way the gate finds ccache. With neither, this refuses by name with
# the install command; under CI (CI=true), where /cache does not exist, it
# prints SKIPPED with the reason instead, as the identity scan does for its
# private term list.
#
#   make verify-alsa-null

source "$(dirname "$0")/lib.sh"

if ! use_rootless_alsa; then
    if [ "${CI:-}" = true ]; then
        say "SKIPPED: no libasound.so.2 on the system and no rootless alsa-lib at $(alsa_prefix); the ALSA null runs need one, and under CI there is no /cache to hold it"
        exit 0
    fi
    missing_prerequisite \
        "the real server and client binaries run on the ALSA null device (stream end and loss, and the restart storm)" \
        "libasound.so.2: the system has none and $(alsa_prefix)/lib holds none" \
        "$ALSA_INSTALL_HINT; or set CHORUS_ALSA_PREFIX to an existing install"
fi

export CHORUS_CLIENT_DEVICE=null
say "chorus: ALSA null runs (host / ALSA null, not hardware); LD_LIBRARY_PATH=${LD_LIBRARY_PATH:-system}"
bash "$REPO_ROOT/tools/stream-end-and-loss.sh"
bash "$REPO_ROOT/tools/restart-storm-run.sh"

# The line-in's capture path (goal 10, ADR 0066) on ALSA's `null` capture
# device, which "creates a stream with zero samples" (alsa-lib's PCM plugin
# page, https://www.alsa-project.org/alsa-doc/alsa-lib/pcm_plugins.html, read
# 2026-09-30): the shipped binary opens it through the same dlopen binding,
# reads 200 ms, asks the capture delay, and must find no signal in silence.
build_once
probe="$("$BIN_DIR/chorus-client" --probe-line-in --line-in null 2>&1)" || {
    printf '%s\n' "$probe" >&2
    say "chorus: the line-in probe on the ALSA null capture device FAILED"
    exit 1
}
say "$probe"
case "$probe" in
    *"line-in-probe device=null usable=1 frames_read=9600 overran=0 "*"signal=0"*) ;;
    *)
        say "chorus: the line-in probe on null did not read 9600 frames of silence without an overrun"
        exit 1
        ;;
esac
say "chorus: ALSA null runs passed (host / ALSA null): stream-end-and-loss, the restart storm and the line-in capture probe"
