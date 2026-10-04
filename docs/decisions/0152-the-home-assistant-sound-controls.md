# 0152: the Home Assistant sound controls are entities on the room's device: two tone numbers, three sound switches, an input select that is take the room, a quiet-hours switch, and one autoplay switch per rule on the device of what the rule targets, all but the select as settings

- Status: accepted, 2026-10-04. Extends 0138 (the Home Assistant integration), which left
  "every other entity" to this goal; uses 0081 (per-room sound), 0088 (the autoplay rule's TV
  fields) and 0150 (quiet hours switched off and on per room) as they are.
- Decided by: the owner for what there is (K83: bass, treble, loudness, night mode, speech
  enhancement, input select, autoplay and quiet-hours switches); this record for which
  platform, which device, which category and which bytes.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `integrations/homeassistant/custom_components/chorus/` (`number.py`,
  `select.py`, `switch.py`, `entity.py`, `coordinator.py`, `_aiochorus/models.py`,
  `_aiochorus/commands.py`, `strings.json`, `icons.json`, `quality_scale.yaml`),
  `integrations/homeassistant/tests/test_sound_controls.py`, `tests/fake_server.py`,
  `integrations/homeassistant/README.md`, `docs/home-assistant.md`

## Context

K83 asks for the sound controls in Home Assistant. The catalog already has everything they
need: `sound` (partial), `take` with a source, `quiet_hours_enabled`, `autoplay`, and the
state's `zones[].sound`, `zones[].quiet_enabled` and `autoplay[]`. No server change was
needed and none is made.

## What was read

All on 2026-10-04. `docs/control-plane.md` ("The commands catalog version 2 adds", "Per-room
sound", "The TV path", the v2 state table); the vectors `fixtures/control/v2/sound.json`,
`sound-partial.json`, `take-source.json`, `quiet_hours_enabled.json`, `autoplay.json`,
`autoplay-tv.json`, `state-rich.json`, `state-quiet-disabled.json`, `state-inputs.json` and
the `error-sound-*`, `error-autoplay-input` and `error-source` refusals. Home Assistant core
2026.9.3 (Apache-2.0), from the installed package: `helpers/entity.py` (an entity's `name`
is a cached property that only setting `_attr_name` clears; `translation_placeholders`),
`helpers/entity_registry.py` (`async_entries_for_config_entry`, `async_remove`),
`helpers/device_registry.py` (`async_get_device_by_identifier`). No GPL source was opened.

## Decision

**Per room, on the room's device.**

| entity | platform | reads | sends |
|---|---|---|---|
| Bass, Treble | `number`, -10 to 10, step 1, `dB` | `zones[].sound.bass`, `.treble` | `sound` with that field alone |
| Loudness, Night mode, Speech enhancement | `switch` | `zones[].sound.loudness`, `.night`, `.speech` | `sound` with that field alone |
| Input | `select` | the source of the room's formed group | `take` for the room with the source |
| Quiet hours | `switch` | `zones[].quiet_enabled` | `quiet_hours_enabled` |

`sound` is a partial update, so each control sends only its own field: two people moving two
sliders cannot overwrite each other with a stale copy of the rest.

**The input select is the media player's source list as an entity.** Its options are the
server's stream and each line-in offered now, by label, from the same function the media
player's `source_list` uses; picking one is `take` for the room, as `select_source` is
(0138). It has no current option while the room plays what is not an input (a player, a
Spotify receiver, nothing). It does not offer "none": off is the media player's `turn_off`.

**One autoplay switch per rule, on the device of the rule's target.** A rule targets a room
or a saved group; its switch is on that device, so a rule that targets a saved group sits
with the saved group and not with one of its rooms. The unique id carries the input and the
target, so a rule given another target is a new switch on the other device and the old one
is removed from the registry, as is the switch of a deleted rule (at once, and at setup for
one left from before a restart). A rule whose target is neither a room nor a saved group of
the state gets no switch. The `autoplay` command replaces the rule, so the switch sends the
rule's `stop_on_standby` and `low_latency` again (each written only when `false`, as the
catalog writes them): switching a TV rule off must not reset its TV fields.

**The name of an autoplay switch** is "Autoplay" and the input's label, or its id where it
has none, through a translation placeholder. Home Assistant computes an entity's name once;
a label changed later shows after a reload.

**Categories.** Everything but the input select is `EntityCategory.CONFIG`: these are
settings of a room, which Home Assistant shows under the device's configuration and
leaves out of a generated dashboard. The input select is a primary control. `entity-category` in `quality_scale.yaml` is
therefore `done` (45 done, 9 exempt; 0138 said 44 and 10). `entity-disabled-by-default`
stays exempt: none of these is noisy or diagnostic, and the owner asked for all of them.

**Refusals.** As 0138: the `field` of the server's `error` picks the message. Two keys are
new: `refused_sound` for `bass`, `treble`, `loudness`, `night` and `speech`, and
`refused_input` for `input`.

**Availability.** Every one is unavailable while the event stream is lost (the coordinator's
rule). The tone numbers and sound switches are also unavailable for a room whose state
carries no `sound`; an autoplay switch while its rule is gone.

## Not chosen

- **One `sound` command carrying all five fields.** A stale field would overwrite another
  client's change; the catalog made the command partial for this.
- **Autoplay switches on the input's endpoint.** Speakers are not devices yet (the next
  task), and the rule is about what a room does.
- **A switch per input instead of per rule.** A switch for an input with no rule would have
  to invent a target when turned on; rules are made in chorus.
- **`tv_upmix`, bass management, room EQ, the quiet-hours windows.** Not in K83's list for
  this task.
- **Overriding the entity's `name` to follow a label live.** It gives up Home Assistant's
  own naming path for a rename that is rare.

## Consequences

- The fake server of the tests applies `sound`, `quiet_hours_enabled` and `autoplay` to its
  model; what the real server does with them is held by the server's own tests, not by
  `tests/test_live_server.py`, which this change does not extend.
- No catalog gap was found: every control had its command, its state field and a vector.
