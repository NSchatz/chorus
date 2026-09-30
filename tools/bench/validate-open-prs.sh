#!/usr/bin/env bash
# Validate every open bench/* PR (K45): what a goal runs at its start (brief
# section 0.1, step 4) before merging them.
#
#   bash tools/bench/validate-open-prs.sh
#
# For each open PR whose head is bench/*: fetch the head, check that the PR
# touches only its own report (docs/measurements/<topic>-<date>[-N].md), that
# report's raw directory, and for the free-run topic the baseline
# (docs/measurements/free-run-baseline.conf); then run
# tools/bench/validate-report.sh on the report in a temporary worktree of the
# head. Prints one line per PR, `bench-pr <n> <branch>: valid` or the failures,
# and exits non-zero when any PR is invalid. Merging stays the goal's call.
#
# Environment (for tests): CHORUS_BENCH_GH (default gh), CHORUS_BENCH_REMOTE
# (default origin), CHORUS_BENCH_ALLOW_FIXTURE_RAW=1 (passes the validator's
# test-only --allow-fixture-raw; never set it when validating a real PR).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GH="${CHORUS_BENCH_GH:-gh}"
REMOTE="${CHORUS_BENCH_REMOTE:-origin}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/chorus-bench-prs-XXXXXX")"
trap 'git -C "$ROOT" worktree prune; rm -rf "$WORK"' EXIT

git -C "$ROOT" fetch -q "$REMOTE" main
PRS="$("$GH" pr list --state open --search 'head:bench/' --json number,headRefName \
    --jq '.[] | "\(.number) \(.headRefName)"')"
if [ -z "$PRS" ]; then
    echo "bench-prs: no open bench/* PR"
    exit 0
fi

BAD=0
while read -r number branch; do
    [ -n "$number" ] || continue
    case "$branch" in bench/*) ;; *) continue ;; esac
    git -C "$ROOT" fetch -q "$REMOTE" "+refs/heads/$branch:refs/bench-validate/$number"
    head="refs/bench-validate/$number"
    base="$(git -C "$ROOT" merge-base "$REMOTE/main" "$head")"
    reports=()
    fails=()
    while IFS= read -r path; do
        case "$path" in
            docs/measurements/raw/*) ;;
            docs/measurements/free-run-baseline.conf) ;;
            docs/measurements/*.md) reports+=("$path") ;;
            *) fails+=("touches $path, outside its report and raw data") ;;
        esac
    done < <(git -C "$ROOT" diff --name-only "$base" "$head")
    [ "${#reports[@]}" = 1 ] || fails+=("carries ${#reports[@]} reports; a bench PR carries exactly one")
    tree="$WORK/pr-$number"
    git -C "$ROOT" worktree add -q --detach "$tree" "$head"
    for report in "${reports[@]}"; do
        stem="$(basename "$report" .md)"
        while IFS= read -r path; do
            case "$path" in
                "$report" | "docs/measurements/raw/$stem/"*) ;;
                docs/measurements/free-run-baseline.conf)
                    case "$stem" in rig3-free-run-*) ;; *) fails+=("changes the free-run baseline from a $stem report") ;; esac ;;
                docs/measurements/raw/*) fails+=("touches $path, another report's raw data") ;;
            esac
        done < <(git -C "$ROOT" diff --name-only "$base" "$head")
        # This checkout's validator against the PR's files: a PR cannot bring
        # its own rules.
        args=(--root "$tree" --base-ref "$REMOTE/main")
        [ "${CHORUS_BENCH_ALLOW_FIXTURE_RAW:-0}" = 1 ] && args+=(--allow-fixture-raw)
        if ! out="$(bash "$ROOT/tools/bench/validate-report.sh" "$report" "${args[@]}" 2>&1)"; then
            fails+=("$(printf '%s' "$out" | tr '\n' ';')")
        fi
    done
    git -C "$ROOT" worktree remove --force "$tree"
    git -C "$ROOT" update-ref -d "$head"
    if [ "${#fails[@]}" -eq 0 ]; then
        echo "bench-pr $number $branch: valid (${reports[0]})"
    else
        BAD=$((BAD + 1))
        echo "bench-pr $number $branch: INVALID"
        printf '  %s\n' "${fails[@]}"
    fi
done <<< "$PRS"
[ "$BAD" -eq 0 ]
