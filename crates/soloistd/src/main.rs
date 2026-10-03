//! `chorus-soloistd`: the supervisor of one Spotify Soloist receiver
//! (`docs/soloist.md`). Everything is in the library, so a test binary can
//! run the same code.

fn main() {
    std::process::exit(chorus_soloistd::run(std::env::args().skip(1).collect()));
}
