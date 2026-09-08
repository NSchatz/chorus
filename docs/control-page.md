# The control page

This is the document every region of chorus's control page links to. The page
itself carries labels of a few words; the paragraphs that say what a figure was
computed over, and what it does not include, are here. Nothing was dropped when
they moved: what is on this page was on that one.

The page is served by the control listener, at `GET /`, and this document is
served beside it at `GET /docs/control-page.md`, so a link from a region resolves
in a browser rather than pointing at a file the browser cannot reach.
`docs/control-plane.md` is the contract for the messages underneath all of it.

## What the page is

One view: the zones this server was configured with, one card each. The page
holds no state of its own. Everything on it comes from the state message the
server fans out, and it changes when the next one arrives. A control sends one
command and then waits for the state that results, which is the same state every
other subscriber is sent. There is no local copy that could disagree with the
server, and no optimistic value that could be wrong.

There is nothing to log in to. The control listener has no authentication and
this page adds none: anything that can reach the address the server was started
on can drive it. That is a property of the deployment, stated here rather than
implied.

## The header

**The connection word.** `Live` means the event stream is open and the figures
below were sent by the server, not remembered by the page. `Connection lost`
means the stream dropped: the browser is retrying, and every figure on the page
is marked `last known` until it comes back. `Connecting` is the moment before
the first state arrives.

A page whose feed has dropped is not a page with nothing to say, so the last
figures stay on screen. What changes is that they stop claiming to be current.
The page does not need to be reloaded when the stream comes back; the marks
return to `live` on their own.

## A zone card

**The name.** What a person called this room. It is set from the box on the card
and it survives a restart. The catalog admits 1 to 64 characters with no control
character and no leading or trailing space; a name outside that is refused, and
the refusal is shown on the card that issued it.

**`id`.** The identifier the server was started with, `--zone <id>`. It never
changes, and no message can create one: the set of rooms is a fact about a house,
so a typo has to be a refusal rather than a new room nobody has.

**`group`.** The group whose stream this zone plays. Every zone is always in a
group; `Ungroup` puts a zone into a group of its own, named for the zone, rather
than into an absent state.

**`stream`.** Where that group's audio is served from.

**The endpoint figure.** This is the aggregate on the card, and it says which
rows it counted. `1 of 2 endpoints attached, 1 away` means: this zone has two
endpoints in its persisted list, one of them is attached and playing right now,
and one is not. The two numbers come from two different fields of the state
message and they mean different things:

- `present` is the set attached right now. It is never persisted, because which
  speakers are switched on is a fact about now and not about the house. This is
  the first number, and it is what "attached" counts.
- `endpoints` is every endpoint this zone has ever had, and it is persisted. This
  is the second number, and it is the set the first number was taken over.

`1 away` is the difference: rows the figure did not count, reported rather than
left for a reader to work out. A zone with nobody attached shows `0 of 2
endpoints attached, 2 away`, and that zero is a measured zero: it means the
speakers are off, not that the figure could not be read.

`Endpoints unavailable` is the other thing entirely. It means the state message
carried no readable endpoint list for this zone, so there is no figure to show.
It is never rendered as `0`, as a dash or as a blank, because each of those reads
as a measurement that was taken.

**Volume.** An amplitude factor from `0.000` to `1.000`, shown as a percentage.
It is not a position on a perceptual curve: `50%` means every sample is
multiplied by one half, which is quieter than half as loud sounds.
`docs/decisions/0016-the-control-catalog.md` records why the curve is not on the
wire. `Unavailable` in place of the percentage, with no slider beside it, means
the state carried no volume this page could read; a slider has to be somewhere,
and anywhere it could be is a value the page would be making up.

**Mute.** The button is a toggle and carries its state where something that is
not reading the screen can find it. The word beside it, `Muted` or `Not muted`,
is the same state in text, so the distinction survives a rendering with no colour
in it. Muting does not change the volume, so unmuting gives back what was set.

**`Figures live` / `Figures last known`.** The freshness of this card. See the
header above.

**A refusal.** `Refused: name` means the server rejected the last command from
this card and named that field as the fault. The control goes back to the value
the server still holds, because showing the refused value would be showing
something no subscriber has. A refusal costs one message and changes nothing:
validation happens before anything is applied, so every subscriber's state is
byte-identical to what it was.

## The footer

`state 7` is the serial: the number of changes applied since the state was
created or loaded. It is how a subscriber tells a message it has seen from one it
has not.

`catalog version 1` is the version of the control catalog this server implements.
`docs/control-plane.md` is that catalog.

`N zones unreadable`, when it appears, counts zones in the state message that
carried no identifier this page could use. They are counted here rather than
dropped in silence, because a row left out for want of a value is still a row.

## When there are no zones

`No zones yet` is a state and not an error. A server with no zone configured
serves `{"v":1,"t":"state","serial":0,"zones":[]}`, which the catalog declares
and `fixtures/control/state-empty.json` pins.

Zones are configured when the server starts, one `--zone` for each room:

```
chorus-server --control-listen 127.0.0.1:4020 --zone kitchen --zone study
```

Restart the server with those and the page shows them, with no further
configuration and nothing to click here first. A control message cannot add one,
for the reason `id` gives above.

## When the state cannot be read at all

`State could not be read` means the request for the state did not answer, or
answered with something that is not a state message. That is different from a
figure that could not be read, which costs only that figure. Check that
`chorus-server` is running on the address this page was served from, then reload.

## Themes, keyboard and colour

The page follows the operating system's light or dark preference. There is no
control on the page to find first, and there is no third choice: what the machine
is set to is what is rendered.

Every control is reachable with Tab, in the order the page reads, and operable
from the keyboard to the same effect a pointer has. The control with focus is
drawn with an indicator outside its own border, in a colour used for nothing
else.

No state on the page is carried by colour alone. Muted from unmuted, live from
last known, and available from unavailable are all differences in the rendered
words, so a forced-colours or greyscale rendering loses none of them.

`docs/frontend-conventions-record.md` maps each clause of the umbrella's frontend
conventions to the rendered assertion that proves it here.
