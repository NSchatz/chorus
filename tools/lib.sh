#!/usr/bin/env bash
# Shared by every verification entry point.
#
# The one rule these enforce: a check whose environment is not here exits
# non-zero, names the prerequisite AND the criterion it was verifying, and is
# never reported as passed, skipped-green, or otherwise satisfied. An unrun
# check has to be visibly unrun, or the evidence table is a promise rather than
# a record.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export REPO_ROOT

TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
BIN_DIR="$TARGET_DIR/debug"
export BIN_DIR

say() { printf '%s\n' "$*"; }

# Like say, but on stderr, so it can be used inside a command substitution
# without becoming part of the value.
note() { printf '%s\n' "$*" >&2; }

# Exit non-zero naming the missing prerequisite and the criterion it blocks.
missing_prerequisite() {
    local criterion="$1"
    local prerequisite="$2"
    local how="$3"
    printf 'MISSING PREREQUISITE\n' >&2
    printf '  criterion:    %s\n' "$criterion" >&2
    printf '  prerequisite: %s\n' "$prerequisite" >&2
    printf '  how to get it: %s\n' "$how" >&2
    printf '  this check is NOT passed, NOT skipped-green and NOT satisfied.\n' >&2
    exit 3
}

build_once() {
    if [ "${CHORUS_SKIP_BUILD:-0}" = "1" ]; then
        return
    fi
    (cd "$REPO_ROOT" && cargo build --quiet --workspace --all-targets)
}

# A value from config/verification.conf.
conf() {
    local key="$1"
    local value
    value="$(sed -n "s/^[[:space:]]*${key}[[:space:]]*=[[:space:]]*\\([^#]*\\).*/\\1/p" \
        "$REPO_ROOT/config/verification.conf" | head -n 1 | tr -d '[:space:]')"
    if [ -z "$value" ]; then
        printf 'config/verification.conf has no %s\n' "$key" >&2
        exit 2
    fi
    printf '%s' "$value"
}

# A value from config/sync.conf. The constants SYNC-4 fixed live there and are
# passed to the client from there, so a check and the thing it checks cannot
# drift apart; crates/client-linux/tests/sync_loop.rs asserts the file and the
# compiled constants agree.
sync_conf() {
    local key="$1"
    local value
    value="$(sed -n "s/^[[:space:]]*${key}[[:space:]]*=[[:space:]]*\\([^#]*\\).*/\\1/p" \
        "$REPO_ROOT/config/sync.conf" | head -n 1 | tr -d '[:space:]')"
    if [ -z "$value" ]; then
        printf 'config/sync.conf has no %s\n' "$key" >&2
        exit 2
    fi
    printf '%s' "$value"
}

# The device a run should use. `default` is the operator's real card; CI and a
# container without one pass something else, and get told when it is not there.
audio_device() {
    printf '%s' "${CHORUS_CLIENT_DEVICE:-default}"
}

# Refuse to continue unless the configured device can be opened at all.
require_audio_device() {
    local criterion="$1"
    local device
    device="$(audio_device)"
    if ! "$BIN_DIR/chorus-client" --device "$device" --probe-device >/dev/null 2>&1; then
        local detail
        detail="$({ "$BIN_DIR/chorus-client" --device "$device" --probe-device 2>&1 || true; } \
            | head -n 3 | tr '\n' ' ')"
        missing_prerequisite \
            "$criterion" \
            "a usable ALSA playback device; '$device' could not be opened ($detail)" \
            "point CHORUS_CLIENT_DEVICE at a real card, a snd-aloop loopback, or the ALSA 'null' device"
    fi
}

# Refuse to continue unless the configured device actually has a ring to report
# a delay about. The ALSA `null` device opens and reports zero delay forever;
# it is a device, and it cannot verify anything the delay is part of.
require_pacing_audio_device() {
    local criterion="$1"
    local device
    device="$(audio_device)"
    if ! "$BIN_DIR/chorus-client" --device "$device" --probe-device --require-pacing \
        >/dev/null 2>&1; then
        local detail
        detail="$({ "$BIN_DIR/chorus-client" --device "$device" --probe-device --require-pacing \
            2>&1 || true; } | grep -E 'device-probe|reports no delay|no usable' | head -n 2 \
            | tr '\n' ' ' || true)"
        missing_prerequisite \
            "$criterion" \
            "an ALSA playback device that reports a delay; '$device' does not ($detail)" \
            "point CHORUS_CLIENT_DEVICE at a real card or a snd-aloop loopback; the ALSA 'null' device is not one, because it accepts every frame instantly and reports a delay of zero"
    fi
}

# The capture device a measurement run should use. `default` is whatever ALSA
# calls the operator's interface; a machine with no interface at all has none
# under any name, which is exactly the case the guard below exists for.
capture_device() {
    printf '%s' "${CHORUS_CAPTURE_DEVICE:-default}"
}

# Refuse to continue unless a capture device can be opened for two channels at
# the rate config/measure.conf declares.
#
# The probe is the measurement binary's own, so this guard and the tool cannot
# disagree about what "usable" means. Note that this is a CAPTURE device and
# require_audio_device above is about a PLAYBACK one: a machine can have either
# without the other, and a measurement run needs the capture side.
require_capture_device() {
    local criterion="$1"
    local device
    device="$(capture_device)"
    if ! "$BIN_DIR/chorus-measure-capture" --probe-capture-device \
        --capture-device "$device" >/dev/null 2>&1; then
        local detail
        detail="$({ "$BIN_DIR/chorus-measure-capture" --probe-capture-device \
            --capture-device "$device" 2>&1 || true; } \
            | grep -E 'capture-device-probe' | head -n 1 | tr '\n' ' ' || true)"
        missing_prerequisite \
            "$criterion" \
            "an ALSA capture device that opens two channels at the declared rate; '$device' does not ($detail)" \
            "connect both endpoints' line outputs to the L and R inputs of one audio interface and point CHORUS_CAPTURE_DEVICE at it"
    fi
}

# Refuse to continue unless this container was granted a real-time priority.
require_rtprio() {
    local criterion="$1"
    local ceiling
    ceiling="$(ulimit -r 2>/dev/null || printf '0')"
    if [ "$ceiling" = "unlimited" ]; then
        return
    fi
    if [ "${ceiling:-0}" -le 0 ]; then
        missing_prerequisite \
            "$criterion" \
            "a granted rtprio ceiling above zero; RLIMIT_RTPRIO reads ${ceiling:-0} here" \
            "run in a container started with 'docker run --ulimit rtprio=<n>', as deploy/run-server.sh does"
    fi
}

# Refuse to continue unless the host allows locking as much memory as the
# server asks for.
require_memlock() {
    local criterion="$1"
    local wanted_bytes="$2"
    local limit_kb
    limit_kb="$(ulimit -l 2>/dev/null || printf '0')"
    if [ "$limit_kb" = "unlimited" ]; then
        return
    fi
    local limit_bytes=$(( ${limit_kb:-0} * 1024 ))
    if [ "$limit_bytes" -lt "$wanted_bytes" ]; then
        missing_prerequisite \
            "$criterion" \
            "a locked-memory limit of at least $wanted_bytes bytes; RLIMIT_MEMLOCK reads $limit_bytes bytes here" \
            "run in a container started with 'docker run --ulimit memlock=<bytes>', as deploy/run-server.sh does"
    fi
}

# Refuse to continue unless the operator named a SECOND endpoint to play the
# grouped stream on. One endpoint cannot be inaudibly apart from anything.
require_second_endpoint() {
    local criterion="$1"
    if [ -z "${CHORUS_SECOND_ENDPOINT:-}" ]; then
        missing_prerequisite \
            "$criterion" \
            "a second wired Linux endpoint to play the same grouped stream; CHORUS_SECOND_ENDPOINT names none" \
            "set CHORUS_SECOND_ENDPOINT to 'user@host' for a second wired Linux machine with an ALSA output, wire both endpoints' line outputs into one audio interface, and point CHORUS_CAPTURE_DEVICE at it"
    fi
}

# A value from firmware/config/endpoint.conf. Same shape as conf() and
# sync_conf(): the endpoint's committed numbers are passed to a run from the
# one file that declares them, so a check and the thing it checks cannot drift
# apart.
endpoint_conf() {
    local key="$1"
    local value
    value="$(sed -n "s/^[[:space:]]*${key}[[:space:]]*=[[:space:]]*\\([^#]*\\).*/\\1/p" \
        "$REPO_ROOT/firmware/config/endpoint.conf" | head -n 1 | tr -d '[:space:]')"
    if [ -z "$value" ]; then
        printf 'firmware/config/endpoint.conf has no %s\n' "$key" >&2
        exit 2
    fi
    printf '%s' "$value"
}

# Refuse to continue unless the ESP-IDF toolchain the endpoint is built with is
# installed AND is the version firmware/config/endpoint.conf declares.
#
# Both halves matter. An absent toolchain cannot build an image; a DIFFERENT
# toolchain can, and the image it builds is one nobody chose, which is worse
# because it looks like success.
require_espidf() {
    local criterion="$1"
    local wanted
    wanted="$(endpoint_conf espidf_version)"
    if [ -z "${IDF_PATH:-}" ]; then
        missing_prerequisite \
            "$criterion" \
            "the ESP-IDF toolchain at version $wanted; IDF_PATH is unset, so no toolchain is installed here" \
            "install ESP-IDF $wanted and source its export.sh, which sets IDF_PATH and puts idf.py on PATH"
    fi
    if ! command -v idf.py >/dev/null 2>&1; then
        missing_prerequisite \
            "$criterion" \
            "the ESP-IDF toolchain at version $wanted; IDF_PATH is ${IDF_PATH} but idf.py is not on PATH" \
            "source \$IDF_PATH/export.sh so idf.py is on PATH"
    fi
    local found
    found="$(idf.py --version 2>/dev/null | head -n 1 || true)"
    if ! printf '%s' "$found" | grep -q "$wanted"; then
        missing_prerequisite \
            "$criterion" \
            "the ESP-IDF toolchain at version $wanted; the installed one reports '${found:-nothing}'" \
            "install ESP-IDF $wanted, or change espidf_version in firmware/config/endpoint.conf deliberately and say why in docs/decisions/"
    fi
}

# Refuse to continue unless the operator named an ESP32-S3 endpoint to run on.
require_esp32s3_endpoint() {
    local criterion="$1"
    if [ -z "${CHORUS_ESP32S3_PORT:-}" ]; then
        missing_prerequisite \
            "$criterion" \
            "an ESP32-S3 endpoint with a TAS5825M-class amplifier and a real loudspeaker attached; CHORUS_ESP32S3_PORT names no serial port" \
            "flash the endpoint onto an ESP32-S3 board wired to the amplifier per firmware/config/endpoint.conf's pin map, and set CHORUS_ESP32S3_PORT to its serial port"
    fi
    if [ ! -e "${CHORUS_ESP32S3_PORT}" ]; then
        missing_prerequisite \
            "$criterion" \
            "an ESP32-S3 endpoint at ${CHORUS_ESP32S3_PORT}; nothing is there" \
            "attach the board and point CHORUS_ESP32S3_PORT at the serial port it appears as"
    fi
}

# Refuse to continue unless the amplifier's register map has been read off the
# datasheet and written into firmware/config/endpoint.conf.
#
# This phase declares every one of those values `unknown` and names no address.
# A hardware run cannot happen until somebody has read them, and refusing here
# is how that stays visible rather than becoming a silent zero.
require_amplifier_registers() {
    local criterion="$1"
    local unknown=""
    local key
    for key in amp_i2c_address amp_reg_device_id amp_reg_fault amp_reg_analog_gain \
        amp_reg_state_control amp_device_id_value amp_fault_clear_value amp_analog_gain_code; do
        if [ "$(endpoint_conf "$key")" = "unknown" ]; then
            unknown="$unknown $key"
        fi
    done
    if [ -n "$unknown" ]; then
        missing_prerequisite \
            "$criterion" \
            "the TAS5825M register map; firmware/config/endpoint.conf still declares these UNKNOWN:$unknown" \
            "read each value off TI's TAS5825M datasheet at bring-up and write it into firmware/config/endpoint.conf. This phase asserts the bring-up behaviour and names no address, which is why they are unknown here"
    fi
}

# Refuse to continue unless there is a real browser engine AND a driver for it.
#
# Both halves matter and neither substitutes for the other. A criterion about a
# RENDERED page is answered by rendering it: what a rule applies to, what wins
# the cascade and what is actually painted are decided by an engine, and a check
# that read the CSS instead would be a check on the stylesheet's text. So this
# refuses rather than degrading, and the refusal names the exact install.
require_browser_driver() {
    local criterion="$1"
    local browser="${CHORUS_BROWSER:-/usr/bin/chromium}"
    if [ ! -x "$browser" ]; then
        missing_prerequisite \
            "$criterion" \
            "a real browser engine to render the page in; '$browser' is not executable here" \
            "install Chromium and point CHORUS_BROWSER at it. Reading the HTML or the CSS instead is not an option: the criterion is about the rendered box"
    fi
    if ! command -v node >/dev/null 2>&1; then
        missing_prerequisite \
            "$criterion" \
            "node, to run the rendering driver; it is not on PATH" \
            "mise use node@22"
    fi
    if [ ! -d "$REPO_ROOT/tools/ui/node_modules/@playwright/test" ]; then
        missing_prerequisite \
            "$criterion" \
            "the @playwright/test driver under tools/ui; it is not installed" \
            "cd tools/ui && pnpm install --ignore-scripts, with PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 since the browser is already here"
    fi
}

# Refuse to continue unless the operator gave this run the three days of wall
# clock the soak criterion asks for.
#
# There is no way to shorten this one and no way to model it: the criterion is
# about a system left alone for three days, and a shorter run is a different
# claim. What stands beside it is a MODELLED 72 hours, which the verification
# record labels a modelled result and not a measurement.
require_soak_window() {
    local criterion="$1"
    local wanted="$2"
    local granted="${CHORUS_SOAK_SECONDS:-0}"
    if ! printf '%s' "$granted" | grep -qE '^[0-9]+$' || [ "$granted" -lt "$wanted" ]; then
        missing_prerequisite \
            "$criterion" \
            "at least $wanted seconds of wall clock to run in; CHORUS_SOAK_SECONDS grants ${granted:-0}" \
            "run this on a host that can be left alone for three days and set CHORUS_SOAK_SECONDS=$wanted. A shorter run is a different claim and this will not make it"
    fi
}

# Refuse to continue unless multicast is usable on this link.
#
# Whether multicast reaches a container and crosses this network's VLANs is an
# open question in this deployment, which is why the endpoint has a static
# fallback at all. This guard is what keeps the answer visible either way: the
# live exchange runs where it can and refuses by name where it cannot, and
# neither is reported as the other.
require_multicast() {
    local criterion="$1"
    if [ "${CHORUS_NO_MULTICAST:-0}" = "1" ]; then
        missing_prerequisite \
            "$criterion" \
            "a link that carries multicast DNS; CHORUS_NO_MULTICAST says this one does not" \
            "run this where UDP port 5353 and the group 224.0.0.251 are reachable, then unset CHORUS_NO_MULTICAST"
    fi
    if ! "$BIN_DIR/chorus-mdns-probe" >/dev/null 2>&1; then
        local detail
        detail="$({ "$BIN_DIR/chorus-mdns-probe" 2>&1 || true; } | head -n 2 | tr '\n' ' ')"
        missing_prerequisite \
            "$criterion" \
            "a link that carries multicast DNS; the probe could not use it ($detail)" \
            "run this where UDP port 5353 can be bound and the group 224.0.0.251 joined. The endpoint's static fallback exists precisely because this cannot be assumed"
    fi
}

# Refuse to continue unless the operator named a device that can be removed.
require_removable_device() {
    local criterion="$1"
    if [ -z "${CHORUS_REMOVABLE_DEVICE:-}" ]; then
        missing_prerequisite \
            "$criterion" \
            "a device that can be removed or made unusable mid-run" \
            "set CHORUS_REMOVABLE_DEVICE to an ALSA device you can unplug or unbind, and CHORUS_REMOVE_COMMAND to the command that removes it"
    fi
    if [ -z "${CHORUS_REMOVE_COMMAND:-}" ]; then
        missing_prerequisite \
            "$criterion" \
            "a command that makes CHORUS_REMOVABLE_DEVICE unusable mid-run" \
            "set CHORUS_REMOVE_COMMAND to the command that removes the device named in CHORUS_REMOVABLE_DEVICE"
    fi
}

# A free loopback port.
# The arguments a server needs to start on a host that granted it neither a
# real-time priority nor enough locked memory.
#
# These are the escapes the server itself documents, and using them is not a
# way of pretending the contract holds: a server started this way says so in
# every status report, and the contract has its own entry point
# (tools/host-contract.sh) which refuses outright when the host grants nothing.
# What this lets the audio-path verifications do is run on a machine that is
# not the deployment target, which is where they are usually run.
server_contract_args() {
    local args=""
    local ceiling limit_kb limit_bytes wanted
    ceiling="$(ulimit -r 2>/dev/null || printf '0')"
    if [ "$ceiling" != "unlimited" ] && [ "${ceiling:-0}" -le 0 ]; then
        args="$args --allow-non-realtime"
        note "chorus: note this host grants no real-time priority (RLIMIT_RTPRIO=$ceiling), so the"
        note "        server is started with --allow-non-realtime and will say so in every status"
        note "        report. The scheduling contract is verified by tools/host-contract.sh."
    fi
    limit_kb="$(ulimit -l 2>/dev/null || printf '0')"
    wanted="$(conf memlock_wanted_bytes)"
    if [ "$limit_kb" != "unlimited" ]; then
        limit_bytes=$(( ${limit_kb:-0} * 1024 ))
        if [ "$limit_bytes" -lt "$wanted" ]; then
            args="$args --allow-unlocked-memory"
            note "chorus: note this host grants $limit_bytes bytes of locked memory and the server"
            note "        wants $wanted, so it is started with --allow-unlocked-memory and will say"
            note "        so in every status report."
        fi
    fi
    printf '%s' "$args"
}

# A loopback port nothing is listening on. Bound and released, so there is a
# small race with anything else doing the same thing; these are verification
# runs on a developer's machine, not a service.
free_port() {
    python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
}

kill_quietly() {
    local pid="${1:-}"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    fi
}
