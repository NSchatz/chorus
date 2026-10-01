# Decision records

One markdown file per significant decision (BRIEF.md section 12). The rule for numbers, the
first line and the Status line is `docs/conventions.md` rule 15, held by
`tools/conventions/check-adrs.sh`. 0022-0024 were numbered 0012, 0021 and 0021 until 2026-09-30
(K48); 0025-0027 record decisions made before the program. From 0032 on, a record's number is
the number of the pull request that added it.

| Number | Record |
|---|---|
| 0001 | [Rust for the server and the Linux client](0001-language-rust.md) |
| 0002 | [repository layout and CI shape](0002-repository-layout-and-ci.md) |
| 0003 | [wire protocol framing, field layout and connection model](0003-wire-protocol-framing.md) |
| 0004 | [the audio chunk header reserves 14 opaque bytes for the TV path](0004-audio-chunk-reserved-bytes.md) |
| 0005 | [what a decoder does with a frame it cannot accept](0005-decoder-frame-validation.md) |
| 0006 | [the deterministic sync simulator and the servo it exercises](0006-sync-simulator-and-servo.md) |
| 0007 | [the server and Linux client language, argued on the merits](0007-server-client-language.md) |
| 0008 | [how the client reaches ALSA](0008-alsa-binding-and-the-audio-sink.md) |
| 0009 | [the transport, and telling a finished stream from a lost one](0009-stream-transport-and-end-of-stream.md) |
| 0010 | [the scheduling and memory contract, and why refusing is the safe answer](0010-the-host-contract.md) |
| 0011 | [the audio path is an enumeration, and the list is graded too](0011-the-audio-path-is-a-list.md) |
| 0012 | [the thread inventory is complete, or it is an error](0012-the-thread-inventory-is-complete-or-it-is-an-error.md) |
| 0013 | [the measurement rig, and the error it publishes with its numbers](0013-the-measurement-rig.md) |
| 0014 | [the sync loop on the real path, and every constant it fixed](0014-the-sync-loop-on-the-real-path.md) |
| 0015 | [the ESP32-S3 endpoint, and every constant it fixed](0015-the-esp32-s3-endpoint.md) |
| 0016 | [the control catalog, its version, and the volume range and curve](0016-the-control-catalog.md) |
| 0017 | [the control subscriber queue ceiling, and dropping the subscriber](0017-the-control-fanout.md) |
| 0018 | [the persisted zone state, its format, and what is deliberately not in it](0018-the-persisted-zone-state.md) |
| 0019 | [a persisted endpoint that never reconnects](0019-a-persisted-endpoint-that-never-comes-back.md) |
| 0020 | [checks that take a worker from the pool they grade](0020-checks-that-take-a-worker-from-the-pool-they-grade.md) |
| 0021 | [the comment density baseline](0021-the-comment-density-baseline.md) |
| 0022 | [the CPU-time bound goes on first, and a check says so](0022-the-cpu-time-bound-goes-on-first.md) |
| 0023 | [a measured contrast ratio is recorded beside the value](0023-a-measured-ratio-is-recorded-beside-the-value.md) |
| 0024 | [the wireless tier](0024-the-wireless-tier.md) |
| 0025 | [multicast DNS and DNS-SD are hand-written, with no external crate](0025-hand-written-mdns.md) |
| 0026 | [the control plane is HTTP with server-sent events, not WebSocket](0026-http-and-server-sent-events-for-the-control-plane.md) |
| 0027 | [the audio port is 4010 and the control port is 4020](0027-the-audio-port-4010-and-the-control-port-4020.md) |
| 0039 | [the v2 key exchange is Noise XX with trust-on-first-use pins, over vendored primitives](0039-the-v2-key-exchange.md) |
| 0040 | [the server negotiates the codec in one step, from a per-link preference and the endpoint's capabilities](0040-codec-negotiation.md) |
| 0041 | [protocol v2 keeps v1's frame, carries whole frames in records, and refuses v1 by name](0041-protocol-v2-framing.md) |
| 0042 | [the endpoint moves to ESP-IDF v6.1](0042-esp-idf-v6-1.md) |
| 0043 | [the endpoint's key exchange is chorus's own state machine over the PSA Crypto API, and its host build compiles the pinned ESP-IDF tree's library](0043-the-endpoint-crypto-over-psa.md) |
| 0044 | [FLAC and Opus decode through vendored dr_flac and libopus on the endpoint, and Symphonia and the same libopus in the Linux client](0044-the-vendored-decoders.md) |
| 0047 | [host evidence comes from a chorus-owned probe crate with two small FFI modules](0047-host-probes.md) |
| 0048 | [the house-scale simulation is many single-client runs against one server timeline, configured under config/sim-house](0048-the-house-scale-simulation.md) |
| 0049 | [bench results come back as schema-checked reports on bench/* branches, raw data hashed and committed up to a size limit](0049-bench-reports.md) |
| 0050 | [a FIFO source is held open and fills silence, and the ALSA null runs are a labelled host step in the gate](0050-fifo-source-and-alsa-null-runs.md) |
| 0054 | [the device-class scripts report on a real device only, a FAIL is published, and a published run hands the checkout back](0054-device-class-bench-reports.md) |
| 0057 | [board profiles over endpoint.conf, the W5500 as the default link, and one image per link profile in the gate](0057-board-profiles-and-the-wired-link.md) |
| 0058 | [the endpoint plays through a jitter buffer whose device delay is the frames the I2S DMA consumed, counted and stamped in the interrupt, and corrects by inserting and dropping frames](0058-the-endpoint-playout-path.md) |
| 0060 | [the endpoint has a serial console for the bench, with runtime-only values, and chorus-measure recovers the produced sample rate](0060-the-endpoint-console.md) |
| 0062 | [chorus flashes through one guarded tool, and the flash guard is checked by a per-language whitelist and by running the tool](0062-the-guarded-flashing-tool.md) |
| 0063 | [a speaker's buttons are the controller role, its status LED follows the visualizer, and its microphone sits behind a gate that starts closed](0063-controls-led-and-mic-gate.md) |
| 0064 | [the TAS5825M's register map comes from TI's datasheet with its page on every line, and bring-up follows the datasheet's startup procedure with 32-bit slots on the wire](0064-the-tas5825m-register-map-and-bring-up.md) |
| 0065 | [the endpoint reports its heap, stacks and FIFO on its console, marks server-timeline boundaries on a GPIO, and EMBEDDED-5's bring-up is a bench run with disconnect abuse](0065-the-embedded5-bring-up-and-the-gpio-marker.md) |
| 0066 | [a Linux endpoint's line-in is the source role: captured through ALSA, offered by signal presence, sent upstream as PCM stamped at its capture instant through the sync offset](0066-line-in-capture-as-the-source-role.md) |
| 0067 | [one controller model in two languages, a Linux front panel read through evdev and the LED class, and the server routes an endpoint's controller commands through the control plane](0067-the-linux-front-panel-and-one-controller-model.md) |
| 0068 | [chorus-client drives N device channels through an output map at the sink's edge, and multichannel on the Linux tier is a Pi 5 parallel-lane DAC HAT](0068-the-output-map.md) |
| 0069 | [the Linux endpoint ships as one .deb per architecture, cross-built for glibc 2.36 with zig, run by a hardened systemd unit with real-time limits](0069-the-linux-endpoint-package.md) |
| 0071 | [a line-in's latency grows from L_local to L_group by a bounded, raised-cosine time stretch planned on the server, proven against a stated glitch criterion in the simulator](0071-latency-growth.md) |
| 0072 | [the schedule library is a pure crate that reads no clock: civil time from TZif with a gap and a fold rule, weekly windows, alarms, sleep fades, integer ramps and generated chimes](0072-the-schedule-library.md) |
| 0075 | [control catalog v2: rooms, bonded sets with a wired-only rule, saved and live groups, take-the-room, Sonos-style group volume, clamped limits and quiet hours, alarm, sleep and autoplay configuration, state-file format 2](0075-control-catalog-v2.md) |
| 0074 | [the room's gain and limit travel on the audio wire as room_volume, and both endpoint kinds enforce min(ramped gain, limit, own ceiling) at every frame](0074-room-volume-on-the-audio-wire.md) |
| 0076 | [the schedule runtime is a pure server module that fires alarms, counts sleep timers down, follows quiet hours and runs line-in autoplay on time it is handed, and returns effects for the conductor](0076-the-schedule-runtime.md) |
| 0077 | [one server serves every group's stream on stream slots cut on one grid, routes each session to its group inside the session, pushes room_volume and controller_state from one conductor, and writes every event stream from one thread](0077-stream-slots-and-one-event-writer.md) |
| 0078 | [the house soak: one --slots 8 server and ten real endpoints on ALSA null under a seeded command load, graded for an hour, reported as a host run that is not timing evidence](0078-the-house-soak.md) |
| 0079 | [the conductor runs the schedule runtime on the civil and monotonic clocks, the slots play generated chimes and endpoints' line-ins, and a line-in's latency grows on the slots' one grid](0079-the-schedule-runtime-wired-into-the-server.md) |
| 0081 | [a room's tone, loudness, night mode, speech enhancement, bass management and room-correction EQ are catalog v2 commands, persisted in state-file format 3, and reach every endpoint of the room as protocol v2 `sound` with its own bonded role](0081-per-room-sound-in-the-catalog-and-on-the-wire.md) |
| 0082 | [the DSP library is one algorithm in two languages, crates/dsp and firmware/src/dsp.c, held to shared fixtures whose expected values come from cited worked examples, and the endpoint chain ends in a look-ahead limiter at the room's limit](0082-the-dsp-library.md) |
| 0083 | [room-correction fitting deconvolves a Farina sweep, smooths it, and greedily fits at most eight peaking filters that stay inside the room_eq bounds, prefer cuts, never boost past +3 dB and leave nulls alone](0083-room-correction-fitting.md) |
