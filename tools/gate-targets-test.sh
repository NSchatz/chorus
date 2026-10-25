#!/usr/bin/env bash
# The standard gate targets (Makefile gate-quick and gate-full) and the workflows that call them.
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

test_gate_quick_keeps_ci_base
test_gate_full_is_gate
test_workflows_call_standard_targets

if [ "$fails" -ne 0 ]; then
    echo "gate targets: $fails failed" >&2
    exit 1
fi
