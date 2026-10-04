# The Home Assistant integration: an adversarial check of its three finish lines, 2026-10

- Checked tree: `origin/main` at `96e6246`, then this pull request's tree (the fixes below).
  Line numbers are in the pull request's tree.
- What was checked: the three finish lines the integration was merged on (ADR 0138, PRs #136
  and #139), each assumed false until a command showed otherwise.
  - **A.** The integration's tests pass under a pinned Home Assistant test harness, meeting the
    chosen quality-scale tier's rules.
  - **B.** Room and saved-group media players join, unjoin, set group volume, select inputs and
    announce in tests.
  - **C.** A test shows every HTTP view requires auth and every webhook is local-only.
- Method: the integration (`integrations/homeassistant/custom_components/chorus`) was broken
  on purpose, one small change at a time, and the narrow test command that claims the
  behaviour was run against each broken tree. A change the tests did not notice is a
  false-green; each one found is fixed here and its row says so. Nothing in the integration's
  behaviour was changed: every mutation was reverted, and the pull request's diff of
  `custom_components/chorus` is one corrected comment in `quality_scale.yaml`.
- Home Assistant core (Apache-2.0) was read at tag 2026.9.3, 2026-10-04, for the rule list
  (https://raw.githubusercontent.com/home-assistant/core/2026.9.3/script/hassfest/quality_scale.py).
  No GPL source was opened.

## Summary

| | Count |
|---|---|
| Mutations of `custom_components/chorus` run against `make ha-test HA_TEST_ARGS="..."` | 49 |
| Caught on main as it was | 42 |
| Not caught on main (false-green), fixed in this pull request | 7 (J4, U4, V6, A7, A14, E5, E10) |
| Caught on this pull request's tree | 49 of 49 |
| Mutations judged equivalent (no behaviour changes; not rows) | 2 (below) |
| Further mutations against `make ha-live`, the whole `make ha-test` and the conventions check | 13, all caught |

## The mutations, one row each

Every row was run on this pull request's tree: the mutation applied (the appendix has each as
a patch), the command run, the mutation reverted. `Exit` is the command's exit status with
the mutation applied (`make` reports pytest's failure as 2). Without the mutation each of the
six commands used below exits 0:

| The command, on this pull request's tree with no mutation | Exit | pytest's summary |
|---|---|---|
| `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k join"` | 0 | 8 passed, 44 deselected |
| `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k unjoin"` | 0 | 3 passed, 49 deselected |
| `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k saved_group"` | 0 | 8 passed, 44 deselected |
| `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k select_source"` | 0 | 1 passed, 51 deselected |
| `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 0 | 24 passed, 28 deselected |
| `make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"` | 0 | 7 passed |

### Join

| Row | The change made to `custom_components/chorus` | The narrow command | Exit | The test that went red |
|---|---|---|---|---|
| J1 | `async_join_players` sends `join` with the leader and the member swapped | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k join"` | 2 | `test_join_from_a_room_alone_forms_a_live_group`, `test_join_of_a_room_named_twice_sends_one_command`, `test_join_takes_each_room_into_the_leaders_group`. |
| J2 | a member already in the leader's group is no longer skipped | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k join"` | 2 | `test_join_of_a_room_named_twice_sends_one_command`, `test_join_takes_each_room_into_the_leaders_group`. |
| J3 | the leader named among its own members is no longer skipped | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k join"` | 2 | `test_join_takes_each_room_into_the_leaders_group`. |
| J4 | each member is judged against the state from before the call, not the state the last command left | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k join"` | 2 | `test_join_of_a_room_named_twice_sends_one_command`. **Fixed in this PR**: green on main; the new `test_join_of_a_room_named_twice_sends_one_command`. |
| J5 | a saved group among the members is no longer refused as a saved group | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k join"` | 2 | `test_join_refuses_a_saved_group_and_a_stranger_before_sending_anything`. |
| J6 | `group_members` always lists the room alone | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k join"` | 2 | `test_join_from_a_room_alone_forms_a_live_group`, `test_join_takes_each_room_into_the_leaders_group`. |
| J8 | members are validated one at a time, as each is sent, not all before the first command | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k join"` | 2 | `test_join_refuses_a_saved_group_and_a_stranger_before_sending_anything`. |

### Unjoin

| Row | The change made to `custom_components/chorus` | The narrow command | Exit | The test that went red |
|---|---|---|---|---|
| U1 | `async_unjoin_player` sends `take` with the source `none` (leave and go silent) | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k unjoin"` | 2 | `test_unjoin_from_a_saved_group_leaves_it_inactive`, `test_unjoin_of_a_room_alone_in_a_group_that_is_not_its_own`, `test_unjoin_takes_the_room_and_the_live_group_dissolves`. |
| U2 | a room already alone in its own group is no longer left alone: `take` is sent anyway | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k unjoin"` | 2 | `test_unjoin_takes_the_room_and_the_live_group_dissolves`. |
| U3 | `async_unjoin_player` sends `join` of the room to itself instead of `take` | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k unjoin"` | 2 | `test_unjoin_from_a_saved_group_leaves_it_inactive`, `test_unjoin_of_a_room_alone_in_a_group_that_is_not_its_own`, `test_unjoin_takes_the_room_and_the_live_group_dissolves`. |
| U4 | unjoin sends nothing for any group of one room, whatever its kind | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k unjoin"` | 2 | `test_unjoin_of_a_room_alone_in_a_group_that_is_not_its_own`. **Fixed in this PR**: green on main; the new `test_unjoin_of_a_room_alone_in_a_group_that_is_not_its_own`. |

### Saved-group volume

| Row | The change made to `custom_components/chorus` | The narrow command | Exit | The test that went red |
|---|---|---|---|---|
| V1 | the saved group's `volume_set` sets the first room's volume instead of the group volume | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k saved_group"` | 2 | `test_saved_group_volume_is_the_group_volume`. |
| V2 | the saved group's `volume_up` steps down | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k saved_group"` | 2 | `test_saved_group_volume_is_the_group_volume`. |
| V3 | the saved group's `volume_set` goes to the saved id without asking whether the group is assembled | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k saved_group"` | 2 | `test_a_saved_group_that_is_not_assembled_has_no_volume_to_set`. |
| V4 | the saved group shows its first room's volume as its own | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k saved_group"` | 2 | `test_rooms_and_saved_groups_from_the_rich_vector`, `test_saved_group_volume_is_the_group_volume`. |
| V5 | the saved group's mute reaches its first room only | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k saved_group"` | 2 | `test_saved_group_volume_is_the_group_volume`. |
| V6 | the saved group shows muted when any one room is muted | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k saved_group"` | 2 | `test_a_saved_group_is_muted_only_when_every_room_is`. **Fixed in this PR**: green on main; the new `test_a_saved_group_is_muted_only_when_every_room_is`. |
| V7 | the saved group's `volume_down` goes to the saved id without asking whether the group is assembled | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k saved_group"` | 2 | `test_a_saved_group_that_is_not_assembled_has_no_volume_to_set`. |
| V8 | the volume step is 20 thousandths, not 50 | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k saved_group"` | 2 | `test_saved_group_volume_is_the_group_volume`. |

### Select source

| Row | The change made to `custom_components/chorus` | The narrow command | Exit | The test that went red |
|---|---|---|---|---|
| S1 | `select_source` sends the name the person picked, not the catalog source | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k select_source"` | 2 | `test_select_source_takes_the_room_or_assembles_the_saved_group`. |
| S2 | an unknown source is sent to the server instead of being refused | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k select_source"` | 2 | `test_select_source_takes_the_room_or_assembles_the_saved_group`. |
| S3 | the saved group's player targets its first room, not the saved group | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k select_source"` | 2 | `test_select_source_takes_the_room_or_assembles_the_saved_group`. |
| S4 | `select_source` sends `take` without the source | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k select_source"` | 2 | `test_select_source_takes_the_room_or_assembles_the_saved_group`. |
| S5 | a line-in's catalog source loses its `line-in:` prefix | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k select_source"` | 2 | `test_select_source_takes_the_room_or_assembles_the_saved_group`. |

### Announce, in a room and in a saved group

| Row | The change made to `custom_components/chorus` | The narrow command | Exit | The test that went red |
|---|---|---|---|---|
| A1 | the announce URL's origin is no longer held to Home Assistant's own | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_announce_off_origin_is_refused_before_any_request`. |
| A2 | the announcement's volume is dropped | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_announce_in_a_saved_group_with_a_volume`. |
| A3 | the saved group's player targets its first room, so the announcement plays in one room | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_announce_in_a_saved_group_with_a_volume`. |
| A4 | a `media-source://` id is no longer resolved | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_announce_resolves_a_media_source_inside_home_assistant`. |
| A5 | only the internal URL counts as Home Assistant's own origin | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_announce_from_the_external_url_is_home_assistants_own_too`. |
| A6 | origins are compared without the port | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_announce_off_origin_is_refused_before_any_request`, `test_announce_to_a_server_that_does_not_list_this_home_assistant_says_so`. |
| A7 | every `url` refusal is reported as the server not listing this Home Assistant | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_a_url_refusal_of_an_announce_when_the_server_cannot_be_asked_again`, `test_the_servers_url_refusal_of_an_announce_is_a_translated_error`. **Fixed in this PR**: green on main under `-k announce` (the tests that hold it were not selected by that name); they are renamed to carry `announce`. |
| A8 | a boolean is accepted as an announce volume | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_an_announce_volume_outside_the_range_is_refused`. |
| A9 | the URL is no longer made absolute and signed (`async_process_play_media_url`) | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_announce_resolves_a_media_source_inside_home_assistant`, `test_announce_signs_a_path_that_needs_auth`. |
| A10 | the server is not asked again who it is after a `url` refusal | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_announce_to_a_server_that_does_not_list_this_home_assistant_says_so`. |
| A11 | origins are compared as if every scheme were `http` | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_announce_off_origin_is_refused_before_any_request`. |
| A12 | the announcement in a saved group is sent to its first room | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_announce_in_a_saved_group_with_a_volume`. |
| A13 | an announce volume outside 0 to 1 is accepted | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_an_announce_volume_outside_the_range_is_refused`. |
| A14 | every announce refusal, not only a `url` one, asks the server who it is and may be reported as a missing origin | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_the_other_announce_refusals_by_field`. **Fixed in this PR**: green on main; `test_the_other_announce_refusals_by_field` now runs against a server that lists no origin and asserts the server was not asked again. |
| A15 | the announce volume is written without three fractional digits | `make ha-test HA_TEST_ARGS="tests/test_media_player.py -k announce"` | 2 | `test_announce_in_a_saved_group_with_a_volume`. |

### Endpoints planted in the integration

| Row | The change made to `custom_components/chorus` | The narrow command | Exit | The test that went red |
|---|---|---|---|---|
| E1 | a view with `requires_auth = False`, registered in `async_setup_entry` | `make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"` | 2 | `test_runtime_every_route_requires_auth_and_every_webhook_is_local_only`, `test_static_scan_of_every_file_of_the_integration`. |
| E2 | a webhook registered in `async_setup_entry` without `local_only=True` | `make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"` | 2 | `test_runtime_every_route_requires_auth_and_every_webhook_is_local_only`, `test_static_scan_of_every_file_of_the_integration`. |
| E3 | a static path registered in `async_setup_entry` | `make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"` | 2 | `test_runtime_every_route_requires_auth_and_every_webhook_is_local_only`, `test_static_scan_of_every_file_of_the_integration`. |
| E4 | a view without auth, built with `type()` and registered through `getattr` so the static scan cannot read it | `make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"` | 2 | `test_runtime_every_route_requires_auth_and_every_webhook_is_local_only`. |
| E5 | a webhook that is not local-only, registered under another domain's name through `getattr` | `make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"` | 2 | `test_runtime_every_route_requires_auth_and_every_webhook_is_local_only`. **Fixed in this PR**: green on main in both halves; the run-time half now audits every webhook added, whatever domain it is registered under. |
| E6 | a static directory put on the aiohttp router directly (`router.add_static`) | `make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"` | 2 | `test_runtime_every_route_requires_auth_and_every_webhook_is_local_only`, `test_static_scan_of_every_file_of_the_integration`. |
| E7 | a webhook with `local_only=False` registered by the `number` platform, not by `__init__.py` | `make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"` | 2 | `test_runtime_every_route_requires_auth_and_every_webhook_is_local_only`, `test_static_scan_of_every_file_of_the_integration`. |
| E8 | a webhook that is not local-only, under the integration's own domain, registered through `getattr` | `make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"` | 2 | `test_runtime_every_route_requires_auth_and_every_webhook_is_local_only`. |
| E9 | a view with `requires_auth = False` registered on the first `turn_on`, after setup | `make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"` | 2 | `test_static_scan_of_every_file_of_the_integration`. |
| E10 | a bare route put on the router on the first `turn_on`, after setup (`router.add_get`) | `make ha-test HA_TEST_ARGS="tests/test_no_unauthenticated_endpoint.py"` | 2 | `test_static_scan_of_every_file_of_the_integration`. **Fixed in this PR**: green on main in both halves; the static half now reports a route put on a router directly (`route-not-auditable`), and the grep backstop matches it. |

In the endpoint rows the two halves of `tests/test_no_unauthenticated_endpoint.py` cover each
other, and the rows show which half saw what. E1, E2, E3 and E7 (written plainly, at setup)
turn both the run-time test and the static scan red. E4, E5, E6 and E8 are written so the
static scan cannot read them and are caught by the run-time comparison alone. E9 and E10 are
made after setup, where the run-time comparison cannot see them, and are caught by the static
scan alone.

### What was false-green, and the fix

1. **E5, the endpoint audit's run-time half skipped a webhook registered under another
   domain's name.** `audit_added` (`tests/endpoint_audit.py`) counted and audited only the
   webhooks whose `domain` was `chorus`, so a webhook the integration registered as, say,
   `media_player`, without `local_only=True` and in a form the static scan could not read,
   passed both halves. It now audits every webhook added since the snapshot (what the
   integration depends on is set up before the snapshot, so what is added afterwards is the
   integration's) and reports a foreign domain name as a finding of its own
   (`webhook-foreign-domain`). The checker's self-test plants both.
2. **E10, neither half saw a route put on the aiohttp router directly after setup.** The
   static scan knew `register_view`, the static-path calls, `add_extra_js_url` and webhooks,
   but not `router.add_get` and its kin; the run-time half sees those only when they are made
   during setup. The static scan now reports every `add_get`, `add_post`, `add_put`,
   `add_patch`, `add_delete`, `add_head`, `add_route`, `add_routes`, `add_resource`,
   `add_view`, `add_static`, `add_subapp` and `register_redirect` call on an object as
   `route-not-auditable`, with a self-test, and the grep backstop in
   `tools/conventions/check-ha-integration.sh` matches `.router.`, `add_static` and
   `add_subapp` and proves it on three more probe lines.
3. **J4, a join that judged every member against the state from before the call** passed: no
   test named a member twice. New test `test_join_of_a_room_named_twice_sends_one_command`.
4. **U4, an unjoin that sent nothing for any group of one room** passed: the only lone room
   in the tests was in its own group. A live group of one room exists (catalog v1's `ungroup`
   leaves one, ADR 0138 "Not chosen"), and unjoin must still take the room out of it. New test
   `test_unjoin_of_a_room_alone_in_a_group_that_is_not_its_own`.
5. **V6, a saved group shown as muted when any one room is** passed: the tests muted all
   rooms or none. New test `test_a_saved_group_is_muted_only_when_every_room_is`.
6. **A7, the narrow command left out the tests that hold the behaviour.** The mutation was
   caught by the whole file but not by `-k announce`, the command a person would run for
   "announce": four announce tests had no `announce` in their names. They are renamed
   (`test_the_servers_url_refusal_of_an_announce_is_a_translated_error`,
   `test_announce_to_a_server_that_does_not_list_this_home_assistant_says_so`,
   `test_a_url_refusal_of_an_announce_when_the_server_cannot_be_asked_again`,
   `test_announce_with_no_free_player_is_its_own_translated_error`); `-k announce` now selects
   24 tests, not 21.
7. **A14, a refusal of the target or the volume reported as a missing announce origin**
   passed: the refusal tests ran against a server that lists this Home Assistant, where the
   wrong path ends in the same error. `test_the_other_announce_refusals_by_field` now runs
   against a server with no announce origin and asserts the server was not asked who it is.

### Mutations judged equivalent (not rows: no command can go red)

- **J7**: dropping the check that a join member belongs to this config entry
  (`media_player.py:496`). A room's unique id starts with its server's id, and the next line
  matches that prefix against this entry's server, so a room of another chorus server is
  refused either way; two entries for one server id cannot exist (the config flow aborts on
  the unique id, `config_flow.py:79-80`).
- **U5**: dropping `len(group.zones) == 1` from unjoin's "already alone" test
  (`media_player.py:539`). A group of kind `room` has exactly one room by the catalog's
  definition (`docs/control-plane.md`), so the two conditions say the same thing.

### Further checks, with other commands

These are not `HA_TEST_ARGS` rows: each names its own command. All were run on this pull
request's tree and exit 0 without the mutation.

| Row | The change | The command | Exit | What went red |
|---|---|---|---|---|
| L1 | as J1: `join` with the leader and the member swapped | `CHORUS_SERVER_BIN=<built server> make ha-live` | 2 | `test_live_server`: the kitchen's members never become all three rooms. |
| L2 | as U3: unjoin sends `join` of the room to itself | `CHORUS_SERVER_BIN=<built server> make ha-live` | 2 | `test_live_server`: the den never leaves. |
| L3 | announce sends `take` instead of `announce` | `CHORUS_SERVER_BIN=<built server> make ha-live` | 2 | `test_live_server`: the room never says `Announcement`. |
| L4 | the group-volume number sends a room `volume` named for the group (`number.py:85`) | `CHORUS_SERVER_BIN=<built server> make ha-live` | 2 | `test_live_server`: the real server refuses `there is no zone 'live-1'`. |
| G1 | an uncovered function added to `config_flow.py` | `make ha-test` | 2 | the config flow's coverage step: `total of 99 is less than fail-under=100`. |
| G2 | a wrongly typed constant added to `const.py` | `make ha-test` | 2 | `mypy --strict`: `Incompatible types in assignment`. |
| G3 | a line of `media_player.py` left unformatted | `make ha-test` | 2 | `ruff format --check`: `1 file would be reformatted`. |
| Q1 | `strict-typing: done` deleted from `quality_scale.yaml` | `bash tools/conventions/check-ha-integration.sh` | 1 | "does not list exactly the rules of quality-scale-rules.txt". |
| Q2 | `brands: done` changed to `brands: todo` | the same | 1 | "every rule is `done` or `exempt` with a comment". |
| Q3 | the comment of the `docs-triggers` exemption deleted | the same | 1 | "every rule is `done` or `exempt` with a comment". |
| Q4 | a line with `hass.http.app.router.add_get(` added to `__init__.py` | the same | 1 | "the integration registers an HTTP view, a route, a webhook, a static path or a script" (the pattern this pull request widened). |
| Q5 | a requirement added to `manifest.json` | the same | 1 | "`requirements` is not []". |
| Q6 | a rule that core does not list added to `quality_scale.yaml` | the same | 1 | "does not list exactly the rules of quality-scale-rules.txt". |

`make ha-hassfest` carries its own proof of this kind: its second step removes
`strict-typing` from the core copy and requires hassfest to fail (tail below).

### Does the tests' fake agree with the real server?

`tests/fake_server.py` is a test double written from `docs/control-plane.md`; every assertion
in `test_media_player.py` on what the entities show after a command rests on it. The same
commands were sent to the fake and to the built `chorus-server` (five rooms, one saved group
of two, one live pair) and the answers compared: each room's group, volume and mute, each
group's kind, rooms, source and volume (live group ids normalised, they are the server's to
choose), and each saved group's `active`. 22 steps in nine sequences, 0 differences:

| Sequence | Steps | Agrees |
|---|---|---|
| join two rooms into a saved group's room, then one of them again | 3 | yes |
| unjoin (`take`) from a live pair, then again | 2 | yes |
| unjoin from a saved group | 1 | yes |
| `take` with `none` on a room of a saved group, then `take` the saved group with `stream` | 2 | yes |
| room volumes, `group_volume`, `group_volume_step` up and down | 5 | yes |
| `group_volume` on a saved group that is not assembled | 2 | yes |
| join a room alone, then take it back | 2 | yes |
| turn off, turn on, turn the saved group off | 3 | yes |
| `take` a room alone without a source | 2 | yes |

That settles what ADR 0138's "Consequences" still listed as ASSUMED until the live test had
run: the live test's command line is accepted by the server and the test passes (tail below);
`join` to a room in a saved group keeps the saved group's id; a `take` without a source
leaves the room on what the fake says; a 1.5 s clip is seen as `Announcement` for long enough
to be observed. The comparison was a one-off script and is not kept as a test (see "Open").

## The three whole runs, on this pull request's tree

### `make ha-test` (the whole run)

```
Name                                         Stmts   Miss Branch BrPart  Cover   Missing
----------------------------------------------------------------------------------------
custom_components/chorus/_aiochorus/sse.py      63      1     28      1    98%   76
custom_components/chorus/media_player.py       289      5     76      5    97%   153-154, 173->163, 245, 501->507, 565, 574
----------------------------------------------------------------------------------------
TOTAL                                         1021      6    200      6    99%

12 files skipped due to complete coverage.
Required test coverage of 95% reached. Total coverage: 99.02%
--------------------------- snapshot report summary ----------------------------
1 snapshot passed.
=========================== short test summary info ============================
SKIPPED [1] tests/test_live_server.py:152: CHORUS_SERVER_BIN is not set: the live test drives a built chorus-server (cargo build -p chorus-server, the
======================= 156 passed, 1 skipped in 16.29s ========================
ha-test: the config flow's coverage
Name                                      Stmts   Miss Branch BrPart  Cover   Missing
-------------------------------------------------------------------------------------
custom_components/chorus/config_flow.py      79      0     16      0   100%
-------------------------------------------------------------------------------------
TOTAL                                        79      0     16      0   100%
ha-test: PASS, wall-clock 42.0s
```

### `make ha-hassfest`

```
Validating services... done in 0.00s
Validating ssdp... done in 0.01s
Validating translations... done in 0.01s
Validating triggers... done in 0.00s
Validating usb... done in 0.01s
Validating zeroconf... done in 0.01s
Validating config_flow... done in 0.01s

Integrations: 1
Invalid integrations: 0

ha-hassfest: PASS (core and custom), wall-clock 11.6s
```

### `make ha-live`

Run as `CHORUS_SERVER_BIN=<build dir>/debug/chorus-server make ha-live`, the server built from
this tree with `cargo build -p chorus-server` under the two heavy locks, into the throwaway
directory `tools/build-dir.sh` names.

```
configfile: pyproject.toml
plugins: xdist-3.8.0, github-actions-annotate-failures-0.4.2, pytest_freezer-0.4.9, syrupy-6.0.0, respx-0.23.1, unordered-0.8.0, timeout-2.4.0, aiohtt
asyncio: mode=Mode.AUTO, debug=False, asyncio_default_fixture_loop_scope=function, asyncio_default_test_loop_scope=function
collected 1 item

tests/test_live_server.py .                                              [100%]

============================== 1 passed in 2.60s ===============================
ha-live: PASS, wall-clock 5.5s
```

## The tier's rules

`bash tools/conventions/check-ha-integration.sh` exits 0 on this pull request's tree:

```
home assistant integration: no requirement, client stands alone, translations equal, harness pytest-homeassistant-custom-component 0.13.366 (Home Assistant 2026.9.3, Python 3.14.8) with 1269 hashed files locked, 54 quality-scale rules (44 done, 10 exempt), no endpoint
```

The pinned list `integrations/homeassistant/quality-scale-rules.txt` was compared with
`ALL_RULES` in core's `script/hassfest/quality_scale.py` at tag 2026.9.3, fetched again for
this check: the same 54 rules in the same tiers (20 Bronze, 10 Silver, 21 Gold, 3 Platinum).
Every one appears in `quality_scale.yaml`:

| Tier | Rule | Status | The reason or the evidence, as judged here |
|---|---|---|---|
| bronze | action-setup | exempt | True: no service is registered anywhere in the integration (no `async_register`, no `services.`). |
| bronze | appropriate-polling | exempt | True: the coordinator has no update interval (`coordinator.py:69`); state arrives on the event stream; the 45 s probe is `_aiochorus/client.py:34`. |
| bronze | brands | done | `brand/icon.png`, `brand/logo.png`. |
| bronze | common-modules | done | `coordinator.py`, `entity.py`. |
| bronze | config-flow | done | `config_flow.py`; `"config_flow": true` in the manifest. |
| bronze | config-flow-test-coverage | done | `make ha-test` holds `config_flow.py` at 100 % and goes red at 99 % (row G1). |
| bronze | dependency-transparency | exempt | **Corrected.** The reason said the repository is private, so the client cannot be an open PyPI package. The repository is public (`gh repo view --json visibility`, 2026-10-04: `PUBLIC`). The exemption stands on a reason that is true: the manifest lists no requirement, so there is no dependency to publish; the client is vendored so that Home Assistant installs nothing. `quality_scale.yaml` now says that. |
| bronze | docs-actions | exempt | True: no action of its own; the entity actions are under "Supported functions" in the README. |
| bronze | docs-conditions | exempt | True: no `condition.py`. |
| bronze | docs-high-level-description | done | README, the opening section. |
| bronze | docs-installation-instructions | done | README "Installation instructions". One sentence in it is stale (see "Open"). |
| bronze | docs-removal-instructions | done | README "Removal instructions". |
| bronze | docs-triggers | exempt | True: no `trigger.py`. |
| bronze | entity-event-setup | done | Entities subscribe through `CoordinatorEntity`, in `async_added_to_hass` (`media_player.py:201`). |
| bronze | entity-unique-id | done | `media_player.py:430`, `:553`; `number.py:59`. |
| bronze | has-entity-name | done | `entity.py:16`. |
| bronze | runtime-data | done | `__init__.py:49`. |
| bronze | test-before-configure | done | The flow asks the server who it is before it creates the entry (`config_flow.py:47`). |
| bronze | test-before-setup | done | `__init__.py:28-44`: `ConfigEntryNotReady`, or `ConfigEntryError` for a server without catalog 2. |
| bronze | unique-config-entry | done | `config_flow.py:79-80`. |
| silver | action-exceptions | done | `ServiceValidationError` and `HomeAssistantError` throughout `media_player.py`; `coordinator.py:149-169`. |
| silver | config-entry-unloading | done | `__init__.py:73`. |
| silver | docs-configuration-parameters | exempt | True: no options flow in `config_flow.py`. |
| silver | docs-installation-parameters | done | README "Installation parameters". |
| silver | entity-unavailable | done | `entity.py:48`, `:83`; `coordinator.py:107-121`. |
| silver | integration-owner | done | `codeowners` in the manifest. |
| silver | log-when-unavailable | done | Once each way: `coordinator.py:92-97`, `:111-118`. |
| silver | parallel-updates | done | `media_player.py:49`, `number.py:17`. |
| silver | reauthentication-flow | exempt | True: the control API has no credentials; the flow's data are a host and a port. |
| silver | test-coverage | done | 99 % over the integration, floor 95 % (tail above). |
| gold | devices | done | `entity.py:27`, `:61`; `__init__.py:51-63`. |
| gold | diagnostics | done | `diagnostics.py`. |
| gold | discovery | done | `zeroconf` in the manifest; `config_flow.py:92`. |
| gold | discovery-update-info | done | `config_flow.py:102`, `:108` (`updates=`). |
| gold | docs-data-update | done | README "Data updates". |
| gold | docs-examples | done | README "Examples". |
| gold | docs-known-limitations | done | README "Known limitations". |
| gold | docs-supported-devices | done | README "Supported devices". |
| gold | docs-supported-functions | done | README "Supported functions". |
| gold | docs-troubleshooting | done | README "Troubleshooting". |
| gold | docs-use-cases | done | README "Use cases". |
| gold | dynamic-devices | done | `media_player.py:86-111`: rooms and saved groups are added as they appear. |
| gold | entity-category | exempt | True: no entity sets a category; the entities are media players and a group volume. |
| gold | entity-device-class | done | `media_player.py:189` (speaker). The group-volume number has no class that fits a percentage of full scale. |
| gold | entity-disabled-by-default | exempt | True: three kinds of entity, each a primary control. |
| gold | entity-translations | done | `_attr_translation_key` on each entity; `strings.json`. |
| gold | exception-translations | done | 21 exception keys in `strings.json`; every raise in the integration names one. |
| gold | icon-translations | done | `icons.json`. |
| gold | reconfiguration-flow | done | `config_flow.py:128`. |
| gold | repair-issues | done | `coordinator.py:187-200`. |
| gold | stale-devices | done | `coordinator.py:123-147`. |
| platinum | async-dependency | done | The vendored client is async only, on aiohttp. |
| platinum | inject-websession | done | `__init__.py:26`, `config_flow.py:47`. |
| platinum | strict-typing | done | `mypy --strict` in `make ha-test` (row G2); hassfest fails with the rule removed (tail above). |

Nine of the ten exemptions' reasons are true as written; one was false and is corrected, and
`docs/decisions/0146-the-ha-integration-audit-corrections.md` records the correction to
ADR 0138. BRIEF.md does not state the reason and needs no change.

## Open

Named here, not fixed in this pull request:

- **A registration that is both hidden from the static scan and made after setup escapes
  both halves** (for example a webhook built with `getattr` inside a service call). The test
  is there to catch a mistake, not code written to hide; review is the control for that.
  Closing it mechanically would take a run-time audit that takes its second snapshot after
  every entity action has been called once.
- **Two stale sentences outside this task's scope.** `integrations/homeassistant/README.md`
  ("HACS cannot install it (the repository is private)") and `docs/conventions.md`
  ("a private repository built to be run by outsiders") still call the repository private.
  One line each; a follow-up.
- **The fake-against-real comparison is not a kept test.** It would take a second test in
  `tests/test_live_server.py` (five rooms and a saved group on the real server, the same
  sequences sent to `FakeChorusServer`), run by `make ha-live`.
- **Announce's limits are unchanged** and are ADR 0136's: an announcement the server accepts
  and then cannot fetch is answered 200. Not a finding of this check.

## Verdict

- **A. The integration's tests pass under a pinned Home Assistant test harness, meeting the
  chosen quality-scale tier's rules: true.** `make ha-test` ends `ha-test: PASS` (156
  passed, 99 % coverage, the config flow at 100 %), `make ha-hassfest` ends
  `ha-hassfest: PASS (core and custom)`, and the gate's pieces go red when broken (G1 to G3,
  Q1 to Q6). The rule list is core's at the pinned tag, and all 54 rules are `done` or
  `exempt`. One part was false and is fixed here: the reason given for the
  `dependency-transparency` exemption (the repository is not private).
- **B. Room and saved-group media players join, unjoin, set group volume, select inputs and
  announce in tests: true.** 39 mutations of those five behaviours, 34 caught on main; the
  five that were not (J4, U4, V6, A7, A14) were gaps at the edges of a tested behaviour, not
  a behaviour without a test, and each now has a test that goes red. The live test passed
  against the built server (not skipped) and goes red for a broken join, unjoin, group
  volume and announce (L1 to L4); the fake the other tests rest on agrees with the real
  server on 22 of 22 steps.
- **C. A test shows every HTTP view requires auth and every webhook is local-only: true for
  a registration written plainly, false as ADR 0138 states it ("would catch the first one
  done wrong, at run time and statically"), and made true by this pull request.** A view
  with `requires_auth = False`, a webhook without `local_only=True` and a static path, planted
  in the integration, each turn the test red (E1 to E3), as do the forms hidden from one half
  (E4, E6 to E9). Two plants passed on main: a webhook registered under another domain's name
  (E5) and a route put on the router after setup (E10). The audit now catches both.

## Appendix: every mutation as a patch

Each block is the row's whole change, against this PR's tree. Save it as `m.diff`, then
`git apply --unidiff-zero m.diff`, run the row's command, and `git apply -R --unidiff-zero m.diff`.

### J1

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -533 +533 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-                commands.join(zone_id, self._zone_id)
+                commands.join(self._zone_id, zone_id)
```

### J2

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -530 +530 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-            if zone_id == self._zone_id or member.group == leader.group:
+            if zone_id == self._zone_id:
```

### J3

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -530 +530 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-            if zone_id == self._zone_id or member.group == leader.group:
+            if member.group == leader.group and zone_id != self._zone_id:
```

### J4

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -532 +532 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-            state = await self.coordinator.async_command(
+            await self.coordinator.async_command(
```

### J5

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -501 +501 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-            if entry.unique_id.startswith(saved_group_identifier(server_id, "")):
+            if False:
```

### J6

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -450 +450 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-        if group is None or len(group.zones) < 2:
+        if True:
```

### J8

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -519 +518,0 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-        zones = [self._zone_of(entity_id) for entity_id in group_members]
@@ -521 +520 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-        for zone_id in zones:
+        for zone_id in (self._zone_of(e) for e in group_members):
```

### U1

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -541 +541 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-        await self.coordinator.async_command(commands.take(self._zone_id))
+        await self.coordinator.async_command(commands.take(self._zone_id, SOURCE_NONE))
```

### U2

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -539 +539 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-        if group is not None and group.kind == "room" and len(group.zones) == 1:
+        if False:
```

### U3

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -541 +541 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-        await self.coordinator.async_command(commands.take(self._zone_id))
+        await self.coordinator.async_command(commands.join(self._zone_id, self._zone_id))
```

### U4

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -539 +539 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-        if group is not None and group.kind == "room" and len(group.zones) == 1:
+        if group is not None and len(group.zones) == 1:
```

### V1

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -602,2 +602,2 @@ class ChorusSavedGroupMediaPlayer(ChorusSavedGroupEntity, ChorusMediaPlayer):
-            commands.group_volume(
-                self._active_group().id, commands.volume_from_level(volume)
+            commands.volume(
+                self._active_group().zones[0], commands.volume_from_level(volume)
```

### V2

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -610 +610 @@ class ChorusSavedGroupMediaPlayer(ChorusSavedGroupEntity, ChorusMediaPlayer):
-            commands.group_volume_step(self._active_group().id, VOLUME_STEP_THOUSANDTHS)
+            commands.group_volume_step(self._active_group().id, -VOLUME_STEP_THOUSANDTHS)
```

### V3

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -603 +603 @@ class ChorusSavedGroupMediaPlayer(ChorusSavedGroupEntity, ChorusMediaPlayer):
-                self._active_group().id, commands.volume_from_level(volume)
+                self._target, commands.volume_from_level(volume)
```

### V4

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -558 +558 @@ class ChorusSavedGroupMediaPlayer(ChorusSavedGroupEntity, ChorusMediaPlayer):
-        return None if (group := self.formed_group) is None else group.volume
+        return None if (zone := self.coordinator.data.zone(self.saved.zones[0])) is None else zone.volume
```

### V5

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -624 +624 @@ class ChorusSavedGroupMediaPlayer(ChorusSavedGroupEntity, ChorusMediaPlayer):
-        for zone_id in () if saved is None else saved.zones:
+        for zone_id in () if saved is None else saved.zones[:1]:
```

### V6

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -567 +567 @@ class ChorusSavedGroupMediaPlayer(ChorusSavedGroupEntity, ChorusMediaPlayer):
-        return all(zone is not None and zone.muted for zone in zones)
+        return any(zone is not None and zone.muted for zone in zones)
```

### V7

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -617 +617 @@ class ChorusSavedGroupMediaPlayer(ChorusSavedGroupEntity, ChorusMediaPlayer):
-                self._active_group().id, -VOLUME_STEP_THOUSANDTHS
+                self._target, -VOLUME_STEP_THOUSANDTHS
```

### V8

```diff
--- a/integrations/homeassistant/custom_components/chorus/const.py
+++ b/integrations/homeassistant/custom_components/chorus/const.py
@@ -21 +21 @@ MEDIA_TYPE_INPUT: Final = "chorus_input"
-VOLUME_STEP_THOUSANDTHS: Final = 50
+VOLUME_STEP_THOUSANDTHS: Final = 20
```

### S1

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -293 +293 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-        await self.coordinator.async_command(commands.take(self._target, catalog))
+        await self.coordinator.async_command(commands.take(self._target, source))
```

### S2

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -287,0 +288,2 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
+            catalog = source
+        if False:
```

### S3

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -552 +552 @@ class ChorusSavedGroupMediaPlayer(ChorusSavedGroupEntity, ChorusMediaPlayer):
-        self._target = saved.id
+        self._target = saved.zones[0]
```

### S4

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -293 +293 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-        await self.coordinator.async_command(commands.take(self._target, catalog))
+        await self.coordinator.async_command(commands.take(self._target))
```

### S5

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -125 +125 @@ def source_map(state: State) -> dict[str, str]:
-        sources[name] = f"{LINE_IN_PREFIX}{input_id}"
+        sources[name] = input_id
```

### A1

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -343 +343 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-        if origin is None or origin not in own_origins(self.hass):
+        if origin is None:
```

### A2

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -361 +361 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-                commands.announce(self._target, url, thousandths)
+                commands.announce(self._target, url, None)
```

### A3

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -552 +552 @@ class ChorusSavedGroupMediaPlayer(ChorusSavedGroupEntity, ChorusMediaPlayer):
-        self._target = saved.id
+        self._target = saved.zones[0]
```

### A4

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -331 +331 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-        if media_source.is_media_source_id(media_id):
+        if False:
```

### A5

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -163 +163 @@ def own_origins(hass: HomeAssistant) -> set[tuple[str, str | None, int | None]]:
-    for internal in (True, False):
+    for internal in (True,):
```

### A6

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -157 +157 @@ def origin_of(url: str) -> tuple[str, str | None, int | None] | None:
-    return (parsed.scheme, parsed.host, parsed.port)
+    return (parsed.scheme, parsed.host, None)
```

### A7

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -370,2 +369,0 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-            if origin in server_origins(self.coordinator):
-                raise
```

### A8

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -351,2 +351 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-                isinstance(level, bool)
-                or not isinstance(level, (int, float))
+                not isinstance(level, (int, float))
```

### A9

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -339 +339 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-            url = async_process_play_media_url(self.hass, media_id)
+            url = media_id
```

### A10

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -369 +368,0 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-            await self.coordinator.async_refresh_server()
```

### A11

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -157 +157 @@ def origin_of(url: str) -> tuple[str, str | None, int | None] | None:
-    return (parsed.scheme, parsed.host, parsed.port)
+    return ("http", parsed.host, parsed.port)
```

### A12

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -361 +361 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-                commands.announce(self._target, url, thousandths)
+                commands.announce(self._zone_id if hasattr(self, "_zone_id") else self.saved.zones[0], url, thousandths)
```

### A13

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -353 +352,0 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-                or not 0 <= level <= 1
```

### A14

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -364,2 +363,0 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-            if err.translation_key != "refused_url":
-                raise
```

### A15

```diff
--- a/integrations/homeassistant/custom_components/chorus/_aiochorus/commands.py
+++ b/integrations/homeassistant/custom_components/chorus/_aiochorus/commands.py
@@ -115 +115 @@ def announce(target: str, url: str, thousandths: int | None = None) -> bytes:
-    return (head + f',"volume":{encode_volume(thousandths)}}}').encode()
+    return (head + f',"volume":{thousandths / 1000}}}').encode()
```

### E1

```diff
--- a/integrations/homeassistant/custom_components/chorus/__init__.py
+++ b/integrations/homeassistant/custom_components/chorus/__init__.py
@@ -65,0 +66 @@ async def async_setup_entry(hass: HomeAssistant, entry: ChorusConfigEntry) -> bo
+    hass.http.register_view(ArtView())
@@ -75,0 +77,15 @@ async def async_unload_entry(hass: HomeAssistant, entry: ChorusConfigEntry) -> b
+
+
+from homeassistant.helpers.http import HomeAssistantView  # noqa: E402
+
+
+class ArtView(HomeAssistantView):
+    """Planted by the audit."""
+
+    url = "/api/chorus/art"
+    name = "api:chorus:art"
+    requires_auth = False
+
+    async def get(self, request):  # type: ignore[no-untyped-def]
+        """Planted."""
+        return self.json({})
```

### E2

```diff
--- a/integrations/homeassistant/custom_components/chorus/__init__.py
+++ b/integrations/homeassistant/custom_components/chorus/__init__.py
@@ -65,0 +66,6 @@ async def async_setup_entry(hass: HomeAssistant, entry: ChorusConfigEntry) -> bo
+    from homeassistant.components import webhook  # noqa: PLC0415
+
+    async def _hook(hass, webhook_id, request):  # type: ignore[no-untyped-def]
+        return None
+
+    webhook.async_register(hass, DOMAIN, "chorus", f"chorus-{entry.entry_id}", _hook)
```

### E3

```diff
--- a/integrations/homeassistant/custom_components/chorus/__init__.py
+++ b/integrations/homeassistant/custom_components/chorus/__init__.py
@@ -65,0 +66,5 @@ async def async_setup_entry(hass: HomeAssistant, entry: ChorusConfigEntry) -> bo
+    from homeassistant.components.http import StaticPathConfig  # noqa: PLC0415
+
+    await hass.http.async_register_static_paths(
+        [StaticPathConfig("/chorus_static", hass.config.path("www"), True)]
+    )
```

### E4

```diff
--- a/integrations/homeassistant/custom_components/chorus/__init__.py
+++ b/integrations/homeassistant/custom_components/chorus/__init__.py
@@ -65,0 +66,5 @@ async def async_setup_entry(hass: HomeAssistant, entry: ChorusConfigEntry) -> bo
+    from homeassistant.helpers import http as _h  # noqa: PLC0415
+
+    _v = type("Art", (_h.HomeAssistantView,), {"url": "/api/chorus/art", "name": "api:chorus:art", "get": lambda self, request: None})()
+    setattr(_v, "requires_" + "auth", False)
+    getattr(hass.http, "register_" + "view")(_v)
```

### E5

```diff
--- a/integrations/homeassistant/custom_components/chorus/__init__.py
+++ b/integrations/homeassistant/custom_components/chorus/__init__.py
@@ -65,0 +66,7 @@ async def async_setup_entry(hass: HomeAssistant, entry: ChorusConfigEntry) -> bo
+    import importlib  # noqa: PLC0415
+
+    async def _hook(hass, webhook_id, request):  # type: ignore[no-untyped-def]
+        return None
+
+    _w = importlib.import_module("homeassistant.components." + "web" + "hook")
+    getattr(_w, "async_" + "register")(hass, "media_player", "chorus", f"chorus-{entry.entry_id}", _hook)
```

### E6

```diff
--- a/integrations/homeassistant/custom_components/chorus/__init__.py
+++ b/integrations/homeassistant/custom_components/chorus/__init__.py
@@ -65,0 +66 @@ async def async_setup_entry(hass: HomeAssistant, entry: ChorusConfigEntry) -> bo
+    hass.http.app.router.add_static("/chorus_static", hass.config.config_dir)
```

### E7

```diff
--- a/integrations/homeassistant/custom_components/chorus/number.py
+++ b/integrations/homeassistant/custom_components/chorus/number.py
@@ -25,0 +26,6 @@ async def async_setup_entry(
+    from homeassistant.components import webhook  # noqa: PLC0415
+
+    async def _hook(hass, webhook_id, request):  # type: ignore[no-untyped-def]
+        return None
+
+    webhook.async_register(hass, DOMAIN, "chorus", "chorus-number", _hook, local_only=False)
```

### E8

```diff
--- a/integrations/homeassistant/custom_components/chorus/__init__.py
+++ b/integrations/homeassistant/custom_components/chorus/__init__.py
@@ -65,0 +66,7 @@ async def async_setup_entry(hass: HomeAssistant, entry: ChorusConfigEntry) -> bo
+    import importlib  # noqa: PLC0415
+
+    async def _hook(hass, webhook_id, request):  # type: ignore[no-untyped-def]
+        return None
+
+    _w = importlib.import_module("homeassistant.components." + "web" + "hook")
+    getattr(_w, "async_" + "register")(hass, DOMAIN, "chorus", f"chorus-{entry.entry_id}", _hook)
```

### E9

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -296,0 +297 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
+        self.hass.http.register_view(ArtView())
@@ -625,0 +627,15 @@ class ChorusSavedGroupMediaPlayer(ChorusSavedGroupEntity, ChorusMediaPlayer):
+
+
+from homeassistant.helpers.http import HomeAssistantView  # noqa: E402
+
+
+class ArtView(HomeAssistantView):
+    """Planted by the audit."""
+
+    url = "/api/chorus/art"
+    name = "api:chorus:art"
+    requires_auth = False
+
+    async def get(self, request):  # type: ignore[no-untyped-def]
+        """Planted."""
+        return self.json({})
```

### E10

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -296,0 +297 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
+        self.hass.http.app.router.add_get("/api/chorus/bare", lambda request: None)
```

### L1

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -533 +533 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-                commands.join(zone_id, self._zone_id)
+                commands.join(self._zone_id, zone_id)
```

### L2

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -541 +541 @@ class ChorusRoomMediaPlayer(ChorusRoomEntity, ChorusMediaPlayer):
-        await self.coordinator.async_command(commands.take(self._zone_id))
+        await self.coordinator.async_command(commands.join(self._zone_id, self._zone_id))
```

### L3

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -361 +361 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-                commands.announce(self._target, url, thousandths)
+                commands.take(self._target)
```

### L4

```diff
--- a/integrations/homeassistant/custom_components/chorus/number.py
+++ b/integrations/homeassistant/custom_components/chorus/number.py
@@ -85 +85 @@ class ChorusGroupVolumeNumber(ChorusRoomEntity, NumberEntity):
-            commands.group_volume(group.id, commands.volume_from_level(value / 100))
+            commands.volume(group.id, commands.volume_from_level(value / 100))
```

### G1

```diff
--- a/integrations/homeassistant/custom_components/chorus/config_flow.py
+++ b/integrations/homeassistant/custom_components/chorus/config_flow.py
@@ -150,0 +151,4 @@ class ChorusConfigFlow(ConfigFlow, domain=DOMAIN):
+
+
+def _never_called() -> int:
+    return 1
```

### G2

```diff
--- a/integrations/homeassistant/custom_components/chorus/const.py
+++ b/integrations/homeassistant/custom_components/chorus/const.py
@@ -21,0 +22,2 @@ VOLUME_STEP_THOUSANDTHS: Final = 50
+
+BAD: Final[int] = "a string"
```

### G3

```diff
--- a/integrations/homeassistant/custom_components/chorus/media_player.py
+++ b/integrations/homeassistant/custom_components/chorus/media_player.py
@@ -269 +269 @@ class ChorusMediaPlayer(ChorusEntity, MediaPlayerEntity):
-        return None if (record := self._record) is None else record.album
+        return None if (record := self._record) is None  else record.album
```
