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
# THE BOARD PROFILE. CHORUS_BOARD_PROFILE names one of firmware/boards/*.conf
# (default: the one endpoint.conf names, brick-s3-wired, the wired classes'
# W5500 link); compact-s3-wifi is the compact speakers' Wi-Fi tier (K91). The
# profile is embedded in the image beside endpoint.conf (docs/decisions/@ADR@-*),
# and the build prints the target, the board, how sure the repository is of it
# and the link, so a log says which image it made.
#
# NEEDS AN ENVIRONMENT.
#   - ESP-IDF at the version firmware/config/endpoint.conf declares, exported
#     (IDF_PATH set, idf.py on PATH)
#
#   . $IDF_PATH/export.sh && ./tools/firmware-image.sh    # or: make firmware-image

source "$(dirname "$0")/lib.sh"

CRITERION="the image build, invoked without the ESP-IDF toolchain present, exits non-zero naming the toolchain and the version it expects and emits no partial image"

OUT_DIR="${CHORUS_IMAGE_OUT:-$REPO_ROOT/firmware/build/image}"

PROFILE_PATH="$(board_profile_path)" || exit 2
PROFILE="$(basename "$PROFILE_PATH" .conf)"
export CHORUS_BOARD_PROFILE="$PROFILE"

say "chorus: the ESP32-S3 endpoint image"
say "  criterion:  AC-17"
say "  target:     esp32s3 (P1: ESP32-S3 everywhere)"
say "  board:      profile $PROFILE, model \"$(board_value board_model)\", $(board_value board_model_status)"
if [ "$(board_value board_model_status)" = ASSUMED ]; then
    say "  needs item: \"$(board_value board_needs_item)\" (the model stays ASSUMED until it is answered)"
fi
if [ "$(endpoint_conf link_transport)" = wired ]; then
    say "  link:       wired, W5500 on $(endpoint_conf eth_spi_host) at $(endpoint_conf eth_spi_clock_mhz) MHz, INT GPIO$(endpoint_conf pin_eth_int)"
else
    say "  link:       wireless, the Wi-Fi tier, power save $(endpoint_conf link_wifi_power_save)"
fi
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
"$REPO_ROOT/firmware/build/chorus-endpoint-config-check" "$REPO_ROOT/firmware/config/endpoint.conf" "$PROFILE_PATH"

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
(cd "$REPO_ROOT/firmware" && idf.py -B "$OUT_DIR" -D SDKCONFIG="$OUT_DIR/sdkconfig" \
    -D CHORUS_BOARD_PROFILE="$PROFILE" "${CCACHE_FLAG[@]}" build)

# Guardrail 2 over what was just built: the generated configuration and both
# linked images, before anything is reported as an image.
bash "$REPO_ROOT/tools/firmware-image-guard.sh" "$OUT_DIR"

say "chorus: image built: target esp32s3, board profile $PROFILE, link $(endpoint_conf link_transport)"
say "chorus: the image is in $OUT_DIR and has NOT been flashed or shipped"
say "chorus: flashing is an operator act; OTA is chorus#FLEET-10 and is not in this phase"
