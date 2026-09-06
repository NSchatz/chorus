# 0019: a persisted endpoint that never reconnects

- Status: decided
- Recorded by: PRODUCT-6 (spec S0043-chorus-product-6)
- Implemented in: `crates/control/src/zones.rs` (`Zone::endpoints`,
  `Zone::present`, `Zones::endpoint_left`); visible in every state message and
  in the persisted file.

## The question

The persisted zone state names the endpoints each zone has. A server restarts,
reads the file, and one of the endpoints it names never appears: it was moved to
another room, or it broke, or it was switched off for the winter. What should
the system do about it?

## Decision

**Nothing, visibly.**

A zone's `endpoints` is every endpoint it has ever had, in the order they first
attached, and it is persisted. A zone's `present` is the subset attached right
now, and it is never persisted and never read from a file. An endpoint that does
not come back stays in `endpoints` and is simply absent from `present`.

Specifically:

- It is **never removed automatically**. There is no timeout after which a zone
  forgets an endpoint.
- Its absence **never blocks the zone**. A zone whose only endpoint is gone
  still has a name, a group, a volume and a mute, still appears in the state
  message and in the UI, and is still commandable.
- Its absence is **reported rather than inferred**: `present` is in the state
  message beside `endpoints`, so a subscriber can see the difference without
  computing it.
- It is removed when a person removes it, by editing the state file, or by
  deleting the file and letting the configured set come back.

## Reasoning

### Why not a timeout

Every timeout is wrong for some house. A speaker unplugged while a room is
decorated is gone for a week and is meant to come back; a speaker moved to
another room is gone forever and its membership is now a lie. No interval
distinguishes those, because the thing that distinguishes them is a person's
intention, which is not on the network.

A timeout that fired would also do its damage silently and at the worst moment:
the zone's membership would change while nobody was looking, and the change
would be persisted, so there would be nothing left to undo it from.

### Why not forget it at once

The membership is what makes the zone's history readable: an operator looking at
a state file after a restart can see that the kitchen has two endpoints and one
of them has not come back, which is the fact they need. A zone that forgot an
endpoint the moment it disconnected would make every restart look like a fresh
install, and would make the distinction between "this zone has one speaker" and
"this zone has two and one is off" impossible to see.

It also costs nothing to keep. An identifier is at most 32 bytes and the list
grows only when a NEW endpoint attaches, which is a thing a person does.

### Why it must not block anything

This is the half that would be easy to get wrong by omission rather than by
choice. A zone whose endpoint has gone is still a room with a volume, and:

- a `volume` or `mute` command naming it succeeds, and the resulting state is
  fanned out. It applies to no endpoint, which is exactly right;
- the UI shows it, with its name, at the volume it is set to, saying how many
  endpoints are playing;
- a `group` command naming it succeeds. When the endpoint does come back it
  joins the group the zone is in, without anything having to be re-issued.

The alternative - refusing commands about a zone with nothing attached - would
mean an operator could not set a room up before switching its speaker on, and
would make the state a function of which speakers happened to be powered.

### What an endpoint coming back does

It sends `attach` on every control connection it opens, not only the first
(`docs/control-plane.md`). So a server that has just restarted and read its
state file learns which endpoints are present from the endpoints themselves,
which is the only place that fact exists.
`tools/restart-storm-run.sh` grades exactly that: four endpoints, a SIGKILL, and
all four back in `present` with nothing said to any of them.

## What this does not decide

Whether an endpoint may be in more than one zone. It may not, today, because
`attach` names one zone and an endpoint runs with one `--zone`. Nothing here
depends on that staying true; a later phase that wants an endpoint in two zones
adds a catalog version and this record is unaffected.
