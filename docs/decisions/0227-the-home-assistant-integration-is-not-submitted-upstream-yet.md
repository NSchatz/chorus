# 0227: the Home Assistant integration is not submitted to Home Assistant core yet; it stays a custom integration, kept as the draft

- Status: decided by the owner, 2026-10-06 (harness task 26, goals issue #490)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `docs/home-assistant.md` ("The upstream draft: what changes for Home Assistant
  core")

## Context

The chorus integration is installed as a custom integration and kept in the form Home
Assistant core asks for (0138, 0164, K42, K61). Submitting it to core is the owner's choice and
the owner's action: no task of this repository opens a pull request, an issue or a comment
outside the owner's repositories (K42). `docs/home-assistant.md` lists what a submission takes:
the changes `tools/ha-hassfest.sh` already makes and grades, plus the client library `_aiochorus`
published on PyPI from a public repository with public CI, the tests moved onto core's fixtures,
the README turned into a home-assistant.io page, and core's generated files regenerated.

## Decision

Asked whether to submit now, not yet, or never, the owner answered: not yet.

1. Nothing is submitted, and no core-form tree or submission files are prepared.
2. The custom integration stays the installed form (0164), and the integration is still kept
   as the draft: `make ha-hassfest` keeps grading it as core in every gate run.
3. The question is reopened only by the owner telling a chorus session to submit it (the
   session then prepares the core, brands and documentation submissions as files for the owner
   to open) or not to submit it ever.

## Not chosen

- **Submit now.** It needs a public repository and public CI for `_aiochorus` and the owner's
  time on three upstream reviews; the owner chose to wait.
- **Never.** It would drop the draft constraint on the integration; the owner kept the option
  open.

## What was read

- `docs/home-assistant.md`, "The upstream draft: what changes for Home Assistant core", at
  `origin/main` 004d1f0, read 2026-10-06.
- `docs/decisions/0164-the-home-assistant-integration-is-installed-from-a-pinned-export.md`,
  read 2026-10-06.
