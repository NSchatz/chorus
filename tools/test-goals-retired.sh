#!/usr/bin/env bash
# test_docs_name_no_live_goals: CLAUDE.md and CARD.md name the harness, not goals.
set -euo pipefail
cd "$(dirname "$0")/.."
pat="goals (task|item|project) (add|list|show|edit|done|recur)|goals serve|goals'? nightly|\.local/bin/goals|/goals:|goals'? app|goals' (Approve|weekly|start)|harness: goals"
if grep -nE "$pat" CLAUDE.md CARD.md; then
  echo "test_docs_name_no_live_goals: live goals text found" >&2
  exit 1
fi
grep -q 'harness plan add chorus' CARD.md || { echo "CARD.md lacks harness plan add chorus" >&2; exit 1; }
echo "test_docs_name_no_live_goals: ok"
