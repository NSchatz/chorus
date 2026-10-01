#!/usr/bin/env bash
# The house soak: a software soak of one server and a whole house of endpoints
# on this host, for goal 11's line E ("a software soak of at least one hour at
# 8 rooms is committed, labelled, with its duration"). ADR 0078; how to run it,
# and how it differs from the three-day hardware soak, is docs/house-soak.md.
#
# HOST SOFTWARE SOAK ON ALSA NULL, NOT A HARDWARE MEASUREMENT AND NOT TIMING
# EVIDENCE. What it grades is that a house keeps working under a steady command
# load for as long as it runs: threads, memory, fanout, limits, sessions. The
# ALSA `null` device accepts every frame at once and reports a delay of zero,
# so nothing about when a sample would have reached a DAC can be read off it.
# This is NOT tools/soak-run.sh (AC-4, three days on real endpoints with the
# capture rig), which keeps refusing here, and nothing here changes it.
#
# What runs, all on loopback:
#
#   - one chorus-server --slots 8 --control-listen ... --serve-forever with
#     eight rooms (ASSUMED names, the same house as
#     docs/measurements/sim-house-8-rooms.md: the owner's room list is an
#     open Needs item), `bathroom` declared wireless, the generated tone as the
#     source for the whole run, and the civil time held fixed (--civil-time)
#     so quiet-hours windows are active or not by what the load sets;
#   - ten chorus-client processes on the ALSA `null` device (the client has no
#     fake sink by design, crates/client-linux/src/config.rs refuses one): one
#     per room, and a second in `living` and `kitchen`, whose two endpoints are
#     bonded FL/FR; the bathroom endpoint runs `--transport wireless`;
#   - one event-stream subscriber holding the state stream for the whole run;
#   - the seeded command load (tools/house-soak/load.py), one command every
#     CHORUS_HOUSE_SOAK_INTERVAL_MS for CHORUS_HOUSE_SOAK_SECONDS;
#   - a sampler of every process's RSS and thread count off /proc.
#
# The grade and the report are tools/house-soak/report.py's, from the raw files
# this leaves in the run directory. Exit 0 when every criterion passes, 1 when
# one fails, 3 (lib.sh's missing prerequisite) when there is no libasound.
#
# Everything is bounded: the script re-runs itself under `timeout` at the soak
# length plus 600 s, the server under the same bound, and every client under
# its own --run-seconds.
#
#   CHORUS_HOUSE_SOAK_SECONDS=3600 tools/house-soak-run.sh   # or: make verify-house-soak
#
# Parameters (environment, each recorded in the report):
#   CHORUS_HOUSE_SOAK_SECONDS      the load window, seconds (default 3600; at least 60)
#   CHORUS_HOUSE_SOAK_SEED         the command load's seed (default 20261001)
#   CHORUS_HOUSE_SOAK_INTERVAL_MS  one command every this many ms (default 2000)
#   CHORUS_HOUSE_SOAK_CIVIL_TIME   the server's fixed civil time (default mon-23:30)
#   CHORUS_HOUSE_SOAK_DIR          the run directory (default $TMPDIR/chorus-house-soak/<UTC stamp>)
#   CHORUS_HOUSE_SOAK_REPORT       the report to write, absolute or relative to the repository (default
#                                  docs/measurements/house-soak-8-rooms.md; empty writes none)
#   CHORUS_HOUSE_SOAK_LABEL        what the report calls the run (default "one-hour soak" at 3600 s,
#                                  else "harness short run")
#   CHORUS_HOUSE_SOAK_BUILD        the commit the binaries were built from (default HEAD; see below)

source "$(dirname "$0")/lib.sh"

SOAK_SECONDS="${CHORUS_HOUSE_SOAK_SECONDS:-3600}"
case "$SOAK_SECONDS" in
    '' | *[!0-9]*)
        say "chorus: CHORUS_HOUSE_SOAK_SECONDS must be a whole number of seconds, not '$SOAK_SECONDS'"
        exit 2
        ;;
esac
if [ "$SOAK_SECONDS" -lt 60 ]; then
    say "chorus: CHORUS_HOUSE_SOAK_SECONDS is $SOAK_SECONDS; the soak is at least 60 s (the RSS warm-up alone is 30 s)"
    exit 2
fi
BOUND_SECONDS=$(( SOAK_SECONDS + 600 ))

# The whole run under one bound, the soak plus 600 s: setup, the drain and the
# report fit in that with room to spare, and a hang anywhere ends it.
if [ "${CHORUS_HOUSE_SOAK_BOUNDED:-0}" != 1 ]; then
    exec timeout --kill-after=30 "$BOUND_SECONDS" \
        env CHORUS_HOUSE_SOAK_BOUNDED=1 bash "$0" "$@"
fi

SEED="${CHORUS_HOUSE_SOAK_SEED:-20261001}"
INTERVAL_MS="${CHORUS_HOUSE_SOAK_INTERVAL_MS:-2000}"
CIVIL_TIME="${CHORUS_HOUSE_SOAK_CIVIL_TIME:-mon-23:30}"
CIVIL_DAY="${CIVIL_TIME%%-*}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
RUN_DIR="${CHORUS_HOUSE_SOAK_DIR:-${TMPDIR:-/tmp}/chorus-house-soak/$STAMP}"
REPORT="${CHORUS_HOUSE_SOAK_REPORT-docs/measurements/house-soak-8-rooms.md}"
if [ -n "${CHORUS_HOUSE_SOAK_LABEL:-}" ]; then
    LABEL="$CHORUS_HOUSE_SOAK_LABEL"
elif [ "$SOAK_SECONDS" -ge 3600 ]; then
    LABEL="one-hour soak"
else
    LABEL="harness short run"
fi

# The rooms (ASSUMED, see above), the bonded ones, and the wireless one.
ROOMS=(living kitchen dining primary office patio bathroom guest)
BONDED=(living kitchen)
WIRELESS=bathroom
MAX_CLIENTS=16
CONTROL_WORKERS=8

CRITERION="a house of eight rooms on one --slots 8 server keeps its threads, memory, fanout, limits and sessions for the whole soak under a seeded command load (host software soak on ALSA null)"

export CHORUS_CLIENT_DEVICE="${CHORUS_CLIENT_DEVICE:-null}"
# The client's libasound: the system's, else the rootless alsa-lib, found the
# way tools/alsa-null-run.sh finds it.
if ! use_rootless_alsa; then
    missing_prerequisite "$CRITERION" \
        "libasound.so.2: the system has none and $(alsa_prefix)/lib holds none" \
        "$ALSA_INSTALL_HINT; or set CHORUS_ALSA_PREFIX to an existing install"
fi

build_once
require_audio_device "$CRITERION"

# The build the report names. The binaries are what `cargo build` made from
# this checkout; the report names HEAD unless told otherwise, and a commit
# given instead must hold the same crates, manifests and configuration as this
# checkout, which is checked here and recorded, so a branch whose only change
# is outside the build can name the main commit it was cut from (a squash
# merge leaves branch commits out of main's history, and rule 11 wants a
# commit in it).
BUILD="$(git -C "$REPO_ROOT" rev-parse "${CHORUS_HOUSE_SOAK_BUILD:-HEAD}")"
BUILD_SAME_TREE=yes
if ! git -C "$REPO_ROOT" diff --quiet "$BUILD" -- crates Cargo.toml Cargo.lock \
    rust-toolchain.toml config; then
    BUILD_SAME_TREE=no
fi
if [ "$BUILD_SAME_TREE" = no ] && [ -n "${CHORUS_HOUSE_SOAK_BUILD:-}" ]; then
    say "chorus: CHORUS_HOUSE_SOAK_BUILD=$CHORUS_HOUSE_SOAK_BUILD does not hold the crates, manifests and config this checkout built; name the commit the binaries came from"
    exit 2
fi

rm -rf "$RUN_DIR"
mkdir -p "$RUN_DIR"
say "chorus: house soak ($LABEL, $SOAK_SECONDS s), host software soak on ALSA null, not a hardware measurement and not timing evidence"
say "chorus: run directory $RUN_DIR"

mono() { python3 -c 'import time; print(f"{time.monotonic():.6f}")'; }
# The kernel's own count of a process's threads, the entries of /proc/<pid>/task.
threads() { sed -n 's/^Threads:[[:space:]]*\([0-9]*\).*/\1/p' "/proc/$1/status" 2>/dev/null || echo 0; }

AUDIO="$(free_port)"
CONTROL="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"

PIDS=()
stop_everything() {
    local pid
    for pid in "${PIDS[@]:-}"; do
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
    PIDS=()
}
trap stop_everything EXIT

# --- parameters, as the report will print them --------------------------------
{
    printf 'label=%s\n' "$LABEL"
    printf 'soak_seconds=%s\n' "$SOAK_SECONDS"
    printf 'bound_seconds=%s\n' "$BOUND_SECONDS"
    printf 'seed=%s\n' "$SEED"
    printf 'interval_ms=%s\n' "$INTERVAL_MS"
    printf 'civil_time=%s\n' "$CIVIL_TIME"
    printf 'rooms=%s\n' "$(IFS=,; printf '%s' "${ROOMS[*]}")"
    printf 'bonded=%s\n' "$(IFS=,; printf '%s' "${BONDED[*]}")"
    printf 'wireless=%s\n' "$WIRELESS"
    printf 'slots=8\n'
    printf 'max_clients=%s\n' "$MAX_CLIENTS"
    printf 'control_workers=%s\n' "$CONTROL_WORKERS"
    printf 'device=%s\n' "$CHORUS_CLIENT_DEVICE"
    printf 'libasound=%s\n' "${LD_LIBRARY_PATH:-system}"
    printf 'build=%s\n' "$BUILD"
    printf 'build_same_tree=%s\n' "$BUILD_SAME_TREE"
    printf 'head=%s\n' "$(git -C "$REPO_ROOT" rev-parse HEAD)"
    printf 'dirty=%s\n' "$([ -z "$(git -C "$REPO_ROOT" status --porcelain -- crates Cargo.toml Cargo.lock config)" ] && echo no || echo yes)"
    printf 'contract_args=%s\n' "${CONTRACT_ARGS[*]:-none}"
    printf 'kernel=%s\n' "$(uname -r)"
    printf 'cpus_online=%s\n' "$(nproc)"
    printf 'cpu_quota=%s\n' "$(cat /sys/fs/cgroup/cpu.max 2>/dev/null || echo unknown)"
    printf 'cpu_model=%s\n' "$(sed -n 's/^model name[[:space:]]*:[[:space:]]*//p' /proc/cpuinfo | head -n 1)"
    printf 'mem_total_kb=%s\n' "$(sed -n 's/^MemTotal:[[:space:]]*\([0-9]*\).*/\1/p' /proc/meminfo)"
    printf 'profile=%s\n' "$(basename "$BIN_DIR")"
} > "$RUN_DIR/params"

# --- the server -----------------------------------------------------------------
ZONE_ARGS=()
for ROOM in "${ROOMS[@]}"; do
    if [ "$ROOM" = "$WIRELESS" ]; then
        ZONE_ARGS+=(--zone "$ROOM=wireless")
    else
        ZONE_ARGS+=(--zone "$ROOM")
    fi
done
timeout --kill-after=10 "$BOUND_SECONDS" "$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "127.0.0.1:$AUDIO" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    --rttime-us "$(conf rttime_us)" \
    --rt-priority "$(conf rt_priority)" \
    --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
    --serve-forever \
    --slots 8 \
    --max-clients "$MAX_CLIENTS" \
    --control-listen "127.0.0.1:$CONTROL" \
    --control-workers "$CONTROL_WORKERS" \
    --civil-time "$CIVIL_TIME" \
    --state-file "$RUN_DIR/zones.state" \
    "${ZONE_ARGS[@]}" \
    "${CONTRACT_ARGS[@]}" >"$RUN_DIR/server.log" 2>&1 &
SERVER_PID=$!
PIDS+=("$SERVER_PID")
waited=0
until grep -q 'control listening on=' "$RUN_DIR/server.log" 2>/dev/null; do
    waited=$(( waited + 1 ))
    if [ "$waited" -gt 100 ] || ! kill -0 "$SERVER_PID" 2>/dev/null; then
        say "FAIL the server never came up; it said:"
        sed 's/^/    /' "$RUN_DIR/server.log"
        exit 1
    fi
    sleep 0.2
done
# `timeout` is the parent; the server is its child, and /proc is read off the server.
SERVER_REAL="$(pgrep -P "$SERVER_PID" -x chorus-server || printf '%s' "$SERVER_PID")"
printf 'server pid=%s\n' "$SERVER_REAL" > "$RUN_DIR/pids"

python3 "$REPO_ROOT/tools/house-soak/load.py" subscribe --control "127.0.0.1:$CONTROL" \
    --out "$RUN_DIR/states.log" 2>"$RUN_DIR/subscriber.err" &
PIDS+=("$!")

python3 "$REPO_ROOT/tools/house-soak/load.py" setup --control "127.0.0.1:$CONTROL" \
    --bonded "$(IFS=,; printf '%s' "${BONDED[*]}")" --wireless "$WIRELESS" \
    --out "$RUN_DIR/setup.jsonl" || {
    say "FAIL the house setup (wired attaches, two bonds, the wireless-room refusal) did not go as expected:"
    sed 's/^/    /' "$RUN_DIR/setup.jsonl"
    exit 1
}

# --- the endpoints ----------------------------------------------------------------
# Each runs long enough to cover the wait for the others, the settle, the load
# window and the closing measurements, and then stops by itself, so its summary
# is read off a clean exit rather than a kill.
CLIENT_SECONDS=$(( SOAK_SECONDS + 90 ))
ENDPOINTS=()
for ROOM in "${ROOMS[@]}"; do
    case " ${BONDED[*]} " in
        *" $ROOM "*) ENDPOINTS+=("$ROOM:$ROOM-l" "$ROOM:$ROOM-r") ;;
        *) ENDPOINTS+=("$ROOM:$ROOM") ;;
    esac
done
for E in "${ENDPOINTS[@]}"; do
    ROOM="${E%%:*}"
    ID="${E#*:}"
    TRANSPORT=()
    [ "$ROOM" = "$WIRELESS" ] && TRANSPORT=(--transport wireless)
    printf '%s %s %s\n' "$ID" "$ROOM" "$(mono)" >> "$RUN_DIR/launch"
    "$BIN_DIR/chorus-client" \
        --ephemeral-identity \
        --server "127.0.0.1:$AUDIO" \
        --control "127.0.0.1:$CONTROL" \
        --zone "$ROOM" \
        --endpoint "$ID" \
        --rejoin \
        --run-seconds "$CLIENT_SECONDS" \
        --device "$CHORUS_CLIENT_DEVICE" \
        --delay-log "$RUN_DIR/endpoint-$ID.log" \
        --sync-interval-ms "$(sync_conf sync_interval_ms)" \
        "${TRANSPORT[@]}" \
        >"$RUN_DIR/endpoint-$ID.out" 2>&1 &
    PIDS+=("$!")
    printf 'endpoint %s pid=%s\n' "$ID" "$!" >> "$RUN_DIR/pids"
done

# Every endpoint has a session before the clock starts.
waited=0
until [ "$(grep -c 'client session ' "$RUN_DIR/server.log" || true)" -ge "${#ENDPOINTS[@]}" ]; do
    waited=$(( waited + 1 ))
    if [ "$waited" -gt 150 ]; then
        say "FAIL not every endpoint opened a session within 30 s; the server has $(grep -c 'client session ' "$RUN_DIR/server.log" || true) of ${#ENDPOINTS[@]}"
        exit 1
    fi
    sleep 0.2
done
# A settle, so the first population and RSS samples are of a house that is up.
sleep 10

proc_sample() {
    local at="$1" kind id pid
    {
        printf '%s server %s %s %s\n' "$at" "$SERVER_REAL" "$(threads "$SERVER_REAL")" \
            "$(sed -n 's/^VmRSS:[[:space:]]*\([0-9]*\).*/\1/p' "/proc/$SERVER_REAL/status" 2>/dev/null || echo 0)"
        while read -r kind id pid; do
            [ "$kind" = endpoint ] || continue
            pid="${pid#pid=}"
            printf '%s %s %s %s %s\n' "$at" "$id" "$pid" "$(threads "$pid")" \
                "$(sed -n 's/^VmRSS:[[:space:]]*\([0-9]*\).*/\1/p' "/proc/$pid/status" 2>/dev/null || echo 0)"
        done < "$RUN_DIR/pids"
    }
}

START_WALL="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
START_MONO="$(mono)"
proc_sample "$START_MONO" > "$RUN_DIR/population-start"
printf 'start_wall=%s\nstart_mono=%s\n' "$START_WALL" "$START_MONO" >> "$RUN_DIR/params"

# RSS and threads every 10 s for the whole window (ASSUMED period: 360 samples
# an hour per process, enough to see a trend and small enough to keep).
(
    while :; do
        proc_sample "$(mono)"
        sleep 10
    done
) > "$RUN_DIR/rss.tsv" 2>/dev/null &
SAMPLER_PID=$!
PIDS+=("$SAMPLER_PID")

say "chorus: ${#ENDPOINTS[@]} endpoints up in ${#ROOMS[@]} rooms; the load runs $SOAK_SECONDS s, one command every $INTERVAL_MS ms, seed $SEED"
python3 "$REPO_ROOT/tools/house-soak/load.py" run --control "127.0.0.1:$CONTROL" \
    --rooms "$(IFS=,; printf '%s' "${ROOMS[*]}")" --seconds "$SOAK_SECONDS" \
    --interval-ms "$INTERVAL_MS" --seed "$SEED" --civil-day "$CIVIL_DAY" \
    --out "$RUN_DIR/commands.jsonl" --switches "$RUN_DIR/switches.jsonl"

# --- the end of the window: what is measured with everything still up ------------
END_MONO="$(mono)"
END_WALL="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
proc_sample "$END_MONO" > "$RUN_DIR/population-end"
kill "$SAMPLER_PID" 2>/dev/null || true
python3 - "$CONTROL" "$RUN_DIR" <<'PY'
import http.client, sys
port, run = int(sys.argv[1]), sys.argv[2]
for path, name in (("/api/report", "report-end"), ("/api/state", "state-end.json")):
    c = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
    c.request("GET", path, headers={"Host": "chorus"})
    open(f"{run}/{name}", "w").write(c.getresponse().read().decode())
PY
printf 'end_wall=%s\nend_mono=%s\n' "$END_WALL" "$END_MONO" >> "$RUN_DIR/params"
say "chorus: the load window closed at $END_WALL; waiting for the endpoints to stop by their run length"

# The endpoints stop by themselves (their --run-seconds), each printing its
# summary; then the server is stopped.
while read -r kind _ pid; do
    [ "$kind" = endpoint ] || continue
    wait "${pid#pid=}" 2>/dev/null || true
done < "$RUN_DIR/pids"
kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true
FINISH_MONO="$(mono)"
printf 'finish_mono=%s\n' "$FINISH_MONO" >> "$RUN_DIR/params"
stop_everything

# --- the grade and the report ------------------------------------------------------
REPORT_ARGS=()
case "$REPORT" in
    '') ;;
    /*) REPORT_ARGS=(--out "$REPORT") ;;
    *) REPORT_ARGS=(--out "$REPO_ROOT/$REPORT") ;;
esac
set +e
python3 "$REPO_ROOT/tools/house-soak/report.py" --run-dir "$RUN_DIR" "${REPORT_ARGS[@]}"
STATUS=$?
set -e
[ -n "$REPORT" ] && say "chorus: report written to $REPORT"
exit "$STATUS"
