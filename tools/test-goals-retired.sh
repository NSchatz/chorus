#!/usr/bin/env bash
# test_agent_files_name_no_retired_tracker: CLAUDE.md and CARD.md name neither the goals program
# nor the harness as a live task tracker. Both are retired (#262); how a spec reaches chorus now
# depends on the development setup, which CARD.md says is not decided yet. In `make verify`.
#
#   bash tools/test-goals-retired.sh
set -euo pipefail
cd "$(dirname "$0")/.."
pat="goals (task|item|project) (add|list|show|edit|done|recur)|goals serve|goals'? nightly|\.local/bin/goals|/goals:|goals'? app|goals' (Approve|weekly|start)|harness: goals|harness (plan|task) (add|list|show|done)|NSchatz/harness|in the harness"
if grep -nE "$pat" CLAUDE.md CARD.md; then
  echo "test_agent_files_name_no_retired_tracker: retired goals or harness tracker text found" >&2
  exit 1
fi
grep -q '^## Hand it work' CARD.md || {
  echo "test_agent_files_name_no_retired_tracker: CARD.md lacks its '## Hand it work' section" >&2
  exit 1
}
echo "test_agent_files_name_no_retired_tracker: ok"
