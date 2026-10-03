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
# THIS SHIPS NOTHING. It builds an image and stops. The image carries the
# firmware update (goal 14: two app slots, the bootloader's rollback, the
# update unit and its one glue unit, firmware/main/esp_ota.c), but nothing here
# installs anything: an install is an explicit action on the server, and on an
# installed speaker it is the owner's (CLAUDE.md, the plan of record).
#
# THE BOARD PROFILE. CHORUS_BOARD_PROFILE names one of firmware/boards/*.conf
# (default: the one endpoint.conf names, brick-s3-wired, the wired classes'
# W5500 link); compact-s3-wifi is the compact speakers' Wi-Fi tier (K91);
# qemu-s3-openeth is the emulator's board (goal 14), not a speaker's. The
# profile is embedded in the image beside endpoint.conf (docs/decisions/0057-*),
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
elif [ "$(endpoint_conf link_transport)" = emulated ]; then
    say "  link:       emulated, the emulator's OpenCores Ethernet; audio output $(endpoint_conf board_audio_output) (not a speaker's image)"
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

# THE PROFILE'S OWN KCONFIG (goal 14). firmware/sdkconfig.defaults is every
# profile's. A profile that needs options the others should not carry has a
# fragment beside it, firmware/sdkconfig.<profile>, laid over the common file
# (ESP-IDF v6.1 docs/en/api-guides/build-system.rst:1097-1137: SDKCONFIG_DEFAULTS
# takes a list, later files win). It is passed on the command line here and not
# set in CMake, which the safety scans refuse, and it is named sdkconfig.* under
# firmware/ so the same scans read it for refused options. A profile with no
# fragment builds from the common file alone, as before.
DEFAULTS=("$REPO_ROOT/firmware/sdkconfig.defaults")
if [ -f "$REPO_ROOT/firmware/sdkconfig.$PROFILE" ]; then
    DEFAULTS+=("$REPO_ROOT/firmware/sdkconfig.$PROFILE")
fi
DEFAULTS_LIST="$(IFS=';'; printf '%s' "${DEFAULTS[*]}")"
say "  kconfig:    ${DEFAULTS[*]#"$REPO_ROOT"/}"
# The generated sdkconfig is a product of those files and of nothing else: nobody
# edits it. ESP-IDF reads defaults only for options the existing sdkconfig does
# not already hold, so a persistent build directory would keep yesterday's value
# of an option whose default changed today. The directory therefore remembers
# what its sdkconfig was generated from, and generates it afresh when that
# changed.
DEFAULTS_SUM="$(cat "${DEFAULTS[@]}" | sha256sum | cut -d' ' -f1) ${DEFAULTS[*]#"$REPO_ROOT"/}"
if [ "$(cat "$OUT_DIR/sdkconfig.generated-from" 2> /dev/null)" != "$DEFAULTS_SUM" ]; then
    if [ -f "$OUT_DIR/sdkconfig" ]; then
        say "chorus: the committed Kconfig defaults changed; generating $OUT_DIR/sdkconfig afresh"
        rm -f "$OUT_DIR/sdkconfig"
    fi
    printf '%s\n' "$DEFAULTS_SUM" > "$OUT_DIR/sdkconfig.generated-from"
fi

# THE EMULATOR RUN'S IMAGES (goal 14, docs/decisions/0111-*). The firmware
# update run needs images of one profile that differ in version, and one that
# never confirms its trial:
#   CHORUS_IMAGE_VERSION      the version the image carries in its application
#                             description (ESP-IDF's PROJECT_VER; default: what
#                             ESP-IDF derives, the checkout's git description)
#   CHORUS_OTA_NEVER_CONFIRM  1 builds the update unit's never_confirm in
#                             (chorus/ota.h): an image that rolls itself back.
#                             Refused for every profile but the emulator's; a
#                             speaker is never built to fail its trial.
IMAGE_VERSION="${CHORUS_IMAGE_VERSION:-}"
NEVER_CONFIRM="${CHORUS_OTA_NEVER_CONFIRM:-0}"
if [ -n "$IMAGE_VERSION" ] && ! [[ "$IMAGE_VERSION" =~ ^[A-Za-z0-9._-]{1,31}$ ]]; then
    say "chorus: REFUSED: CHORUS_IMAGE_VERSION '$IMAGE_VERSION' is not 1 to 31 of [A-Za-z0-9._-]"
    exit 2
fi
case "$NEVER_CONFIRM" in
    0) ;;
    1)
        if [ "$(endpoint_conf link_transport)" != emulated ]; then
            say "chorus: REFUSED: CHORUS_OTA_NEVER_CONFIRM=1 builds an image that rolls itself back; only the emulator's board (link_transport = emulated) is built that way, and $PROFILE is not it"
            exit 2
        fi
        say "  update:     NEVER CONFIRMS its trial (CHORUS_OTA_NEVER_CONFIRM=1): the emulator run's bad image"
        ;;
    *)
        say "chorus: REFUSED: CHORUS_OTA_NEVER_CONFIRM is '$NEVER_CONFIRM', not 0 or 1"
        exit 2
        ;;
esac
VERSION_ARGS=()
[ -n "$IMAGE_VERSION" ] && VERSION_ARGS=(-D PROJECT_VER="$IMAGE_VERSION")
say "  version:    ${IMAGE_VERSION:-ESP-IDF default, the git description}"
# CMake keeps a -D value in its cache, so a directory that once carried a
# version or the never-confirm build would carry it into a later build that
# asked for neither. The directory remembers what it was configured with and
# is configured afresh (ccache keeps that cheap) when that changes.
VARIANT="version=${IMAGE_VERSION} never_confirm=${NEVER_CONFIRM}"
if [ "$(cat "$OUT_DIR/image-variant" 2> /dev/null)" != "$VARIANT" ]; then
    rm -f "$OUT_DIR/CMakeCache.txt"
    printf '%s\n' "$VARIANT" > "$OUT_DIR/image-variant"
fi

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
    -D SDKCONFIG_DEFAULTS="$DEFAULTS_LIST" \
    -D CHORUS_BOARD_PROFILE="$PROFILE" -D CHORUS_OTA_NEVER_CONFIRM="$NEVER_CONFIRM" \
    "${VERSION_ARGS[@]}" "${CCACHE_FLAG[@]}" build)

# The profile this image was built with, where the flash tool and the staging
# tool read it (<build>/board_profile.conf): the copy the component embedded,
# not one an earlier configure step may have left there naming another.
cp "$OUT_DIR/esp-idf/main/board_profile.conf" "$OUT_DIR/board_profile.conf"

# Guardrail 2 over what was just built: the generated configuration and both
# linked images, before anything is reported as an image.
bash "$REPO_ROOT/tools/firmware-image-guard.sh" "$OUT_DIR"
# The guard passed (set -e): the four things it holds this image to, one line
# per target, which the gate copies into its summary.
say "safety scan: $PROFILE: no eFuse write, no Secure Boot, no Flash Encryption, no anti-rollback: pass"

say "chorus: image built: target esp32s3, board profile $PROFILE, link $(endpoint_conf link_transport)"
say "chorus: the image is in $OUT_DIR and has NOT been flashed or shipped"
say "chorus: flashing is the owner's act, and so is an OTA install on an installed speaker"
