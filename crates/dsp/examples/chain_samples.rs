//! Prints a `chain` fixture's golden samples: the Rust chain's output over the
//! fixture's last 64 frames, as `samples_from`, `samples_tolerance` and one
//! `samples.<o>` line per output. `tools/dsp-fixtures/generate.py` appends
//! them to the fixtures that ask (`golden = 1`); they hold the C chain to the
//! Rust chain sample for sample, and they are not a worked example.
//!
//! `cargo run -p chorus-dsp --example chain_samples -- fixtures/dsp/<file>`

use chorus_dsp::fixture::{run_chain, Fields};

/// How many frames the golden lines carry, from the end of the run.
const COUNT: usize = 64;

fn main() {
    let path = std::env::args().nth(1).expect("a chain fixture path");
    let text = std::fs::read_to_string(&path).expect("the fixture reads");
    let fields = Fields::parse(&text).expect("the fixture parses");
    let run = run_chain(&fields, 4096).expect("the chain runs");
    let frames = run.outputs.first().map_or(0, Vec::len);
    let from = frames.saturating_sub(COUNT);
    println!("samples_from = {from}");
    println!("samples_tolerance = 1e-6");
    for (o, out) in run.outputs.iter().enumerate() {
        let values: Vec<String> = out[from..].iter().map(|v| format!("{v:?}")).collect();
        println!("samples.{o} = {}", values.join(" "));
    }
}
