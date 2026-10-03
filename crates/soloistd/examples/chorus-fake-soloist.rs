//! `chorus-fake-soloist`: the fake Soloist the supervisor's tests run
//! (`crates/soloist-fake` is the whole of it and says what it fakes). An
//! example, never a `[[bin]]`: no image and no release builds it.

fn main() {
    std::process::exit(chorus_soloist_fake::run(std::env::args().skip(1).collect()));
}
