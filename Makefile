check:
	cargo build --workspace --all-targets
	cargo test --workspace

build:
	cargo build --workspace --all-targets

test:
	cargo test --workspace

probe:
	cargo run --quiet --release --example probe -p chorus-alsa -- null

# make one ARGS="..." runs a single test target, for a tighter loop.
one:
	cargo test $(ARGS)

# Every verification that needs no device and no privilege. This is what CI
# runs beside the suite.
verify: tools-executable
	bash tools/refusals.sh
	bash tools/unrun-checks-are-visibly-unrun.sh
	bash tools/measure/capture-refusals.sh

tools-executable:
	chmod +x tools/*.sh tools/measure/*.sh

# The verifications that need an environment. Each one exits non-zero naming
# its missing prerequisite rather than reporting green.
verify-device: tools-executable
	bash tools/stream-end-and-loss.sh
	bash tools/start-fill-and-log-shape.sh
	bash tools/delay-log-shape.sh
	bash tools/overflow-run.sh

verify-host: tools-executable
	bash tools/host-contract.sh
	bash tools/spin-test.sh

# PRODUCT-6's control plane, end to end: a real server, real endpoints and real
# control subscribers on real sockets. Needs a playback device that OPENS; the
# ALSA `null` device is enough, because what is graded is which stream an
# endpoint is on and what every subscriber was told, not the value of any
# reported delay.
verify-control: tools-executable
	bash tools/control-plane-run.sh

# AC-3: four endpoints attached and playing, the server SIGKILLed and replaced,
# every one back to advancing its played-frame counter with no operator action.
verify-restart-storm: tools-executable
	bash tools/restart-storm-run.sh

# AC-5, AC-6 and AC-10: the served page RENDERED in a real browser engine.
# Needs Chromium and the Playwright driver under tools/ui; refuses by name
# without either, and reading the CSS instead is not an option.
verify-ui: tools-executable
	bash tools/ui-render-run.sh

# AC-4: three days of wall clock and the RIG-3 capture rig. NOT PASSED in this
# repository; this target exits non-zero naming both. What stands beside it is
# the MODELLED 72 hours in `cargo test -p chorus-client-linux --test soak_72h`,
# which docs/verification-record.md labels a modelled result and not a
# measurement.
verify-soak: tools-executable
	bash tools/soak-run.sh

# The live multicast half of AC-2. Whether multicast reaches a container and
# crosses this network's VLANs is an open question, which is why the endpoint
# has a static fallback; this runs the live exchange where it can and refuses
# by name where it cannot. The packet-graded half needs none of it and is in
# `make check`.
verify-mdns: tools-executable
	bash tools/mdns-live-run.sh

# The other half of AC-2's fallback sentence: "connects to the configured static
# address AND PLAYS". Needs a playback device that opens and no multicast at
# all; the browse returning nothing is the antecedent, and a browse that cannot
# run at all makes this refuse by name rather than pass on an easier case.
verify-discovery-fallback: tools-executable
	bash tools/discovery-fallback-run.sh

# The measurement run that needs two endpoints, an audio interface and real
# loudspeakers. Beside verify-device because it follows the same rule: it exits
# non-zero naming its missing prerequisite rather than reporting green.
verify-measure-device: tools-executable
	bash tools/measure/capture-run.sh

# SYNC-4's AC-1: an hour of two wired endpoints playing one grouped stream,
# measured by the rig. Needs a second endpoint on top of everything
# verify-measure-device needs, and refuses by name without one. AC-1 is NOT
# passed in this repository and docs/verification-record.md says so.
verify-sync-hour: tools-executable
	bash tools/sync-hour-run.sh

# Regenerate the committed measurement fixtures from their committed
# parameters. `cargo test -p chorus-measure --test report_shape` asserts the
# result is byte-identical to what is committed, so this target is for changing
# a fixture's parameters and never for making a red assertion green.
measure-fixtures:
	cargo run --quiet -p chorus-measure --bin chorus-measure -- fixtures

# Regenerate the committed sync cross-check vectors from the committed
# scenarios. Same rule as measure-fixtures, and for the same reason:
# `cargo test -p chorus-sync --test crosscheck_vectors` asserts the result is
# byte-identical to what is committed, and firmware/tests/test_sync.c holds the
# C endpoint to the same files. This target is for changing a scenario and
# never for making a red assertion green.
sync-vectors:
	cargo run --quiet -p chorus-sync --bin chorus-sync-vectors

# Regenerate the committed DNS-SD packet vectors from their committed
# parameters. Same rule again: `cargo test -p chorus-discovery --test
# dnssd_vectors` asserts the result is byte-identical to what is committed, so
# this target is for changing a fixture's parameters and never for making a red
# assertion green.
discovery-vectors:
	cargo run --quiet -p chorus-discovery --bin chorus-discovery-vectors

# --- the ESP32-S3 endpoint ---------------------------------------------------
#
# The endpoint's host build and every verification of it that needs no device.
# Needs a C compiler and this workspace's own cargo; no ESP-IDF, no ESP32-S3,
# no amplifier, no privilege. What genuinely needs hardware lives behind
# tools/endpoint-rig-run.sh and refuses by name.
firmware-check: tools-executable
	$(MAKE) -f firmware/Makefile check

# The image build. Refuses by name without the ESP-IDF toolchain and the
# version firmware/config/endpoint.conf declares, and emits no partial image.
firmware-image: tools-executable
	bash tools/firmware-image.sh

# The three things CI names as separate, fail-on-red steps.
firmware-golden-vectors:
	$(MAKE) -f firmware/Makefile golden-vectors

firmware-sync-scenarios:
	$(MAKE) -f firmware/Makefile sync-scenarios

firmware-safety-scans:
	$(MAKE) -f firmware/Makefile safety-scans

# AC-1 and AC-3: an ESP32-S3 endpoint playing a grouped stream beside a Linux
# endpoint, measured by the RIG-3 rig, and the produced sample rate measured
# rather than read back from the configuration. NEITHER IS PASSED HERE. This
# target exits non-zero naming its missing prerequisite; see
# docs/verification-record.md, which quotes the refusal.
verify-endpoint-rig: tools-executable
	bash tools/endpoint-rig-run.sh

# The saved reports over the committed fixtures, which is what puts a file in
# docs/measurements/. Needs no device and no privilege.
measure-fixture-reports: tools-executable
	bash tools/measure/fixture-reports.sh

ten-minute-run: tools-executable
	bash tools/ten-minute-run.sh

# The device-class checks that do not depend on the delay a device reports, run
# against the ALSA `null` device. See docs/sound-2.md for what `null` can and
# cannot stand in for.
verify-null-device: tools-executable
	CHORUS_CLIENT_DEVICE=null bash tools/stream-end-and-loss.sh
	CHORUS_CLIENT_DEVICE=null bash tools/start-fill-and-log-shape.sh
