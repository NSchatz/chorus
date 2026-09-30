# Flash-guard fixtures

Read by `tools/conventions/check-flash-guard-fixtures.sh`. Every file under `forbidden/` holds
one way to set, export or bypass the owner-at-bench variable and must make
`check-flash-guard.sh` fail when it is the only file scanned; every file under `allowed/`
holds approved read forms (or a setting under `docs/`) and must pass. The file path under each
directory is the path the scanner sees, so `forbidden/.claude/settings.json` is scanned as
`.claude/settings.json`.

The first twelve forbidden forms are the ones the planning review of the brief found
(`.claude/goals/2026-09-chorus-research/review-brief-v1.md`, H2); the rest are further ways
to set it in the languages and files this repository uses.
