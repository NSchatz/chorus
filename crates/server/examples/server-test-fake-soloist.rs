//! The fake Soloist (`crates/soloist-fake` is the whole of it and says what
//! it fakes), as a program `crates/server/tests/soloist_receivers.rs` hands
//! the supervisor as its `--soloist-bin`. An example, never a `[[bin]]`: no
//! image and no release builds it.

fn main() {
    std::process::exit(chorus_soloist_fake::run(std::env::args().skip(1).collect()));
}
