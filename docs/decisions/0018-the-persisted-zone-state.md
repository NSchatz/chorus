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
name = The Kitchen \#1
group = downstairs
volume = 0.375
muted = 0
endpoints = endpoint-a,endpoint-b
```

**An unescaped `#` starts a comment, and every value is escaped.** The two
escapes are `\\` for a backslash and `\#` for a hash, and the format has no
others: an escape it does not have is a refusal naming the sequence, never a
dropped backslash. So `The Kitchen #1` is written `name = The Kitchen \#1` and
comes back as `The Kitchen #1`.

**Written by rename**: to a temporary beside it, then renamed over the old one.

**A render this build cannot read back is never written.** `write_file` runs
the rendered text through this module's own loader and compares the re-render
before it installs anything, so a field that later carries text nobody escaped
is a loud refusal on the machine that wrote it rather than a zone that comes
back under a different name after the next restart.

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

### Why every value is escaped, and why the comment character forced it

A `key = value` file with `#` comments has exactly one hazard, and this format
hit it. A zone NAME is human-set text: `docs/control-plane.md` declares it as
any printable character up to 64 of them, which is the right rule for a rename
box a person types into, and it admits `#`. Written plainly, `name = Kitchen #1`
reads back as `Kitchen` - silent corruption - and `name = #1` reads back as
nothing at all, which the loader refuses, so the server replacing a killed one
does not start and NO endpoint returns to playback. Both were real on this
branch and both are pinned by `crates/server/tests/regress_0043_f1.rs`.

Three routes were available:

1. **Narrow the name rule** so `#` is not a name character. Rejected: it is a
   user-visible capability cut made to suit a storage detail, and "Kitchen #1"
   is a name a person will reasonably type. The wire catalog would then have to
   refuse it, and the reason would be a comment character in a file the person
   never sees.
2. **Quote the value** (`name = "Kitchen #1"`). Rejected as a bigger change to a
   format whose whole argument is that reading it needs nothing but a splitter
   on `=`: quoting brings the question of what an unquoted value means, and
   whether a quote inside a quoted value is doubled or escaped.
3. **Escape the value**, which is what is done. Two escapes, `\\` and `\#`, both
   applied on write and resolved on read, and an unescaped `#` still starts a
   comment so the file stays hand-commentable. A person editing by hand sees
   one backslash rule and no new punctuation.

The escaping is applied to **every** value rendered, not only to `name`. Only
`name` can carry a `#` today - identifiers, the volume literal, `muted`,
`serial` and `format` cannot - but the comment stripping is per line across the
whole loader, so a field that later becomes free text would reintroduce the
fault by being the one place somebody forgot. Making it uniform costs nothing:
escaping a value that contains neither `\` nor `#` returns it unchanged.

The format version stays at **1**. Version 1 has never shipped: this branch is
its first appearance, so there is no file anywhere that the old reading applies
to, and bumping to 2 would announce a migration for a population of zero.

### Why a write is checked before it is installed

The property AC-3 rests on is "a state file this build writes is one this build
reads back as the same state". Escaping makes it true; checking makes it stay
true. `write_file` renders, loads the rendered text, re-renders what it loaded
and compares, and installs nothing if the two differ. The cost is one parse of a
few hundred bytes per applied command; the benefit is that the next hole in this
format is found by the process that wrote the file, on the line that wrote it,
instead of by a person whose rooms came back with the wrong names.

A refused write is reported exactly as any other failed persist is: on stderr,
saying the change is in force in this process and will not survive a restart,
and the previous readable file is left where it was.

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
- **Refusing a `#` in a zone name instead of escaping it.** Rejected above, in
  "Why every value is escaped".
