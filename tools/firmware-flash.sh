#!/usr/bin/env bash
# Flash the ESP32-S3 endpoint image onto a board: the owner's act, at the bench.
#
#   tools/firmware-flash.sh [--port PORT] [--baud N] [--image-dir DIR]
#   tools/firmware-flash.sh --print [--port PORT] [--baud N] [--image-dir DIR]
#
# THE GUARD (brief section 0.7, K4; docs/decisions/0062-*). This writes to a
# device, so it refuses unless the owner-at-bench variable is exactly 1. Only the
# owner sets it, on the bench machine's command line, as docs/bench-packet.md
# shows; nothing in this repository sets it, and tools/conventions/check-flash-guard.sh
# fails the gate if anything outside docs/ does. The guard is checked right after
# the arguments are read, before any file is opened and before any program runs.
#
# --print needs no guard: it prints the exact command a flash would run and runs
# nothing (no esptool, no python, no serial port is opened). The gate's
# behavioural test (tools/conventions/check-flash-tools-refuse.sh) uses it.
#
# WHAT IT FLASHES. The image `make firmware-image` builds (tools/firmware-image.sh,
# default directory firmware/build/image), for the board profile that build
# recorded (CHORUS_BOARD_PROFILE picks it at build time; a differing
# CHORUS_BOARD_PROFILE here is refused, so the image flashed is the board named).
# The offsets and files are the build's own `flash_args`, written by ESP-IDF
# v6.1's esptool_py component (components/esptool_py/project_include.cmake:199
# and :213, Apache-2.0, local copy /cache/esp/esp-idf-v6.1 read 2026-09-30).
#
# THE COMMAND is the one ESP-IDF v6.1's own `idf.py flash` runs: `python -m
# esptool --chip esp32s3` (project_include.cmake:372) with `--before` and
# `--after` from the build's sdkconfig (CONFIG_ESPTOOLPY_BEFORE/AFTER,
# project_include.cmake:472-478), then `write-flash` with the build's flash mode,
# size and frequency and its offset/file pairs. Option names per esptool v5
# (https://docs.espressif.com/projects/esptool/en/latest/esp32s3/esptool/basic-commands.html
# and .../advanced-options.html, read 2026-09-30; the v6.1 environment here ships
# esptool v5.4.0). The baud defaults to idf.py's 460800
# (tools/idf_py_actions/serial_ext.py:35 in v6.1).
#
# GUARDRAIL 2 (BRIEF.md section 3.1): no eFuse burn on development hardware. The
# tool never runs espefuse or any eFuse, Secure Boot, Flash Encryption or
# anti-rollback command, and refuses:
#   - any argument that names one (efuse, secure, encrypt, anti-rollback, burn);
#   - an image whose build wrote encrypted flash arguments (encrypted_*flash_args)
#     or whose sdkconfig turns on an option firmware/check/efuse-kconfig.list
#     refuses (checked here, so --print refuses it too);
#   - before a real flash, anything tools/firmware-image-guard.sh finds in the
#     built configuration and both linked images.

source "$(dirname "$0")/lib.sh"

usage() {
    note "usage: tools/firmware-flash.sh [--print] [--port PORT] [--baud N] [--image-dir DIR]"
}

refuse() {
    note "firmware-flash: REFUSED: $*"
    note "firmware-flash: nothing was flashed"
    exit 2
}

print_only=0
port="${CHORUS_ESP32S3_PORT:-}"
baud=460800
image_dir="${CHORUS_IMAGE_OUT:-$REPO_ROOT/firmware/build/image}"

# The arguments are read first and any that names an eFuse, Secure Boot, Flash
# Encryption or anti-rollback operation is refused by name, whatever else holds.
args=("$@")
for a in "${args[@]}"; do
    case "${a,,}" in
        *efuse* | *secure* | *encrypt* | *rollback* | *burn*)
            refuse "argument '$a' names an eFuse, Secure Boot, Flash Encryption or anti-rollback operation; guardrail 2 (BRIEF.md section 3.1) forbids them on development hardware"
            ;;
    esac
done
while [ $# -gt 0 ]; do
    case "$1" in
        --print) print_only=1 ;;
        --port)
            [ $# -ge 2 ] || { usage; refuse "--port needs a serial port"; }
            port="$2"
            shift
            ;;
        --baud)
            [ $# -ge 2 ] || { usage; refuse "--baud needs a number"; }
            baud="$2"
            shift
            ;;
        --image-dir)
            [ $# -ge 2 ] || { usage; refuse "--image-dir needs a directory"; }
            image_dir="$2"
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            usage
            refuse "unknown argument '$1'"
            ;;
    esac
    shift
done

# The guard: exactly 1, set by the owner at the bench, or nothing runs.
if [ "$print_only" -eq 0 ] && [ "${CHORUS_OWNER_AT_BENCH:-}" != 1 ]; then
    refuse "the owner-at-bench variable is not 1 (docs/bench.md, "The flash guard", names it). Flashing writes to a device, which is the owner's act at the bench (brief section 0.7, K4): only the owner sets it, on the bench machine's command line, as docs/bench-packet.md shows. Nothing in this repository sets it. Use --print to see the command without running it."
fi

[[ "$baud" =~ ^[0-9]+$ ]] || refuse "--baud '$baud' is not a number"
[ -n "$port" ] || refuse "no serial port: set CHORUS_ESP32S3_PORT or pass --port"

[ -d "$image_dir" ] || refuse "no image at $image_dir; build it first with: make firmware-image"
flash_args="$image_dir/flash_args"
sdkconfig="$image_dir/sdkconfig"
[ -f "$flash_args" ] || refuse "$flash_args is missing; $image_dir is not an ESP-IDF build directory (make firmware-image)"
[ -f "$sdkconfig" ] || refuse "$sdkconfig is missing; the build's configuration cannot be checked"

# The board: the profile the build recorded, and the one asked for, if any.
built_profile=""
if [ -f "$image_dir/board_profile.conf" ]; then
    built_profile="$(sed -n 's/^board_profile *= *//p' "$image_dir/board_profile.conf" | head -n 1)"
fi
[ -n "$built_profile" ] || refuse "$image_dir/board_profile.conf names no board_profile; rebuild with make firmware-image"
if [ -n "${CHORUS_BOARD_PROFILE:-}" ] && [ "$CHORUS_BOARD_PROFILE" != "$built_profile" ]; then
    refuse "CHORUS_BOARD_PROFILE is $CHORUS_BOARD_PROFILE but the image in $image_dir was built for $built_profile; rebuild with CHORUS_BOARD_PROFILE=$CHORUS_BOARD_PROFILE make firmware-image"
fi
# The emulator's image is not a speaker's (goal 14): its link is a controller no
# board has and it brings up no amplifier. It goes into an emulator's flash
# file (tools/qemu-boot-run.sh) and never onto a device, with or without the
# owner at the bench.
if command grep -q -E '^link_transport *= *emulated' "$image_dir/board_profile.conf"; then
    refuse "the image in $image_dir was built for the emulator's board ($built_profile, link_transport = emulated); it is never written to a device. Build a speaker's profile with CHORUS_BOARD_PROFILE=<profile> make firmware-image"
fi

# Guardrail 2, over what would be written. Encrypted flashing leaves its own
# argument files; an eFuse-burning option in the build's sdkconfig is refused
# against the same list the source scan and the image guard read.
for f in "$image_dir"/encrypted*flash_args "$image_dir"/*encrypted*flash_args; do
    [ -e "$f" ] && refuse "$f exists: the build asks for encrypted flashing, which needs Flash Encryption eFuses (guardrail 2)"
done
kconfig_list="$REPO_ROOT/firmware/check/efuse-kconfig.list"
[ -r "$kconfig_list" ] || refuse "$kconfig_list is unreadable, so the image's configuration cannot be checked"
burning="$(awk '
    FNR == NR {
        if ($1 == "refuse") refuse[$2] = 1
        if ($1 == "derived") derived[$2] = 1
        next
    }
    /^CONFIG_[A-Za-z0-9_]+=y$/ {
        name = substr($0, 1, index($0, "=") - 1)
        if (name in derived) next
        for (p in refuse) if (index(name, p) == 1) { print name; break }
    }' "$kconfig_list" "$sdkconfig")"
[ -z "$burning" ] || refuse "the image's sdkconfig turns on ${burning//$'\n'/ }, which firmware/check/efuse-kconfig.list refuses (guardrail 2)"

# The flash settings and the offset/file pairs, as the build wrote them: the
# first line holds the write-flash options, every other line one offset and file.
read -r -a settings < "$flash_args"
for s in "${settings[@]}"; do
    case "$s" in
        --flash-mode | --flash-size | --flash-freq | dio | qio | dout | qout | opi | [0-9]*m | [0-9]*MB | keep | detect) ;;
        *) refuse "unexpected write-flash option '$s' in $flash_args" ;;
    esac
done
pairs=()
while read -r offset file; do
    [ -n "$offset" ] || continue
    [[ "$offset" =~ ^0x[0-9a-fA-F]+$ ]] || refuse "unexpected line '$offset $file' in $flash_args"
    [ -n "$file" ] || refuse "offset $offset in $flash_args names no file"
    pairs+=("$offset" "$file")
done < <(tail -n +2 "$flash_args")
[ "${#pairs[@]}" -gt 0 ] || refuse "$flash_args names no image to write"

before="$(sed -n 's/^CONFIG_ESPTOOLPY_BEFORE="\(.*\)"$/\1/p' "$sdkconfig" | head -n 1)"
after="$(sed -n 's/^CONFIG_ESPTOOLPY_AFTER="\(.*\)"$/\1/p' "$sdkconfig" | head -n 1)"
before="${before:-default-reset}"
after="${after:-hard-reset}"

cmd=(python -m esptool --chip esp32s3 --port "$port" --baud "$baud" --before "$before" --after "$after"
    write-flash "${settings[@]}" "${pairs[@]}")

say "chorus: flash the ESP32-S3 endpoint image"
say "  board:      profile $built_profile"
say "  image:      $image_dir"
say "  port:       $port"
say "  command:    (cd $image_dir && ${cmd[*]})"
if [ "$print_only" -eq 1 ]; then
    say "chorus: --print: nothing was run and nothing was flashed"
    exit 0
fi

# A real flash: the image guard over the built configuration and both linked
# images first, then the owner's port, then esptool.
bash "$REPO_ROOT/tools/firmware-image-guard.sh" "$image_dir" || refuse "the image guard found an eFuse risk in $image_dir (above)"
[ -e "$port" ] || refuse "nothing is at $port; attach the board and name its serial port"
for ((i = 1; i < ${#pairs[@]}; i += 2)); do
    [ -f "$image_dir/${pairs[i]}" ] || refuse "$image_dir/${pairs[i]} is missing; rebuild with make firmware-image"
done
cd "$image_dir" || refuse "cannot enter $image_dir"
"${cmd[@]}"
rc=$?
if [ "$rc" -ne 0 ]; then
    note "firmware-flash: esptool exited $rc"
    exit "$rc"
fi
say "chorus: flashed $built_profile to $port; open the console with: idf.py -p $port monitor (docs/bench-packet.md)"
