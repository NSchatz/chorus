Saved harness reports (BRIEF.md section 10). A timing claim with no report here is not evidence.

Every report carries two lines, held by `tools/conventions/check-measurements.sh`
(`docs/conventions.md` rule 11):

- `Source: hardware|host|simulation|synthetic`: `hardware` is a run on the real devices and the
  real rig; `host` is a real run on a development host (no device); `simulation` is the sync
  simulator; `synthetic` is generated fixture data run through the harness. Only `hardware` is
  timing evidence (BRIEF.md section 3.1 rule 3).
- `Build measured: <40-hex sha>`: a commit in this history (a squash-merged change cites the
  merge commit, not a branch commit).

As of 2026-09-30 no report here is `hardware`: four are `synthetic` and two are `host`. The build
commits the reports written before the rule first named no longer exist; each report says which
commit replaced it and why (K48, audit A-6).
