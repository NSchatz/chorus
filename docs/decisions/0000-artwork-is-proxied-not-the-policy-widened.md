# 0000: now-playing artwork is fetched by chorus-server and served from its own origin by group, under the fetch policy, a size bound and a deadline; the Content-Security-Policy is not widened

- Status: accepted, 2026-10-05. Decides how a page of this server shows the artwork of a
  group's now-playing record.
- Decided by: the task that asked for it (its goal: artwork "under the unchanged
  Content-Security-Policy", fetched from "only the URL already in that group's now-playing
  record"). The route's shape, its bounds, the closed set of image kinds, the ceiling on
  fetches at once and the validator are this record's. Every number below is ASSUMED, not
  measured.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/server/src/artwork.rs` (new); the `/api/artwork` arm of the route
  table, `serve_artwork`, `ControlState::artwork_through` and the ceiling set in
  `ControlPlane::spawn_workers`, all in `crates/server/src/control.rs`; `crates/server/src/main.rs`
  (the fetcher is given the server's fetch policy); `crates/server/tests/artwork.rs`;
  `audio-path.conf` (the module's exclusion and its reason); `docs/control-plane.md`
  ("Now-playing artwork").

## Context

A group that plays a player source has a now-playing record (record 0119), and the record can
carry an artwork URL: `http` or `https`, at most 2048 bytes, no control character and no space
(`chorus_control::rooms::NowPlaying::bounded`). The server's runtime sets it and no command
does. The URL is somebody else's address: a UPnP control point's media server on the LAN, a
station's site, a streaming service's image host.

Every page this server hands a browser carries one Content-Security-Policy, whose image rule
is `img-src 'self' data:` (`crates/server/src/control.rs`, `CONTENT_SECURITY_POLICY`; record
0182 holds the app under `/app/` to the same constant). Under it a browser does not load an
image from the record's URL. So the app cannot show artwork until one of two things changes:
the policy, or where the image comes from.

Three more facts bind the answer:

- The control plane has no authentication (record 0027). Whatever a route does, anybody who
  can reach the port can ask it to.
- The brief's rule on fetching (section 4.8): "No arbitrary URL fetch except the input paths
  the decisions name". The media fetcher (`chorus-fetch`, record 0120) is how the server
  fetches a URL it was given, under a policy over the resolved address.
- The control plane's threads are a fixed pool made before the scheduling report
  (`crates/server/src/control.rs`, the module's first section). Nothing may add a thread per
  request, and a request that waits on the network holds a worker while it waits.

## Decision

1. **The policy is not changed.** `CONTENT_SECURITY_POLICY` stays byte for byte what it was.
   The test writes it out a second time and compares it with what the artwork answer carries.
2. **`GET /api/artwork?group=<id>` answers with the image at the artwork URL of that group's
   now-playing record.** The request names a group and nothing else is read from it: no
   parameter, header or path segment is ever fetched. `400` when no group is named; `404`, in
   one wording, when the group has no record, the record has no artwork, or the server has no
   such group. This is not a proxy that takes a URL, and it cannot be turned into one from
   outside: the only way to choose what it fetches is to be the player's driver.
3. **The fetch is the media fetcher's, under the server's own fetch policy.**
   `chorus_fetch::open_cancellable` with the policy `main.rs` builds for the players (never
   this machine's loopback, never one of this server's own ports, each redirect resolved and
   checked like the first address, the same CA bundle). For this route the policy's times are
   tightened: 3 s per connect, at most 3 redirects. A record whose artwork URL names the
   control port itself is refused with the fetcher's words, and the test shows it.
4. **A size bound and a deadline.** At most 4 MiB is read; a declared length above that is
   refused before the body is touched, and an undeclared body is cut off at the first byte
   past it. The whole fetch has 8 s on the monotonic clock (`Instant`): the fetcher's cancel
   hook gives up a wait on a connected socket within 100 ms of the deadline and is asked
   before every connect, and the read loop looks at the deadline between reads, so an origin
   that drips a byte at a time is held to it. What the hook cannot leave early is a connect
   in progress and name resolution (record 0120, "Not bounded here"): a fetch can outlive
   the deadline by one connect timeout, and a dead resolver holds it for the resolver's own
   time.
5. **Only an image is passed on, judged by its first bytes.** JPEG, PNG, GIF and WebP, each by
   its signature; the answer's `Content-Type` is the one the bytes say, with
   `X-Content-Type-Options: nosniff`. The upstream `Content-Type` is not believed either way:
   an origin that says `application/octet-stream` for a JPEG is served, and one that says
   `image/png` for a page is refused. SVG is refused on purpose: it is a document that can
   carry script, and it would be served from the control plane's own origin. Something that
   is not an image is refused as soon as its first 12 bytes are in, and none of it is sent to
   the browser. A refusal is `502` with the reason; the deadline is `504`.
6. **A control worker runs the fetch, and at most half the pool at once.** No thread is
   added. A fetch holds its worker until it ends, so the number running at once is held to
   half of `--control-workers` (one at least); one more is answered `503` and nothing waits.
   Commands and state always have the other half, whatever an artwork's origin does.
7. **Nothing is kept.** No cache in memory and none on disk: every answer is a fetch. The
   answer carries a strong `ETag` (the 64-bit FNV-1a hash of the bytes and their length, the
   shape record 0182 gives a file of the app) and `Cache-Control: no-cache`, so a browser
   keeps the image and asks before every use; `If-None-Match` naming the tag is answered
   `304`. The tag is made from the bytes served, not from the URL: a new track's artwork has
   other bytes and so another tag, and an origin that keeps one URL for every track's cover is
   still never answered `304` for the wrong picture.
8. **Not on the audio path.** `crates/server/src/artwork.rs` is excluded in `audio-path.conf`
   with its reason: it is called by a control worker (an ordinary thread) and by nothing
   else, touches no PCM and no stamp, and reads `Instant` only.

## Not chosen

- **Widen the policy: `img-src 'self' data: http: https:`.** One line, and no server code.
  Against it: (a) it gives up the property the policy's own comment states, that nothing
  loads from anywhere but this origin, for every page and for good, to show one picture;
  (b) every browser showing a room then contacts the artwork's host itself, so a third party
  learns the address of each phone in the house and when it looks at the app; (c) a page
  reached over https cannot load the `http://` artwork a control point on the LAN serves
  (browsers block or upgrade mixed content; ASSUMED from the platform's documented behaviour,
  not re-read for this record), and that is the commonest artwork there is; (d) an image
  request to any host is a way to carry data out of a page, which `img-src` exists to stop.
- **Widen the policy to named origins.** The origins are not known ahead: a control point's
  media server is at whatever address it has today.
- **A proxy that takes the URL** (`/api/artwork?url=...`). Simplest for the app, which already
  has the URL in the state message. Against it: with no authentication it is an open relay
  for anybody on the network, exactly the "arbitrary URL fetch" the brief rules out. The
  fetch policy would still refuse loopback and the server's own ports, but private ranges
  are allowed on purpose (record 0120), so it would read any device on the LAN for whoever
  asked.
- **The image inside the state message, as a `data:` URL.** The policy already allows `data:`.
  Against it: megabytes in a message that goes to every subscriber on every change, past the
  fanout's bounds, and a change to the now-playing record, which is out of this task's scope.
- **A cache of the last image per group.** It would save the fetch on a revalidation and when
  several phones show one room. Left out because it has to answer "is this still the
  artwork" for an origin that reuses one URL, and because it would hold megabytes for the
  server's whole life. The ceiling of decision 6 bounds what several phones at once cost;
  a cache can be added behind the same route if use shows the fetches matter.
- **A thread, or a pool, for artwork.** The thread population is fixed and graded
  (`crates/server/tests/control_thread_population.rs`); the workers are there and decision 6
  keeps them from being used up.
- **Trusting the upstream `Content-Type`.** It is the origin's claim. The bytes decide.

## Consequences

- The app can show artwork with `<img src="/api/artwork?group=<id>">` and nothing else. The
  browser asks again when the element is made again; a later change in the app decides when
  that is (when the record's title or artwork URL changes, for example). That is the app's
  task, not this one's.
- Each artwork answer is one outbound fetch by the server. Its address is what an artwork
  host sees, never a phone's.
- A worker is held for up to the deadline (and one connect timeout) by an origin that does
  not answer; at most half the pool, by decision 6.
- A fetch holds at most 4 MiB, on an ordinary thread, for the length of one answer.
- An origin that serves artwork in another format (AVIF, BMP, SVG) shows no artwork; the
  refusal says so. Adding a kind is one line in `artwork::media_type_of` and its test.
- An origin that needs a header or a cookie to serve its artwork shows none: the fetcher
  sends neither.

## What was read

All on 2026-10-05, all in this repository; no outside source was opened for this record, and
what it says about browsers (mixed content, what `img-src` governs) and about the four image
signatures is ASSUMED from common description, as marked.

- `crates/server/src/control.rs` (the policy constant and its comment, the route table, the
  worker pool, the request bounds), `crates/server/src/app.rs` and record 0182 (the entity
  tag, `If-None-Match`, the cache rules), `crates/server/src/mediaplayer.rs` (`fetch_policy`,
  how a player opens a URL with a cancel hook), `crates/server/src/main.rs` (where the policy
  is built).
- `crates/fetch/src/lib.rs`, `policy.rs`, `http.rs` and `error.rs`; record 0120.
- `crates/control/src/rooms.rs` (`NowPlaying`, `bounded`); record 0119; `docs/control-plane.md`
  (the now-playing record, "How the messages travel"); `docs/inputs.md`, "The security rule";
  record 0027.
- `audio-path.conf` (its rule and the server's exclusions).
