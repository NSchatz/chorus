# 0001: Rust for the server and the Linux client

- Status: decided
- Decision owner: the owner (Noah), ruling recorded 2026-08-22
- BRIEF.md section 12, decision 1
- Recorded by: FOUNDATION-1 (spec S0001-chorus-foundation-1)

## Decision

The primary implementation language for the chorus server and the Linux client
is **Rust**, with plain threads rather than an async runtime.

This file records a decision that was already made. It does not reopen it.

## Reasoning

1. The server's job is soft-real-time. BRIEF.md 5.1 states the case directly: a
   garbage collector on the hot path is "a failure mode the project exists to
   avoid", and the measured comparisons cited there show GC languages missing
   modest tail-latency targets under memory pressure where Rust stays flat. The
   whole point of the project is holding sub-millisecond playout sync, so the
   language must not introduce a pause the servo then has to chase.
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
