# P10: The Home Assistant dashboard UI, and what MQTT carries beside the integration

- Decisions: K84, K46
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K
- If deferred: Stock cards; MQTT off by default
- Builds on: goal 15 (MQTT, "device discovery for what P10 says MQTT carries, against a fake broker"), goal 18 (the room and saved-group media players the dashboard shows), goal 19 ("the settled card or stock-card setup" and the homelab install PR)

## Question

Two questions from the brief's §5 table, row P10: what the Home Assistant dashboard for chorus
should be, and what MQTT carries beside chorus's own HA integration.

The decisions that bound them, quoted:

- K84: "**Research and propose**: goal 1 checks what HA's stock cards do with grouping in 2026 vs a
  custom chorus Lovelace card (rooms, drag-to-group, group volume, now playing, inputs) and
  proposes. Not chosen as final: custom card; standard cards only." Both ends stay open.
- K46: "zones as HA entities via MQTT discovery on homelab's Mosquitto ... (Refined by K56 and K61:
  MQTT carries only what P10 approves beside chorus's own HA integration.)"
- K61: "a media_player per room and per group (join/unjoin, volume, source, announce, browse,
  play_media) plus chorus-specific entities; installed as a custom integration (copy into HA's
  config, a Needs Owner step per release; HACS impossible while private) ... MQTT discovery (K46)
  carries what the integration does not, without duplicate entities."
- I14: "a media_player per room and per saved group; live groups appear as room members".
- K54 (chorus groups rooms), K77 (Sonos-style group volume: "the group slider scales every room
  relatively"), K16 and K86 (the PWA is the app, on phones, desktops and wall tablets).
- K40 and K82: the PWA sits behind Traefik `lan-only` plus the household login; "from outside,
  control goes through HA or the homelab's VPN". So HA is the off-LAN control surface.

## Constraints that bind every option

- §4.8: "The HA integration adds no unauthenticated endpoint to HA (a homelab rule; HA is public):
  every HTTP view it registers has `requires_auth=True` and every webhook is `local_only`, with a
  test that says so; homelab PR bodies state it."
- K40's noted consequence: anyone logged into HA can operate every enabled chorus entity. A
  dashboard card is browser code running with that user's HA session (the homelab's own words,
  below).
- K28: homelab changes are PRs chorus opens and never merges. The homelab rule for cards: "Native
  Sections, tile, heading and graph cards come first; a custom card is added only where native falls
  short ... Cards are browser code that runs with the user's HA session, so they are never installed
  from the HACS UI ... Each card is pinned in `www/cards/cards.lock` (repository, release, asset,
  sha256) and vendored into git" (homelab `docs/home-automation.md`, read-only clone at commit
  d82e2ae, read 2026-09-30). The homelab runs `lovelace: resource_mode: yaml`
  (`home-automation/homeassistant/config/configuration.yaml`, same clone).
- K9 and K26: private, OSS-quality; every dependency on the licence allowlist. Any browser code is
  built with the P5 toolchain, pinned, with no runtime import from a CDN.
- §4.2: the gate runs exactly one browser smoke test (the PWA's, P5). Any card code here is unit
  tested without a browser.
- K61 and I14 fix the entity model the dashboard shows (goal 18); P10 does not change it.
- Rule 8: fitness for chorus's requirements only.

## Re-verification of the planning research

The planning research (`research-ha-integration.md` §4 and §6, 2026-09-29) recommended "U1 [stock
cards] in the first HA goal; U2 [custom tile features] as a later, optional goal", with U2's
JavaScript "served by the integration" through `hass.http.async_register_static_paths` and
registered explicitly in the homelab's YAML resources; and for MQTT "Limit: deferred, off by
default, never beside the integration". Re-checked 2026-09-30 against primary sources:

1. **Release notes 2026.7, 2026.8, 2026.9 (new; the research read up to 2026.6).** Fetched the raw
   posts from `home-assistant/home-assistant.io` branch `current`. None adds a group-volume control,
   drag-to-group, or a view of which players are grouped. 2026.7: an Activity timeline, grouped
   Updates, a projector device class for media players. 2026.8: card-picker favourites; the WiiM
   integration "added multi-room grouping" (an integration, not a UI). 2026.9: three tile features
   (light effect, vacuum fan speed, target humidity), a fix so an **Inline** feature position shows
   every feature ("the first one sits next to the entity name ... the others appear below it in two
   columns"), and media-browser search reaching players with their own library. **Unchanged:** stock
   HA 2026.9 still has no group volume and no drag-to-group.
2. **Tile features (current docs).** `source/dashboards/features.markdown` lists for media players:
   `media-player-playback` (controls include `turn_on`, `turn_off`, play/pause/stop, previous/next,
   `volume_down`, `volume_up`, `volume_mute`, `shuffle`, `repeat`), `media-player-sound-mode`,
   `media-player-source` (with a `sources` filter list), `media-player-volume-buttons`,
   `media-player-volume-slider`; and `numeric-input` (slider or buttons) for any `number` entity.
   No grouping feature. Confirms the research.
3. **Grouping semantics (developer docs and core 2026.9.3).** The dev docs: `group_members` is "A
   dynamic list of player entities which are currently grouped together for synchronous playback. If
   the platform has a concept of defining a group leader, the leader should be the first element";
   `GROUPING` means "Entity can be grouped with other players for synchronous playback".
   `media_player/__init__.py` at tag 2026.9.3: `join` and `unjoin` are registered with
   `[MediaPlayerEntityFeature.GROUPING]` (lines 384-387, 469), and `state_attributes` publishes
   `group_members` only `if self.support_grouping` (lines 1138-1141). Confirms the research.
4. **The join dialog (frontend 20260826.7, the version HA 2026.9.3 pins in
   `frontend/manifest.json`).** `dialog-join-media-players.ts` lists entities that are
   `media_player`, have the **same platform** as the opened player, and support `GROUPING`; on submit
   it calls `join` with the ticked set, then `unjoin` on each unticked former member (lines 138-206).
   `more-info-media_player.ts` has a per-player volume slider and the join button, and no group
   volume. Confirms the research. Consequence for chorus: the dialog will list chorus rooms only
   (saved-group players carry no `GROUPING` in the I14 model), and never another brand's speakers
   (K55 holds by construction).
5. **HA's "media player group" helper (core 2026.9.3 `group/media_player.py`).** Its
   `async_set_volume_level` sends one `volume_set` with the same level to every member (lines
   419-430): that is "one shared level", which K77 did not choose. It is not a way to get K77's
   group volume. (New; not in the research.)
6. **Custom card delivery and authentication (new finding; changes the research's U2 delivery).**
   - HA's dev docs: files in `www` are "accessible without authentication via the UI at `/local`"
     (`registering-resources.md`).
   - Core 2026.9.3 `http/server.py`: `async_register_static_paths` registers an aiohttp static
     resource and a plain `GET` route (lines 367-397). Authentication is enforced only inside
     `HomeAssistantView` request handling (`helpers/http.py` line 62: `if view.requires_auth and not
     authenticated`); the auth middleware only marks the request (`http/auth.py` line 260). So a
     static path an integration registers is **served without authentication**.
   - The frontend loads a Lovelace resource by appending `<script type="module" src=...>`
     (`src/common/dom/load_resource.ts` at 20260826.7), which carries no bearer token, so a resource
     cannot sit behind HA's auth in the first place (signed `authSig` URLs exist in `http/auth.py`
     but the resource loader does not produce them; **ASSUMED** from reading the loader).
   - Result: the research's "the integration serves its JS with `async_register_static_paths`" adds
     an unauthenticated route to HA. It serves only a public script with no data and no control, the
     same class as HA's own `/local` and frontend files, but it contradicts §4.8's literal "adds no
     unauthenticated endpoint". Goal 19 item 2 repeats the research ("any browser code is served by
     the integration"). If any browser code is chosen, it should ship the homelab's way instead:
     vendored under `www/cards/chorus/` with a `cards.lock` line and served by HA's existing
     `/local`, so the integration registers no HTTP route at all. This needs goal 19 item 2's wording
     amended (the owner's call at Checkpoint K; P10 does not edit the brief).
   - The homelab's `scripts/ha-cards-update.sh` downloads assets with an anonymous
     `curl https://github.com/$repo/releases/download/...` (line 35). A private repo's release asset
     is not downloadable anonymously (**ASSUMED**, GitHub's usual behaviour for private repos), so
     for chorus the homelab PR would carry the vendored file itself plus the lock line whose sha256
     matches the release asset.
   - `frontend.add_extra_js_url` still exists (core 2026.9.3 `frontend/__init__.py` line 417, "This
     function allows custom integrations to register extra js or module url to load"). It loads the
     module on every page for every user without a line in the homelab repo; the research's "do not
     use it" stands.
7. **HA's own headers.** `http/headers.py` at 2026.9.3 adds `Referrer-Policy: no-referrer`,
   `X-Content-Type-Options: nosniff`, and `X-Frame-Options: SAMEORIGIN` when `use_x_frame_options`
   is on (default `True`, `http/__init__.py` line 102). No `Content-Security-Policy` is set anywhere
   in the `http` component files read. So a card runs with no CSP around it and full use of the
   logged-in session. (New; the research did not check.)
8. **Custom card API stability.** The developer blog's "Frontend component updates in 2026.8"
   (2026-07-31) says "Custom card authors can use Home Assistant frontend components, but internal
   Home Assistant UI APIs may change"; the 2026.5 post records removals (`ha-fab` removed, old switch
   tokens removed). A new component-update post appeared for 2026.5, 2026.6, 2026.7 and 2026.8. The
   documented card and card-feature contract (`setConfig`, `hass`, `getGridOptions`,
   `window.customCardFeatures` with `isSupported`) is small and documented; anything beyond it is
   informal.
9. **MQTT (re-fetched).** `mqtt/const.py` at 2026.9.3: `SUPPORTED_COMPONENTS` has 32 platforms and
   no `media_player` (confirms the research and `verify-ha-casting.md` claim 1). Homelab (clone
   d82e2ae): Mosquitto runs with `allow_anonymous=false`; its users are `homeassistant` and
   `frigate`; the integration matrix lists HA and Frigate (stopped, no camera) as its only clients.
   The homelab documents no MQTT client other than HA and Frigate; its MQTT plan has not yet
   enumerated LAN clients (homelab `docs/network.md` "MQTT plan", same clone), so a non-HA consumer
   is an open input, not a known absence. Confirms the research.

Net change from the research: the dashboard recommendation stands (stock cards), but its "U2 later"
has no later goal to land in (goals 20-27 carry no HA UI work), and U2's delivery as an integration
static path conflicts with §4.8; both are resolved below. The MQTT recommendation stands in
substance, made concrete enough for goal 15 to test.

HA core 2026.9.4 (released 2026-09-27) is the current patch release and pins the same frontend
(`home-assistant-frontend==20260826.7`); the 2026.9.3 findings above hold for it.

Adversarially verified 2026-09-30 (goal-1 verifier 3): 12 claims confirmed, 0 refuted, 2 partly right, 0 unverifiable; corrections applied; the recommendation stands.

## Options

### Dashboard

#### Option A: Stock cards only (a Sections view)

- What: one Sections view "Music" (or a section in the homelab's existing Home dashboard, which
  already has a "what is playing" area): per room, a tile for `media_player.<room>` with
  `media-player-playback`, `media-player-volume-slider` (with mute) and `media-player-source`
  filtered to that room's inputs; per room, a tile for a per-room `number.<room>_group_volume` (the
  planning research's entity model, `research-ha-integration.md` §2; goal 18 item 2 names group
  volume but not this entity, so goal 18 must build it as a chorus-specific entity under K61) with
  `numeric-input`, carrying a positive visibility condition so it shows only while the room is
  grouped (the homelab's own rule for conditional cards); per saved group ("Downstairs", "Whole
  house"), a tile for its media player, whose volume is the K77 group volume; a media control card
  for now playing with artwork (K65). Grouping through the stock join dialog (rooms only, as found
  above). The 2026.9 Inline feature fix lets a compact room tile carry volume inline and playback
  below.
- Costs: money none. Effort: part of goal 19; a dashboard YAML of a few hundred lines (**ASSUMED**
  size) in the homelab PR, a copy under `integrations/homeassistant/` as the documented example, and
  a pytest that every entity the YAML references exists in the integration's fixture entity set (no
  browser). Maintenance: YAML only; changes only when the entity model changes. Gate time:
  seconds.
- Risks: live-group volume is a separate tile rather than part of the room tile; no at-a-glance
  map of which rooms are grouped; renames of built-in feature types across HA releases (low; they
  are documented).
- Fit: meets K84's list except drag-to-group, which K54 and K16 give to the PWA; from outside the
  LAN (K82) every operation is still reachable (join and unjoin through the dialog, room volume, live
  and saved group volume, inputs, now playing). Zero browser code, so §4.8 holds literally and the
  homelab card rule is met.

#### Option B: Stock cards plus one or two custom tile features

- What: Option A plus `custom:chorus-group-volume` (the live group's relative volume slider and its
  member rooms as chips, on the room tile) and possibly `custom:chorus-group-chips`, registered via
  `window.customCardFeatures`. The code reads only `hass.states` and calls HA services
  (`number.set_value`, `media_player.join`/`unjoin`); it never talks to chorus-server.
- Costs: effort about one third of a goal (**ASSUMED**) inside goal 19: a small module built with the
  P5 stack, vitest plus happy-dom unit tests (no second browser test, §4.2), a release asset, a
  homelab `cards.lock` line and vendored file per release, and the YAML resource with `?v=`.
  Maintenance: re-test on every HA frontend release the homelab adopts. Delivery must be the
  homelab's `www/cards` path (see re-verification item 6), which needs goal 19 item 2 amended.
- Risks: informal frontend APIs change (item 8); a card runs with the user's full HA session and no
  CSP (item 7), so a supply-chain slip in its build reaches every HA user; it duplicates what Option A
  already shows as a separate tile.
- Fit: improves presentation of K77's live-group volume in HA; adds no function A lacks. Meets the
  homelab's "only where native falls short" rule narrowly (native shows group volume, only less
  neatly).

#### Option C: A full custom chorus Lovelace card

- What: `chorus-rooms-card` with rooms, drag-to-group, group volume, now playing and inputs, as K84
  describes, talking to HA only (entity states and service calls).
- Costs: roughly a goal of its own (**ASSUMED**: the card re-implements much of the PWA's core
  screens, goal 21); no goal slot exists for it, so goal 19 would grow past a day (K3's cap).
  Drag-and-drop can be unit tested only with synthetic events (the one browser test belongs to the
  PWA). Same delivery, pinning and amendment needs as B, at larger size.
- Risks: largest exposure to frontend churn and to the session-power problem; two UIs (PWA and card)
  to keep consistent with one entity model; drag-to-group through HA means a burst of `join`/`unjoin`
  calls, which the goal-18 integration must coalesce.
- Fit: closest to Sonos-app parity inside HA. But K16 makes the PWA the app, K54 puts grouping with
  chorus, and off-LAN users have the VPN route to the full PWA (K82). The homelab rule argues
  against it.

#### Option D: Embed the PWA in HA (iframe card or panel)

- What: a webpage card or iframe panel pointing at the PWA.
- Costs: small YAML.
- Risks: does not work. chorus-server's CSP sends `frame-ancestors 'none'`
  (`crates/server/src/control.rs`, `CONTENT_SECURITY_POLICY`, read 2026-09-30), so the browser refuses
  to frame the page; the PWA is `lan-only` behind the household login (K40) while HA is reachable
  from outside, so off-LAN (the case K82 routes through HA) it cannot load; if HA is served over
  HTTPS and the PWA is reached over HTTP, mixed content is blocked (**ASSUMED**, depends on the
  homelab's routing).
- Fit: none. Listed to close it.

A variant of B and C, **serving the card from chorus-server** instead of HA, is declined for the same
reasons as D plus two: module scripts "require the use of the CORS protocol for cross-origin
fetching" (MDN `<script>` reference), so chorus-server would have to send
`Access-Control-Allow-Origin` for HA's origin; and a CORS request made without a `crossorigin`
attribute, as HA's loader makes it, sends no cookies cross-origin (MDN: "There is no exchange of user
credentials via cookies ... unless destination is the same origin" for `anonymous`; that the module
default matches it is **ASSUMED** from the HTML spec), so Traefik's login would answer with a
redirect and the card would never load. A card that called chorus-server's API directly would also
bypass HA's login onto an unauthenticated control API (K40). Any chorus card talks to HA only.

### MQTT

#### Option M1: No MQTT (not built)

- What: P10 approves nothing for MQTT; goal 15 records the decision instead of building a client.
- Costs: zero code. Needs goal 15's line C ("MQTT discovery publishes what P10 settled against a fake
  broker") amended to a decision record. Gives up K46's "interface for tools outside HA".
- Risks: none technical. An outside user (K9) without HA has only the HTTP + SSE API and chorusctl.
- Fit: consistent with K61; narrows K46 to zero.

#### Option M2: Off by default; opt-in read-only state and event topics, no HA discovery

- What: when the owner enables it in chorus-server's configuration, chorus publishes to the broker
  with its own credentials: retained room and saved-group state (playing, input, volume, group
  membership, now-playing title and artist; no artwork bytes), non-retained speaker button events,
  and a server online/offline last-will topic, under one versioned prefix (for example
  `chorus/v1/...`, **proposed**). Payloads reuse the control API's state message and its
  `fixtures/control` so there is one schema. **No `homeassistant/...` discovery topics and no command
  topics**: MQTT adds no second control path and no entity in HA.
- Costs: goal 15 builds a small MQTT 3.1.1 publisher (hand-written or a permissive crate, a per-item
  call BRIEF §5.8 already allows), tested against a fake broker; a homelab PR adds a Mosquitto user
  and a publish-only ACL on the prefix, and the password is an owner step (Needs item) kept outside
  git. Maintenance small.
- Risks: a publisher with no known consumer in the house today (item 9: the homelab documents only
  HA and Frigate as MQTT clients, and its LAN clients are not yet enumerated); a broker credential
  to manage.
- Fit: K61's "no duplicate entities" holds by construction (nothing is discovered); K46's
  outside-HA interface is kept at low cost; exactly the If-deferred "MQTT off by default".

#### Option M3: Discovery in a "no integration" mode

- What: M2 plus HA discovery for installs that run no chorus integration (sensors, numbers, switches,
  events, update entities; never a media player, since MQTT has none), refused while the integration
  is connected (the integration raises a repair issue if it sees discovery mode on).
- Costs: a second entity schema for about ten platforms in Rust beside the Python integration's,
  kept in step; more tests; more ACL.
- Risks: duplicate entities if the exclusion fails; a weaker HA experience than the integration
  (no media player at all) offered as if it were one.
- Fit: serves an outside user who will not install a custom integration; the homelab has no such
  need. Poor value.

#### Option M4: Full discovery beside the integration

- Declined by K61's "without duplicate entities": every entity MQTT could describe is already an
  integration entity (goal 18 and 19), and MQTT cannot describe the media players at all.

## Comparison

Dashboard:

| Criterion | A stock | B stock + tile features | C full card | D embed PWA |
|---|---|---|---|---|
| K84 list covered | all but drag-to-group | all but drag-to-group | all | all, but only on the LAN |
| Off-LAN control (K82) | yes | yes | yes | no |
| Browser code to pin and test | none | small | large | none |
| §4.8 as written | holds | holds only via homelab `www/cards` (goal 19 item 2 amended) | same as B | holds |
| Exposure to frontend churn | documented features only | informal APIs, small surface | informal APIs, large surface | none |
| Effort | part of goal 19 | about a third of a goal extra (ASSUMED) | about a goal extra (ASSUMED) | trivial, but broken |
| Homelab card rule | met | narrowly | against | n/a |

MQTT:

| Criterion | M1 none | M2 opt-in read-only | M3 no-integration discovery | M4 full discovery |
|---|---|---|---|---|
| Duplicate entities (K61) | none | none | none if exclusion works | yes |
| Second control path | none | none | none (state only) or some | yes |
| Serves non-HA tools (K46) | no | yes | yes | yes |
| Code and ACL | none | small | medium | medium |
| Goal 15 line C | needs amending | testable as written | testable | declined |

## Recommendation

**Recommendation:** Stock cards only (Option A) in goal 19, no custom card or card features; MQTT off by default with an opt-in, read-only state and event publisher and no HA discovery (Option M2).

Stock HA 2026.9 already covers every K84 item except drag-to-group, and the re-check of 2026.7 to
2026.9 found nothing that closes that gap, while K54 and K16 put drag-to-group in the PWA, which
off-LAN users reach through the VPN (K82). A costs no browser code, keeps §4.8 true without any
exception, and follows the homelab's card rule. The owner gives up a single room tile that also
carries the live group's volume and member chips (Option B) and a Sonos-like card inside HA
(Option C). If the owner later wants B, it ships through the homelab's `www/cards` path rather than an
integration static path, and goal 19 item 2's "served by the integration" is amended then. For MQTT,
M2 keeps K46's outside-HA interface at small cost with no duplicate entities and no second control
path; the owner gives up HA discovery for installs without the integration (M3), which no one in the
house needs. If the owner prefers zero MQTT code, M1 is the clean alternative and costs only
amending goal 15's line C.

## If the owner defers

Later goals build on "Stock cards; MQTT off by default", which matches the recommendation's
dashboard half exactly. For MQTT, "off by default" leaves open what goal 15 builds behind the
switch; goal 15 then builds M2 (the smallest thing that is "off by default" and satisfies its line C
without duplicate entities) and records that reading in `docs/decisions/`. Cost of deferring: none
beyond the recommendation's own.

## Open inputs

- The owner's choice at Checkpoint K between M2 and M1 (whether any MQTT code is wanted at all).
- A requirement this proposal places on goal 18: build a per-room `number.<room>_group_volume`
  (0-100, `available` only while the room is grouped, per `research-ha-integration.md` §2) as a
  chorus-specific entity under K61. Goal 18 item 2 names group volume but not this entity; Option
  A's per-room group-volume tile depends on it.
- Whether any non-HA MQTT consumer exists in the house: the homelab documents only HA and Frigate
  as Mosquitto clients, and its MQTT plan ("Find the LAN clients") has not yet enumerated LAN
  clients. M2 is opt-in, so the answer does not change the recommendation.
- If Option B or C is chosen: an amendment to goal 19 item 2 ("any browser code is served by the
  integration") to "vendored in the homelab's `www/cards` with a lock line", so §4.8 stays literal.
- If M2: a Mosquitto user and publish-only ACL for chorus (a homelab PR in goal 15) and its password
  (an owner step, a Needs item filed by goal 15).
- ASSUMED values: the dashboard YAML size; the effort fractions for B and C; that the resource
  loader never produces signed URLs; that private release assets cannot be fetched anonymously; that
  module scripts default to same-origin credentials; whether the homelab serves HA over HTTPS (affects
  D's mixed-content point only).
- Whether HA gains a group-volume or grouping-map feature in a later release: re-check at goal 19
  (it would only strengthen A).

## Sources

- Home Assistant 2026.7 release notes (raw), https://raw.githubusercontent.com/home-assistant/home-assistant.io/current/source/_posts/2026-07-01-release-20267.markdown, read 2026-09-30
- Home Assistant 2026.8 release notes (raw), https://raw.githubusercontent.com/home-assistant/home-assistant.io/current/source/_posts/2026-08-05-release-20268.markdown, read 2026-09-30
- Home Assistant 2026.9 release notes (raw), https://raw.githubusercontent.com/home-assistant/home-assistant.io/current/source/_posts/2026-09-02-release-20269.markdown, read 2026-09-30
- Home Assistant 2026.6 release notes (raw, media player tile features), https://raw.githubusercontent.com/home-assistant/home-assistant.io/current/source/_posts/2026-06-03-release-20266.markdown, read 2026-09-30
- Dashboard features documentation (raw), https://raw.githubusercontent.com/home-assistant/home-assistant.io/current/source/dashboards/features.markdown, read 2026-09-30
- Media player entity developer docs (raw), https://raw.githubusercontent.com/home-assistant/developers.home-assistant/master/docs/core/entity/media-player.md, read 2026-09-30
- HA core 2026.9.3 `media_player/__init__.py` (Apache-2.0), https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/media_player/__init__.py, read 2026-09-30
- HA core 2026.9.3 `group/media_player.py`, https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/group/media_player.py, read 2026-09-30
- HA core 2026.9.3 `frontend/manifest.json` (pins `home-assistant-frontend==20260826.7`) and `frontend/__init__.py` (`add_extra_js_url`), https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/frontend/manifest.json and .../frontend/__init__.py, read 2026-09-30
- HA core 2026.9.3 `http/server.py`, `http/static.py`, `http/auth.py`, `http/headers.py`, `http/__init__.py`, `http/config.py`, `http/const.py`, and `helpers/http.py`, https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/http/ and https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/helpers/http.py, read 2026-09-30
- HA core 2026.9.3 `mqtt/const.py`, https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/mqtt/const.py, read 2026-09-30
- HA core 2026.9.4 `frontend/manifest.json` (pins `home-assistant-frontend==20260826.7`), https://raw.githubusercontent.com/home-assistant/core/2026.9.4/homeassistant/components/frontend/manifest.json, and the latest core release (2026.9.4, published 2026-09-27), https://api.github.com/repos/home-assistant/core/releases/latest, read 2026-09-30
- NSchatz/homelab `docs/network.md` ("MQTT plan") and `docs/home-automation.md` (Mosquitto users), `origin/main` `d82e2ae`, read 2026-09-30
- HA frontend 20260826.7 (Apache-2.0, `LICENSE.md` checked): `src/components/media-player/dialog-join-media-players.ts`, `src/dialogs/more-info/controls/more-info-media_player.ts`, `src/common/dom/load_resource.ts`, `src/panels/lovelace/common/load-resources.ts`, https://raw.githubusercontent.com/home-assistant/frontend/20260826.7/, read 2026-09-30
- Registering resources (dev docs, raw), https://raw.githubusercontent.com/home-assistant/developers.home-assistant/master/docs/frontend/custom-ui/registering-resources.md, read 2026-09-30
- Custom card feature (dev docs, raw), https://raw.githubusercontent.com/home-assistant/developers.home-assistant/master/docs/frontend/custom-ui/custom-card-feature.md, read 2026-09-30
- Custom card (dev docs, raw), https://raw.githubusercontent.com/home-assistant/developers.home-assistant/master/docs/frontend/custom-ui/custom-card.md, read 2026-09-30
- Frontend component updates in 2026.5 and 2026.8 (dev blog, raw), https://raw.githubusercontent.com/home-assistant/developers.home-assistant/master/blog/2026-05-04-frontend-component-updates-2026.5.md and .../2026-07-31-frontend-component-updates-2026.8.md, read 2026-09-30
- MDN, `<script>` element (module scripts and CORS), https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/script, read 2026-09-30
- MDN, the `crossorigin` attribute, https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Attributes/crossorigin, read 2026-09-30
- HACS private repositories FAQ, https://www.hacs.xyz/docs/faq/private_repositories/ (via `verify-ha-casting.md` claim 2, read 2026-09-29; not re-fetched)

## What was read

- The program brief `/workspace/.claude/goals/2026-09-chorus.md`: §0.8, §0.9, §0.11, §1 (K9, K16,
  K18, K26, K27, K28, K30, K33, K40, K42, K43, K46, K54, K56, K59, K60, K61, K62, K64, K65, K77,
  K78, K81, K82, K83, K84, K85, K86, K87, K92, K93, K95), §1.2 (I1-I20), §3.3, §4, §5, §19 (goal 15),
  §22 (goal 18), §23 (goal 19), §25 and §26 (goals 21, 22).
- Planning research: `research-ha-integration.md` (§0, §1.4, §2, §4-§9), `verify-ha-casting.md`,
  `research-pwa-conventions.md` §1, `research-toolchain-env.md` §2 and §5,
  `verify-toolchain-conventions.md`.
- chorus baseline (`/cache/wt/chorus/chorus/baseline`, commit 4ecc782): BRIEF.md §5.8,
  `crates/server/src/control.rs` (lines 185-240, 500-610: the CSP and the UI routes),
  `crates/server/src/ui/index.html`, `docs/control-page.md` (lines 1-80).
- Homelab read-only clone (`/cache/tmp/plan-2026-09-chorus/homelab-ro`, commit d82e2ae):
  `docs/home-automation.md` (dashboards, custom cards, integration matrix, "What can change
  things"), `home-automation/homeassistant/config/configuration.yaml` (lovelace block),
  `home-automation/homeassistant/www/cards/cards.lock` (header), `scripts/ha-cards-update.sh`
  (fetch lines), `home-automation/mqtt/docker-compose.yml` (listener and anonymous settings),
  `docs/network.md` ("MQTT plan", per the goal-1 verifier 3 report).
- `/cache/tmp/chorus-g1/verify/verify-3.md` (P10 section), read 2026-09-30.
- Every URL in Sources. GitHub API listings of `home-assistant.io/source/_posts`,
  `developers.home-assistant/blog`, `developers.home-assistant/docs/frontend/custom-ui`, the
  frontend tag list, and the frontend 20260826.7 tree.
- No GPL source was opened. Only Apache-2.0 files (HA core tag 2026.9.3, HA frontend tag
  20260826.7) and documentation were read.
