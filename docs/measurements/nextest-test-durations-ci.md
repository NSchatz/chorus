# The workspace tests' own durations on the nightly gate, and the per-test limits taken from them

Source: host
Build measured: `f1fe36c0ced3d6393f6e527c3eac49739beba115`

Not timing evidence for the audio path: these are how long the tests take on a CI runner, read
to size their per-test limits (`docs/decisions/0000-a-test-past-its-limit-is-ended.md`).

What ran: the nightly `make gate-full` on GitHub Actions (`ubuntu-latest`, `CARGO_BUILD_JOBS=2`),
its test step `cargo nextest run --workspace --locked` under cargo-nextest 0.9.146 with
`test-threads = 8`. Two runs, each 1926 tests, all passed: run 40194545229 on 2026-10-25 at the
commit above (Summary 439.678 s, one test SLOW past 60 s) and run 39606025523 on 2026-10-21 at
`742a66653f37e6add6eaf698731e0c8687a88b18` (Summary 417.977 s). The durations are nextest's own
per-test times from each run's `target/gate/test.log` (the `gate-logs` artifact, kept because a
later step of each run failed); a test's own time excludes its wait for a test-group slot.

Every test over 20 s in either run, with the slower of its two times, and the limit set in
`.config/nextest.toml`: 3x the time, rounded up to 10 s. Every other test took under 20 s in
both runs and keeps the 60 s default.

| Package | Binary | Test | Slower time (s) | Limit (s) |
|---|---|---|---|---|
| chorus-client-linux | tv_capture | a_1_khz_tone_from_a_tv_200_ppm_slow_and_drifting_comes_out_within_1_ppm_of_nominal | 63.757 | 200 |
| chorus-client-linux | tv_capture | a_1_khz_tone_from_a_tv_1400_ppm_fast_comes_out_within_1_ppm_of_nominal | 59.996 | 180 |
| chorus-client-linux | tv_capture | a_tv_200_ppm_fast_keeps_its_stamps_on_its_capture_instants_for_ten_minutes | 53.015 | 160 |
| chorus-client-linux | tv_capture | a_tv_1400_ppm_fast_keeps_its_stamps_on_its_capture_instants_for_ten_minutes | 50.518 | 160 |
| chorus-client-linux | tv_capture | a_tv_200_ppm_slow_keeps_its_stamps_on_its_capture_instants_for_ten_minutes | 46.231 | 140 |
| chorus-client-linux | tv_capture | a_tv_1400_ppm_slow_keeps_its_stamps_on_its_capture_instants_for_ten_minutes | 45.753 | 140 |
| chorus-protocol | firmware_session | a_good_image_installs_and_confirms_and_a_bad_one_is_rolled_back_over_a_real_session | 48.103 | 150 |
| chorus-server | adoption | a_new_speaker_is_adopted_named_and_assigned_a_room | 26.008 | 80 |
| chorus-server | regress_0031_f1 | serve_forever_still_serves_a_second_client_after_the_first_stream_ends | 20.723 | 70 |
| chorus-server | soloist_receivers | an_alarm_whose_receiver_never_reports_playing_rings_the_chime_after_the_bound | 22.778 | 70 |
| chorus-server | tv_capture_rate_match | a_tv_300_ppm_fast_on_optical_is_rate_matched_so_the_servers_port_neither_drains_nor_fills | 41.364 | 130 |
| chorus-server | tv_capture_rate_match | negative_control_the_same_tv_as_a_line_in_drifts_at_its_300_ppm | 41.223 | 130 |
| chorus-server | tv_low_latency | loss_on_both_legs_is_repaired_by_the_fec_and_is_lost_without_it | 26.014 | 80 |
| chorus-server | upnp_control_point | searches_are_answered_by_target_inside_mx_and_never_without_it | 26.860 | 90 |
| chorus-sync | latency_growth | the_committed_report_is_what_the_generator_writes | 55.838 | 170 |
| chorus-sync | latency_growth | a_wired_room_joins_and_leaves_without_a_glitch | 55.577 | 170 |
| chorus-sync | latency_growth | a_run_reproduces | 55.150 | 170 |
| chorus-sync | latency_growth | the_scenarios_take_each_shared_number_from_its_one_copy | 54.592 | 170 |
| chorus-sync | latency_growth | the_naive_jump_fails_the_criterion_with_a_gap | 53.749 | 170 |
| chorus-sync | latency_growth | a_wifi_room_joins_when_the_buffer_is_deep_enough_and_the_playing_room_never_glitches | 47.176 | 150 |
| chorus-sync | lowlat_budget | below_the_floor_a_repaired_chunk_misses_its_playout_point | 35.431 | 110 |
| chorus-sync | lowlat_budget | a_run_repeats_exactly | 35.307 | 110 |
| chorus-sync | lowlat_budget | at_or_above_the_floor_no_chunk_is_ever_late | 31.315 | 100 |
| chorus-sync | lowlat_budget | every_committed_scenario_is_listed_and_named_after_its_file | 30.773 | 100 |
| chorus-sync | lowlat_budget | bursts_beat_a_single_parity_and_the_interleave_wins_some_back | 29.811 | 90 |
| chorus-sync | lowlat_budget | a_clean_wired_link_loses_nothing | 29.310 | 90 |
| chorus-sync | lowlat_budget | fec_removes_the_independent_loss_its_control_shows | 28.986 | 90 |
| chorus-sync | lowlat_budget | the_committed_report_is_what_the_generator_writes | 28.189 | 90 |

`tv_capture_rate_match` has two tests and both are over 20 s, so one binary-level filter at the
slower one's limit covers it. The slowest test left at the default is
`chorus-client-linux::soak_72h seventy_two_modelled_hours_hold_the_bound_and_every_resync_has_a_name`,
19.331 s at its slower, 0.67 s under a third of 60 s.
