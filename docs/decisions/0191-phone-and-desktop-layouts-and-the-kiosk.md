# 0191: the app has a phone and a desktop layout with one breakpoint at 48em, chosen in script and selected by attribute, and a kiosk mode switched by `?kiosk`, kept in storage, with 64-pixel controls and a screen wake lock that is asked for again when the page is visible again

- Status: accepted, 2026-10-05.
- Decided by: the owner for what is asked (K86: the app is for phones, wall tablets in kiosk
  mode and the desktop, with no television layout; the kiosk is "always on, big touch
  targets, no browser chrome"; proposal P5, `docs/proposals/P5-app-stack.md`, "Common to every
  option": "Kiosk (K86): Screen Wake Lock, a kiosk layout class, big targets"). Where the
  breakpoint is, how a layout is selected, what the navigation is, the kiosk's sizes, that
  the switch is remembered and where the wake lock is tested are this record's: each is one
  module, one token or one rule, cheap to reverse.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `web/src/layout.js`, `web/src/mode.js`, `web/src/wake-lock.js`,
  `web/src/main.js`, `web/src/chorus-app.js`, `web/src/app.css`, `web/src/room-card.js` and
  `web/src/group-card.js` (the slider's least width); `web/test/layout.test.js`,
  `web/test/mode.test.js`, `web/test/wake-lock.test.js`; `web/smoke/app.spec.js`.

## Context

Until this record the app was one column at every width, and `?kiosk` hid the header and did
nothing else. K86 names three places the app is used: a phone in a hand, a desktop, and a
tablet on a wall that is never switched off and is touched at arm's length.

Two rules of the app bear on how a layout can be written. An element's styles name tokens of
`tokens.css` and never a literal length (`web/README.md`), and a media query cannot read a
custom property, so a breakpoint written as a media query in an element's styles is a literal
length no token can replace. And exactly one test starts a browser
(`docs/decisions/0183-the-one-browser-smoke-test.md`): what a browser lays out can only be
asserted there.

## Decisions

1. **Two layouts, `phone` and `desktop`, and one breakpoint: 48em.** A viewport at least 48em
   wide is the desktop layout; a narrower one is the phone layout. An em of a media query is
   the browser's initial font size, so the breakpoint is 768 CSS pixels unless a person has
   asked their browser for larger text, and then it moves with the text. 48em is where the two
   columns below each fit the 320 pixels a card is designed to (`--surface-column-min`) with
   the wider one half as wide again. There is no third layout and no television layout (K86).
2. **The breakpoint is written once, in `web/src/layout.js`, and the layout is chosen in
   script.** `chorus-app` asks `watchLayout`, which asks the browser with `matchMedia` and
   follows its `change` event, and reflects the answer as the attribute `layout`. The styles
   select on the attribute (`:host([layout="desktop"])`) and hold no media query, so the rule
   that an element's styles carry no literal length stands with no exception. The app paints
   nothing before its script runs, so choosing in script costs no first paint in the wrong
   layout. Where there is no `matchMedia` the layout is the phone's: one column fits any width.
3. **The phone layout is one column with the navigation at the bottom edge.** The groups, then
   the rooms, each as wide as the screen. The navigation is a bar fixed to the bottom edge of
   the viewport, where the thumb of the hand that holds the phone reaches it; the app keeps the
   bar's height free below the last card, so the bar covers nothing.
4. **The desktop layout is two columns: the groups, with what each plays, beside the rooms.**
   The groups column is the narrower (one part to the rooms' two, neither under 320 pixels),
   and it is first, as it is first in the document, so the order a keyboard and a screen
   reader meet the regions in is the order they are painted in. The header and the signed-out
   words run across both. The navigation is in the header.
5. **The navigation is two buttons, "Groups" and "Rooms", that move the view and the focus to
   the region they name.** The app has two regions and no other screen yet; a later screen
   adds a button. They are buttons and not links to a fragment: the regions are in a shadow
   root, where a fragment does not reach.
6. **Kiosk mode is switched by the address and remembered.** `/app/?kiosk` (bare, or with any
   value but `0` or `false`) enters it and writes the choice to the browser's `localStorage`
   (`chorus.kiosk`); `/app/?kiosk=0` leaves it and clears the choice; an address that does not
   name the kiosk gets what was last chosen in that browser. So a wall tablet opened once with
   `?kiosk` is a kiosk after a reload, after its browser restarts, and from the installed
   app's start address, which is `/app/` with no query (record 0190). Where storage is refused
   the address alone decides the page it is on.
7. **A kiosk's controls are at least 64 by 64 CSS pixels, and its text is larger.** The
   ordinary app paints every control at a border box of at least 44 by 44 (`--control-size`).
   Under `mode="kiosk"` `chorus-app` sets that token, and the two that size a button's width,
   to `--kiosk-control-size` (64 pixels), and the body, meta and heading sizes to the kiosk's
   (20, 17 and 20 pixels). The elements under it read the same tokens they always did; none
   has a kiosk rule of its own. The kiosk's tokens are in `web/src/app.css`, because
   `tokens.css` is the control page's file carried over whole.
8. **A slider is never narrower than two controls** (`--slider-min-width`), in either mode: a
   row that cannot give it that wraps, and the slider takes a line. The smoke test found the
   group volume slider squeezed to 54 pixels in the kiosk's narrower column; a slider that
   narrow has 54 pixels of travel for a hundred steps.
9. **A kiosk shows nothing of the app around the rooms.** The wordmark is not drawn. Wide (the
   desktop layout, which is what a tablet on a wall gets), the header is not drawn at all: both
   regions are in view and there is nothing to navigate between. Narrow, the bottom bar stays.
   The browser's own chrome is not the page's to remove: the installed app is `standalone`
   (record 0190), and a tablet's kiosk browser is set up on the tablet, which is the owner's.
10. **A kiosk keeps its screen on with the Screen Wake Lock API, and asks again each time the
    page becomes visible.** `web/src/wake-lock.js` asks for a `screen` lock when the kiosk
    starts, if the page is visible, and on every `visibilitychange` to visible while it holds
    none: a browser releases the lock when the page is hidden and does not give it back. Only
    `main.js` starts it, and only in kiosk mode: a phone's screen is the phone's to switch off.
11. **Without the API, or refused, the kiosk works and its screen sleeps as the tablet is
    set.** `navigator.wakeLock` exists only in a secure context (HTTPS or the loopback
    address), so on the server's plain HTTP port there is none. `keepAwake` then does nothing,
    listens to nothing, and neither throws nor rejects; a refused request is not an error
    either, and is made again the next time the page is visible. The app says nothing about
    it on screen: what a kiosk shows is the rooms.
12. **The layouts and the kiosk are asserted in the one browser test, in tests of their own
    in the same file.** `web/smoke/app.spec.js` gains four tests after the first, named for
    the phone layout, the desktop layout, the breakpoint and kiosk mode; `tools/web.sh` still
    holds the run to the tests of exactly one file. The logic that needs no browser (the
    breakpoint as a number, following `matchMedia`, the kiosk switch and its memory, the wake
    lock's requests and its absence) is unit-tested in node.

## Consequences

- The headless Chromium of the smoke test has the Screen Wake Lock API and refuses every
  request of it with `NotAllowedError` (seen 2026-10-05, also with the `wakeLockScreen`
  permission granted over the DevTools protocol): it has no screen. The kiosk test therefore
  stands in for `navigator.wakeLock`, after asserting the browser's own is there, with an
  object that records each request and gives a sentinel with a `release` event; and it hides
  the page by releasing the sentinels and dispatching `visibilitychange` with the document's
  visibility set, which is the order a browser does it in. What is asserted is the app's
  behaviour against that object: one request on entering, none while hidden, one more when
  visible again, one after a reload, and none at all outside kiosk mode. That a real tablet's
  browser grants the lock and holds the screen on is not shown by any test here; it is seen on
  the tablet, which is the owner's.
- A wall tablet reaches the server over HTTPS, or the wake lock is absent (decision 11). The
  household's reverse proxy gives that (K40).
- The kiosk choice is per browser. A person who opens `?kiosk` on a phone by mistake leaves
  it with `?kiosk=0`; there is no control on screen for either, since a kiosk shows nothing of
  the app around the rooms.
- A later screen adds a navigation button and, if it is a third region, decides where it sits
  in the desktop's columns.
- The control page (`crates/server/src/ui/`) is untouched: its tokens file is the one the
  app's was copied from, and the app's additions are in `app.css`.

## What was read

- The goal program's K86 at the commit `CLAUDE.md` pins, and `docs/proposals/P5-app-stack.md`
  ("Kiosk (K86)", "Secure context").
- `docs/decisions/0181-the-web-app-stack.md`, `docs/decisions/0183-the-one-browser-smoke-test.md`,
  `docs/decisions/0187-groups-in-the-app.md`, `docs/decisions/0190-the-app-installs-behind-the-login.md`.
- `web/src/tokens.css` (the control sizes and the scale), `docs/control-page.md` ("Themes,
  keyboard and colour").
- Not read again for this record, and stated from the web platform's specifications as the
  author knows them: that a screen wake lock is released when its document is hidden and is
  not reacquired by the browser (Screen Wake Lock API), that the API is exposed only in a
  secure context, and that an em in a media query is relative to the initial font size (Media
  Queries Level 4). The unit tests hold the app to the first two as stated; the smoke test
  holds the third (767 pixels is one column and 768 is two, at the default text size).
