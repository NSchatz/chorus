# 0190: the app installs behind the household login: a manifest fetched with credentials, a hand-written service worker that never answers or keeps the API and keeps only the app's own plain 200, and a signed-out state read from a redirect that is not followed

- Status: accepted, 2026-10-05.
- Decided by: the owner for what is asked (K40: the app is reached behind the household
  login, chorus adds no authentication of its own; proposal P5, `docs/proposals/P5-app-stack.md`,
  "Common to every option": the hand-written service worker and the signed-out state). How
  the worker's rules are split from the worker, what the shell is and how its cache is named,
  what counts as the login's answer, how the icons are made and the one directive added to
  the server's policy are this record's: each is one module or one line, cheap to reverse.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `web/src/worker.js`, `web/src/sw.js`, `web/src/install.js`,
  `web/src/manifest.webmanifest`, `web/src/icons/`, `web/icons.mjs`, `web/src/index.html`,
  `web/src/api.js` (`signedOut`), `web/src/state.js`, `web/src/chorus-app.js`,
  `web/src/rooms.js`, `web/build.mjs`; `web/test/worker.test.js`, `web/test/install.test.js`,
  `web/test/signed-out.test.js`, `web/test/fake-server.js`; `web/smoke/app.spec.js`,
  `web/smoke/fake-login.js`; `crates/server/src/control.rs` (`CONTENT_SECURITY_POLICY`),
  `crates/server/tests/app_serving.rs`.

## Context

The app is served under `/app/` by chorus-server (record 0182) and is used through a reverse
proxy that puts a household login in front of it (K40). Two things follow from the login and
neither is about chorus-server, which has no authentication:

- A browser fetches a web manifest without credentials unless the link says otherwise, even
  on the page's own origin. Behind a login that fetch is refused, and an app with no manifest
  does not install.
- A login lapses. The proxy then answers any request with a redirect to its login page, or
  refuses it. A page that follows the redirect is handed the login page's HTML where it asked
  for JSON, and a service worker that keeps what it is handed serves the login page as the app
  from then on.

Record 0182 fixed what this needs from the server before the worker existed: `sw.js` at the
app's root, never `immutable`; the document `no-cache`; a file named by its hash immutable.

## Decisions

1. **The manifest is `manifest.webmanifest` beside the document, linked with
   `crossorigin="use-credentials"`.** Its `start_url`, `scope` and `id` are `/app/`, the
   permanent path of record 0182, written absolute because an installed app keeps them; its
   display mode is `standalone`; its icon addresses are relative to it. The build copies it as
   written and gives it no hashed name: the server answers `no-cache` for it.
2. **The icons are three committed PNG files drawn by `web/icons.mjs`** (180 for a phone's
   home screen, 192 and 512 for the manifest), a dot and two rings in the app's accent on its
   dark background. The background fills the square and the drawing stays inside the middle
   60 percent, so the 512 file is also the maskable icon. The script is run by hand when the
   drawing changes and never by the build: the bytes chorus-server embeds then do not depend
   on the zlib of the node that built them, which the gate's rebuild-and-compare would
   otherwise be held to.
3. **The server's Content-Security-Policy gains `manifest-src 'self'` and nothing else.**
   `default-src 'none'` covers a manifest, so without the directive the browser refuses the
   app's own. The service worker needs none: `worker-src` falls back to `script-src 'self'`,
   and the worker's own requests are same-origin under `connect-src 'self'`. Record 0182's
   "the policy, unchanged" is amended by this one directive; the two tests that hold a second
   copy of the policy hold the new one.
4. **The service worker is written by hand in two files.** `worker.js` is the rules as pure
   functions over a cache store and a network it is given; `sw.js` hands the browser's
   install, activate and fetch events to it and decides nothing. The build bundles the two
   into `dist/sw.js` and fills in the list of the shell's files and the build's version. No
   library and no generator is involved, and the unit tests run the rules in node with a
   scripted network and a scripted cache.
5. **The API is not the worker's.** For any address under the server's `/api/` (the state, a
   command, the artwork and every event stream) and for anything that is not the app's (another
   origin, the server's other pages, a request that is not a `GET`) the fetch handler does not
   call `respondWith`: the request is the browser's own, exactly as with no worker, which is
   what network-only means and is the one way an event stream is not held by a worker in
   between. The worker makes no request of its own for such an address and opens no cache.
6. **One function, `mayStore`, decides what is written to a cache, and every write goes
   through it.** It allows only a response with status `200`, of type `basic`, that was not
   redirected, for a `GET` of an address under the app's directory that is not the API. So a
   redirect that was not followed (`opaqueredirect`), the page a followed redirect ended on,
   a `401`, a `404`, a gateway's `502` and another origin's response are never kept, whichever
   path they arrive on.
7. **A navigation asks the network first.** The document it answers with is kept as the shell,
   under the app's own address whatever query the navigation carried. The shell is what a
   navigation gets when the network gives no answer, or a `5xx` (a proxy answering for a
   server that is down). A redirect to the login is handed to the browser as it came, so the
   browser follows it. A file named by its hash is answered from the cache; any other file of
   the app asks the network first.
8. **The shell is every file of the build's output but the worker, cached whole at install or
   not at all.** Each is fetched with `redirect: "manual"`; if one answer may not be stored
   (the login has lapsed), the install fails, nothing is written and the browser tries again
   at the next load. The cache is named `chorus-app-<version>`, the version a digest of the
   files, and activating deletes the other `chorus-app-` caches. The worker does not skip
   waiting and does not claim open pages: a page is the worker's from its next load.
9. **Signed out is read from the answer, in the API client.** Every request of `api.js` is
   made with `redirect: "manual"`. An `opaqueredirect` or a `401` is the login's answer:
   chorus-server answers neither on any route, so both can only come from in front of it. The
   event stream then reports `signed-out` in place of `lost` and keeps trying, so a person who
   signs in in another tab is live again with no reload; a command that meets the login says so
   at once.
10. **The shell says "Signed out" with a link to the page's own address.** Following it is a
    navigation, which the login answers with its page and returns from. The rooms last known
    stay on the page, marked as last known. Nothing can be changed while signed out: a command
    is answered by the login and never reaches the server.

## Consequences

- A person offline, or with the server down, gets the app's shell, which says the server
  cannot be reached and follows it when it is back. Controlling rooms offline is not a thing
  this does.
- A new build reaches an open page at its next load (the document is asked of the network
  first); the new worker takes over when the old one's pages are closed.
- The fake login of the smoke test can expire, and from then on redirects whatever is asked
  of it: the smoke test sees a login's two answers to an API call, the `401` before the
  sign-in and the redirect after the expiry.
- Whether the real login's redirect completes inside an installed app on the owner's phones
  is not something a test here shows. It is the phone check of the app's plan, the owner's.
- What the real login answers to an API call with a lapsed session (a redirect or a `401`) is
  not measured here; both are read as signed out.

## What was read

- `docs/proposals/P5-app-stack.md` ("Access", "Secure context", "The existing
  Content-Security-Policy", "Common to every option").
- The planning research `research-pwa-conventions.md`, section 1.4 (the manifest needs
  credentials, quoting MDN's manifest page; Chromium's install criteria; the service worker
  rules), and the goal program's section 25, item 3, both at the commit `CLAUDE.md` pins.
- `docs/decisions/0181-the-web-app-stack.md`, `docs/decisions/0182-the-app-is-served-under-app.md`,
  `docs/decisions/0183-the-one-browser-smoke-test.md`,
  `docs/decisions/0185-the-apps-state-layer-and-its-live-test.md`.
- `crates/server/src/app.rs`, `crates/server/build.rs`, `crates/server/src/control.rs` (the
  policy and the routes under `/api/`).
- Not read again for this record, and stated from the web platform's specifications as the
  author knows them: that `manifest-src` falls back to `default-src` and `worker-src` to
  `script-src` (Content Security Policy Level 3), and that a navigation request reaches a
  worker with redirect mode `manual` (Fetch). The smoke test holds what follows from each:
  the manifest loads under the policy, the worker registers under it, and a navigation with
  the login expired ends on the login page with nothing of it cached.
