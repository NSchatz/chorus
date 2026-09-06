//! Regenerate the committed sync cross-check vectors.
//!
//! Everything this does is in `chorus_sync::crosscheck`, so the generator and
//! the test that grades it cannot disagree about the format. See that module
//! for why the vectors are files.
//!
//!   make sync-vectors

fn main() {
    chorus_sync::crosscheck::write_all();
}
