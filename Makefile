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

# The measurement run that needs two endpoints, an audio interface and real
# loudspeakers. Beside verify-device because it follows the same rule: it exits
# non-zero naming its missing prerequisite rather than reporting green.
verify-measure-device: tools-executable
	bash tools/measure/capture-run.sh

# Regenerate the committed measurement fixtures from their committed
# parameters. `cargo test -p chorus-measure --test report_shape` asserts the
# result is byte-identical to what is committed, so this target is for changing
# a fixture's parameters and never for making a red assertion green.
measure-fixtures:
	cargo run --quiet -p chorus-measure --bin chorus-measure -- fixtures

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
