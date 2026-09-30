//! Compiles the vendored libopus (`third_party/opus`) from the one list every
//! build reads, `third_party/opus/chorus-build.txt`: its definitions and its
//! translation units. The endpoint's host build (`firmware/Makefile`) and the
//! ESP-IDF image read the same list, so this crate runs the endpoint's decoder
//! code, compiled the same way (fixed point, `-ffp-contract=off`,
//! `-fno-fast-math`). libopus's conformance tool `src/opus_compare.c` is
//! compiled beside it with its `main` renamed, so the fixture tests can run the
//! official judge in-process.

use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let opus = manifest.join("../../third_party/opus");
    let list_path = opus.join("chorus-build.txt");
    let list = fs::read_to_string(&list_path).expect("third_party/opus/chorus-build.txt");
    println!("cargo:rerun-if-changed={}", list_path.display());

    let mut build = cc::Build::new();
    build
        .include(opus.join("include"))
        .include(opus.join("celt"))
        .include(opus.join("silk"))
        .include(opus.join("src"))
        .flag_if_supported("-std=c11")
        .flag_if_supported("-ffp-contract=off")
        .flag_if_supported("-fno-fast-math")
        // Optimised in every profile, as the endpoint builds it: libopus
        // warns that it is "very slow" unoptimised, and a debug test build
        // would otherwise decode at a fraction of real time.
        .opt_level(2)
        .warnings(false);
    for line in list.lines() {
        if let Some(name) = line.strip_prefix("define ") {
            build.define(name.trim(), None);
        } else if let Some(unit) = line.strip_prefix("source ") {
            let path = opus.join(unit.trim());
            println!("cargo:rerun-if-changed={}", path.display());
            build.file(path);
        }
    }
    build.compile("chorus_opus");

    let compare = opus.join("src/opus_compare.c");
    println!("cargo:rerun-if-changed={}", compare.display());
    cc::Build::new()
        .file(compare)
        .define("main", "chorus_opus_compare_main")
        .warnings(false)
        .compile("chorus_opus_compare");
    println!("cargo:rustc-link-lib=m");
}
