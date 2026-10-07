# P13: The rack amp's zones and channels

- Decisions: K70, K72, K74, K96
- Status: ACCEPTED (2026-10-07, the owner): as written, zero zones and zero channels; no rack amp
  and no soundbar, the LCR set is the theater front for both TV rooms (first put forward
  2026-10-06; the owner dropped the rack amp on 2026-10-04)
- If deferred: No rack amp is designed and nothing else changes; the two TV rooms keep the LCR
  set and the hidden TV hub (moot since 2026-10-07)
- Builds on: goal 26 (§30 item 1, "P13 (K74): zones and channels for the 2U rack amp from the
  room list and wiring, marked PROPOSED"; item 3, the theater front's two variants), I11 ("P11-P13
  are written in the goals that build them"), the owner's house plan of 2026-10-04 (P4, "The house
  plan"; P12, "The owner's room list")

## Question

How many zones, and how many amplifier channels, does the 2U rack amp (K74) drive? K74 left the
count to "research and propose" from the room list, the speaker-wire runs back to the rack and the
power budget. Goal 26 also asks for the theater front in two variants (K72): a soundbar and an
LCR set. This proposal answers both from the room list.

## The room input

The owner's room list, answered on 2026-10-04 and recorded in two places:

- `docs/proposals/P4-bench-purchase.md`, "Decision: the owner's final list": "The rack amp (K70,
  K74) is dropped: no room in the plan wires speakers back to the rack", and "The house plan":
  six rooms and 19 speakers.
- `docs/proposals/P12-enclosures.md`, "The owner's room list (answered 2026-10-04)": two 5.1
  rooms (two-way L, C and R and a subwoofer on mains, compact surrounds on PoE+), a kitchen
  compact pair, one bathroom compact, a garage compact pair, and an outdoor pair on a deck. "The
  rack amp is dropped."

| Room | Speakers | Power | Wired back to the rack? |
|---|---|---|---|
| Living room (5.1, TV) | two-way L, C, R; a subwoofer; two compact surrounds | mains; PoE+ | no |
| Master bedroom (5.1, TV) | two-way L, C, R; a subwoofer; two compact surrounds | mains; PoE+ | no |
| Kitchen | a compact pair | PoE+ | no |
| Garage | a compact pair | PoE+ | no |
| Primary bath | one compact | PoE+ | no |
| Deck (outdoor) | an outdoor pair | PoE+ | no |

Every speaker is a powered chorus speaker with its own amplifier on Ethernet (K90; P4 "every
speaker gets Ethernet"). None is a passive in-wall, in-ceiling or outdoor speaker on a speaker
wire run to the rack, which is the only kind K74 drives.

**ASSUMED inputs: none.** The room list and its wiring are the owner's answer of 2026-10-04. The
rack's free units and depth (the other Needs item K74 named) are no longer needed.

## Zones and channels

- **Zones: 0.** A rack amp zone is a room whose passive speakers are wired back to the rack; the
  list has none.
- **Channels: 0.** With no zone there is no amplifier channel, no sub out and no line out to an
  AVR to size, and no power budget to set.

So no rack amp package is designed (goal 26 item 2 and its done-when line B have nothing to
build), and `builds/chorus-rackamp-v1/` is not made.

## The theater front: the soundbar is not designed

K72 asked for two variants: a soundbar (touch and status LED on top, a microphone with hardware
mute, eARC and optical) and a separate LCR set with a hidden hub. On the room list:

- **No room uses a soundbar.** Both TV rooms are 5.1 with a two-way left, centre and right.
- **The LCR set is the theater front for both TV rooms:** three two-ways and the hidden TV hub.
  Both TVs have an optical output, so each hub is a Pi with an S/PDIF receiver and needs no ARC
  extractor (P4, "The house plan"); that is P2's Option A hub, which P2 already named as the LCR
  set's ("For K72: the LCR set's hidden hub is the Option A hub").
- So the soundbar is not designed and `builds/chorus-soundbar-v1/` is not made. The LCR set's
  own document is the next task, not this proposal.

## Recommendation

Accept the count: zero zones and zero channels, no rack amp, no soundbar. Goal 26 closes on the
LCR set alone.

## What would revive them

- **The rack amp:** a room whose passive speakers are wired back to the rack (in-wall,
  in-ceiling or outdoor, K74). Then: that room's zone and channel count goes here (the zones,
  the channels per zone, any sub out and line out to an AVR, from the wire runs and the power
  budget, with any unanswered input marked ASSUMED); the feature list of K70, K74 and K96 (2U,
  Linux where it fits, multichannel class-D, line-level and sub outs, line-in and optical in,
  front buttons, a pairing button and LED); and the hardware half in
  `docs/hardware/linux-multichannel.md`, "The rack amp: several zones on one card" (one
  `chorus-client` per zone on a `dshare`-shared card), whose example layout is ASSUMED until
  then. The rack's free units and depth become an input again.
- **The soundbar:** a TV room the owner wants without a separate left, centre and right (for
  example a stereo TV room with no space for three speakers). Then K72's soundbar variant within
  P2's settled scope: optical, plus an HDMI port for ARC through a bought extractor and CEC; a
  real ARC/eARC input inside the bar needs P2's Option B board, which is not approved.

## Open inputs

None. The owner's room list answers every input this proposal names.

Owed elsewhere: the owner's devices repo still lists `builds/chorus-rackamp-v1/` and
`builds/chorus-soundbar-v1/` as waiting on chorus, and P14 still counts them among its builds.
Closing goal 26 (the project's last task) tells devices to drop both.

## What was read

- `docs/proposals/P4-bench-purchase.md` ("Decision: the owner's final list", "The house plan")
  at `origin/main` 01ef1c8, read 2026-10-06.
- `docs/proposals/P12-enclosures.md` (the owner's decisions that bound it, the room list) at
  `origin/main` 01ef1c8, read 2026-10-06.
- `docs/proposals/P2-theater-scope.md` ("For K72") at `origin/main` 01ef1c8, read 2026-10-06.
- `docs/hardware/linux-multichannel.md` ("The rack amp: several zones on one card") at
  `origin/main` 01ef1c8, read 2026-10-06.
- The program brief, §1 (K70, K72, K74, K96, I11) and §30 (goal 26):
  https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md,
  read 2026-10-06.
- goals project 33 (the plan the owner approved 2026-10-06) and task 242, read 2026-10-06.
