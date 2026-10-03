# shellcheck shell=bash
# The pinned emulator, for the runs that boot the endpoint image in it (goal 14,
# docs/decisions/0109-*). Sourced after tools/lib.sh.
#
# Espressif's QEMU fork runs the esp32s3 image as built: the real second-stage
# bootloader, the partition table, NVS, the session. It is a pinned binary
# (tools/qemu/pins.conf) with a pinned library environment
# (tools/qemu/libs.explicit.txt); tools/qemu-env.sh installs both rootless, and
# everything here only uses them. A run whose emulator is missing or is not the
# pinned one refuses by name: it is never passed and never skipped green.
#
# NOTHING HERE IS TIMING EVIDENCE. An emulator's clock is not a crystal's.
#
# NO eFUSE IS EVER WRITTEN. The emulator keeps its eFuses in a file; every run
# gets a fresh one holding the chip's defaults, and the run fails unless the
# file is byte for byte what it was when the emulator exits (guardrail 2).

QEMU_PINS="$REPO_ROOT/tools/qemu/pins.conf"
QEMU_LIBS_LOCK="$REPO_ROOT/tools/qemu/libs.explicit.txt"

# A value from tools/qemu/pins.conf.
qemu_pin() {
    local key="$1" value
    value="$(sed -n "s/^${key} = //p" "$QEMU_PINS" | head -n 1)"
    if [ -z "$value" ]; then
        printf '%s has no %s\n' "$QEMU_PINS" "$key" >&2
        exit 2
    fi
    printf '%s' "$value"
}

# Where the emulator lives: CHORUS_QEMU_HOME, one directory per pinned version
# under it, so two versions never share one.
qemu_home() {
    printf '%s' "${CHORUS_QEMU_HOME:-/cache/opt/chorus-qemu}"
}

qemu_program() {
    printf '%s/%s/qemu/bin/qemu-system-xtensa' "$(qemu_home)" "$(qemu_pin qemu_version)"
}

# Where its libraries live: a conda environment, CHORUS_QEMU_LIBS.
qemu_libs() {
    printf '%s' "${CHORUS_QEMU_LIBS:-/cache/opt/chorus-qemu-libs}"
}

# The emulator, with its libraries on the loader's path for this one command
# only: the environment carries its own libz, libstdc++ and glib, which must
# not leak into the compilers, cargo or python.
qemu_run() {
    LD_LIBRARY_PATH="$(qemu_libs)/lib" "$(qemu_program)" "$@"
}

# Whether the installed emulator and libraries are the pinned ones. Prints
# nothing on success. On failure prints one line saying what is wrong and
# returns 1 (absent) or 2 (present and not the pinned one).
qemu_verify() {
    local program libs want found
    program="$(qemu_program)"
    libs="$(qemu_libs)"
    if [ ! -x "$program" ]; then
        printf 'the emulator %s is not installed at %s\n' "$(qemu_pin qemu_version)" "$program"
        return 1
    fi
    want="$(qemu_pin qemu_binary_sha256)"
    found="$(sha256sum "$program" | cut -d' ' -f1)"
    if [ "$found" != "$want" ]; then
        printf '%s is not the pinned emulator: sha256 %s, pinned %s\n' "$program" "$found" "$want"
        return 2
    fi
    if [ ! -d "$libs/conda-meta" ]; then
        printf 'the emulator'"'"'s library environment is not installed at %s\n' "$libs"
        return 1
    fi
    # Every package of the lock is installed at the lock's digest, and nothing
    # else is: the environment's own records (conda-meta) are read, not trusted
    # names.
    if ! found="$(python3 - "$QEMU_LIBS_LOCK" "$libs/conda-meta" <<'PY'
import glob, json, os, sys
lock, meta = sys.argv[1], sys.argv[2]
want = {}
for line in open(lock, encoding="utf-8"):
    line = line.strip()
    if line.startswith("https://"):
        url, digest = line.split("#sha256:")
        want[url] = digest
have = {}
for path in glob.glob(os.path.join(meta, "*.json")):
    record = json.load(open(path, encoding="utf-8"))
    have[record.get("url", path)] = record.get("sha256", "")
missing = sorted(u for u in want if u not in have)
extra = sorted(u for u in have if u not in want)
differ = sorted(u for u in want if u in have and have[u] != want[u])
if missing or extra or differ:
    what = []
    if missing:
        what.append("%d of the lock's packages are not installed (first: %s)" % (len(missing), os.path.basename(missing[0])))
    if extra:
        what.append("%d installed packages are not in the lock (first: %s)" % (len(extra), os.path.basename(extra[0])))
    if differ:
        what.append("%d packages differ from the lock's sha256 (first: %s)" % (len(differ), os.path.basename(differ[0])))
    print("; ".join(what))
    sys.exit(1)
PY
    )"; then
        printf 'the library environment at %s is not the pinned one: %s\n' "$libs" "$found"
        return 2
    fi
    # It loads and is the version the pin names, with the machine the image is for.
    if ! found="$(qemu_run --version 2>&1 | head -n 1)" || [[ "$found" != *"($(qemu_pin qemu_version))"* ]]; then
        printf 'the emulator does not run as the pinned version: %s\n' "${found:-no output}"
        return 2
    fi
    if ! qemu_run -machine help 2> /dev/null | command grep -q '^esp32s3 '; then
        printf 'the emulator at %s has no esp32s3 machine\n' "$program"
        return 2
    fi
    return 0
}

# Refuse by name unless the pinned emulator and its libraries are here.
qemu_require() {
    local criterion="$1" why
    if ! why="$(qemu_verify)"; then
        missing_prerequisite \
            "$criterion" \
            "Espressif QEMU $(qemu_pin qemu_version) and its library environment: $why" \
            "bash tools/qemu-env.sh install (rootless: the pinned release archive and the pinned conda-forge packages, each checked against its sha256)"
    fi
}

# ESP-IDF, as the gate finds it: already exported, or through CHORUS_IDF_ENV.
qemu_idf_env() {
    if [ -z "${IDF_PATH:-}" ] && [ -n "${CHORUS_IDF_ENV:-}" ] && [ -f "$CHORUS_IDF_ENV" ]; then
        # ESP-IDF's export script reads variables it has not set.
        set +u
        # shellcheck disable=SC1090
        . "$CHORUS_IDF_ENV" > /dev/null 2>&1
        set -u
    fi
}

# The flash size the image was configured for, in MB, from its generated
# sdkconfig: the emulator's flash file is made exactly that size.
qemu_flash_mb() {
    local build="$1" size
    size="$(sed -n 's/^CONFIG_ESPTOOLPY_FLASHSIZE="\([0-9]*\)MB"$/\1/p' "$build/sdkconfig" | head -n 1)"
    if [ -z "$size" ]; then
        printf '%s/sdkconfig names no CONFIG_ESPTOOLPY_FLASHSIZE\n' "$build" >&2
        return 1
    fi
    printf '%s' "$size"
}

# qemu_flash_image <build directory> <output file>: the emulator's flash, from
# the build's own binaries at the build's own offsets (tools/qemu/flash_image.py).
qemu_flash_image() {
    local build="$1" out="$2" size
    size="$(qemu_flash_mb "$build")" || return 1
    python3 "$REPO_ROOT/tools/qemu/flash_image.py" "$build" "$out" "$size"
}

# qemu_fresh_efuses <output file>: the chip's default eFuses, as ESP-IDF's own
# emulator support writes them for a new file (tools/idf_py_actions/qemu_ext.py
# in the pinned v6.1: QEMU_TARGETS['esp32s3'].default_efuse, written at :284
# when the file does not exist; Apache-2.0, read 2026-10-02). The bytes are
# taken from that tree at run time, so they are the pinned ESP-IDF's.
qemu_fresh_efuses() {
    local out="$1"
    [ -n "${IDF_PATH:-}" ] || { printf 'IDF_PATH is unset; the default eFuse bytes are read from the ESP-IDF tree\n' >&2; return 1; }
    python - "$out" <<'PY'
import os, sys
sys.path.insert(0, os.path.join(os.environ["IDF_PATH"], "tools"))
sys.path.insert(0, os.path.join(os.environ["IDF_PATH"], "tools", "idf_py_actions"))
import qemu_ext
with open(sys.argv[1], "wb") as out:
    out.write(qemu_ext.QEMU_TARGETS["esp32s3"].default_efuse)
PY
}

# qemu_boot <flash file> <eFuse file> <serial log> <seconds>: start the emulator
# headless in the background, its serial port into the log, and leave its pid
# in QEMU_PID. The arguments are the ones ESP-IDF's own `idf.py qemu` builds
# (qemu_ext.py:105-139, 244-310), without a monitor:
#   - the flash and the eFuse file as drives; the eFuse file is opened read and
#     write because the machine requires a writable drive, and the caller proves
#     afterwards that nothing was written;
#   - the machine's default PSRAM part, which is quad (the emulated board's
#     image is built for quad: firmware/sdkconfig.qemu-s3-openeth says why);
#     `-m 8M`, the part the reference board carries (ASSUMED, as the board
#     is). Not ESP-IDF's launcher's 32M: with 32 MB of PSRAM mapped the
#     application had no virtual address range left to map a partition
#     (`esp_mmu_map_virt(522): no such vaddr range`, then NVS refused), tried
#     2026-10-03 (docs/decisions/0109-*);
#   - the timer group's watchdog off, as ESP-IDF's launcher sets it: the
#     emulator's sense of time under a loaded host is not the chip's;
#   - a user network with the OpenCores controller: the guest gets an address by
#     DHCP and reaches the host's loopback through the gateway, and nothing
#     outside this machine is reachable from the listening side;
#   - `timeout` over all of it, so a hung guest cannot outlive the run.
qemu_boot() {
    local flash="$1" efuses="$2" serial="$3" seconds="$4"
    : > "$serial"
    LD_LIBRARY_PATH="$(qemu_libs)/lib" timeout "$seconds" "$(qemu_program)" -M esp32s3 -m 8M \
        -drive "file=$flash,if=mtd,format=raw" \
        -drive "file=$efuses,if=none,format=raw,id=efuse" \
        -global driver=nvram.esp32s3.efuse,property=drive,value=efuse \
        -global driver=timer.esp32s3.timg,property=wdt_disable,value=true \
        -nic user,model=open_eth \
        -nographic -monitor none -serial "file:$serial" > "$serial.emulator" 2>&1 &
    QEMU_PID=$!
}

# Stop the emulator started by qemu_boot.
qemu_stop() {
    if [ -n "${QEMU_PID:-}" ] && kill -0 "$QEMU_PID" 2> /dev/null; then
        kill "$QEMU_PID" 2> /dev/null || true
        wait "$QEMU_PID" 2> /dev/null || true
    fi
    QEMU_PID=""
}

# wait_for_line <file> <seconds> <extended regex>: 0 as soon as a line of the
# file matches, 1 when the time passes or the emulator is gone without one.
wait_for_line() {
    local file="$1" seconds="$2" pattern="$3" i
    for ((i = 0; i < seconds * 5; i++)); do
        if command grep -q -a -E "$pattern" "$file" 2> /dev/null; then
            return 0
        fi
        if [ -n "${QEMU_PID:-}" ] && ! kill -0 "$QEMU_PID" 2> /dev/null; then
            command grep -q -a -E "$pattern" "$file" 2> /dev/null
            return
        fi
        sleep 0.2
    done
    return 1
}
