# 0000: the app is served under /app/ for good, compiled into chorus-server by a std-only build script; the document and the service worker are no-cache, a file named by its hash is immutable

- Status: accepted, 2026-10-05. Fixes what proposal P5 and record 0181 left to the change
  that serves the app: its path, how its files reach the binary, and its validators and cache
  rules.
- Decided by: the owner for the embedding itself (proposal P5, `docs/proposals/P5-app-stack.md`,
  "Embedding" and "Common to every option", approved at Checkpoint K; record
  `0181-the-web-app-stack.md` commits the output). The path, the entity tag, the cache rules
  and the closed table of media types are this record's.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/server/build.rs`; `crates/server/src/app.rs`; the `/app` arm of the
  route table and the `If-None-Match` header in `crates/server/src/control.rs`;
  `crates/server/tests/app_serving.rs`; `deploy/Dockerfile` (the build context carries
  `web/dist`); `docs/control-plane.md` ("The app under `/app/`").

## Context

`web/dist` is the app's build output, committed and held to `web/src` by the gate step
`web-build` (record 0181). Until this change nothing served it. Four things bind how it is
served:

- `chorus-server` is built with no JavaScript toolchain, in the workspace, in the image and in
  CI, and must stay so (P5, "Embedding").
- The control page at `/` is not moved or changed by this work, and its
  Content-Security-Policy (`default-src 'none'; script-src 'self'; style-src 'self'; ...`) is
  not loosened.
- The app becomes an installable app with a service worker (a later change). An installed
  app's start URL, its manifest's scope and its service worker's scope are all paths, and a
  browser keeps them: the path chosen now cannot be changed later without stranding installs.
- P5 fixed the cache rules a service worker needs: `sw.js` and `index.html` `no-cache`,
  hashed assets immutable.

## Decision

1. **`/app/` is the app's permanent path.** `GET /app/` and `GET /app/index.html` are the
   document; `GET /app/<path>` is the file of `web/dist` at that path; `GET /app` answers `308`
   to `/app/`, because the document names its assets with relative links; any other path under
   `/app/` is the server's ordinary `404`. Only `GET` reads it. The path does not move when the
   app later replaces the control page: `/` may then lead to it, and `/app/` stays.
2. **The files are compiled in by `crates/server/build.rs`, which uses the standard library
   alone and starts no program.** It walks `web/dist`, sorts the paths, and writes one table
   entry per file with `include_bytes!`; `src/app.rs` includes the table and answers from it.
   There is no route per file and no list of files anywhere in the Rust source, so a file the
   app's build adds is served by the next build of the server. Cargo is told to run the script
   again when any directory or file under `web/dist` changes. Nothing is opened at run time,
   and a request's path is only compared with the table's, so no path can reach outside it.
   `crates/server/Cargo.toml` gains no build dependency.
3. **A media type is never guessed.** The script holds a closed table from extension to
   `Content-Type`, and an extension it does not name fails the build with the line to add.
   Under `script-src 'self'; style-src 'self'` a script or stylesheet with the wrong type is
   silently not applied; a failed build is the cheaper place to find that. Responses carry
   `X-Content-Type-Options: nosniff`.
4. **Every file has a strong `ETag`: the 64-bit FNV-1a hash of its bytes and its length, in
   hexadecimal, computed at build time.** A request whose `If-None-Match` is `*` or names the
   tag (a `W/` prefix ignored, RFC 9110 section 13.1.2) is answered `304` with no body, the
   tag and the cache rule. The tag is a validator, not a signature: it has to change when the
   committed bytes change, and nobody chooses those bytes against it. The standard library has
   no cryptographic hash, and decision 2 rules out a dependency to get one.
5. **Two cache rules, chosen by the path.**
   - `public, max-age=31536000, immutable` for `assets/<name>-<hash>.<ext>` where the hash is
     eight characters of `A-Z` and `0-9`, the shape the pinned esbuild gives a file named by
     its content (record 0181, item 6). A change to such a file is a new name.
   - `no-cache` for everything else. The document `index.html` and `sw.js`, the path reserved
     under `/app/` for the service worker so that its default scope is the app, are named in
     the code and are `no-cache` whatever else changes. A file the rule does not recognise is
     `no-cache` too: revalidating a file that never changes costs one `304`, and caching for a
     year a file that does change cannot be undone from the server.
6. **The same Content-Security-Policy on every response under `/app`, unchanged.** The route
   passes the control page's constant; the test writes the policy out a second time and
   compares it byte for byte with what `/app/` and `/` carry.
7. **The image's build context carries `web/dist` and nothing else of `web/`.**
   `deploy/Dockerfile` copies the directory; the image test already stages the context from
   the file's `COPY` lines and compiles it, which is what catches a context that lacks it.

## Not chosen

- **Serving the app at `/` now.** The control page is there, and moving it is out of this
  change. Mounting the app at a path of its own lets both be served until the app replaces the
  page.
- **`include_str!` per file and a route per file, as the control page has.** Asset names carry
  a hash that changes with every edit of `web/src`; a hand-written route table would have to
  be edited with each one, and a forgotten edit is a page with no script.
- **Reading `web/dist` from disk at run time.** The image would carry a second artifact that
  can disagree with the binary, and a path from a request would reach the file system.
- **A cryptographic hash for the tag** (a vendored SHA-256, or a build dependency). Nothing
  here needs collision resistance against a chosen input, and either one is more code in the
  build than the thing it would protect.
- **A weak tag, or `Last-Modified`.** The build has no meaningful modification time (a
  checkout's is the checkout's), and a reproducible build must not read one.
- **`application/octet-stream` for an unknown extension.** See decision 3.
- **Answering an unknown path under `/app/` with the document** (a single-page fallback). The
  app has one document and no client-side routes by path; a `404` for a mistyped asset is
  worth more than a route nobody uses.
- **Compression.** Out of this change: about 21 kB is served today, on a local network, and
  the immutable rule means once.

## Consequences

- An extension the app has not shipped before needs one line in `MEDIA_TYPES`
  (`crates/server/build.rs`); the failed build names it.
- The service worker and the manifest are a later change. When `sw.js` appears in `web/dist`
  it is served `no-cache` with no change here, and `tests/app_serving.rs` holds the served
  header to that; the manifest's media type (`application/manifest+json`) is already in the
  table.
- A file outside `assets/`, or under it without a hash in its name, is revalidated on every
  use. That is the intended default, not an oversight to optimise.
- `HEAD` is not answered for the app, as for no other route of this listener.

## Sources

- `docs/proposals/P5-app-stack.md`, "Embedding", "The existing Content-Security-Policy" and
  the service worker's cache rules under "Common to every option"; read 2026-10-05.
- `docs/decisions/0181-the-web-app-stack.md`, items on the committed output and the asset
  names; `web/build.mjs` (`entryNames: "assets/[name]-[hash]"`); read 2026-10-05.
- RFC 9110, HTTP Semantics, sections 8.8.3 (`ETag`), 13.1.2 (`If-None-Match`, weak comparison)
  and 15.4.5 (`304`), 15.4.9 (`308`); RFC 9111 section 5.2.2.4 (`no-cache`); RFC 8246
  (`immutable`). Cited from the published texts; this record adds no measurement.
- Checked here, 2026-10-05: `cargo test -p chorus-server --test app_serving` (4 tests) and
  `--test serving` (11 tests) pass; with a file added to `web/dist` the same test passes after
  the rebuild cargo starts by itself, its assertion that the table equals the directory
  included.

## What was read

- `docs/proposals/P5-app-stack.md` ("Embedding", "Common to every option", the
  re-verification of the planning research) and `docs/decisions/0181-the-web-app-stack.md`.
- `crates/server/src/control.rs` (the Content-Security-Policy constant, the request reader,
  the responders and the route table), `crates/server/tests/control_request_rules.rs` and
  `tests/serving.rs` (how the control listener is tested), `crates/server/Cargo.toml`.
- `web/README.md`, `web/build.mjs` and `web/dist` (the names and sizes of the output).
- `deploy/Dockerfile`, `deploy/README.md` and `tools/image.sh` (the build context and the
  test that compiles it); `docs/control-plane.md` ("How the messages travel", "What a request
  may be"); `docs/conventions.md` rule 15 and `tools/conventions/check-adrs.sh`.
- The RFCs under Sources, from their published texts. No GPL source and no reciprocally
  licensed design file was opened.
