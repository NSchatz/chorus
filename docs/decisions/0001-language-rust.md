# 0001: Rust for the server and the Linux client

- Status: decided
- Decision owner: the owner (Noah), ruling recorded 2026-08-22
- BRIEF.md section 12, decision 1
- Recorded by: FOUNDATION-1 (spec S0001-chorus-foundation-1)
- Evidence commit: `7e50d3fd0295c119efc122608b56460906354015` in this
  repository, the squash merge of FOUNDATION-1. Everything this entry rests on
  is present there: the `crates/protocol` and `crates/sync` libraries under
  the virtual `Cargo.toml` workspace, both with an empty `[dependencies]`, and
  the golden vectors under `fixtures/protocol/`. Check any of them with
  `git show 7e50d3f:<path>`; the tree is `git ls-tree -r --name-only 7e50d3f`.

## Decision

The primary implementation language for the chorus server and the Linux client
is **Rust**, with plain threads rather than an async runtime.

This file records a decision that was already made. It does not reopen it.

## Reasoning

1. The server's job is soft-real-time. BRIEF.md 5.1 states the case directly: a
   garbage collector on the hot path is "a failure mode the project exists to
   avoid". 5.1 goes on to assert that "measured comparisons show GC languages
   missing modest tail-latency targets under memory pressure where Rust stays
   flat", and that assertion carries no source: it names no study, dataset,
   link or number, and BRIEF.md section 11 lists no such reference, so there is
   nothing there for a reader to go and consult. Treat it as the brief's
   position and not as evidence. The decision does not rest on it either way,
   because the load-bearing part is the failure mode named just above it: the
   whole point of the project is holding sub-millisecond playout sync, so the
   language must not introduce a pause the servo then has to chase. Under
   BRIEF.md guardrail 3, the first tail-latency number this project actually
   stands behind will be one its own harness measured.
2. The protocol is implemented twice. CLAUDE.md working agreement item 5 says
   the protocol, sync and DSP cores are pure libraries with shared fixtures "so
   the Rust and C implementations cannot drift apart". FOUNDATION-1 commits the
   golden vectors that the later ESP32-S3 C mirror (roadmap phase EMBEDDED-5)
   will be held to byte for byte. That story assumes the primary implementation
   is the Rust one.
3. Rust has no runtime to keep off the audio path, so the same reasoning that
   picks it for the server picks it for the Linux client, which runs the same
   sync logic against ALSA.
4. BRIEF.md 5.1 names Go only as a fallback "if Rust velocity becomes a
   problem". No phase has reported one, so that conditional never fired. Go is
   not refused on principle; it is simply not what the merits selected here.

## What was explicitly not a factor

Which toolchain happened to be installed on the machine that wrote the code.
An absent toolchain is an install away and is never a reason to pick a
language. An earlier attempt at this phase reached for what was already on
`PATH`; that attempt was discarded and the ruling above replaced it.

## Consequences

- Cargo workspace, stable toolchain, `std` only (see 0002).
- Plain threads when concurrency arrives; an async runtime is not adopted by
  this decision and would need its own entry.
- The C mirror in EMBEDDED-5 is validated against the fixtures this phase
  commits, not against a second reading of the prose.

## Revisit when

Rust velocity becomes a measured problem for the server or the Linux client,
which is the only trigger BRIEF.md 5.1 attaches to the Go fallback.
