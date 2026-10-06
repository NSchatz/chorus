//! The design-record seam: every file under `fixtures/design-record/`, read by
//! `chorus_dsp::design_record` and run through chorus's own biquads
//! (`fixtures/README.md`, `design-record/`; `docs/dsp.md`, "Design records").
//! Rust-only by declaration: a record is read where a design is turned into
//! settings, which no endpoint does.
//!
//! A record is the plain JSON the acoustics package of the owner's shared
//! Python library exports. Beside each `<name>.json` is `<name>.provenance`:
//! the library release, the export command, the date and the file's sha256,
//! which this test recomputes, so the committed record is the exported one.
//!
//! What is held, per crossover of a record:
//!
//! 1. the designed response of the record's biquads (`f64`, on the unit
//!    circle) at every response point, to the record's own `tolerance_db`;
//! 2. the running `f32` filters built from those biquads: a sine at each
//!    response frequency goes through both branches, and once settled each
//!    branch's level (where the record puts it above [`RUN_FLOOR_DB`], the
//!    crossover frequency always among them) and the level of the two added
//!    are within [`RUN_TOLERANCE_DB`] of the record's points, once what
//!    rounding the coefficients to `f32` does to the design is taken out
//!    (itself held under [`ROUNDING_LIMIT_DB`]: it shows at a low crossover);
//! 3. for an LR4, that the record's sections are the ones chorus designs for
//!    the same rate and frequency, and that `Lr4` built from the record gives
//!    the cascade's samples bit for bit.
//!
//! A file this reader does not know is a failure, not a skip, and so is a
//! record with a schema version it does not know.

use std::path::{Path, PathBuf};

use chorus_dsp::biquad::{Biquad, Coefficients};
use chorus_dsp::crossover::{db_of, Lr4, Lr4Design};
use chorus_dsp::design_record::{
    Crossover, CrossoverKind, DesignRecord, RecordError, SCHEMA, VERSION,
};
use chorus_dsp::fixture::Fields;

/// How closely the running `f32` filters must match a record's response
/// points, in dB. The record's own `tolerance_db` (1e-6) is for a response
/// computed in double precision; a filter running in single precision "agrees
/// to its own precision, which is the consumer's to state" (the record's
/// schema). Stated here: 0.001 dB, about 0.01 % in amplitude, once the
/// rounding of the coefficients themselves ([`ROUNDING_LIMIT_DB`]) is taken
/// out. The run prints the largest difference it measured (`--nocapture`):
/// 0.000027 dB for `lr4-2000hz-48k.json` on 2026-10-06 with that rounding
/// left in, 0.000011 dB with it out; 0.000990 dB for `chorus-sub-v1.json`
/// (the mains' branch at 40 Hz, 24.6 dB down).
const RUN_TOLERANCE_DB: f64 = 0.001;

/// A branch is held to its point by the running check only where the record
/// puts it above this level: further down, a single-precision filter's own
/// rounding noise is a visible part of what comes out. The designed response
/// (check 1) holds every point at every level.
const RUN_FLOOR_DB: f64 = -40.0;

/// How far the designed response of a record's sections may move when their
/// coefficients are rounded to the `f32` the running filters hold, in dB. A
/// section's gain at the bottom of the band is its numerator over
/// `1 + a1 + a2`, and at a low crossover that denominator is small beside the
/// rounding of `a1` and `a2` (half a unit in the last place each: 6.0e-8 and
/// 3.0e-8). For the subwoofer's 80 Hz at 48 kHz `1 + a1 + a2` is 1.09e-4, so a
/// section may move by 8.2e-4 of its gain and the two of an LR4 branch by
/// 0.0143 dB; at 2000 Hz the same sum is 0.058 and the bound is 0.00003 dB.
/// Stated here: 0.02 dB. The run prints the largest it found (`--nocapture`):
/// 0.006324 dB for `chorus-sub-v1.json` on 2026-10-06, at 20 Hz.
const ROUNDING_LIMIT_DB: f64 = 0.02;

/// The amplitude of the sine the running check plays.
const SINE_AMPLITUDE: f64 = 0.5;

/// How far a record's coefficients may be from chorus's own design of the same
/// crossover: two double-precision evaluations of the same cookbook formulas.
const DESIGN_TOLERANCE: f64 = 1e-12;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/design-record")
}

/// One record with what its provenance file says of it.
#[derive(Debug)]
struct Loaded {
    name: String,
    record: DesignRecord,
    release: String,
}

/// Reads every file of a fixture directory, or says which one it could not:
/// a file that is neither a record nor a provenance, a record without its
/// provenance (and the reverse), a provenance that lacks a field or whose
/// sha256 is not the record's, and a record the reader refuses.
fn walk(dir: &Path) -> Result<Vec<Loaded>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .map(|e| e.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    files.sort();
    let mut loaded = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let sibling = |ext: &str| {
            let other = path.with_extension(ext);
            if other.is_file() {
                Ok(other)
            } else {
                Err(format!("{name}: no .{ext} file beside it"))
            }
        };
        match path.extension().and_then(|e| e.to_str()) {
            _ if !path.is_file() => return Err(format!("{name}: not a file")),
            Some("provenance") => {
                sibling("json")?;
            }
            Some("json") => {
                let bytes = std::fs::read(path).map_err(|e| format!("{name}: {e}"))?;
                let text = String::from_utf8(bytes.clone()).map_err(|e| format!("{name}: {e}"))?;
                let record = DesignRecord::parse(&text).map_err(|e| format!("{name}: {e}"))?;
                let prov_path = sibling("provenance")?;
                let prov_name = prov_path.file_name().unwrap().to_string_lossy().to_string();
                let prov = std::fs::read_to_string(&prov_path)
                    .map_err(|e| format!("{prov_name}: {e}"))
                    .and_then(|t| Fields::parse(&t).map_err(|e| format!("{prov_name}: {e}")))?;
                let field = |key: &str| match prov.get(key) {
                    Some(v) if !v.is_empty() => Ok(v.to_string()),
                    _ => Err(format!("{prov_name}: no '{key}'")),
                };
                for key in ["source", "command", "exported"] {
                    field(key)?;
                }
                for (key, want) in [
                    ("record", name.clone()),
                    ("schema", SCHEMA.to_string()),
                    ("version", VERSION.to_string()),
                    ("sha256", hex(&sha256(&bytes))),
                ] {
                    let got = field(key)?;
                    if got != want {
                        return Err(format!(
                            "{prov_name}: {key} is {got}, the record's is {want}"
                        ));
                    }
                }
                if format!("{}.json", record.name) != name {
                    return Err(format!("{name}: the record names itself {}", record.name));
                }
                loaded.push(Loaded {
                    name,
                    record,
                    release: field("release")?,
                });
            }
            _ => return Err(format!("{name}: a file this test does not read")),
        }
    }
    if loaded.is_empty() {
        return Err(format!("{} holds no design record", dir.display()));
    }
    Ok(loaded)
}

/// The settled level of each branch and of their sum, in dB relative to the
/// input, for a sine at `f_hz` through `f32` sections built from the record.
/// Two seconds are played and the last one measured (`sqrt(2 x mean square)`):
/// a whole number of cycles for a whole-number frequency, long after the
/// filters' transients have died away.
fn run_levels(c: &Crossover, f_hz: f64) -> (f64, f64, f64) {
    assert!(
        c.sample_rate_hz.fract() == 0.0 && f_hz.fract() == 0.0,
        "the running check measures whole cycles: a whole-number rate and frequency"
    );
    let rate = c.sample_rate_hz as usize;
    let mut low: Vec<Biquad> = c.low.iter().map(Biquad::new).collect();
    let mut high: Vec<Biquad> = c.high.iter().map(Biquad::new).collect();
    let mut lr4 = c.lr4_design().map(|d| Lr4::new(&d));
    let mut squares = (0f64, 0f64, 0f64);
    for n in 0..2 * rate {
        let phase = 2.0 * core::f64::consts::PI * f_hz * n as f64 / c.sample_rate_hz;
        let x = (SINE_AMPLITUDE * phase.sin()) as f32;
        let l = low.iter_mut().fold(x, |v, q| q.process(v));
        let h = high.iter_mut().fold(x, |v, q| q.process(v));
        if let Some(lr4) = &mut lr4 {
            let (sl, sh) = lr4.split(x);
            assert!(
                sl.to_bits() == l.to_bits() && sh.to_bits() == h.to_bits(),
                "frame {n} at {f_hz} Hz: Lr4 built from the record is not the cascade of its sections"
            );
        }
        if n >= rate {
            let (l, h) = (f64::from(l), f64::from(h));
            squares.0 += l * l;
            squares.1 += h * h;
            // The two branches are added as they are (the record's schema).
            squares.2 += (l + h) * (l + h);
        }
    }
    let level = |sum: f64| 20.0 * ((2.0 * sum / rate as f64).sqrt() / SINE_AMPLITUDE).log10();
    (level(squares.0), level(squares.1), level(squares.2))
}

/// The crossover whose coefficients are the `f32` values the running filters
/// hold (`Biquad::new` rounds each to single precision), written back as
/// `f64`: what the running check really plays through.
fn as_run(c: &Crossover) -> Crossover {
    let round = |s: &Coefficients| {
        let [b0, b1, b2, a1, a2] = Biquad::new(s).coefficients().map(f64::from);
        Coefficients { b0, b1, b2, a1, a2 }
    };
    Crossover {
        low: c.low.iter().map(round).collect(),
        high: c.high.iter().map(round).collect(),
        ..c.clone()
    }
}

/// How far rounding the coefficients to `f32` moves the designed level of the
/// low branch, of the high branch and of their sum at `f_hz`, in dB.
fn rounding_db(c: &Crossover, f_hz: f64) -> (f64, f64, f64) {
    let run = as_run(c);
    (
        db_of(run.response_low(f_hz)) - db_of(c.response_low(f_hz)),
        db_of(run.response_high(f_hz)) - db_of(c.response_high(f_hz)),
        db_of(run.response_sum(f_hz)) - db_of(c.response_sum(f_hz)),
    )
}

/// The largest [`rounding_db`] among the levels the running check holds at a
/// record's response points.
fn worst_rounding_db(c: &Crossover) -> f64 {
    let mut worst = 0f64;
    for p in &c.response {
        let (low, high, sum) = rounding_db(c, p.f_hz);
        if p.low_db > RUN_FLOOR_DB {
            worst = worst.max(low.abs());
        }
        if p.high_db > RUN_FLOOR_DB {
            worst = worst.max(high.abs());
        }
        worst = worst.max(sum.abs());
    }
    worst
}

fn check_crossover(name: &str, tolerance_db: f64, c: &Crossover) -> f64 {
    // 1. the designed response, to the record's own tolerance.
    c.check_response(tolerance_db)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    assert!(
        c.response.iter().any(|p| p.f_hz == c.crossover_hz),
        "{name}: no response point at the crossover frequency"
    );

    // 3. an LR4 is the one chorus designs itself.
    if c.kind == CrossoverKind::Lr4 {
        let from_record = c
            .lr4_design()
            .unwrap_or_else(|| panic!("{name}: an lr4 whose two sections per branch differ"));
        let own = Lr4Design::new(c.sample_rate_hz, c.crossover_hz).expect("chorus designs it");
        let pairs = |a: &Coefficients, b: &Coefficients| {
            [
                (a.b0, b.b0),
                (a.b1, b.b1),
                (a.b2, b.b2),
                (a.a1, b.a1),
                (a.a2, b.a2),
            ]
        };
        for (got, want) in pairs(&from_record.low, &own.low)
            .into_iter()
            .chain(pairs(&from_record.high, &own.high))
        {
            assert!(
                (got - want).abs() <= DESIGN_TOLERANCE,
                "{name}: a coefficient is {got} in the record, {want} in chorus's design"
            );
        }
    }

    // 2. the running filters.
    let mut worst = 0f64;
    for p in &c.response {
        let (low, high, sum) = run_levels(c, p.f_hz);
        // What rounding the coefficients to f32 does to each designed level:
        // small by statement, and taken out before the running filters are
        // held to the record, so the tolerance is the filters' own.
        let (low_rounding, high_rounding, sum_rounding) = rounding_db(c, p.f_hz);
        let mut hold = |what: &str, got: f64, want: f64, rounding: f64| {
            assert!(
                rounding.abs() <= ROUNDING_LIMIT_DB,
                "{name}: rounding its coefficients to f32 moves {what} at {} Hz by {rounding} dB",
                p.f_hz
            );
            let off = (got - rounding - want).abs();
            assert!(
                off <= RUN_TOLERANCE_DB,
                "{name}: {what} at {} Hz runs at {got} dB ({rounding} dB of it f32 coefficient rounding), the record says {want} dB",
                p.f_hz
            );
            worst = worst.max(off);
        };
        let at_crossover = p.f_hz == c.crossover_hz;
        if at_crossover {
            // Each branch's level at the crossover frequency, by the record
            // and by the alignment (-6.02 dB, RaneNote 160).
            assert!(
                p.low_db > RUN_FLOOR_DB && p.high_db > RUN_FLOOR_DB,
                "{name}: a branch is below the floor at its own crossover"
            );
            if c.kind == CrossoverKind::Lr4 {
                let half = 20.0 * 0.5f64.log10();
                assert!((p.low_db - half).abs() < 1e-6 && (p.high_db - half).abs() < 1e-6);
            }
        }
        if p.low_db > RUN_FLOOR_DB {
            hold("the low branch", low, p.low_db, low_rounding);
        }
        if p.high_db > RUN_FLOOR_DB {
            hold("the high branch", high, p.high_db, high_rounding);
        }
        // The flat sum, at every point.
        hold("the sum", sum, p.sum_db, sum_rounding);
        if c.kind == CrossoverKind::Lr4 {
            assert!(
                p.sum_db.abs() <= tolerance_db,
                "{name}: an lr4 sum that is not flat"
            );
        }
    }
    worst
}

#[test]
fn every_record_runs_its_crossover() {
    let loaded = walk(&dir()).unwrap_or_else(|e| panic!("fixtures/design-record: {e}"));
    for l in &loaded {
        let mut worst = 0f64;
        for (i, c) in l.record.crossovers.iter().enumerate() {
            let name = format!("{} crossovers[{i}]", l.name);
            worst = worst.max(check_crossover(&name, l.record.tolerance_db, c));
            // The designed response through chorus's own evaluator agrees
            // with the reader's (one implementation, two entries).
            if let Some(d) = c.lr4_design() {
                let f = c.crossover_hz;
                assert_eq!(
                    db_of(d.response_sum(f, c.sample_rate_hz)),
                    db_of(c.response_sum(f)),
                    "{name}"
                );
            }
        }
        let rounding = l
            .record
            .crossovers
            .iter()
            .map(worst_rounding_db)
            .fold(0f64, f64::max);
        println!(
            "{} (release {}): {} crossovers; the running f32 filters are within {worst:.6} dB of the record (tolerance {RUN_TOLERANCE_DB} dB) once the rounding of their coefficients to f32 is taken out (at most {rounding:.6} dB, limit {ROUNDING_LIMIT_DB} dB)",
            l.name,
            l.release,
            l.record.crossovers.len()
        );
    }
}

/// The compact speaker's record (`docs/hardware/compact-speaker.md`): the file
/// and the sha256 it was exported with. The digest is written here as well as
/// in the provenance, so a change of the record and its provenance together
/// still fails until this test is changed with them.
const COMPACT_RECORD: &str = "chorus-compact-v1.json";
const COMPACT_SHA256: &str = "eb578e02df7df63e3b376e8b2f49a9fda812ab0898fcd159de03f35ddce9b67d";
/// The compact's crossover: the frequency the design document states.
const COMPACT_CROSSOVER_HZ: f64 = 3500.0;

#[test]
fn the_compact_record_is_the_exported_one_and_runs() {
    // Held to its sha256: the bytes are the exported ones.
    let bytes = std::fs::read(dir().join(COMPACT_RECORD)).expect("the compact record reads");
    assert_eq!(
        hex(&sha256(&bytes)),
        COMPACT_SHA256,
        "{COMPACT_RECORD} is not the exported file"
    );
    // The provenance beside it names the same digest (the walk recomputes it).
    let loaded = walk(&dir()).unwrap_or_else(|e| panic!("fixtures/design-record: {e}"));
    let compact = loaded
        .iter()
        .find(|l| l.name == COMPACT_RECORD)
        .expect("the walk reads the compact record");
    let prov = std::fs::read_to_string(dir().join(COMPACT_RECORD).with_extension("provenance"))
        .expect("the compact provenance reads");
    assert_eq!(
        Fields::parse(&prov).expect("it parses").get("sha256"),
        Some(COMPACT_SHA256)
    );

    // Parsed with chorus's own reader: one LR4 for 48 kHz at the design's
    // crossover frequency.
    let record = DesignRecord::parse(&String::from_utf8(bytes).expect("the record is UTF-8"))
        .expect("the compact record parses");
    assert_eq!(record, compact.record);
    assert_eq!(record.name, "chorus-compact-v1");
    assert_eq!(record.crossovers.len(), 1, "a two-way has one crossover");
    let c = &record.crossovers[0];
    assert_eq!(c.kind, CrossoverKind::Lr4);
    assert_eq!(c.sample_rate_hz, 48000.0);
    assert_eq!(c.crossover_hz, COMPACT_CROSSOVER_HZ);
    assert!(c.lr4_design().is_some());

    // Its branch and sum levels through chorus's own filters, as for every
    // record: the designed response to the record's tolerance, the sections
    // against chorus's own LR4 design, and the running f32 filters at every
    // response point (each branch at -6.02 dB at the crossover, the sum flat).
    let worst = check_crossover(COMPACT_RECORD, record.tolerance_db, c);
    assert!(worst <= RUN_TOLERANCE_DB);
    let (low, high, sum) = run_levels(c, COMPACT_CROSSOVER_HZ);
    let half = 20.0 * 0.5f64.log10();
    assert!(
        (low - half).abs() <= RUN_TOLERANCE_DB,
        "the woofer branch runs at {low} dB"
    );
    assert!(
        (high - half).abs() <= RUN_TOLERANCE_DB,
        "the tweeter branch runs at {high} dB"
    );
    assert!(sum.abs() <= RUN_TOLERANCE_DB, "the sum runs at {sum} dB");
    println!(
        "{COMPACT_RECORD}: at {COMPACT_CROSSOVER_HZ} Hz the branches run at {low:.6} and {high:.6} dB, the sum at {sum:.6} dB; within {worst:.6} dB of the record at every point"
    );
}

/// The active two-way's record (`docs/hardware/twoway-speaker.md`): the file
/// and the sha256 it was exported with, written here as well as in the
/// provenance for the reason given at [`COMPACT_SHA256`].
const TWOWAY_RECORD: &str = "chorus-twoway-v1.json";
const TWOWAY_SHA256: &str = "b2dbe5805ce21b43ebc7404f4f627c78020bb3a7a5c8613f6ed7235218e0dcf5";
/// The two-way's crossover: the frequency the design document states.
const TWOWAY_CROSSOVER_HZ: f64 = 2000.0;

#[test]
fn the_twoway_record_is_the_exported_one_and_runs() {
    // Held to its sha256: the bytes are the exported ones.
    let bytes = std::fs::read(dir().join(TWOWAY_RECORD)).expect("the two-way record reads");
    assert_eq!(
        hex(&sha256(&bytes)),
        TWOWAY_SHA256,
        "{TWOWAY_RECORD} is not the exported file"
    );
    // The provenance beside it names the same digest (the walk recomputes it).
    let loaded = walk(&dir()).unwrap_or_else(|e| panic!("fixtures/design-record: {e}"));
    let twoway = loaded
        .iter()
        .find(|l| l.name == TWOWAY_RECORD)
        .expect("the walk reads the two-way record");
    let prov = std::fs::read_to_string(dir().join(TWOWAY_RECORD).with_extension("provenance"))
        .expect("the two-way provenance reads");
    assert_eq!(
        Fields::parse(&prov).expect("it parses").get("sha256"),
        Some(TWOWAY_SHA256)
    );

    // Parsed with chorus's own reader: one LR4 for 48 kHz at the design's
    // crossover frequency.
    let record = DesignRecord::parse(&String::from_utf8(bytes).expect("the record is UTF-8"))
        .expect("the two-way record parses");
    assert_eq!(record, twoway.record);
    assert_eq!(record.name, "chorus-twoway-v1");
    assert_eq!(record.crossovers.len(), 1, "a two-way has one crossover");
    let c = &record.crossovers[0];
    assert_eq!(c.kind, CrossoverKind::Lr4);
    assert_eq!(c.sample_rate_hz, 48000.0);
    assert_eq!(c.crossover_hz, TWOWAY_CROSSOVER_HZ);
    assert!(c.lr4_design().is_some());

    // Its branch and sum levels through chorus's own filters: the designed
    // response to the record's tolerance, the sections against chorus's own
    // LR4 design, and the running f32 filters at every response point (each
    // branch at -6.02 dB at the crossover, the sum flat).
    let worst = check_crossover(TWOWAY_RECORD, record.tolerance_db, c);
    assert!(worst <= RUN_TOLERANCE_DB);
    let (low, high, sum) = run_levels(c, TWOWAY_CROSSOVER_HZ);
    let half = 20.0 * 0.5f64.log10();
    assert!(
        (low - half).abs() <= RUN_TOLERANCE_DB,
        "the woofer branch runs at {low} dB"
    );
    assert!(
        (high - half).abs() <= RUN_TOLERANCE_DB,
        "the tweeter branch runs at {high} dB"
    );
    assert!(sum.abs() <= RUN_TOLERANCE_DB, "the sum runs at {sum} dB");
    println!(
        "{TWOWAY_RECORD}: at {TWOWAY_CROSSOVER_HZ} Hz the branches run at {low:.6} and {high:.6} dB, the sum at {sum:.6} dB; within {worst:.6} dB of the record at every point"
    );
}

/// The subwoofer's record (`docs/hardware/subwoofer.md`): the file and the
/// sha256 it was exported with, written here as well as in the provenance for
/// the reason given at [`COMPACT_SHA256`].
const SUB_RECORD: &str = "chorus-sub-v1.json";
const SUB_SHA256: &str = "7efc717e20999a509a2c5790601803249dd0462a750836cbf6f0363433b6e0ed";
/// The subwoofer's low-pass: the default bass-management frequency the design
/// document states, which is the chain's own default.
const SUB_LOWPASS_HZ: f64 = 80.0;

#[test]
fn the_sub_record_is_the_exported_one_and_runs() {
    // Held to its sha256: the bytes are the exported ones.
    let bytes = std::fs::read(dir().join(SUB_RECORD)).expect("the subwoofer record reads");
    assert_eq!(
        hex(&sha256(&bytes)),
        SUB_SHA256,
        "{SUB_RECORD} is not the exported file"
    );
    // The provenance beside it names the same digest (the walk recomputes it).
    let loaded = walk(&dir()).unwrap_or_else(|e| panic!("fixtures/design-record: {e}"));
    let sub = loaded
        .iter()
        .find(|l| l.name == SUB_RECORD)
        .expect("the walk reads the subwoofer record");
    let prov = std::fs::read_to_string(dir().join(SUB_RECORD).with_extension("provenance"))
        .expect("the subwoofer provenance reads");
    assert_eq!(
        Fields::parse(&prov).expect("it parses").get("sha256"),
        Some(SUB_SHA256)
    );

    // Parsed with chorus's own reader: one LR4 for 48 kHz at the design's
    // low-pass frequency, which is the default the chain's settings hold.
    let record = DesignRecord::parse(&String::from_utf8(bytes).expect("the record is UTF-8"))
        .expect("the subwoofer record parses");
    assert_eq!(record, sub.record);
    assert_eq!(record.name, "chorus-sub-v1");
    assert_eq!(
        record.crossovers.len(),
        1,
        "bass management is one crossover"
    );
    let c = &record.crossovers[0];
    assert_eq!(c.kind, CrossoverKind::Lr4);
    assert_eq!(c.sample_rate_hz, 48000.0);
    assert_eq!(c.crossover_hz, SUB_LOWPASS_HZ);
    assert_eq!(
        SUB_LOWPASS_HZ,
        f64::from(chorus_dsp::settings::CROSSOVER_DEFAULT_HZ),
        "the design's low-pass is the chain's default crossover"
    );
    assert!(c.lr4_design().is_some());

    // Its branch and sum levels through chorus's own filters: the designed
    // response to the record's tolerance, the sections against chorus's own
    // LR4 design, and the running f32 filters at every response point. At the
    // low-pass frequency the subwoofer's branch (low) and a main's branch
    // (high) each run at -6.02 dB and add to a flat sum.
    let worst = check_crossover(SUB_RECORD, record.tolerance_db, c);
    assert!(worst <= RUN_TOLERANCE_DB);
    let (low, high, sum) = run_levels(c, SUB_LOWPASS_HZ);
    // This low in the band the f32 coefficients' rounding is the larger part
    // of what the running filters are off by ([`ROUNDING_LIMIT_DB`]): it is
    // held small, and the filters are held to the design less that rounding.
    let (low_rounding, high_rounding, sum_rounding) = rounding_db(c, SUB_LOWPASS_HZ);
    let rounding = worst_rounding_db(c);
    assert!(rounding <= ROUNDING_LIMIT_DB);
    let half = 20.0 * 0.5f64.log10();
    assert!(
        (low - low_rounding - half).abs() <= RUN_TOLERANCE_DB,
        "the subwoofer branch runs at {low} dB"
    );
    assert!(
        (high - high_rounding - half).abs() <= RUN_TOLERANCE_DB,
        "the mains' branch runs at {high} dB"
    );
    assert!(
        (sum - sum_rounding).abs() <= RUN_TOLERANCE_DB,
        "the sum runs at {sum} dB"
    );
    // And as they run, rounding and all: each branch and the sum within the
    // stated rounding limit of -6.02 dB and of flat.
    assert!((low - half).abs() <= ROUNDING_LIMIT_DB);
    assert!((high - half).abs() <= ROUNDING_LIMIT_DB);
    assert!(sum.abs() <= ROUNDING_LIMIT_DB);
    for p in &c.response {
        let (l, h, s) = run_levels(c, p.f_hz);
        let (lr, hr, sr) = rounding_db(c, p.f_hz);
        println!(
            "{SUB_RECORD}: at {} Hz low {l:.6} (record {:.6}, rounding {lr:.6}), high {h:.6} (record {:.6}, rounding {hr:.6}), sum {s:.6} (rounding {sr:.6})",
            p.f_hz, p.low_db, p.high_db
        );
    }
    println!(
        "{SUB_RECORD}: at {SUB_LOWPASS_HZ} Hz the branches run at {low:.6} and {high:.6} dB, the sum at {sum:.6} dB ({low_rounding:.6}, {high_rounding:.6} and {sum_rounding:.6} dB of that from rounding the coefficients to f32); less the rounding, within {worst:.6} dB of the record at every point; the rounding is at most {rounding:.6} dB at any point"
    );
}

fn committed_record() -> String {
    std::fs::read_to_string(dir().join("lr4-2000hz-48k.json")).expect("the record reads")
}

/// The committed record with one piece of text replaced, which must be there.
fn edited(from: &str, to: &str) -> String {
    let text = committed_record();
    assert!(text.contains(from), "the record has no {from}");
    text.replacen(from, to, 1)
}

#[test]
fn an_unknown_schema_version_is_refused() {
    assert!(DesignRecord::parse(&committed_record()).is_ok());
    // A later version, an earlier one, and the known one written as anything
    // but the integer it is.
    for version in ["2", "0", "-1", "1.0", "1e0", "\"1\"", "true", "null", "[1]"] {
        let text = edited("\"version\": 1,", &format!("\"version\": {version},"));
        assert!(
            matches!(
                DesignRecord::parse(&text),
                Err(RecordError::UnknownVersion(_))
            ),
            "version {version} was not refused as unknown: {:?}",
            DesignRecord::parse(&text).map(|r| r.name)
        );
    }
    // No version at all.
    assert!(matches!(
        DesignRecord::parse(&edited("  \"version\": 1,\n", "")),
        Err(RecordError::UnknownVersion(_))
    ));
    // A later version is refused as that even when it brings keys this reader
    // has never seen.
    assert!(matches!(
        DesignRecord::parse(&edited(
            "\"version\": 1,",
            "\"version\": 2,\n  \"enclosure\": {},"
        )),
        Err(RecordError::UnknownVersion(_))
    ));
    // Another schema.
    assert!(matches!(
        DesignRecord::parse(&edited("speaker-design-record", "room-design-record")),
        Err(RecordError::UnknownSchema(_))
    ));
}

#[test]
fn an_edited_record_is_refused() {
    // A key version 1 does not have.
    assert!(matches!(
        DesignRecord::parse(&edited(
            "\"version\": 1,",
            "\"version\": 1,\n  \"enclosure\": {},"
        )),
        Err(RecordError::Shape(_))
    ));
    // A missing section, an unknown kind, another coefficient form.
    for (from, to) in [
        ("\"kind\": \"lr4\"", "\"kind\": \"lr2\""),
        ("\"kind\": \"lr4\"", "\"kind\": \"lr8\""),
        ("a0 = 1", "a0 = 2"),
        ("\"sample_rate_hz\": 48000", "\"sample_rate_hz\": 3000"),
        ("\"b0\": 0.01440144034651121,", ""),
    ] {
        assert!(
            matches!(
                DesignRecord::parse(&edited(from, to)),
                Err(RecordError::Shape(_))
            ),
            "{from} -> {to} was read"
        );
    }
    // A coefficient or a response point that was changed: the biquads no
    // longer give the record's own points.
    for (from, to) in [
        ("\"low_db\": -6.020599913", "\"low_db\": -6.020499913"),
        ("\"sum_db\": 0.0", "\"sum_db\": 0.001"),
        ("\"a2\": 0.6905989232414971", "\"a2\": 0.6905989"),
        ("\"sample_rate_hz\": 48000", "\"sample_rate_hz\": 44100"),
    ] {
        assert!(
            matches!(
                DesignRecord::parse(&edited(from, to)),
                Err(RecordError::Response(_))
            ),
            "{from} -> {to} was read"
        );
    }
    // Text that is not JSON.
    assert!(matches!(
        DesignRecord::parse(&committed_record().replace("],", "]")),
        Err(RecordError::Json(_))
    ));
}

/// A copy of the fixture directory under the test's own scratch directory.
fn copy_of_the_fixtures(name: &str) -> PathBuf {
    let to = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&to);
    std::fs::create_dir_all(&to).expect("a scratch directory");
    for entry in std::fs::read_dir(dir()).expect("fixtures/design-record exists") {
        let path = entry.expect("a directory entry").path();
        std::fs::copy(&path, to.join(path.file_name().unwrap())).expect("a fixture copies");
    }
    to
}

#[test]
fn a_file_the_walk_does_not_read_fails_it() {
    let unknown_version = edited("\"version\": 1,", "\"version\": 2,");
    type Spoil<'a> = &'a dyn Fn(&Path);
    let cases: [(&str, Spoil, &str); 7] = [
        (
            "a stray file",
            &|d| std::fs::write(d.join("notes.txt"), "not a record\n").unwrap(),
            "notes.txt: a file this test does not read",
        ),
        (
            "a directory",
            &|d| std::fs::create_dir(d.join("drafts")).unwrap(),
            "drafts: not a file",
        ),
        (
            "a record without its provenance",
            &|d| std::fs::remove_file(d.join("lr4-2000hz-48k.provenance")).unwrap(),
            "lr4-2000hz-48k.json: no .provenance file beside it",
        ),
        (
            "a provenance without its record",
            &|d| std::fs::remove_file(d.join("lr4-2000hz-48k.json")).unwrap(),
            "lr4-2000hz-48k.provenance: no .json file beside it",
        ),
        (
            "a record that is not the exported bytes",
            &|d| {
                let path = d.join("lr4-2000hz-48k.json");
                let text = std::fs::read_to_string(&path).unwrap();
                std::fs::write(&path, text + "\n").unwrap();
            },
            "lr4-2000hz-48k.provenance: sha256 is",
        ),
        (
            "a record of an unknown schema version",
            &|d| std::fs::write(d.join("lr4-2000hz-48k.json"), &unknown_version).unwrap(),
            "lr4-2000hz-48k.json: unknown schema version 2",
        ),
        (
            "a provenance without its release",
            &|d| {
                let path = d.join("lr4-2000hz-48k.provenance");
                let text = std::fs::read_to_string(&path).unwrap();
                std::fs::write(&path, text.replace("release = ", "released = ")).unwrap();
            },
            "lr4-2000hz-48k.provenance: no 'release'",
        ),
    ];
    for (i, (what, spoil, want)) in cases.iter().enumerate() {
        let d = copy_of_the_fixtures(&format!("design-record-spoiled-{i}"));
        assert!(
            walk(&d).is_ok(),
            "{what}: the copy does not walk before it is spoiled"
        );
        spoil(&d);
        match walk(&d) {
            Ok(_) => panic!("{what}: the walk passed"),
            Err(e) => assert!(e.starts_with(want), "{what}: the walk failed with {e:?}"),
        }
        std::fs::remove_dir_all(&d).expect("the scratch directory goes");
    }
    // An empty directory is a failure too.
    let empty = Path::new(env!("CARGO_TARGET_TMPDIR")).join("design-record-empty");
    let _ = std::fs::remove_dir_all(&empty);
    std::fs::create_dir_all(&empty).unwrap();
    assert!(walk(&empty)
        .unwrap_err()
        .ends_with("holds no design record"));
    std::fs::remove_dir_all(&empty).unwrap();
}

/// SHA-256 (FIPS 180-4, <https://csrc.nist.gov/pubs/fips/180-4/upd1/final>,
/// read 2026-10-06), here so the provenance's digest is checked without a
/// dependency; held to the standard's own "abc" example below.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    let mut out = [0u8; 32];
    for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *chunk = word.to_be_bytes();
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn the_digest_is_sha_256() {
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    // Two blocks: the standard's 448-bit example.
    assert_eq!(
        hex(&sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}
