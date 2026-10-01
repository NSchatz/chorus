.DEFAULT_GOAL := check

# The ESP-IDF environment `make gate` sources when IDF_PATH is unset: the
# rootless install this repository is built with (ESP-IDF v6.1 under /cache,
# see firmware/config/endpoint.conf). Point it elsewhere for another install.
CHORUS_IDF_ENV ?= /cache/esp/chorus-idf-v6.1-export.sh
export CHORUS_IDF_ENV

# The gate: every check a change passes before it merges, each step timed
# (tools/gate.sh says what it runs). Callers take chorus-heavy.lock; the recipe
# never does. gate-fast is the conventions checks alone (tools/conventions/), for
# docs-only changes.
gate: tools-executable
	bash tools/gate.sh full

gate-fast: tools-executable
	bash tools/gate.sh fast

# The chorus-server OCI image as a tarball, built with no container daemon, and
# its test: unpacked, `--help` run, the control plane read back (tools/image.sh).
image: tools-executable
	bash tools/image.sh

# The Linux endpoint package: one .deb per architecture (arm64, amd64), cross-built
# rootless with zig and cargo-zigbuild at the glibc floor, and checked (readelf,
# dpkg-deb, systemd-analyze verify of the unit). ARCHES="amd64" builds one
# (tools/endpoint-package.sh; docs/linux-endpoint.md installs it).
endpoint-packages: tools-executable
	bash tools/endpoint-package.sh $(ARCHES)

# The artifacts of one release into dist/v$(VERSION)/ (tools/release.sh;
# docs/release.md says how a release is cut). Publishes nothing.
release: tools-executable
	bash tools/release.sh $(VERSION)

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

# Every verification that needs no device and no privilege and answers in
# seconds. This is what CI runs beside the suite. The one no-device verification
# that is NOT here is verify-control-determinism, which is minutes of repeated
# runs by design; it has its own target and its own CI step so that a red build
# says the determinism claim broke rather than "the verifications".
verify: tools-executable
	bash tools/refusals.sh
	bash tools/unrun-checks-are-visibly-unrun.sh
	bash tools/measure/capture-refusals.sh
	bash tools/wireless-expectations-check.sh
	bash tools/bench/e2e-test.sh

# The control-plane thread-population checks, run over and over on one build,
# plus two starved runs that must go red naming the busy-worker refusal. Those
# checks take workers from a fixed pool to grade AC-12, so one green run of them
# is one scheduling and not a determinism claim; the repetition count is
# committed in config/verification.conf. Needs no device, no privilege and no
# network. It is MINUTES rather than seconds, which is why it is its own target
# and its own CI step rather than part of `make verify`.
verify-control-determinism: tools-executable
	bash tools/control-determinism.sh

tools-executable:
	chmod +x tools/*.sh tools/measure/*.sh tools/bench/*.sh

# The verifications that need an environment. Each one exits non-zero naming
# its missing prerequisite rather than reporting green. All four run even when
# one fails, because on a real device each writes its own bench report (and with
# CHORUS_BENCH_PR=1 opens its own PR, a FAIL included); the target still exits
# non-zero if any did.
verify-device: tools-executable
	@rc=0; for s in stream-end-and-loss start-fill-and-log-shape delay-log-shape overflow-run; do \
		echo "bash tools/$$s.sh"; bash tools/$$s.sh || rc=1; \
	done; exit $$rc

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

# The house-scale simulation report (docs/decisions/0048-the-house-scale-simulation.md). A simulation,
# never timing evidence; it names the commit it was built from, which has to be
# on main for docs/measurements/ to accept it.
sim-house:
	cargo run --release --quiet -p chorus-sync --bin chorus-sim-house -- \
		--build "$$(git rev-parse HEAD)" --out docs/measurements/sim-house-8-rooms.md

# Regenerate the committed DNS-SD packet vectors from their committed
# parameters. Same rule again: `cargo test -p chorus-discovery --test
# dnssd_vectors` asserts the result is byte-identical to what is committed, so
# this target is for changing a fixture's parameters and never for making a red
# assertion green.
discovery-vectors:
	cargo run --quiet -p chorus-discovery --bin chorus-discovery-vectors

# Regenerate the committed time zone fixtures from the host's tz database
# (fixtures/README.md, schedule/). A different tzdata is a reviewed diff:
# `cargo test -p chorus-schedule` must still agree with the zdump listing the
# same run writes, and a changed expectation is a changed rule, never drift.
schedule-fixtures: tools-executable
	bash tools/schedule-fixtures.sh

.PHONY: schedule-fixtures

# Regenerate the room_volume sequence the real server sends over every volume
# path (fixtures/README.md, volume/), from a run of the test that captures it.
# The same test, run without this, asserts the run reproduces the committed
# file byte for byte, and firmware/tests/test_volume.c holds the C endpoint to
# it. For changing a step of that test, never for making a red assertion green.
volume-sequence:
	CHORUS_WRITE_FIXTURES=1 cargo test --quiet -p chorus-server --test limits_hold_for_every_volume_path

.PHONY: volume-sequence

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

# Guardrail 2 over a BUILT image: the configuration the build generated and
# what the app and the bootloader actually link (audit A-15, A-16). Needs the
# exported ESP-IDF environment and a build directory:
#   make firmware-image-guard FIRMWARE_BUILD_DIR=<the idf.py -B directory>
# Its self-test, which needs neither, runs in firmware-safety-scans.
FIRMWARE_BUILD_DIR ?= firmware/build/image
firmware-image-guard: tools-executable
	bash tools/firmware-image-guard.sh $(FIRMWARE_BUILD_DIR)

# Flash the built image: the owner's act at the bench. tools/firmware-flash.sh
# refuses unless the owner set the owner-at-bench variable on their own command
# line (docs/bench-packet.md); this target never sets it. PORT names the board.
firmware-flash: tools-executable
	bash tools/firmware-flash.sh $(if $(PORT),--port $(PORT))

# The three things CI names as separate, fail-on-red steps.
firmware-golden-vectors:
	$(MAKE) -f firmware/Makefile golden-vectors

firmware-sync-scenarios:
	$(MAKE) -f firmware/Makefile sync-scenarios

firmware-safety-scans:
	$(MAKE) -f firmware/Makefile safety-scans

# chorus#WIFI-7's host-gradable half: the power save mode the endpoint SETS
# rather than inherits, the readback, the two ways that mode can fail to be in
# effect, and the refusal to join on a credential this repository does not have.
# Named separately so a red CI build says the wireless bring-up broke rather
# than "the endpoint". Also run by `make firmware-check`.
firmware-wireless:
	$(MAKE) -f firmware/Makefile wireless

# AC-1 and AC-3: an ESP32-S3 endpoint playing a grouped stream beside a Linux
# endpoint, measured by the RIG-3 rig, and the produced sample rate measured
# rather than read back from the configuration. NEITHER IS PASSED HERE. This
# target exits non-zero naming its missing prerequisite; see
# docs/verification-record.md, which quotes the refusal.
verify-endpoint-rig: tools-executable
	bash tools/endpoint-rig-run.sh

# chorus#WIFI-7's AC-2 and AC-3: a Wi-Fi endpoint measured with modem sleep
# disabled and again with the platform default left in place, beside a second
# endpoint in another room, captured by the RIG-3 rig. NEITHER IS PASSED HERE.
# This target exits non-zero naming its missing prerequisite; see
# docs/verification-record.md, which quotes the refusal. What stands beside it
# with no radio present is the report shape and the arithmetic, graded against
# committed MODELLED series by `cargo test -p chorus-measure --test
# report_shape`, which those series and every report written from them label as
# modelled and not as a measurement.
verify-wireless: tools-executable
	bash tools/wireless-characterization-run.sh

# The saved reports over the committed fixtures, which is what puts a file in
# docs/measurements/. Needs no device and no privilege.
measure-fixture-reports: tools-executable
	bash tools/measure/fixture-reports.sh

ten-minute-run: tools-executable
	bash tools/ten-minute-run.sh

# The same ten minutes on the ALSA `null` device, labelled host / ALSA null: the
# continuity half (zero underruns, no rate change, the buffer under its
# ceiling). The delay bounds are NOT GRADED and the ten-minute criterion is not
# passed by it; see tools/ten-minute-run.sh.
ten-minute-run-null: tools-executable
	CHORUS_CLIENT_DEVICE=null CHORUS_TEN_MINUTE_NULL=1 bash tools/ten-minute-run.sh

# `make gate`'s ALSA `null` step: stream-end-and-loss, the restart storm and the line-in probe on
# `null`, finding a rootless libasound when the system has none
# (CHORUS_ALSA_PREFIX, default /cache/opt/chorus-alsa). See tools/alsa-null-run.sh.
verify-alsa-null: tools-executable
	bash tools/alsa-null-run.sh

# The device-class checks that do not depend on the delay a device reports, run
# against the ALSA `null` device. See docs/sound-2.md for what `null` can and
# cannot stand in for.
verify-null-device: tools-executable
	CHORUS_CLIENT_DEVICE=null bash tools/stream-end-and-loss.sh
	CHORUS_CLIENT_DEVICE=null bash tools/start-fill-and-log-shape.sh
 \
	.PHONY: release image endpoint-packages build check discovery-vectors firmware-check firmware-golden-vectors  \
	firmware-flash firmware-image firmware-image-guard firmware-safety-scans firmware-sync-scenarios  \
	firmware-wireless gate gate-fast measure-fixture-reports measure-fixtures one  \
	probe sim-house sync-vectors ten-minute-run ten-minute-run-null test tools-executable verify verify-alsa-null verify-control  \
	verify-control-determinism verify-device verify-discovery-fallback  \
	verify-endpoint-rig verify-host verify-mdns verify-measure-device  \
	verify-null-device verify-restart-storm verify-soak verify-sync-hour  \
	verify-wireless 
