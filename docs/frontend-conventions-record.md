# The frontend conventions, clause by clause

The umbrella's `documentation/frontend-conventions.md` binds every user
interface this project ships. chorus ships one: the control page the server
hands out on its control listener. This file says, for each of the eleven
clauses, which rendered assertion proves it here, or why it cannot apply.

It is not prose that can drift. `tools/ui/check-clause-record.js` reads the
table below on every `make verify-ui` and exits non-zero, naming the clause, if
any of the eleven is absent, carries both an assertion and an exemption, carries
neither, or names an assertion that did not run in that invocation. So a clause
cannot be quietly dropped, an exemption cannot be silent, and a row cannot go on
claiming an assertion after the assertion has stopped running.

An assertion name is a claim declared in `tools/ui/claims.js`. Each one is
proved by a test in `tools/ui/ui.spec.js` against a page a browser engine
painted, and each one is shown going red in `tools/ui/mutation.spec.js` against
a page mutated to break exactly that claim. That division is a rule and not a
habit: every ledger line carries the file it was written from, and
`tools/ui/check-claims.js` discounts and names a demonstration recorded
anywhere but `mutation.spec.js`, so a `demonstrates()` call sitting beside the
real page cannot make the count come out. `docs/control-page.md` is what the
page's regions link to.

## The record

| clause | assertions | exemption |
|---|---|---|
| F1 | contrast-in-both-themes, keyboard-operation, focus-visible, accessible-names, not-colour-alone, target-size | |
| F2 | rendered-by-a-real-engine | |
| F3 | unreadable-figure, stale-not-current | |
| F4 | aggregate-states-its-set | |
| F5 | unreadable-figure | |
| F6 | stale-not-current, paused-feed-visible, loading-never-unresolved | |
| F7 | three-states, loading-never-unresolved | |
| F8 | short-labels-and-doc-link | |
| F9 | reflow-360 | |
| F10 | themes-follow-preference, contrast-in-both-themes | |
| F11 | csp-clean | |

## Why each row says what it says

**F1 Accessibility.** WCAG 2.2 AA. Contrast is measured from the framebuffer in
both themes (1.4.3 and 1.4.11), the tab order is walked and every control
operated by key (2.1.1), the focus indicator is a measured pixel difference
clearing 3:1 (2.4.7), every accessible name is read out of the tree Chromium
computed (4.1.2), no state is carried by hue alone (1.4.1), and every control is
painted at 24 by 24 CSS pixels or more (2.5.8).

**F2 Rendered claims need a real engine.** Every assertion in the table is
graded on a page Chromium painted, and `tools/ui-render-run.sh` refuses by name
rather than reading source text when there is no engine or no driver.
`rendered-by-a-real-engine` is the assertion that the page under all of them was
actually drawn: the framebuffer has more than one colour in it and the pixels
where a zone card sits are the colour the engine computed for it.

**F3 Absence is not zero.** chorus's state message declares every field
mandatory, so there is no nullable field in the ordinary case. The clause still
bites twice. A figure the state did not carry renders as the word `Unavailable`
and never as `0`, a dash, `NaN`, `undefined` or a blank
(`unreadable-figure`). A figure whose feed has dropped is marked `last known`
rather than left reading as a current measurement (`stale-not-current`; a feed
that stopped delivering without dropping is the same rule, asserted under F6). A
zone with nobody attached showing zero endpoints is a genuine measured zero and
is correct. A zone whose NAME could not be read falls back to its identifier and
says `name unavailable` beside it, rather than passing the identifier off as a
name somebody chose, and a state whose zones all fail to identify renders the
error state rather than `No zones yet` - both graded inside `unreadable-figure`.

**F4 Every aggregate states its set.** The endpoint figure reads `1 of 2
endpoints attached, 1 away`: the rows counted, the set they were counted over,
and the rows left out, all beside the figure and in seven words.

**F5 One unreadable figure costs nothing else.** The same assertion as F3's
first half, asserted from the other side: the doctored zone renders
`Unavailable` while every other zone and every other region of the view still
draws.

**F6 Stale is never shown as current.** The clause names three ways a feed
stops - "a dropped stream, failed poll or paused feed" - and they are not one
thing under three names. A dropped stream is visible from the connection; a
paused feed is the case where the connection says nothing is wrong, and a page
that watched only the connection would be blind to exactly it; a failed poll is
what finds the second, and it is also the only thing that can find a server
that was already stopped when the page was opened. All three are asserted:

- **Dropped** (`stale-not-current`). The feed is severed at the socket under a
  live page; the connection reads `Connection lost` and every figure reads
  `last known` inside ten seconds with no interaction; the feed is restored and
  the figures read as current again without the page being reloaded.
- **Paused, and the failed poll that finds it** (`paused-feed-visible`). The
  server stops answering with nothing severed: the fixture holds every
  established connection open and answers no request on it, which is what a
  stopped process looks like from a browser - `EventSource.readyState` stays
  OPEN and no error fires. The assertion checks with the fixture that the
  stream is still open, so it is grading the paused case and not the dropped one
  over again, and requires the page to read `Not answering` with every figure
  `last known`, then to return to `live` when the server resumes. What finds it
  is the page's own poll of the server failing: `chorus.js` probes `/api/state`
  on a timer and a figure reads as current only when the stream is up AND the
  last probe came back. For a page that has already received a state, that timer
  bounds how long a paused feed can be shown as live at under nine seconds. The
  demonstration is the mechanism this page used before, served at
  `/demo/stale-blind`: freshness from `onerror` and a `readyState` watchdog
  alone, shown still reading `live` twelve seconds into a pause.
- **The first poll, which nothing else can watch**
  (`loading-never-unresolved`). The timer above is started when the page's own
  first `/api/state` request settles, so it is no help at all to a page opened
  while the server is already stopped: that request never comes back, and
  without a deadline on it the page subscribes to nothing and shows `Loading`
  for as long as the tab is open. So that one request carries its own deadline,
  `STATE_TIMEOUT`, after which the failure is rendered as the error notice, and
  the assertion grades it against a server that accepts the connection and
  answers nothing rather than against a delay chosen to land inside the window.
  The demonstration is the bootstrap this page used before, served at
  `/demo/loading-blind`: one `fetch("/api/state")` with no deadline on it, shown
  still reading `Loading` twenty seconds into a pause.

**F7 Three states, all of them.** Loading is graded against a state request that
never answers at all, empty against a real server with no zone configured, and
error against a state request that fails. Each is asserted to be alone. The
clause's other limb - that a loading state is never left unresolved - has no
condition on it, so it is graded against the state that breaks it and not
against a slow answer chosen to arrive inside the window
(`loading-never-unresolved`): the page is opened against a server that accepts
the connection and returns nothing, and has to reach the actionable error notice
with no interaction and inside a bounded time. A merely slow request is asserted
in the same breath to still resolve INTO the zone list, so that deadline cannot
be bought by calling every slow server a stopped one.

**F8 Explanation lives in docs.** No text run on the surface is more than twelve
words, every region carries exactly one link, and the link is followed in the
engine to `docs/control-page.md`, which has to answer with the document.

**F9 Phone first.** At 360 by 640 the document does not scroll sideways, no
control is painted outside the viewport or under 24 by 24, and the one thing
too wide to fit scrolls inside its own container. It is graded against the
widest state the control catalog admits and not only against a name that
happens to fit: the zone is renamed, through the page's own command route, to an
unbroken sixty-four-character name with no space and no hyphen in it, and the
measurement is taken again with the name asserted still whole on the card. The
demonstration is that same card with the wrapping rule off the heading, shown
dragging the rename box off the right-hand edge.

**F10 Both themes.** A light preference paints a light page and light panels and
a dark one paints dark, decided by the luminance of the pixels rather than by
which media query is in the file; and the contrast assertions run once per
theme.

**F11 A Content-Security-Policy is required.** The server sends one with the
page, its stylesheet, its script and its document. The browser's own violation
reports are asserted empty, and the page is asserted to still work under it:
the stylesheet applied, the script ran, the state request answered, the event
stream delivered another subscriber's change, and a command the page issued was
accepted.

## No clause is exempted

Every one of the eleven applies to this surface and every one is asserted. The
exemption column is empty on purpose and the checker requires it to stay empty
as long as an assertion is named: a future exemption has to be written down,
with its reason, in the row it excuses.

One claim in `tools/ui/claims.js` is deliberately not in the table.
`refusal-shown` answers chorus's own acceptance criterion about a refused
command rather than a clause of the conventions, and the checker does not
require every claim to be cited by a clause, only that every clause cites a
claim that ran.
