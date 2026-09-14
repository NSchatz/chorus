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
	bash tools/pinning-check.sh
	bash tools/styling-check.sh
	bash tools/interface-craft-check.sh
	bash tools/refusals.sh
	bash tools/unrun-checks-are-visibly-unrun.sh
	bash tools/measure/capture-refusals.sh
	bash tools/wireless-expectations-check.sh
	$(MAKE) verify-comment-density

# Every image reference, every action reference and every dependency manifest in
# the tree, against the umbrella's documentation/pinning-conventions.md, plus the
# committed demonstrations that show the check going red on each shape it
# refuses. Needs no device, no privilege and NO NETWORK: it resolves no tag and
# asks no registry anything, which is why this repository carries no scheduled
# liveness workflow (P8). Also run by `make verify`; named separately so a red
# CI build says the pins broke rather than "the workspace".
verify-pinning: tools-executable
	bash tools/pinning-check.sh

# The control page's stylesheet SOURCES, against the umbrella's styling
# conventions S1 to S10: every colour and length resolving to a token, three
# tiers named for role, the 4px scale, a value chosen by hand per theme, the
# measured contrast ratio recorded beside each pair, separation by border and
# surface, and one accent hue. Plus the committed trees that show the check
# going red on each shape it refuses. Needs node, no device, no privilege and no
# network. Also run by `make verify`; named separately so a red CI build says
# the styling broke rather than "the verifications".
#
# It is a check of SOURCE TEXT and it is not the rendered one. What the page
# PAINTS - the contrast floors, the faces, the reconciliation of each recorded
# ratio against the pixels - is `make verify-ui`, in a real browser engine.
verify-styling: tools-executable
	bash tools/styling-check.sh

# The control page's committed IDENTITY, against the umbrella's interface-craft
# conventions C1 and C2: docs/interface-craft-record.md names a display face, a
# text face, an accent, a radius signature and a shadow signature with one
# sentence each and agrees with the token file, and no source under
# crates/server/src/ui carries an entry of C2's blocklist that the record does
# not name as an exception with its reason. Plus the committed trees that show
# the check going red on every entry of that list and on every other way it can
# fail. The source directory is RESOLVED rather than written down, so a fifth
# file landing beside the four is swept rather than missed.
#
# Needs node, no device, no privilege and no network. Also run by `make verify`;
# named separately so a red CI build says the design record broke rather than
# "the verifications".
#
# It is a check of SOURCE TEXT and it is not the rendered one, which is what C2
# asks for in as many words. What the page PAINTS is `make verify-ui`, in a real
# browser engine, and clauses C3 to C8 belong there and not here.
#
# EXIT CODES, distinct per failure mode. The repository's own verdict wins when
# there is one, because a demonstration that also failed is the less actionable
# of the two:
#
#   0   the record holds, this repository's sources carry no unnamed C2 entry,
#       and every committed demonstration produced what it demonstrates
#   2   an identity source carries a C2 blocklist entry the record does not name
#       as an exception; the entry, the file and the line are named
#   3   a capability this check needs is missing, refused in tools/lib.sh's shape
#   4   a category has stopped matching: the identity sources resolved to an
#       empty set, or a blocklist entry was tested against no source at all
#   5   the design record names an exception and gives it no reason, so the
#       exception is not granted and what it covered is reported
#   6   the design record names an exception for an entry no identity source
#       carries, so a permission that stopped being needed is withdrawn
#   7   the design record is absent
#   8   the design record is there and cannot be read
#   9   the design record does not parse in the shape this check expects
#   10  the record's five identity entries are wrong: one is missing, named
#       twice, carries no reason, or disagrees with the token file
#   11  a committed demonstration did not produce what it demonstrates, the base
#       tree did not pass, or the scan did not print what it measured
verify-interface-craft: tools-executable
	bash tools/interface-craft-check.sh

# The control-plane thread-population checks, run over and over on one build,
# plus two starved runs that must go red naming the busy-worker refusal. Those
# checks take workers from a fixed pool to grade AC-12, so one green run of them
# is one scheduling and not a determinism claim; the repetition count is
# committed in config/verification.conf. Needs no device, no privilege and no
# network. It is MINUTES rather than seconds, which is why it is its own target
# and its own CI step rather than part of `make verify`.
verify-control-determinism: tools-executable
	bash tools/control-determinism.sh

# Every tracked .rs file against the prose ceiling docs/comment-density-record.md
# declares, then the record itself, then the committed demonstrations that show
# this gate going red on each shape it refuses. Counted from the Rust token
# stream by crates/comment-density, never by matching a line against a pattern.
#
# Needs no device, no privilege and NO NETWORK: it opens no socket and asks no
# registry anything, and the counter takes no external crate, which is what buys
# that. It calls no require_* guard from tools/lib.sh either, so
# tools/unrun-checks-are-visibly-unrun.sh has nothing to register and stays
# complete. Also run by `make verify`; named separately so a red CI build says
# the prose ceiling broke rather than "the workspace".
#
# EXIT CODES, distinct per failure mode. When more than one holds, the first
# listed below wins, so the most actionable one is the one a reader sees:
#
#   0   every measured file is at or under the ceiling, the record agrees with
#       this gate and with the tree, and every demonstration produced what it
#       demonstrates
#   6   the sweep matched no tracked .rs file at all, which is a category that
#       has stopped matching rather than a compliant tree
#   5   a tracked .rs file could not be read; it is named and never skipped
#   4   a tracked .rs file could not be tokenized to completion; it is named
#       with the byte offset at which tokenizing stopped, and is never counted
#       as zero prose
#   2   a measured file is over the ceiling
#   3   docs/comment-density-record.md and this gate disagree: no ceiling
#       declared, a value this gate does not enforce, a row whose two code-line
#       counts differ, a row naming a path this gate does not measure, or a row
#       the tree no longer matches
#   7   a committed demonstration did not produce what it demonstrates
#   8   a capability this check needs is missing, refused in tools/lib.sh's shape
verify-comment-density:
	cargo run --quiet -p chorus-comment-density -- gate

# Rewrite the generated half of docs/comment-density-record.md from the tree as
# it stands. The before column is carried over untouched, so the untrimmed
# measurement survives; --baseline overwrites it and is a deliberate change to
# what the baseline means. Neither can make a red ceiling green: the ceiling is
# measured against the tree and never against the record.
comment-density-record:
	cargo run --quiet -p chorus-comment-density -- record

# The comment counter's own suite, alone, against its own fixtures. This is
# where every counting rule and every refusal path is graded, because a run of
# the gate over a compliant tree exits zero whether or not the counter reads a
# raw string correctly. Needs no device, no privilege and no network, and calls
# no require_* guard.
#
# The suite lives in a workspace crate, so CI reaches it through the
# `cargo test --workspace` step as well; this target is a grading lane and not a
# bypass, named separately so a red build says the counting broke rather than
# "the workspace". Deliberately not `make check`, which is the whole workspace
# suite: a counting assertion that reddens when an unrelated crate regresses
# grades nothing.
verify-comment-density-suite:
	cargo test --quiet -p chorus-comment-density

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

# The device-class checks that do not depend on the delay a device reports, run
# against the ALSA `null` device. See docs/sound-2.md for what `null` can and
# cannot stand in for.
verify-null-device: tools-executable
	CHORUS_CLIENT_DEVICE=null bash tools/stream-end-and-loss.sh
	CHORUS_CLIENT_DEVICE=null bash tools/start-fill-and-log-shape.sh
