#!/usr/bin/env bash
# The standard gate targets (Makefile gate-quick and gate-full) and the workflows that call them,
# the per-test limit of the web tests' run, the failure lines a red gate step prints, and the
# default goal (help, not a build).
# chorus has no host gate, so the targets are aliases: this reads what make would run (`make -n`)
# and what the workflows' run steps call, and builds and runs nothing.
#
#   bash tools/gate-targets-test.sh
set -euo pipefail

cd "$(dirname "$0")/.."

fails=0
pass() { echo "ok   $1"; }
fail() {
    echo "FAIL $1: $2" >&2
    fails=$((fails + 1))
}

# 1. A CHORUS_GATE_BASE already in the environment (ci.yml exports it) wins over the merge base.
test_gate_quick_keeps_ci_base() {
    local out base
    out="$(CHORUS_GATE_BASE=abc123 make -s -n gate-quick)"
    if ! grep -q 'bash tools/gate.sh changed' <<< "$out"; then
        fail "${FUNCNAME[0]}" "make -n gate-quick does not run tools/gate.sh changed: $out"
        return
    fi
    # The recipe line as make would run it, with tools/gate.sh changed swapped for a printenv.
    base="$(CHORUS_GATE_BASE=abc123 bash -c "$(grep 'tools/gate.sh changed' <<< "$out" |
        sed 's|bash tools/gate.sh changed|printenv CHORUS_GATE_BASE|')")"
    if [ "$base" = abc123 ]; then
        pass "${FUNCNAME[0]}"
    else
        fail "${FUNCNAME[0]}" "tools/gate.sh changed would run with CHORUS_GATE_BASE='$base', not abc123"
    fi
}

# 2. gate-full is the full gate.
test_gate_full_is_gate() {
    local out
    out="$(make -s -n gate-full)"
    if grep -qx 'bash tools/gate.sh full' <<< "$out"; then
        pass "${FUNCNAME[0]}"
    else
        fail "${FUNCNAME[0]}" "make -n gate-full does not run 'bash tools/gate.sh full': $out"
    fi
}

# 3. The workflows call the standard names, and neither the old ones.
test_workflows_call_standard_targets() {
    local ci=.github/workflows/ci.yml nightly=.github/workflows/nightly.yml runs
    runs="$(grep -hE '^[[:space:]]*(- )?run:' "$ci" "$nightly")"
    if ! grep -qE '^[[:space:]]*(- )?run: make gate-quick$' "$ci"; then
        fail "${FUNCNAME[0]}" "$ci does not run make gate-quick"
    elif ! grep -qE '^[[:space:]]*(- )?run: make gate-full$' "$nightly"; then
        fail "${FUNCNAME[0]}" "$nightly does not run make gate-full"
    elif grep -qE 'make gate-changed|make gate([[:space:]]|$)' <<< "$runs"; then
        fail "${FUNCNAME[0]}" "a run step still calls make gate-changed or a bare make gate"
    else
        pass "${FUNCNAME[0]}"
    fi
}

# 4. The web tests' run (tools/web.sh) holds each test to a minute.
test_web_sh_sets_60_second_limit() {
    local line
    line="$(grep -E '^[[:space:]]*node .*--test( |$)' tools/web.sh)"
    if [ -z "$line" ]; then
        fail "${FUNCNAME[0]}" "tools/web.sh has no node --test line"
    elif grep -q -- '--test-timeout=60000 ' <<< "$line"; then
        pass "${FUNCNAME[0]}"
    else
        fail "${FUNCNAME[0]}" "tools/web.sh's node --test line has no --test-timeout=60000: $line"
    fi
}

# 5. A failed step names what failed (tools/failure-lines.sh): each nextest FAIL line once, its
# Summary and a panic, and nothing from a log with no failure in it.
test_failure_lines_name_the_failing_tests() {
    local log out
    log="$(mktemp)"
    printf '%s\n' \
        '        PASS [   0.010s] (1/3) crate::t a_passing_test' \
        '        FAIL [   0.100s] (2/3) crate::t a_failing_test' \
        "    thread 'a_failing_test' (1) panicked at crates/x/tests/t.rs:1:1:" \
        '     Summary [   1.000s] 3 tests run: 2 passed, 1 failed, 0 skipped' \
        '        FAIL [   0.100s] (2/3) crate::t a_failing_test' \
        'test result: ok. 1 passed; 0 failed' > "$log"
    out="$(bash tools/failure-lines.sh "$log")"
    if [ "$(grep -c 'FAIL \[' <<< "$out")" != 1 ] || ! grep -q 'Summary \[' <<< "$out" ||
        ! grep -q 'panicked at' <<< "$out" || grep -q 'PASS\|test result: ok' <<< "$out"; then
        fail "${FUNCNAME[0]}" "tools/failure-lines.sh printed: $out"
    elif [ -n "$(printf 'all good\n' > "$log" && bash tools/failure-lines.sh "$log")" ]; then
        fail "${FUNCNAME[0]}" "tools/failure-lines.sh printed lines from a log with no failure"
    else
        pass "${FUNCNAME[0]}"
    fi
    rm -f "$log"
}

# 6. A bare `make` prints help and runs no cargo command (the default goal was once `check`, the
# whole workspace's build and tests).
test_bare_make_builds_nothing() {
    local out
    out="$(make -s -n)"
    if grep -qE '^(env [^ ]+ )?cargo ' <<< "$out"; then
        fail "${FUNCNAME[0]}" "make -n with no target would run cargo: $out"
    elif ! grep -q 'CI is the gate' <<< "$(make -s)"; then
        fail "${FUNCNAME[0]}" "a bare make does not print the help"
    else
        pass "${FUNCNAME[0]}"
    fi
}

test_gate_quick_keeps_ci_base
test_gate_full_is_gate
test_workflows_call_standard_targets
test_web_sh_sets_60_second_limit
test_failure_lines_name_the_failing_tests
test_bare_make_builds_nothing

if [ "$fails" -ne 0 ]; then
    echo "gate targets: $fails failed" >&2
    exit 1
fi
