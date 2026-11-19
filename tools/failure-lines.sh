#!/usr/bin/env bash
# The lines of a failed gate step's log that name what failed, for the gate's own output: nextest's
# FAIL, TIMEOUT and signal lines and its Summary, a test's panic, a MISSING PREREQUISITE, rustc's
# errors and a failed `cargo test` result. Each line once, in log order, at most 38, then nextest's
# Summary whatever the count. A log with none prints nothing, and tools/gate.sh then shows its tail
# alone.
#
#   bash tools/failure-lines.sh LOG
set -euo pipefail
log="${1:?usage: tools/failure-lines.sh LOG}"
lines="$(grep -E '^\s*(FAIL|TIMEOUT|SIGSEGV|SIGABRT|SIGKILL|LEAK-FAIL) \[|^\s*Summary \[|panicked at |MISSING PREREQUISITE|^error(\[E[0-9]+\])?: |^test result: FAILED' "$log" |
    awk '!seen[$0]++' || true)"
[ -n "$lines" ] || exit 0
grep -v '^\s*Summary \[' <<< "$lines" | head -n 38 || true
grep '^\s*Summary \[' <<< "$lines" || true
