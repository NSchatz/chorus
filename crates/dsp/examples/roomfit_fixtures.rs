//! `make roomfit-fixtures`: regenerates every recording in `fixtures/roomfit/` from its
//! `.params` file. For changing a fixture's parameters, never for making a red assertion green:
//! `crates/dsp/tests/roomfit.rs` asserts that regenerating reproduces every committed recording
//! byte for byte.
//!
//! Usage: `cargo run -p chorus-dsp --example roomfit_fixtures -- <fixtures/roomfit>`

use chorus_dsp::roomfit::synthetic::{wav_bytes, Params, Room};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(dir) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: roomfit_fixtures <fixtures/roomfit>");
        return ExitCode::from(2);
    };
    let mut names: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "params"))
            .collect(),
        Err(e) => {
            eprintln!("roomfit_fixtures: {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    };
    names.sort();
    for path in names {
        let made = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| Params::parse(&t))
            .and_then(|p| {
                let room = Room::from_params(&p)?;
                let out = dir.join(p.text("output")?);
                std::fs::write(&out, wav_bytes(room.sweep.rate_hz, &room.render()))
                    .map_err(|e| e.to_string())?;
                Ok(out)
            });
        match made {
            Ok(out) => println!("roomfit_fixtures: wrote {}", out.display()),
            Err(e) => {
                eprintln!("roomfit_fixtures: {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
