# 0002: repository layout and CI shape

- Status: decided
- BRIEF.md section 12, decision 2
- Recorded by: FOUNDATION-1 (spec S0001-chorus-foundation-1)

## Decision

Follow BRIEF.md section 7's suggested shape, scoped to what exists now.

```
chorus/
  Cargo.toml               # virtual workspace manifest
  crates/protocol/         # wire format: framing, catalog, encoder, decoder
  crates/sync/             # virtual clocks, jitter models, servo, simulator
  fixtures/protocol/       # golden vectors and their canonical inputs
  fixtures/sync/           # committed simulator scenarios
  docs/decisions/          # this log
  docs/measurements/       # harness reports (empty until RIG-3)
  .github/workflows/ci.yml # the one CI workflow
```

Directories BRIEF.md section 7 lists that this phase does **not** create:
`crates/dsp`, `crates/server`, `crates/client-linux`, `firmware/esp32s3/`,
`tools/measure/`, `deploy/`. Each arrives with the phase that needs it, so the
tree never carries an empty promise.

## Reasoning

- Section 7's properties are the load-bearing part: the sync, protocol and DSP
  cores are pure libraries with no I/O, and the fixtures sit beside them so the
  firmware mirror can be validated against the same files. A Cargo workspace
  gives exactly that: `crates/protocol` and `crates/sync` are libraries with no
  socket, no clock read, and no audio device between them.
- Fixtures live at the repository root rather than inside a crate because a
  second-language implementation has to read them without a Cargo project.
- BRIEF.md names no CI system. GitHub Actions is where this repository already
  lives and where its umbrella reads check results from, so one workflow file
  under `.github/workflows/` is the shape with the fewest moving parts.

## CI shape

One workflow, `ci`, on every push and every pull request. It builds the
workspace and then runs the two regressions this phase exists to protect as
separately named steps, so a red build says which one broke:

- "Golden-vector round trip" runs the `chorus-protocol` test targets.
- "Simulator regression" runs the `chorus-sync` test targets.
- A final workspace-wide `cargo test` catches anything outside those two.

Any failing step fails the build. There is no `continue-on-error` anywhere in
the workflow.

`cargo fmt --check` and `cargo clippy` are deliberately **not** gates yet: no
formatting or lint baseline has been agreed for this repository, and a gate
nobody has agreed on only teaches people to ignore red. They arrive with the
first entry that agrees the baseline.

## Dependencies and the lockfile

Both crates are `std` only, with zero external dependencies. This is not an
accident of this phase: BRIEF.md 3.2 says fewer dependencies is a feature, and
every candidate here (a PRNG, a hex parser, a key-value parser) is small and
instructive, which is exactly the case where that section says build rather
than vendor. The seeded PRNG in particular has to be reproducible byte for byte
across a Rust host and a C mirror, so owning it is the point.

`Cargo.lock` is therefore not committed and CI does not pass `--locked`: with
no dependency graph the lock records nothing. Both decisions flip the day the
first external dependency lands, which is itself a decision-log entry (a new
dependency is not a quiet change).

## Consequences

- CI needs no network beyond checking the repository out, so it stays fast and
  cannot go red because a registry was slow.
- Adding a crate means adding a workspace member; nothing else moves.

## Revisit when

The first external dependency is proposed, or the first phase needs `server`,
`client-linux`, `dsp`, `firmware` or `deploy` to exist.
