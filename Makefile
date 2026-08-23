check:
	cargo build --workspace --all-targets
	cargo test --workspace

build:
	cargo build --workspace --all-targets

test:
	cargo test --workspace

probe:
	cargo run --quiet --release --example probe -p chorus-alsa -- null

# make one T=<target> runs a single test target, for a tighter loop.
one:
	cargo test $(ARGS)
