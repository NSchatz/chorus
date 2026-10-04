# 0163: the Home Assistant dashboard is one Sections view of stock cards and tile features, kept in chorus as an example whose entities, types and lack of any link are held by a test, with no custom card and nothing served to the frontend

- Status: accepted, 2026-10-04. Builds the dashboard half of proposal P10
  (`docs/proposals/P10-ha-dashboard-mqtt.md`, Option A); corrects one line of 0138 (the Home
  Assistant integration). The MQTT half of P10 is 0116.
- Decided by: the owner for the choice (K84; P10's recommendation "Stock cards only (Option A)
  ... no custom card or card features", approved at Checkpoint K, as 0116 records); this
  record for the layout, which cards carry what, the visibility conditions, where the file
  lives and what its test holds.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `integrations/homeassistant/dashboard/chorus.yaml`,
  `integrations/homeassistant/tests/test_dashboard.py`, `docs/home-assistant.md` ("The
  dashboard")

## Context

P10 asked what the Home Assistant dashboard for chorus should be and recommended stock cards
only; the owner approved the recommendation at Checkpoint K. P10 left one input open:
"Whether HA gains a group-volume or grouping-map feature in a later release: re-check at goal
19 (it would only strengthen A)". The entities the dashboard shows exist now: a media player
per room and per saved group and the per-room group-volume number (0138), and the sound
controls (0152).

0138 ended with "Left to goal 20: the dashboard and anything served to the frontend, which is
where the first HTTP view or static path would appear and where the endpoint test's allowlist
would first be used". That line was written before the task that builds the dashboard and
does not match P10: the dashboard serves nothing to the frontend.

## What was read

All on 2026-10-04. No GPL source was opened; Home Assistant's documentation and the names of
the files of one directory of its frontend (Apache-2.0) were read.

- The re-check P10 asks for. The latest stable core release is still 2026.9.4 (published
  2026-09-27) and 2026.10.0b0 is a pre-release (published 2026-09-30) that pins
  `home-assistant-frontend==20260930.0`:
  https://api.github.com/repos/home-assistant/core/releases and
  https://raw.githubusercontent.com/home-assistant/core/2026.10.0b0/homeassistant/components/frontend/manifest.json.
  The 2026.10 release notes as drafted on the documentation's `rc` branch
  (https://raw.githubusercontent.com/home-assistant/home-assistant.io/rc/source/_posts/2026-10-07-release-202610.markdown)
  were searched for group, volume, join, media player, tile and card features: for
  dashboards they add a slider tile feature for select entities, two tile features for
  timers and more visibility conditions (template, sun, zone, device); for media players
  nothing on the dashboard. The frontend's card features at tag 20260930.0
  (https://api.github.com/repos/home-assistant/frontend/contents/src/panels/lovelace/card-features?ref=20260930.0)
  still list five for media players: playback, sound mode, source, volume buttons and volume
  slider. **Unchanged:** Home Assistant has no group-volume control, no drag-to-group and no
  view of which players are grouped, in 2026.9 or in the 2026.10 beta. Option A stands as
  approved; nothing here depends on 2026.10.
- The tile features and their options, the tile card's `features_position` (`inline`: "the
  first feature is displayed next to the name"), the media control card, the heading card
  and the `state` and `numeric_state` visibility conditions:
  https://raw.githubusercontent.com/home-assistant/home-assistant.io/current/source/dashboards/features.markdown,
  and under `.../current/source/_dashboards/`: `tile.markdown`, `media-control.markdown`,
  `heading.markdown`, `conditional.markdown`.
- The house style the copy in the owner's homelab repository must fit: its Home dashboard
  and the dashboards part of its `docs/home-automation.md` (read-only clone at commit
  f0284f2). Sections views in YAML mode; native cards first; a card that is not always
  shown names the states in which it shows or carries a numeric test, never `state_not`,
  because a missing entity is not "unavailable" to the frontend.
- chorus's own: P10, 0116, 0138, 0152, `docs/home-assistant.md`, the integration's tests and
  `fixtures/control/v2/state-rich.json`.

## Decision

**One file, one view.** `integrations/homeassistant/dashboard/chorus.yaml` is a whole
dashboard (`title`, `views`) with one Sections view, "Music", of at most three columns. It is
the documented example; the copy in the owner's homelab repository is taken from it by a
later task and is not part of this one.

**What it shows.**

- A "Groups" section: a tile per saved group's media player with three features,
  `media-player-playback`, `media-player-volume-slider` (which carries mute) and
  `media-player-source`. A saved group's volume is the K77 group volume.
- A "Now playing" section: a media control card per room, which shows the title, the artist
  and the artwork. Each is visible only while its player is `playing`, `paused` or
  `buffering`, which the integration reports only when the room's group has a now-playing
  record; a room that plays the server's stream or a line-in is `on` and has no card here.
- A section per room: the room's tile with the same three features; a tile for the room's
  group-volume number with `numeric-input` as a slider, visible only while the number is a
  number (`numeric_state`, `above: -1`); bass and treble as `numeric-input` buttons; loudness,
  night mode, speech enhancement and quiet hours as tiles with an inline `toggle`.

**The group-volume tile shows only while the room is grouped.** The number is unavailable
while the room is alone (0138); `unavailable` is not a number, so a numeric test hides the
tile, and hides it also when the entity does not exist. That is the homelab's rule for
conditional cards, applied. `test_dashboard_group_volume_tile_shows_only_while_the_room_is_grouped`
evaluates the conditions in Python as the documentation states them, before and after an
unjoin. That the frontend treats `unavailable` as failing a numeric test is ASSUMED from the
documentation and from the homelab's use of the same test; no browser ran here (a browser
test is out of scope, brief section 4.2).

**Grouping is Home Assistant's join dialog.** The room tile's more-info dialog has the join
button, and the dialog lists the chorus rooms only (P10, re-verification item 4). The
dashboard adds nothing for it. Drag-to-group is the chorus app's (K54, K16).

**The entity ids are the tests' house.** A living room, a kitchen, a study and a bedroom, and
one saved group, with the default ids Home Assistant 2026.9 gives them, which repeat the
room's name (0138). The file is adapted by replacing each room's prefix; the document says
how.

**The test holds what can be held without a browser**
(`make ha-test HA_TEST_ARGS="-k dashboard"`):

- the file parses with Home Assistant's own YAML loader into one Sections view;
- every `entity`, in a card and in a visibility condition, is an entity of the integration
  in the entity registry after setup against the fake server, enabled, with a state; every
  media player of the house and every room's group volume is on the dashboard; a tile
  feature sits on the kind of entity it is documented for;
- every view, section, card, badge and feature `type` and every visibility `condition` is on
  an explicit list (`sections`; `grid`; `heading`, `tile`, `media-control`; the five features
  used; `state`, `numeric_state`), and every mapping that carries a `type` was reached by
  the walk, so a nested card cannot escape the list;
- no `resources`, no action (`tap_action` and its kin), no `url`, `url_path` or
  `navigation_path` key, no strategy, and no string or comment with a URL, a `/local/` path
  or `custom:`;
- each check is run on a bad example it must name: a custom card, a custom feature, a nested
  card, a missing entity, another integration's entity, a disabled entity, a feature on the
  wrong domain, a resource and a link.

**Nothing is served.** The integration gains no file, no HTTP view, no static path and no
script; `tests/test_no_unauthenticated_endpoint.py` is unchanged and its allowlist is still
unused. Brief section 4.8 holds literally.

## Not chosen

- **A custom card or custom tile features** (P10 Options B and C) and **embedding the app**
  (Option D): declined by the owner at Checkpoint K.
- **A section inside the homelab's Home dashboard.** P10 allows either; where the copy goes
  is the task that makes it. A whole dashboard is the form that can be both registered as it
  is and cut into sections.
- **The input `select` as a tile.** `media-player-source` on the room's tile sends the same
  `take` (0152), so a second control for it is noise. The select stays for automations.
- **The autoplay switches.** Their ids carry the input's id, so an example cannot name them
  for another house, and they are settings changed rarely; they are on the room's device
  page.
- **A `sources:` filter on `media-player-source`.** The room's `source_list` is already the
  server's stream and the line-ins offered now; a filter would repeat labels that differ per
  house. The document names the option.
- **`state_not: unavailable` for the group-volume tile.** It would show a tile for an entity
  that does not exist.
- **Home Assistant's media player group helper for group volume.** It sets one level on
  every member, which K77 did not choose (P10, re-verification item 5).
- **A generated dashboard (a strategy or a template).** A strategy is browser code; a
  script that writes the YAML from the state would need to run somewhere. The file is short
  enough to edit.
- **Validating against the frontend's card schemas.** They are TypeScript in the frontend,
  not something the Python harness can load; the allowlist of types and the documented
  options are what is held.

## Consequences

- The example names four rooms and one group that are the tests' and not anyone's house. A
  copy must be edited, and the entity check then has to be run against the real house: the
  homelab has its own dashboard check for that.
- A room or a saved group added to `fixtures/control/v2/state-rich.json` fails
  `test_dashboard_every_entity_exists_in_the_integration` until the dashboard has it.
- A change of the integration's entity names or keys (a translation, a unique id) fails the
  same test, which is the point: the example cannot drift from the integration.
- A stock type added to the dashboard must be added to the test's list in the same change.
- A rename of a stock feature type in a later Home Assistant release is not seen by this
  test (P10 names the risk as low); it is seen when the harness pin moves and the
  documentation is read again.
- How the dashboard looks, and whether the inline toggles and the conditional tiles lay out
  well on a phone, was not seen: nothing here rendered it. The first look is the owner's,
  on the homelab's copy.
- 0138's line that left "the dashboard and anything served to the frontend" to goal 20 is
  corrected in this change.
