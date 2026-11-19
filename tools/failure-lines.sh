#!/usr/bin/env bash
# The lines of a failed gate step's log that name what failed, for the gate's own output: nextest's
# FAIL, TIMEOUT and signal lines and its Summary, a test's panic, a MISSING PREREQUISITE, rustc's
# errors and a failed `cargo test` result. Each line once, in log order, at most 40. A log with
# none prints nothing, and tools/gate.sh then shows its tail alone.
#
#   bash tools/failure-lines.sh LOG
set -euo pipefail
log="${1:?usage: tools/failure-lines.sh LOG}"
grep -E '^\s*(FAIL|TIMEOUT|SIGSEGV|SIGABRT|SIGKILL|LEAK-FAIL) \[|^\s*Summary \[|panicked at |MISSING PREREQUISITE|^error(\[E[0-9]+\])?: |^test result: FAILED' "$log" |
    awk '!seen[$0]++' | head -n 40 || true
