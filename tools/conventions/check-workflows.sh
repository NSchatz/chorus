#!/usr/bin/env bash
# Rule "Workflows and YAML": the GitHub workflows are clean under the pinned actionlint, and
# every tracked YAML file under yamllint -s with the repository's .yamllint.
. "$(dirname "$0")/lib.sh"
need actionlint || exit 1
need yamllint || exit 1
rc=0
mapfile -t wf < <(git ls-files '.github/workflows/*.yml' '.github/workflows/*.yaml')
actionlint "${wf[@]}" || { fail "Workflows and YAML" "actionlint found problems in the workflows"; rc=1; }
mapfile -t yml < <(git ls-files '*.yml' '*.yaml')
yamllint -s "${yml[@]}" || { fail "Workflows and YAML" "yamllint -s found problems"; rc=1; }
[ "$rc" = 0 ] && echo "actionlint $(actionlint --version | head -n 1): ${#wf[@]} workflow(s) clean; $(yamllint --version): ${#yml[@]} YAML file(s) clean"
exit "$rc"
