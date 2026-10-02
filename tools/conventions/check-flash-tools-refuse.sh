#!/usr/bin/env bash
# Rule "The flash guard", behaviourally: every flashing tool refuses unless the owner-at-bench
# variable is exactly 1, and runs nothing before it refuses. check-flash-guard.sh is lexical and
# cannot see a name decoded at run time; this runs the tools.
#
# 1. The tools. Every tracked file outside docs/ that reads the guard, and every one that calls
#    esptool, espefuse or `idf.py ... flash`, must be a flashing tool listed below (so a new
#    device-writing tool cannot join the repository without joining this test).
# 2. Refusal. With shims for idf.py, esptool, esptool.py, espefuse, espefuse.py, python and
#    python3 first on PATH, each recording any call, every tool is run against a complete
#    fixture image and an existing port, so the guard is the only thing that can stop it, with
#    the variable unset and set to "", 0, true, yes, " 1", "1 " and 01. Each run must exit
#    non-zero, name the owner-at-bench variable and leave the shim log empty. The value 1 is
#    never used here, or anywhere in this repository outside docs/.
# 3. --print (no guard): the command it prints is esptool's write-flash for esp32s3 with the
#    fixture's offsets and no eFuse, Secure Boot or encryption word, and no shim runs. It still
#    refuses an argument naming an eFuse operation, an image asking for encrypted flashing, an
#    eFuse-burning sdkconfig option, a board profile the image was not built for, and an image
#    built for the emulator's board (goal 14), which is never written to a device.
#
# This file sets the variable (to values other than 1), so it builds the name from pieces and is
# one of the two files check-flash-guard.sh does not scan.
. "$(dirname "$0")/lib.sh"
name="CHORUS_OWNER_AT_""BENCH"
rule="The flash guard"

# tool | arguments that make it do its real job against the fixture
tools=(
    "tools/firmware-flash.sh"
)

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
rc=0
bad() {
    echo "FAIL: $1"
    rc=1
}

# 1. Every guard reader and every device writer is a listed tool.
listed=" ${tools[*]} "
while IFS= read -r f; do
    [[ "$listed" == *" $f "* ]] || bad "$f reads the owner-at-bench guard or calls a flashing program but is not a flashing tool this test runs"
done < <(git ls-files -z | xargs -0 command grep -l -I -E "\\\$\\{$name:-\\}|env::var\\(\"$name\"\\)|getenv\\(\"$name\"\\)|\\besptool|\\bespefuse|idf\\.py[^#]*\\bflash\\b" -- 2> /dev/null |
    command grep -v -E '^(docs/|\.claude/goals/|tools/conventions/fixtures/|tools/conventions/check-flash-tools-refuse\.sh$|tools/conventions/check-flash-guard\.sh$)' |
    command grep -v -E '\.md$')
for t in "${tools[@]}"; do
    [ -x "$t" ] || bad "$t is listed as a flashing tool but is not an executable file"
done

# 2. The shims and a complete fixture image.
shims="$scratch/shims"
log="$scratch/calls.log"
mkdir -p "$shims"
for s in idf.py esptool esptool.py espefuse espefuse.py python python3; do
    printf '#!/bin/sh\necho "%s $*" >> "%s"\nexit 0\n' "$s" "$log" > "$shims/$s"
    chmod +x "$shims/$s"
done
: > "$log"
img="$scratch/image"
mkdir -p "$img/bootloader" "$img/partition_table"
printf '%s\n' '--flash-mode dio --flash-freq 80m --flash-size 2MB' '0x0 bootloader/bootloader.bin' \
    '0x8000 partition_table/partition-table.bin' '0x10000 chorus-endpoint.bin' > "$img/flash_args"
printf '%s\n' 'CONFIG_IDF_TARGET="esp32s3"' 'CONFIG_ESPTOOLPY_BEFORE="default-reset"' \
    'CONFIG_ESPTOOLPY_AFTER="hard-reset"' 'CONFIG_SECURE_BOOT_V2_PREFERRED=y' '# CONFIG_SECURE_BOOT is not set' > "$img/sdkconfig"
printf 'board_profile = brick-s3-wired\n' > "$img/board_profile.conf"
: > "$img/bootloader/bootloader.bin"
: > "$img/partition_table/partition-table.bin"
: > "$img/chorus-endpoint.bin"
port="$scratch/ttyFAKE0"
: > "$port"

run() { # run <env args...> -- <tool args...>
    local envargs=()
    while [ "$1" != -- ]; do
        envargs+=("$1")
        shift
    done
    shift
    out="$(env "${envargs[@]}" PATH="$shims:$PATH" CHORUS_IMAGE_OUT="$img" CHORUS_ESP32S3_PORT="$port" \
        bash "$@" 2>&1)"
    code=$?
}

values=("" "0" "true" "yes" " 1" "1 " "01")
for t in "${tools[@]}"; do
    runs=0
    for i in unset "${!values[@]}"; do
        if [ "$i" = unset ]; then
            run -u "$name" -- "$t"
            shown="unset"
        else
            run "$name=${values[$i]}" -- "$t"
            shown="'${values[$i]}'"
        fi
        runs=$((runs + 1))
        [ "$code" -ne 0 ] || bad "$t with the variable $shown exited 0"
        [[ "$out" == *"owner-at-bench variable"* ]] || bad "$t with the variable $shown did not name the owner-at-bench variable: $(printf '%s' "$out" | tail -n 1)"
        if [ -s "$log" ]; then
            bad "$t with the variable $shown ran: $(tr '\n' ';' < "$log")"
            : > "$log"
        fi
    done
    echo "$t: refuses in all $runs runs (unset, empty, 0, true, yes, ' 1', '1 ', 01), naming the variable; no shim was called"
done

# 3. --print, and the refusals that hold even there (firmware-flash.sh's own options).
t=tools/firmware-flash.sh
run -u "$name" -- "$t" --print
[ "$code" -eq 0 ] || bad "$t --print exited $code: $(printf '%s' "$out" | tail -n 2)"
cmdline="$(printf '%s\n' "$out" | sed -n 's/^  command: *//p')"
want="python -m esptool --chip esp32s3 --port $port --baud 460800 --before default-reset --after hard-reset write-flash --flash-mode dio --flash-freq 80m --flash-size 2MB 0x0 bootloader/bootloader.bin 0x8000 partition_table/partition-table.bin 0x10000 chorus-endpoint.bin"
[[ "$cmdline" == *"$want)" ]] || bad "$t --print printed '$cmdline', expected it to end in '$want)'"
if printf '%s' "$cmdline" | command grep -q -i -E 'efuse|secure|encrypt|burn|rollback'; then
    bad "$t --print names an eFuse, Secure Boot or encryption operation: $cmdline"
fi
[ -s "$log" ] && bad "$t --print ran: $(tr '\n' ';' < "$log")"
echo "$t --print: $want"

expect_refusal() { # expect_refusal <what> <text in the refusal> <env args...> -- <tool args...>
    local what="$1" text="$2"
    shift 2
    run "$@"
    if [ "$code" -eq 0 ] || [[ "$out" != *"REFUSED"*"$text"* ]]; then
        bad "$t did not refuse $what: $(printf '%s' "$out" | tail -n 1)"
    fi
    [ -s "$log" ] && bad "$t ran something while refusing $what"
    : > "$log"
    echo "$t --print refuses $what"
}
expect_refusal "an argument naming an eFuse operation" "guardrail 2" -u "$name" -- "$t" --print --efuse-burn
expect_refusal "a board profile the image was not built for" "was built for brick-s3-wired" \
    -u "$name" CHORUS_BOARD_PROFILE=compact-s3-wifi -- "$t" --print
printf 'board_profile = qemu-s3-openeth\nlink_transport = emulated\n' > "$img/board_profile.conf"
expect_refusal "an image built for the emulator's board" "never written to a device" -u "$name" -- "$t" --print
printf 'board_profile = brick-s3-wired\n' > "$img/board_profile.conf"
: > "$img/encrypted_app-flash_args"
expect_refusal "an image asking for encrypted flashing" "encrypted flashing" -u "$name" -- "$t" --print
rm -f "$img/encrypted_app-flash_args"
echo 'CONFIG_SECURE_FLASH_ENC_ENABLED=y' >> "$img/sdkconfig"
expect_refusal "an eFuse-burning sdkconfig option" "CONFIG_SECURE_FLASH_ENC_ENABLED" -u "$name" -- "$t" --print

if [ "$rc" -ne 0 ]; then
    fail "$rule" "a flashing tool does not refuse without the owner at the bench"
    exit 1
fi
echo "flash tools: ${#tools[@]} tool(s), each refuses without the owner at the bench and runs nothing first"
