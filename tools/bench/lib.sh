#!/usr/bin/env bash
# The bench report library (K45): every hardware entry point calls it at its end.
#
# A hardware run writes its report into docs/measurements/<topic>-<date>.md with
# `Source: hardware`, the build it measured (the checkout's HEAD, a commit on
# origin/main, on a clean tree), the device(s), the date, the result, and every
# raw file with its sha256; then, only when asked (CHORUS_BENCH_PR=1), it
# commits on bench/<date>-<topic>, pushes and opens a PR with gh. Every later
# goal validates open bench/* PRs with tools/bench/validate-report.sh and merges
# them. docs/bench.md is the owner-facing prose; tools/bench/topics.conf lists
# the topics and their required fields.
#
# An entry point has two halves:
#   measure         needs the hardware, refuses by name without it (tools/lib.sh
#                   require_* guards), and leaves its raw files in a run
#                   directory with a manifest (bench-run.conf)
#   report and PR   needs no hardware: analyses the raw files, writes the
#                   report, publishes it when asked. `<entry point>
#                   --report-from <run dir>` runs this half alone, which is how
#                   a run is re-reported and how tools/bench/e2e-test.sh drives
#                   it from committed fixture captures.
#
# Raw data: a file of at most BENCH_RAW_FILE_LIMIT bytes (16 MiB) is committed
# under docs/measurements/raw/<stem>/, up to BENCH_RAW_TOTAL_LIMIT (64 MiB) per
# report, in the order the run listed them; anything larger is hashed in the
# report only and stays on the owner's machine (in the run directory). GitHub
# warns on files above 50 MiB, blocks them above 100 MiB and asks for
# repositories ideally under 1 GB
# (https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github,
# read 2026-09-30), so a 16 MiB file and a 64 MiB report stay well inside both
# and leave room for many bench sessions.
#
# Environment:
#   CHORUS_BENCH_PR=1          commit, push and open the PR (default: write the
#                              report into the working tree and stop)
#   CHORUS_BENCH_GH            the gh to call (default: gh); tests pass a fake
#   CHORUS_BENCH_REMOTE        the remote to check against and push to
#                              (default: origin); tests use a local bare repo
#   CHORUS_BENCH_RUN_DIR       where the measure half keeps the run (default: a
#                              new directory under CHORUS_MEASURE_OUT or TMPDIR)
#   CHORUS_BENCH_DATE          the run's date, YYYY-MM-DD (default: today, UTC)
#   CHORUS_BENCH_DEVICE_NOTE   free text the owner adds to the device lines,
#                              for example the board model (no hostnames,
#                              addresses or names: the validator refuses them)
#   CHORUS_BENCH_NO_FETCH=1    do not fetch the remote before checking HEAD
#   CHORUS_BENCH_ALLOW_FIXTURE_RAW=1
#                              let a raw file equal a committed fixture; only
#                              the fixture-driven test sets it

BENCH_DIR="$REPO_ROOT/tools/bench"
BENCH_SCHEMA="chorus-bench-report/1"
BENCH_RAW_FILE_LIMIT=16777216
BENCH_RAW_TOTAL_LIMIT=67108864
BENCH_GH="${CHORUS_BENCH_GH:-gh}"
BENCH_REMOTE="${CHORUS_BENCH_REMOTE:-origin}"
# shellcheck disable=SC2034 # read by the entry points that source this
BENCH_REPORT_FROM=""
BENCH_ARGS=()
BENCH_FIELDS=()
BENCH_ANALYSES=()
BENCH_EXTRA_FILES=()
# chorus-measure names the build of THIS checkout, whichever worktree built it.
export CHORUS_MEASURE_ROOT="$REPO_ROOT"

bench_say() { printf 'bench: %s\n' "$*"; }

# Refuse the bench step by name. Deliberately not a require_* guard: those mark
# an entry point as environment-dependent for
# tools/unrun-checks-are-visibly-unrun.sh, and this is a refusal about the
# checkout, not about the hardware.
bench_refuse() {
    printf 'BENCH REPORT REFUSED\n  reason: %s\n  nothing has been committed, pushed or opened.\n' "$*" >&2
    exit 4
}

# The value of one manifest key (the first), or of every one with --all.
bench_manifest() {
    local key="$1" all="${2:-}"
    local file="$BENCH_RUN_DIR/bench-run.conf"
    [ -f "$file" ] || return 0
    if [ "$all" = "--all" ]; then
        sed -n "s/^${key} = //p" "$file"
    else
        sed -n "s/^${key} = //p" "$file" | head -n 1
    fi
}

# One column of a topic's line in topics.conf.
bench_topic_column() {
    local topic="$1" column="$2"
    awk -F'|' -v t="$topic" -v c="$column" '
        /^[[:space:]]*#/ || NF < 3 { next }
        { k = $1; gsub(/^[ \t]+|[ \t]+$/, "", k) }
        k == t { v = $c; gsub(/^[ \t]+|[ \t]+$/, "", v); print v; exit }
    ' "$BENCH_DIR/topics.conf"
}

# Split `--report-from <dir>` off an entry point's arguments; the rest are left
# in BENCH_ARGS.
bench_args() {
    BENCH_ARGS=()
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --report-from)
                [ "$#" -ge 2 ] || bench_refuse "--report-from needs a run directory"
                # shellcheck disable=SC2034 # read by the entry points that source this
                BENCH_REPORT_FROM="$2"
                shift 2
                ;;
            *)
                BENCH_ARGS+=("$1")
                shift
                ;;
        esac
    done
}

# The checkout the report will name: a clean tree whose HEAD is a commit on
# <remote>/main. A dirty tree or an unpushed commit is refused, because the
# build a report names has to be one anybody can check out.
bench_check_checkout() {
    local head
    head="$(git -C "$REPO_ROOT" rev-parse HEAD)"
    if [ -n "$(git -C "$REPO_ROOT" status --porcelain)" ]; then
        bench_refuse "the checkout at $head has uncommitted changes; a hardware report names a build anybody can check out, so commit or stash them first (git status)"
    fi
    if [ "${CHORUS_BENCH_NO_FETCH:-0}" != "1" ]; then
        git -C "$REPO_ROOT" fetch -q "$BENCH_REMOTE" main \
            || bench_refuse "could not fetch $BENCH_REMOTE main to check that $head is on it"
    fi
    if ! git -C "$REPO_ROOT" merge-base --is-ancestor "$head" "$BENCH_REMOTE/main" 2>/dev/null; then
        bench_refuse "HEAD $head is not a commit on $BENCH_REMOTE/main (an unpushed or branch commit); check out main and pull, then run again"
    fi
    printf '%s' "$head"
}

# Start the measure half: a run directory with its manifest. Called after the
# entry point's require_* guards, so a machine without the hardware still
# refuses by the prerequisite's name, and before anything is started, so a
# checkout that cannot be reported is refused before hours of measurement.
bench_begin() {
    local topic="$1" criterion="$2"
    local entry date build
    entry="$(bench_topic_column "$topic" 2)"
    [ -n "$entry" ] || bench_refuse "'$topic' is not a topic in tools/bench/topics.conf"
    build="$(bench_check_checkout)"
    date="${CHORUS_BENCH_DATE:-$(date -u +%F)}"
    printf '%s' "$date" | grep -qE '^[0-9]{4}-[0-9]{2}-[0-9]{2}$' \
        || bench_refuse "CHORUS_BENCH_DATE '$date' is not YYYY-MM-DD"
    BENCH_RUN_DIR="${CHORUS_BENCH_RUN_DIR:-$(mktemp -d "${CHORUS_MEASURE_OUT:-${TMPDIR:-/tmp}}/chorus-bench-$topic-XXXXXX")}"
    mkdir -p "$BENCH_RUN_DIR/raw" "$BENCH_RUN_DIR/analysis"
    {
        printf '# chorus bench run manifest, written by the measure half (tools/bench/lib.sh)\n'
        printf 'topic = %s\n' "$topic"
        printf 'entry_point = %s\n' "$entry"
        printf 'date = %s\n' "$date"
        printf 'build = %s\n' "$build"
        printf 'criterion = %s\n' "$criterion"
    } > "$BENCH_RUN_DIR/bench-run.conf"
    if [ -n "${CHORUS_BENCH_DEVICE_NOTE:-}" ]; then
        bench_device "$CHORUS_BENCH_DEVICE_NOTE"
    fi
    export BENCH_RUN_DIR
    bench_say "run directory $BENCH_RUN_DIR (topic $topic, build $build, $date)"
}

# Record a device the run used, by what it is (an ALSA name, a board model),
# never by a hostname, an address or a person's name.
bench_device() {
    printf 'device = %s\n' "$*" >> "$BENCH_RUN_DIR/bench-run.conf"
}

# Record the command that reproduces the run.
bench_reproduce() {
    printf 'reproduce = %s\n' "$*" >> "$BENCH_RUN_DIR/bench-run.conf"
}

# Start the report half from an existing run directory.
bench_load() {
    local dir="$1" topic="$2"
    [ -f "$dir/bench-run.conf" ] || bench_refuse "'$dir' has no bench-run.conf; it is not a run directory the measure half wrote"
    BENCH_RUN_DIR="$(cd "$dir" && pwd)"
    export BENCH_RUN_DIR
    local found
    found="$(bench_manifest topic)"
    [ "$found" = "$topic" ] || bench_refuse "'$dir' is a run of '$found', and this entry point reports '$topic'"
    mkdir -p "$BENCH_RUN_DIR/analysis"
}

# Add one key and value to the report's `## Fields` table.
bench_field() {
    BENCH_FIELDS+=("$1|${2:-none}")
}

# One more tracked file for the bench commit (the free-run baseline): the
# analysis wrote <source> in the run directory, and bench_finish copies it to
# <tracked path> once the checkout has been checked, with local paths scrubbed.
bench_extra_file() {
    BENCH_EXTRA_FILES+=("$1|$2")
}

# Run one analysis command with its output kept for the report. Returns the
# command's exit status and never exits: a failed analysis is a result.
bench_analysis() {
    local name="$1"
    shift
    local out="$BENCH_RUN_DIR/analysis/$name.txt" rc
    set +e
    "$@" > "$out" 2>&1
    rc=$?
    set -e
    BENCH_ANALYSES+=("$name|$rc")
    sed 's/^/    /' "$out"
    return "$rc"
}

# The run directory, the checkout and the home directory taken out, so no local
# directory layout lands in a committed file; and the second endpoint's ssh
# target (CHORUS_SECOND_ENDPOINT, `user@host`) and the server host the owner
# named (CHORUS_SERVER_HOST) replaced by what they are, so no address or
# hostname does either (K27). The report never prints them itself; this is for
# anything a tool echoed into the analysis output or a device note.
bench_scrub() {
    local stem="$1" endpoint="${CHORUS_SECOND_ENDPOINT:-}" server="${CHORUS_SERVER_HOST:-}"
    local args=(-e "s#$BENCH_RUN_DIR/raw/#docs/measurements/raw/$stem/#g"
        -e "s#$BENCH_RUN_DIR#<run>#g"
        -e "s#$REPO_ROOT/##g"
        -e "s#${HOME:-/nonexistent-home}#~#g")
    if [ -n "$endpoint" ]; then
        args+=(-e "s#$(bench_sed_escape "$endpoint")#<second endpoint>#g")
        local host="${endpoint#*@}"
        [ "$host" != "$endpoint" ] && [ "${#host}" -gt 3 ] \
            && args+=(-e "s#$(bench_sed_escape "$host")#<second endpoint>#g")
    fi
    [ -n "$server" ] && args+=(-e "s#$(bench_sed_escape "$server")#<server host>#g")
    sed "${args[@]}"
}

# A literal string made safe for the left side of a sed s### expression.
bench_sed_escape() {
    printf '%s' "$1" | sed -e 's/[]\/#.*^$[]/\\&/g'
}

# The report's file stem: <topic>-<date>, or with -2, -3 ... when that name
# is taken here or on the remote.
bench_stem() {
    local topic="$1" date="$2" n=1 stem branch
    while :; do
        stem="$topic-$date"
        branch="bench/$date-$topic"
        if [ "$n" -gt 1 ]; then
            stem="$stem-$n"
            branch="$branch-$n"
        fi
        if [ ! -e "$REPO_ROOT/docs/measurements/$stem.md" ] \
            && ! git -C "$REPO_ROOT" rev-parse -q --verify "refs/heads/$branch" > /dev/null \
            && { [ "${CHORUS_BENCH_PR:-0}" != "1" ] \
                || [ -z "$(git -C "$REPO_ROOT" ls-remote --heads "$BENCH_REMOTE" "$branch")" ]; }; then
            printf '%s %s' "$stem" "$branch"
            return
        fi
        n=$((n + 1))
    done
}

# shellcheck disable=SC2016 # the backticks are literal Markdown in the report
# Write the report and, when asked, publish it. <result> is PASS, FAIL,
# MEASURED (a characterization with no bound) or INCOMPLETE; <summary> is one
# line. Exits the entry point: 0 for PASS or MEASURED, 1 otherwise.
bench_finish() {
    local result="$1" summary="$2"
    case "$result" in PASS | FAIL | MEASURED | INCOMPLETE) ;; *) bench_refuse "result '$result' is not PASS, FAIL, MEASURED or INCOMPLETE" ;; esac
    local topic date build entry criterion head stem branch
    topic="$(bench_manifest topic)"
    date="$(bench_manifest date)"
    entry="$(bench_manifest entry_point)"
    criterion="$(bench_manifest criterion)"
    build="$(bench_manifest build)"
    head="$(bench_check_checkout)"
    if [ -n "$build" ] && [ "$build" != "$head" ]; then
        bench_refuse "the run was measured on $build and this checkout is at $head; report it from the checkout that ran it (git checkout $build)"
    fi
    read -r stem branch <<< "$(bench_stem "$topic" "$date")"
    local rel="docs/measurements/$stem.md"
    local report="$REPO_ROOT/$rel"
    local rawrel="docs/measurements/raw/$stem"

    # The raw files, in the order the run left them (sorted by name).
    local rawfiles=() f
    while IFS= read -r f; do rawfiles+=("$f"); done \
        < <(cd "$BENCH_RUN_DIR/raw" 2>/dev/null && find . -type f | sed 's#^\./##' | LC_ALL=C sort)
    [ "${#rawfiles[@]}" -gt 0 ] || bench_refuse "the run directory has no raw data under raw/; a report with nothing hashed is not evidence"

    if [ "${CHORUS_BENCH_ALLOW_FIXTURE_RAW:-0}" != "1" ]; then
        local fixture_hashes
        fixture_hashes="$(cd "$REPO_ROOT" && git ls-files -z fixtures | xargs -0 sha256sum | cut -d' ' -f1)"
        for f in "${rawfiles[@]}"; do
            if printf '%s\n' "$fixture_hashes" | grep -qx "$(sha256sum "$BENCH_RUN_DIR/raw/$f" | cut -d' ' -f1)"; then
                bench_refuse "raw file '$f' is byte-identical to a committed fixture; a fixture is not a hardware capture"
            fi
        done
    fi

    local devices=()
    while IFS= read -r f; do [ -n "$f" ] && devices+=("$f"); done < <(bench_manifest device --all)
    [ "${#devices[@]}" -gt 0 ] || devices=("not recorded by the measure half")
    local reproduce
    reproduce="$(bench_manifest reproduce)"
    [ -n "$reproduce" ] || reproduce="$entry --report-from <run directory>"

    mkdir -p "$REPO_ROOT/docs/measurements"
    local committed_total=0 kept size sha rows=()
    rm -rf "${REPO_ROOT:?}/$rawrel"
    for f in "${rawfiles[@]}"; do
        size="$(wc -c < "$BENCH_RUN_DIR/raw/$f" | tr -d ' ')"
        sha="$(sha256sum "$BENCH_RUN_DIR/raw/$f" | cut -d' ' -f1)"
        if [ "$size" -le "$BENCH_RAW_FILE_LIMIT" ] \
            && [ $((committed_total + size)) -le "$BENCH_RAW_TOTAL_LIMIT" ]; then
            kept=committed
            committed_total=$((committed_total + size))
            mkdir -p "$REPO_ROOT/$rawrel/$(dirname "$f")"
            cp "$BENCH_RUN_DIR/raw/$f" "$REPO_ROOT/$rawrel/$f"
        else
            kept=owner
        fi
        rows+=("| \`$rawrel/$f\` | $size | $sha | $kept |")
    done

    {
        printf '# Bench report: %s, %s\n\n' "$topic" "$date"
        printf 'Schema: %s\n' "$BENCH_SCHEMA"
        printf 'Source: hardware\n'
        printf 'Build measured: `%s`\n' "$head"
        printf 'Topic: %s\n' "$topic"
        printf 'Entry point: `%s`\n' "$entry"
        printf 'Date: %s\n' "$date"
        for f in "${devices[@]}"; do printf 'Device: %s\n' "$f"; done
        printf 'Result: %s: %s\n' "$result" "$summary"
        printf 'Criterion: %s\n' "$criterion"
        printf 'Reproduce with: `%s`\n\n' "$reproduce"
        printf 'Written by the bench script on the owner'"'"'s machine (tools/bench/lib.sh, K45).\n'
        printf 'Only a `hardware` report is timing evidence (BRIEF.md section 3.1 rule 3), and this one\n'
        printf 'is evidence for exactly the criterion above, graded as the result line says.\n\n'
        printf '## Fields\n\n| field | value |\n|---|---|\n'
        for f in "${BENCH_FIELDS[@]}"; do printf '| %s | %s |\n' "${f%%|*}" "${f#*|}"; done
        printf '\n## Raw data\n\n'
        printf 'Files of at most %s bytes are committed under `%s/`, up to %s bytes per\n' \
            "$BENCH_RAW_FILE_LIMIT" "$rawrel" "$BENCH_RAW_TOTAL_LIMIT"
        printf 'report; a larger file is hashed here and kept on the owner'"'"'s machine.\n\n'
        printf '| file | bytes | sha256 | kept |\n|---|---|---|---|\n'
        for f in "${rows[@]}"; do printf '%s\n' "$f"; done
        printf '\n## Analysis\n'
        for f in "${BENCH_ANALYSES[@]}"; do
            printf '\n### %s (exit %s)\n\n```text\n' "${f%%|*}" "${f#*|}"
            bench_scrub "$stem" < "$BENCH_RUN_DIR/analysis/${f%%|*}.txt" | tail -n 60
            printf '```\n'
        done
    } | bench_scrub "$stem" > "$report"

    local extra
    for extra in "${BENCH_EXTRA_FILES[@]}"; do
        bench_scrub "$stem" < "${extra%%|*}" > "$REPO_ROOT/${extra#*|}"
        bench_say "updated ${extra#*|}"
    done

    bench_say "wrote $rel"
    local validate_args=(--base-ref "$BENCH_REMOTE/main")
    [ "${CHORUS_BENCH_ALLOW_FIXTURE_RAW:-0}" = 1 ] && validate_args+=(--allow-fixture-raw)
    if ! bash "$BENCH_DIR/validate-report.sh" "$rel" "${validate_args[@]}"; then
        bench_refuse "the report does not validate against its schema (above); it is left at $rel for a look and nothing was published"
    fi

    if [ "${CHORUS_BENCH_PR:-0}" = "1" ]; then
        bench_publish "$topic" "$date" "$result" "$summary" "$rel" "$rawrel" "$branch"
    else
        bench_say "not published (set CHORUS_BENCH_PR=1 to commit on $branch, push and open the PR)"
    fi
    case "$result" in PASS | MEASURED) exit 0 ;; *) exit 1 ;; esac
}

# Commit on bench/<date>-<topic>, push, and open the PR.
# shellcheck disable=SC2016 # the backticks are literal Markdown in the PR body
bench_publish() {
    local topic="$1" date="$2" result="$3" summary="$4" rel="$5" rawrel="$6" branch="$7"
    local body back
    body="$(mktemp "${TMPDIR:-/tmp}/chorus-bench-pr-XXXXXX")"
    # Where the checkout was, to go back to once the branch is pushed: the next
    # bench script of a session (`make verify-device` runs four) starts from the
    # same clean commit on main, not from this bench commit.
    back="$(git -C "$REPO_ROOT" symbolic-ref -q --short HEAD || git -C "$REPO_ROOT" rev-parse HEAD)"
    git -C "$REPO_ROOT" switch -q -c "$branch"
    git -C "$REPO_ROOT" add -- "$rel"
    [ -d "$REPO_ROOT/$rawrel" ] && git -C "$REPO_ROOT" add -- "$rawrel"
    local extra
    for extra in "${BENCH_EXTRA_FILES[@]}"; do git -C "$REPO_ROOT" add -- "${extra#*|}"; done
    git -C "$REPO_ROOT" commit -q -m "bench: $topic $date, $result" \
        -m "Written by $(bench_manifest entry_point) on the owner's bench machine (K45)." \
        -m "Result: $result: $summary"
    git -C "$REPO_ROOT" push -q -u "$BENCH_REMOTE" "$branch"
    {
        printf '## Bench report: %s, %s\n\n' "$topic" "$date"
        printf -- '- Result: **%s**: %s\n' "$result" "$summary"
        printf -- '- Report: `%s`\n' "$rel"
        printf -- '- Build measured: `%s`\n' "$(bench_manifest build)"
        printf -- '- Schema: %s; validate with `bash tools/bench/validate-report.sh %s`\n\n' "$BENCH_SCHEMA" "$rel"
        sed -n '/^## Fields/,/^## Raw data/p' "$REPO_ROOT/$rel" | sed '$d'
        printf '\nOpened by the bench script from the owner'"'"'s machine. The next goal validates the\n'
        printf 'report against its schema and merges it, or comments why not (K45).\n'
    } > "$body"
    "$BENCH_GH" pr create --base main --head "$branch" \
        --title "bench: $topic $date ($result)" --body-file "$body"
    rm -f "$body"
    if git -C "$REPO_ROOT" symbolic-ref -q HEAD > /dev/null && [ "$back" != "$(git -C "$REPO_ROOT" rev-parse HEAD)" ] \
        && git -C "$REPO_ROOT" rev-parse -q --verify "refs/heads/$back" > /dev/null; then
        git -C "$REPO_ROOT" switch -q "$back"
    else
        git -C "$REPO_ROOT" switch -q --detach "$back"
    fi
    bench_say "pushed $branch and opened its PR; the checkout is back at $back"
}

# The figures of one `chorus-measure lag` run, from its kept output:
# "<median> <p95> <max> <used> <total>", or nothing when it resolved nothing.
bench_lag_figures() {
    sed -n 's/^chorus-measure: median \([-+0-9.]*\) us, p95 \([0-9.]*\) us, max \([0-9.]*\) us over \([0-9]*\) of \([0-9]*\) windows$/\1 \2 \3 \4 \5/p' "$1" | tail -n 1
}

# One key of a delay log's summary record, or "none".
bench_delaylog_value() {
    local log="$1" key="$2" v
    v="$(sed -n "s/^summary.* $key=\\([-0-9]*\\).*/\\1/p" "$log" 2>/dev/null | tail -n 1)"
    printf '%s' "${v:-none}"
}

# The median of the numbers on stdin, one per line (the absolute values).
bench_median_abs() {
    awk '{ print ($1 < 0 ? -$1 : $1) }' | LC_ALL=C sort -g | awk '{ a[NR] = $1 } END {
        if (NR == 0) { print "none"; exit }
        if (NR % 2) printf "%.3f\n", a[(NR + 1) / 2]; else printf "%.3f\n", (a[NR / 2] + a[NR / 2 + 1]) / 2
    }'
}

# Hard resyncs a delay log records after its first <seconds> seconds.
bench_hard_resyncs_after() {
    local log="$1" seconds="$2"
    awk -v limit="$((seconds * 1000000))" '
        /^event / && / kind=hard-resync / {
            for (i = 1; i <= NF; i++) if ($i ~ /^mono_us=/) { split($i, kv, "="); if (kv[2] + 0 > limit) n++ }
        }
        END { print n + 0 }' "$log"
}

# Lag every raw capture-*.wav of the run: one analysis per capture, a field per
# capture, and in BENCH_LAG_MEDIANS / BENCH_LAG_P95S / BENCH_LAG_RESOLVED /
# BENCH_LAG_TOTAL what the entry point grades.
bench_lag_captures() {
    local label="$1" wav n figures
    BENCH_LAG_MEDIANS=""
    BENCH_LAG_P95S=""
    BENCH_LAG_RESOLVED=0
    BENCH_LAG_TOTAL=0
    for wav in "$BENCH_RUN_DIR"/raw/capture-*.wav; do
        [ -f "$wav" ] || continue
        n="$(basename "$wav" .wav)"
        n="${n#capture-}"
        BENCH_LAG_TOTAL=$((BENCH_LAG_TOTAL + 1))
        if bench_analysis "lag-capture-$n" "$BIN_DIR/chorus-measure" lag "$wav" \
            --label "$label-$n" --source hardware --out "$BENCH_RUN_DIR/analysis" \
            --baseline "$REPO_ROOT/docs/measurements/free-run-baseline.conf"; then
            read -r -a figures <<< "$(bench_lag_figures "$BENCH_RUN_DIR/analysis/lag-capture-$n.txt")"
            BENCH_LAG_RESOLVED=$((BENCH_LAG_RESOLVED + 1))
            BENCH_LAG_MEDIANS="$BENCH_LAG_MEDIANS${figures[0]}"$'\n'
            BENCH_LAG_P95S="$BENCH_LAG_P95S${figures[1]}"$'\n'
            bench_field "capture_${n}_lag_us" "median ${figures[0]}, p95 ${figures[1]}, max ${figures[2]}"
        else
            bench_field "capture_${n}_lag_us" "did not resolve"
        fi
    done
}
