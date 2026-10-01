//! The room-correction bounds and the fitter's output, held across the two crates that own them
//! (goal 12's cross-checks, ADR "the DSP chain's cost on the chip").
//!
//! `crates/dsp` fits a room and spells the result as the catalog's `room_eq` command
//! (`room_eq_command_json`); `crates/control` is the catalog that has to take it. Each crate
//! tests its own side against a vector, so nothing yet said that what one emits the other
//! accepts, or that the two copies of the bounds (`chorus_dsp::settings::ROOM_EQ_*` and
//! `chorus_control::ROOM_EQ_*`) are the same numbers. The test lives here because this is where
//! the dependency graph already has both: `chorus-server` depends on `chorus-control` and
//! dev-depends on `chorus-dsp`, so neither library gains an edge to the other (and no cycle).
//! The C copy (`CHORUS_DSP_ROOM_EQ_*` against the wire's `CHORUS_V2_SOUND_EQ_*`) is held the
//! same way by `firmware/tests/test_dsp.c`.

use chorus_control::catalog::{decode_command, Command};
use chorus_dsp::roomfit::synthetic::{parse_wav, Params, Room};
use chorus_dsp::roomfit::{fit_recording, room_eq_command_json, FitConfig, Target};
use chorus_dsp::settings as dsp;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/roomfit")
}

/// The two crates' bounds are the same numbers, field by field.
#[test]
fn the_dsp_and_control_room_eq_bounds_are_the_same() {
    assert_eq!(dsp::ROOM_EQ_MAX_FILTERS, chorus_control::ROOM_EQ_MAX_FILTERS);
    assert_eq!(
        (dsp::ROOM_EQ_FREQ_MIN_HZ, dsp::ROOM_EQ_FREQ_MAX_HZ),
        chorus_control::ROOM_EQ_FREQ_HZ
    );
    assert_eq!(
        (dsp::ROOM_EQ_GAIN_MIN_CDB, dsp::ROOM_EQ_GAIN_MAX_CDB),
        chorus_control::ROOM_EQ_GAIN_CDB
    );
    assert_eq!(
        (dsp::ROOM_EQ_Q_MIN_MILLI, dsp::ROOM_EQ_Q_MAX_MILLI),
        chorus_control::ROOM_EQ_Q_MILLI
    );
}

/// Every fixture room the fitter is expected to fit: its `room_eq` command decodes and validates
/// in the catalog, and the catalog reads back exactly the filters the fit chose.
#[test]
fn the_catalog_accepts_every_fixture_fit_as_room_eq() {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixtures())
        .expect("fixtures/roomfit exists")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "params"))
        .collect();
    paths.sort();
    let mut fitted = 0;
    for p in paths {
        let params = Params::parse(&std::fs::read_to_string(&p).unwrap()).unwrap();
        if params.text("expect").unwrap() != "fit" {
            continue;
        }
        let room = Room::from_params(&params).unwrap();
        let bytes = std::fs::read(fixtures().join(params.text("output").unwrap())).unwrap();
        let (_, samples) = parse_wav(&bytes).unwrap();
        let fit = fit_recording(&samples, &room.sweep, &Target::flat(), &FitConfig::default())
            .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert!(!fit.filters.is_empty(), "{}: no filters", p.display());

        let json = room_eq_command_json("living", &fit.filters);
        let command =
            decode_command(&json).unwrap_or_else(|e| panic!("{}: `{json}` refused: {e}", p.display()));
        match command {
            Command::RoomEq {
                zone,
                filters: Some(filters),
                enabled: Some(true),
            } => {
                assert_eq!(zone, "living");
                let read_back: Vec<(u16, i16, u16)> = filters
                    .iter()
                    .map(|f| (f.freq_hz, f.gain_cdb, f.q_milli))
                    .collect();
                let chosen: Vec<(u16, i16, u16)> = fit
                    .filters
                    .iter()
                    .map(|f| (f.freq_hz, f.gain_cdb, f.q_milli))
                    .collect();
                assert_eq!(read_back, chosen, "{}: the catalog read `{json}`", p.display());
            }
            other => panic!("{}: `{json}` decoded as {other:?}", p.display()),
        }
        fitted += 1;
    }
    assert!(fitted >= 3, "only {fitted} fixture rooms are fitted");
}
