//! The receiver supervisor (`chorus_soloistd::run`, the whole of
//! `chorus-soloistd`), as a program `crates/server/tests/soloist_receivers.rs`
//! can run: a test of one crate has no path to another crate's binary. An
//! example, never a `[[bin]]`: the supervisor that ships is
//! `crates/soloistd`'s own.

fn main() {
    std::process::exit(chorus_soloistd::run(std::env::args().skip(1).collect()));
}
