#!/usr/bin/env bash
# What K independent streams cost a running chorus-server in CPU and memory, on
# this host: the measurement behind proposal P11 (decision K76, "the
# independent-stream limit from CPU/memory measurements of receivers and
# decoders on the homelab's class of host"). The report it feeds is
# docs/measurements/concurrent-streams-host.md.
#
# HOST MEASUREMENT OF CPU TIME AND MEMORY. NOT A GATE STEP, NOT TIMING EVIDENCE
# AND NOT A PASS/FAIL CHECK: it prints figures, and the only thing it refuses
# is a run that did not happen (a server that did not start, a renderer that
# did not play, a prerequisite that is not here). On a shared host every CPU
# figure is an upper bound, and the script records the load average at both
# ends of every window so a reader can see how shared it was.
#
# What runs, all on loopback, release-built:
#
#   - one chorus-server --slots 16 --players 16 --upnp --serve-forever with
#     sixteen rooms (room-01 to room-16), its renderers on the loopback seams
#     crates/server/tests/upnp_control_point.rs uses (--upnp-listen
#     127.0.0.1:0, --upnp-ssdp-port 0, --upnp-ssdp-group at a loopback socket,
#     --media-allow-loopback), at the rate, channels, format and chunk of
#     config/verification.conf;
#   - sixteen chorus-client processes on the ALSA `null` device, one per room,
#     so every stream is carried the whole way: fetched, decoded, resampled to
#     the server's rate, cut into its slot's chunks and sent on a session;
#   - a local HTTP media server holding one 130 s file per settled format
#     (tools/concurrent-streams/harness.py media, the pinned reference
#     encoders of `make decode-fixtures`);
#   - a scripted UPnP control point that plays the file on K renderers at once
#     (K rooms, K player threads, K slots), for each K of CHORUS_STREAMS_KS,
#     and a /proc sampler over a fixed window at each K.
#
# Modes (CHORUS_STREAMS_MODE):
#   formats  (default) for each format of CHORUS_STREAMS_FORMATS: a fresh
#            server, an idle window (K = 0, rooms attached, nothing playing),
#            then a window at each K.
#   idle     what the renderers and players cost when nothing plays and no
#            endpoint is connected: the same server three times, without
#            --players and --upnp, with --players 16, and with --players 16
#            --upnp (16 renderers), the last also with one control point
#            subscribed to every service of every renderer.
#
#   decoders the decoders alone: `chorus-server --probe-media` on each file,
#            three times (no fetch, no resampler, no stream, no thread).
#
# Parameters (environment, each recorded in the run's `params`):
#   CHORUS_STREAMS_MODE      formats | idle | decoders (default formats)
#   CHORUS_STREAMS_FORMATS   default "wav48 wav flac mp3 vorbis opus alac"
#   CHORUS_STREAMS_KS        default 1,2,4,8,16
#   CHORUS_STREAMS_WINDOW    the sampled window, seconds (default 60)
#   CHORUS_STREAMS_SETTLE    seconds between PLAYING and the window (default 10)
#   CHORUS_STREAMS_DIR       the run directory (default $TMPDIR/chorus-concurrent-streams/<UTC stamp>)
#   CHORUS_STREAMS_MEDIA     where the media files are kept between runs (default <run dir>/media)
#   REFDEC                   the reference programs' prefix (default /cache/opt/chorus-refdec)
#   CHORUS_SKIP_BUILD=1      use the release binaries already in target/release
#
#   make concurrent-streams          # callers hold the heavy locks, as for the gate

source "$(dirname "$0")/lib.sh"

MODE="${CHORUS_STREAMS_MODE:-formats}"
FORMATS="${CHORUS_STREAMS_FORMATS:-wav48 wav flac mp3 vorbis opus alac}"
KS="${CHORUS_STREAMS_KS:-1,2,4,8,16}"
WINDOW="${CHORUS_STREAMS_WINDOW:-60}"
SETTLE="${CHORUS_STREAMS_SETTLE:-10}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
RUN_DIR="${CHORUS_STREAMS_DIR:-${TMPDIR:-/tmp}/chorus-concurrent-streams/$STAMP}"
MEDIA_DIR="${CHORUS_STREAMS_MEDIA:-$RUN_DIR/media}"
REFDEC="${REFDEC:-/cache/opt/chorus-refdec}"
ROOMS=16
HARNESS="$REPO_ROOT/tools/concurrent-streams/harness.py"
CRITERION="the CPU and memory K concurrent independent streams cost chorus-server on this host (P11, K76): a measurement, not a check"

case "$MODE" in
    formats | idle | decoders) ;;
    *)
        say "chorus: CHORUS_STREAMS_MODE must be formats, idle or decoders, not '$MODE'"
        exit 2
        ;;
esac
case "$WINDOW$SETTLE" in
    '' | *[!0-9]*)
        say "chorus: CHORUS_STREAMS_WINDOW and CHORUS_STREAMS_SETTLE are whole seconds"
        exit 2
        ;;
esac

# The whole run under one bound: every window, its settle and a minute of
# start-up per server, so a hang anywhere ends it.
if [ "${CHORUS_STREAMS_BOUNDED:-0}" != 1 ]; then
    exec timeout --kill-after=30 3600 env CHORUS_STREAMS_BOUNDED=1 bash "$0" "$@"
fi

# The measurement is of the release build: the debug profile's decoders cost
# several times as much and would say nothing about a deployment.
BIN_DIR="$TARGET_DIR/release"
export BIN_DIR
if [ "${CHORUS_SKIP_BUILD:-0}" != 1 ]; then
    (cd "$REPO_ROOT" && cargo build --quiet --release --locked -p chorus-server -p chorus-client-linux)
fi
for BIN in chorus-server chorus-client; do
    if [ ! -x "$BIN_DIR/$BIN" ]; then
        missing_prerequisite "$CRITERION" "$BIN_DIR/$BIN, the release build" \
            "cargo build --release --locked -p chorus-server -p chorus-client-linux"
    fi
done

export CHORUS_CLIENT_DEVICE="${CHORUS_CLIENT_DEVICE:-null}"
if [ "$MODE" = formats ]; then
    if ! use_rootless_alsa; then
        missing_prerequisite "$CRITERION" \
            "libasound.so.2: the system has none and $(alsa_prefix)/lib holds none" \
            "$ALSA_INSTALL_HINT; or set CHORUS_ALSA_PREFIX to an existing install"
    fi
    require_audio_device "$CRITERION"
fi
if [ "$MODE" != idle ]; then
    if [ ! -x "$REFDEC/bin/ffmpeg" ] && [ ! -e "$MEDIA_DIR/media.tsv" ]; then
        missing_prerequisite "$CRITERION" \
            "the pinned reference encoders at $REFDEC (ffmpeg, lame, flac), to make the test signals" \
            "fixtures/README.md has the install command and the pins; or point REFDEC at the prefix"
    fi
fi

mkdir -p "$RUN_DIR"
say "chorus: concurrent streams ($MODE), a host measurement of CPU and memory on a shared host: upper bounds, not timing evidence"
say "chorus: run directory $RUN_DIR"

read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"

{
    printf 'mode=%s\n' "$MODE"
    printf 'formats=%s\n' "$FORMATS"
    printf 'ks=%s\n' "$KS"
    printf 'window_s=%s\n' "$WINDOW"
    printf 'settle_s=%s\n' "$SETTLE"
    printf 'rooms=%s\n' "$ROOMS"
    printf 'head=%s\n' "$(git -C "$REPO_ROOT" rev-parse HEAD)"
    printf 'dirty=%s\n' "$([ -z "$(git -C "$REPO_ROOT" status --porcelain -- crates Cargo.toml Cargo.lock config third_party)" ] && echo no || echo yes)"
    printf 'profile=release\n'
    printf 'server_sha256=%s\n' "$(sha256sum "$BIN_DIR/chorus-server" | cut -d ' ' -f 1)"
    printf 'rustc=%s\n' "$(cd "$REPO_ROOT" && rustc --version)"
    printf 'contract_args=%s\n' "${CONTRACT_ARGS[*]:-none}"
    printf 'kernel=%s\n' "$(uname -srm)"
    printf 'cpu_model=%s\n' "$(sed -n 's/^model name[[:space:]]*:[[:space:]]*//p' /proc/cpuinfo | head -n 1)"
    printf 'cpus_in_cpuinfo=%s\n' "$(grep -c '^processor' /proc/cpuinfo)"
    printf 'nproc=%s\n' "$(nproc)"
    printf 'cpu_max=%s\n' "$(cat /sys/fs/cgroup/cpu.max 2>/dev/null || echo unknown)"
    printf 'memory_max=%s\n' "$(cat /sys/fs/cgroup/memory.max 2>/dev/null || echo unknown)"
    printf 'mem_total_kb=%s\n' "$(sed -n 's/^MemTotal:[[:space:]]*\([0-9]*\).*/\1/p' /proc/meminfo)"
    printf 'clk_tck=%s\n' "$(getconf CLK_TCK)"
    printf 'loadavg_at_start=%s\n' "$(cut -d ' ' -f 1-3 /proc/loadavg)"
    printf 'started_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'device=%s\n' "$CHORUS_CLIENT_DEVICE"
} > "$RUN_DIR/params"
if command -v lscpu >/dev/null 2>&1; then
    lscpu > "$RUN_DIR/lscpu.txt"
fi

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

ROOM_NAMES=()
ZONE_ARGS=()
for i in $(seq 1 "$ROOMS"); do
    ROOM="$(printf 'room-%02d' "$i")"
    ROOM_NAMES+=("$ROOM")
    ZONE_ARGS+=(--zone "$ROOM")
done

# start_server <name> <extra flags...>: one server in $RUN_DIR/<name>, its real
# pid in SERVER_REAL, its control address in CONTROL, its SSDP port in SSDP_PORT
# (0 without --upnp).
start_server() {
    local name="$1"
    shift
    SERVER_DIR="$RUN_DIR/$name"
    mkdir -p "$SERVER_DIR/identity"
    AUDIO="$(free_port)"
    CONTROL_PORT="$(free_port)"
    CONTROL="127.0.0.1:$CONTROL_PORT"
    "$BIN_DIR/chorus-server" \
        --identity-dir "$SERVER_DIR/identity" \
        --state-file "$SERVER_DIR/state" \
        --listen "127.0.0.1:$AUDIO" \
        --rate "$(conf sample_rate_hz)" \
        --channels "$(conf channels)" \
        --format "$(conf sample_format)" \
        --chunk-us "$(conf chunk_us)" \
        --rttime-us "$(conf rttime_us)" \
        --rt-priority "$(conf rt_priority)" \
        --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
        --serve-forever \
        --slots "$ROOMS" \
        --max-clients "$ROOMS" \
        --control-listen "$CONTROL" \
        "${ZONE_ARGS[@]}" \
        "${CONTRACT_ARGS[@]}" \
        "$@" >"$SERVER_DIR/server.log" 2>&1 &
    SERVER_REAL=$!
    PIDS+=("$SERVER_REAL")
    local waited=0
    until grep -q 'control listening on=' "$SERVER_DIR/server.log" 2>/dev/null &&
        grep -q 'thread role=' "$SERVER_DIR/server.log" 2>/dev/null; do
        waited=$((waited + 1))
        if [ "$waited" -gt 150 ] || ! kill -0 "$SERVER_REAL" 2>/dev/null; then
            say "FAIL the server ($name) never came up; it said:"
            sed 's/^/    /' "$SERVER_DIR/server.log" | tail -n 40
            exit 1
        fi
        sleep 0.2
    done
    SSDP_PORT=0
    case " $* " in
        *" --upnp "*)
            waited=0
            until grep -q 'upnp renderers listening on=' "$SERVER_DIR/server.log"; do
                waited=$((waited + 1))
                if [ "$waited" -gt 150 ]; then
                    say "FAIL the server ($name) never said its renderers were listening"
                    exit 1
                fi
                sleep 0.2
            done
            SSDP_PORT="$(sed -n 's/.*upnp renderers listening on=.*ssdp_port=\([0-9]*\).*/\1/p' "$SERVER_DIR/server.log" | head -n 1)"
            ;;
    esac
}

stop_server() {
    kill "$SERVER_REAL" 2>/dev/null || true
    wait "$SERVER_REAL" 2>/dev/null || true
}

# The discovery notifications need somewhere to go that is not the network: a
# loopback UDP socket nobody reads (the kernel drops what does not fit).
python3 - "$RUN_DIR/notify-port" <<'PY' &
import socket, sys, time
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.bind(("127.0.0.1", 0))
open(sys.argv[1], "w").write(str(s.getsockname()[1]))
while True:
    time.sleep(3600)
PY
PIDS+=("$!")
until [ -s "$RUN_DIR/notify-port" ]; do sleep 0.1; done
NOTIFY="127.0.0.1:$(cat "$RUN_DIR/notify-port")"
UPNP_ARGS=(--upnp --upnp-listen 127.0.0.1:0 --upnp-ssdp-port 0 --upnp-ssdp-group "$NOTIFY" --media-allow-loopback)

if [ "$MODE" = idle ]; then
    start_server idle-base
    python3 "$HARNESS" measure --pid "$SERVER_REAL" --server-log "$SERVER_DIR/server.log" --out "$RUN_DIR" \
        --config "16 rooms, 16 slots, control plane; no players, no renderers" \
        --window "$WINDOW" --settle "$SETTLE" --idle-bare
    stop_server
    start_server idle-players --players "$ROOMS"
    python3 "$HARNESS" measure --pid "$SERVER_REAL" --server-log "$SERVER_DIR/server.log" --out "$RUN_DIR" \
        --config "the same with --players 16" \
        --window "$WINDOW" --settle "$SETTLE" --idle-bare
    stop_server
    start_server idle-upnp --players "$ROOMS" "${UPNP_ARGS[@]}"
    python3 "$HARNESS" measure --pid "$SERVER_REAL" --server-log "$SERVER_DIR/server.log" --out "$RUN_DIR" \
        --config "the same with --players 16 --upnp (16 renderers)" \
        --window "$WINDOW" --settle "$SETTLE" --idle-bare --subscribe \
        --ssdp-port "$SSDP_PORT" --renderers "$ROOMS"
    stop_server
else
    if [ ! -e "$MEDIA_DIR/media.tsv" ]; then
        python3 "$HARNESS" media --prefix "$REFDEC" --out "$MEDIA_DIR" > /dev/null
    fi
    cp "$MEDIA_DIR/media.tsv" "$RUN_DIR/media.tsv"
fi
if [ "$MODE" = decoders ]; then
    python3 "$HARNESS" probe --server "$BIN_DIR/chorus-server" --dir "$MEDIA_DIR" --out "$RUN_DIR"
elif [ "$MODE" = formats ]; then
    python3 "$HARNESS" serve --dir "$MEDIA_DIR" --port-file "$RUN_DIR/media-port" &
    PIDS+=("$!")
    until [ -s "$RUN_DIR/media-port" ]; do sleep 0.1; done
    MEDIA_PORT="$(cat "$RUN_DIR/media-port")"

    for FORMAT in $FORMATS; do
        start_server "server-$FORMAT" --players "$ROOMS" "${UPNP_ARGS[@]}"
        CLIENTS=()
        for ROOM in "${ROOM_NAMES[@]}"; do
            "$BIN_DIR/chorus-client" \
                --ephemeral-identity \
                --server "127.0.0.1:$AUDIO" \
                --control "$CONTROL" \
                --zone "$ROOM" \
                --endpoint "speaker-$ROOM" \
                --rejoin \
                --run-seconds 3600 \
                --device "$CHORUS_CLIENT_DEVICE" \
                --no-delay-log \
                --sync-interval-ms "$(sync_conf sync_interval_ms)" \
                >"$SERVER_DIR/endpoint-$ROOM.out" 2>&1 &
            CLIENTS+=("$!")
            PIDS+=("$!")
        done
        waited=0
        until [ "$(sed -n 's/.*client session peer=[^ ]* id=\([^ ]*\) .*/\1/p' "$SERVER_DIR/server.log" | sort -u | wc -l)" -ge "$ROOMS" ]; do
            waited=$((waited + 1))
            if [ "$waited" -gt 300 ]; then
                say "FAIL not every endpoint opened a session within 60 s"
                tail -n 5 "$SERVER_DIR"/endpoint-room-01.out | sed 's/^/    /'
                exit 1
            fi
            sleep 0.2
        done
        python3 "$HARNESS" measure --pid "$SERVER_REAL" --server-log "$SERVER_DIR/server.log" --out "$RUN_DIR" \
            --config "16 rooms, 16 endpoints, --players 16 --upnp" \
            --format "$FORMAT" --ks "$KS" --window "$WINDOW" --settle "$SETTLE" \
            --ssdp-port "$SSDP_PORT" --renderers "$ROOMS" --control "$CONTROL" \
            --rooms "$(IFS=,; printf '%s' "${ROOM_NAMES[*]}")" --media-port "$MEDIA_PORT"
        for PID in "${CLIENTS[@]}"; do
            kill "$PID" 2>/dev/null || true
        done
        stop_server
    done
fi

{
    printf 'loadavg_at_end=%s\n' "$(cut -d ' ' -f 1-3 /proc/loadavg)"
    printf 'finished_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} >> "$RUN_DIR/params"
stop_everything
say ""
if [ -e "$RUN_DIR/windows.jsonl" ]; then
    say "chorus: the report's inputs (CPU as % of one core, from the scheduler's run time; memory in kB):"
    python3 "$HARNESS" summary "$RUN_DIR/windows.jsonl"
fi
say ""
sed 's/^/    /' "$RUN_DIR/params"
