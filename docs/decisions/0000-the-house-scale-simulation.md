# 0000: the house-scale simulation is many single-client runs against one server timeline, configured under config/sim-house

- Status: accepted (goal 7, 2026-09-30)
- Decided by: the goal (item 1, K75 sizes it; K91 fixes the Wi-Fi speakers' place); cheap to
  reverse, so made and recorded here
- Implemented in: `crates/sync/src/house.rs`, `crates/sync/src/house_report.rs`,
  `crates/sync/src/bin/chorus-sim-house.rs`, `config/sim-house/*.house`; held by
  `crates/sync/tests/house_regression.rs`; report `docs/measurements/sim-house-8-rooms.md`

## Context

BRIEF.md section 2.2 is written in inter-device error: two rooms under 5 ms, a stereo pair or
a room under 0.5 ms. The simulator (ADR 0006) modelled one client against one server, which
says how far one endpoint's playout is from the server timeline and nothing about two endpoints
at once. K75 sizes everything to 8 rooms, about 20 speakers.

## Decision

1. **A house is N independent single-client runs against one server timeline.** The server
   only timestamps, so in the model the endpoints do not interact: each is one
   `chorus_sync::run_recorded` with its own crystal, path, jitter stream, wander stream and servo,
   all sharing the server's crystal and the true-time grid. Inter-device error for a pair at a
   true time is the difference of their playout errors at that time. Nothing new is invented
   below the house layer, so everything the C mirror holds for one endpoint holds for each.
2. **Pairs are classed and graded against BRIEF.md section 2.2**: same room (stereo pair,
   theater set; "Stereo pair / same room", 0.5 ms and 0.2 ms) and cross room (wired/wired,
   wired/Wi-Fi, Wi-Fi/Wi-Fi; "Multiroom music, different rooms", 5 ms and 1 ms). The Wi-Fi
   rows use the multiroom row because the wireless tier's bound is that row (ADR 0024,
   `wireless_bound_us = 5000`). A class passes when its max is under the acceptable bound.
3. **Settling is a fixed 60 s**, the acquisition window the real rig already excludes
   (`sync_hour_settle_seconds`, `config/sync.conf`), and the regression asserts no endpoint
   steps after it. A fixed window rather than a per-endpoint settle index, so every pair is
   compared over the same true times.
4. **The house runs the real client's servo** (SYNC-4's constants, `config/sync.conf`, asserted
   equal), not FOUNDATION-1's reference servo, because the question is what the house would do.
5. **House files live in `config/sim-house/`, not `fixtures/sync/`.** Every file under
   `fixtures/sync/` must be read by a Rust test and by the C test
   (`tools/conventions/check-shared-fixtures.sh`), and the endpoint's C core models one endpoint,
   not a house: a C house reader would be code with no firmware use, written only to satisfy the
   rule. The rule's purpose (the two implementations cannot drift) is already met below the
   house: the single-endpoint arithmetic, including the asymmetry, burst and wander models the
   house uses, is held exchange by exchange through `fixtures/sync/`. The house regression is
   Rust-only and says so.
6. **Two houses, switched and routed, identical but for their links** (asserted), so the P3
   comparison (option b' with a server leg versus option b through the firewall) changes one
   thing. Every link parameter is ASSUMED and marked in the file.
7. **The report is generated**, parameters included, by `chorus-sim-house` from the files that
   ran, with `Source: simulation` and the build it ran from. The room layout is ASSUMED: the
   owner's room list is an open Needs item.

## Consequences

- `crates/sync/tests/house_regression.rs` pins that both houses stay inside every acceptable
  bound. That is a property of the model under ASSUMED parameters, never a timing claim
  (BRIEF.md section 3.1 rule 3); the rig replaces it.
- Not modelled: correlation between endpoints' network delays (a shared congested trunk would
  move them together, which would help inter-device error and hurt absolute error), DAC and
  acoustic delay, the client's round-trip admission and staleness rules, loss and reordering.

## Revisit when

The owner's room list arrives (replace the ASSUMED layout), WIFI-7 characterizes a real Wi-Fi
link (replace the burst parameters), or the routed-versus-L2 measurement of P3 exists (replace
the router hop's).
