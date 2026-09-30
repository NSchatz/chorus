#!/usr/bin/env bash
# Build the ESP32-S3 endpoint image.
#
# AC-17: "WHEN the image build is invoked without the ESP-IDF toolchain present
# THE SYSTEM SHALL exit non-zero naming the toolchain and the version it
# expects, and SHALL NOT emit a partial image."
#
# The refusal comes FIRST, before any output directory is created and before
# any object is compiled, so there is nothing partial to leave behind. That
# ordering is the assertion, not the message: a build that refused after
# writing half an image would have satisfied the words and broken the point.
#
# The version it expects is firmware/config/endpoint.conf's espidf_version, and
# the refusal names it. A DIFFERENT installed version is refused too: an image
# built by a toolchain nobody chose looks exactly like an image that was.
#
# THIS SHIPS NOTHING. It builds an image and stops. OTA, rollback and every
# form of image activation belong to chorus#FLEET-10, and
# firmware/check/endpoint_scan.c fails the suite if a line of the endpoint tree
# so much as names one.
#
# NEEDS AN ENVIRONMENT.
#   - ESP-IDF at the version firmware/config/endpoint.conf declares, exported
#     (IDF_PATH set, idf.py on PATH)
#
#   . $IDF_PATH/export.sh && ./tools/firmware-image.sh    # or: make firmware-image

source "$(dirname "$0")/lib.sh"

CRITERION="the image build, invoked without the ESP-IDF toolchain present, exits non-zero naming the toolchain and the version it expects and emits no partial image"

OUT_DIR="${CHORUS_IMAGE_OUT:-$REPO_ROOT/firmware/build/image}"

say "chorus: the ESP32-S3 endpoint image"
say "  criterion:  AC-17"
say "  toolchain:  esp-idf $(endpoint_conf espidf_version)"
say "  IDF_PATH:   ${IDF_PATH:-<unset>}"
say "  output:     $OUT_DIR"

# Every prerequisite is checked before anything is created, so a build that
# cannot run leaves the tree exactly as it found it.
require_espidf "$CRITERION"

# The configuration gates the image for the same reason it gates the host
# build: a pin map that claims a reserved GPIO, or a 24-bit slot width with an
# MCLK multiple not divisible by three, refuses rather than builds.
make -f "$REPO_ROOT/firmware/Makefile" config-check

# A persistent build directory configured against another ESP-IDF tree (the
# gate's directory from before the v6.1 move, ADR 0042) fails in the bootloader
# subproject: CMake refuses a cache made from a different source directory. Such
# a directory is discarded and configured afresh; ccache keeps the rebuild cheap.
stale_idf() {
    local top="$OUT_DIR/CMakeCache.txt" boot="$OUT_DIR/bootloader/CMakeCache.txt" v
    if [ -f "$top" ]; then
        v="$(sed -n 's/^IDF_PATH:[A-Z]*=//p' "$top" | head -n 1)"
        [ -n "$v" ] && [ "$v" != "$IDF_PATH" ] && return 0
    fi
    if [ -f "$boot" ]; then
        v="$(sed -n 's/^CMAKE_HOME_DIRECTORY:INTERNAL=//p' "$boot" | head -n 1)"
        [ -n "$v" ] && [ "${v#"$IDF_PATH"/}" = "$v" ] && return 0
    fi
    return 1
}
if stale_idf; then
    say "chorus: $OUT_DIR was configured against another ESP-IDF tree; configuring it afresh"
    rm -rf "$OUT_DIR"
fi

mkdir -p "$OUT_DIR"
say "chorus: building"
# The generated sdkconfig lives in the build directory, not in firmware/, so two
# build directories never share one and the image guard reads the configuration
# this build actually used. CHORUS_IDF_CCACHE=1 builds through ccache (the gate
# sets it); idf.py takes its job count from IDF_PY_BUILD_JOBS.
CCACHE_FLAG=()
if [ "${CHORUS_IDF_CCACHE:-0}" = 1 ]; then
    CCACHE_FLAG=(--ccache)
fi
say "  jobs:       IDF_PY_BUILD_JOBS=${IDF_PY_BUILD_JOBS:-<unset: the ninja default>}"
(cd "$REPO_ROOT/firmware" && idf.py -B "$OUT_DIR" -D SDKCONFIG="$OUT_DIR/sdkconfig" "${CCACHE_FLAG[@]}" build)

# Guardrail 2 over what was just built: the generated configuration and both
# linked images, before anything is reported as an image.
bash "$REPO_ROOT/tools/firmware-image-guard.sh" "$OUT_DIR"

say "chorus: the image is in $OUT_DIR and has NOT been flashed or shipped"
say "chorus: flashing is an operator act; OTA is chorus#FLEET-10 and is not in this phase"
