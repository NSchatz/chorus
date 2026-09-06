# 0018: the persisted zone state, its format, and what is deliberately not in it

- Status: decided
- Recorded by: PRODUCT-6 (spec S0043-chorus-product-6)
- Implemented in: `crates/control/src/persist.rs`; exercised end to end by
  `tools/restart-storm-run.sh`, which kills a server with SIGKILL and compares
  what its replacement serves.

## Decision

A `key = value` text file, with a `[zone <id>]` section per zone, at the path
`--state-file` names. Format version 1:

```
format = 1
serial = 11

[zone kitchen]
name = The Kitchen
group = downstairs
volume = 0.375
muted = 0
endpoints = endpoint-a,endpoint-b
```

**Written by rename**: to a temporary beside it, then renamed over the old one.

**Read once, at start, and it is the whole answer or it is not consulted.** A
server with a state file does not merge it with `--zone` arguments.

**A file this build cannot read stops the server**, exit 8, naming the file and
what was wrong with it. Every field is required and nothing is defaulted.

**`present` is not persisted.** Which endpoints are attached is a fact about now.

## Reasoning

### Why not the state message's own JSON

It would have been one line of code. It is rejected because the state message is
a WIRE format with a catalog version on it, and a wire format that is also a
storage format cannot be changed without a migration for every file anybody has.
`docs/decisions/0016` already commits the catalog to refusing a version it does
not implement; a state file that shared that version would mean a server upgrade
refusing to start on the state it wrote yesterday.

Two formats, two versions, one conversion between them. The conversion is in one
file and is the thing the tests exercise.

### Why `key = value` and `[zone <id>]`

It is what every other committed file in this repository is: `config/sync.conf`,
`config/verification.conf`, `audio-path.conf`, `fixtures/*/*.fields`,
`fixtures/sync/*.cfg`. A person with an editor can fix a volume in it, a diff of
it is readable, and reading it needs nothing but a splitter on `=`.

The section header carries the zone identifier, which is why identifiers are
restricted to lower-case letters, digits and hyphens
(`docs/control-plane.md`): a `]` or a newline in an identifier would be an
identifier that does not come back.

### Why the write is a rename

The criterion this serves is about a server killed with SIGKILL. A process
killed in the middle of `write` leaves a truncated file; a process killed
between the write and the rename leaves the old file untouched. So the file on
disk is always one whole state - the previous one or the new one - and never
half of either.

The state is persisted BEFORE it is fanned out, so a subscriber that has been
told a change happened cannot be told something the disk would contradict after
a restart. A persist that fails is reported on stderr and does NOT refuse the
command: the change IS in force in this process, and saying it was refused would
be a lie in the other direction.

### Why the file is not merged with the command line

Merging would mean deciding what a zone in the file and not on the command line
is. It could be a zone the operator has just removed from the configuration, or
a zone the operator forgot to pass this time, and there is nothing in either
input that tells those apart. Guessing wrong in the first direction resurrects a
room that was deleted; guessing wrong in the second silently discards a room's
name and volume.

So: a state file, if there is one, is the whole set of zones. To add a room, add
`--zone` and delete the state file, or edit the state file. `make verify` and
every tool in `tools/` that starts a server without `--state-file` gets the
configured set, which is the ordinary case for a verification run.

### Why a bad file stops the server

The alternative is starting with defaults, which means every zone's name,
group, volume and mute silently reverting to what it was on the day the server
was first configured. A person whose house went to full volume in every room at
once because a state file had a typo in it is owed a refusal instead, and the
refusal names the line.

### Why `present` is not in it

An endpoint that is switched on is a fact about now, and a file cannot know it.
Persisting it would mean a restarted server serving a state message that says
four endpoints are playing when the house is empty, until each of them fails to
appear - and nothing would make them fail to appear, because nothing is looking.

The membership (`endpoints`) IS persisted, because it is a fact about the house:
that this endpoint belongs to this zone. What happens to a persisted endpoint
that never comes back is
`docs/decisions/0019-a-persisted-endpoint-that-never-comes-back.md`.

## Alternatives rejected

- **A directory of one file per zone.** Rejected: an atomic rename covers one
  file, and a state spread over several has no instant at which it is all
  consistent.
- **Persisting on a timer rather than on every change.** Rejected: the window
  between the change and the write is exactly the window in which a SIGKILL
  loses it, and the criterion is about a SIGKILL.
- **No persistence, with the endpoints re-announcing what they last had.**
  Rejected: the server is authoritative, and a state reconstructed from
  whichever endpoints happened to come back first is not one state.
