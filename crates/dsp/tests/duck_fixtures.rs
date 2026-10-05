//! The announcement mixer's fixtures: every file under `fixtures/dsp/duck/`,
//! run against `chorus_dsp::duck` (`fixtures/README.md`, `dsp/duck/`). Rust-only
//! by declaration: the mix runs on the server, so there is no C mirror.
//!
//! Each fixture names the frames things happen at and the gains at chosen
//! frames, worked by hand from the envelope `docs/dsp.md` states. The test
//! reads the envelope off the mixer with two probe runs (a music of ones under
//! a silent clip gives the music's gain per frame; a silent music under a clip
//! of ones gives the clip's), holds it to the fixture's numbers, and then holds
//! the real run to `music * gain + clip * clip gain`, bit for bit. A file this
//! reader does not know is a failure, not a skip.

use std::path::{Path, PathBuf};

use chorus_dsp::duck::{Duck, DuckParams, DuckState};
use chorus_dsp::fixture::{signal, Fields};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dsp/duck")
}

fn fixtures() -> Vec<(String, Fields)> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir())
        .expect("fixtures/dsp/duck exists")
        .map(|e| e.expect("a directory entry").path())
        .collect();
    files.sort();
    assert!(!files.is_empty(), "fixtures/dsp/duck holds no fixtures");
    files
        .iter()
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(path).expect("a fixture reads");
            let f = Fields::parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(f.get("kind"), Some("duck"), "{name}: unknown kind");
            assert!(
                f.get("source").is_some_and(|s| !s.is_empty()) && f.get("read").is_some(),
                "{name}: no source and read date"
            );
            (name, f)
        })
        .collect()
}

struct Case {
    params: DuckParams,
    channels: usize,
    frames: usize,
    start_at: usize,
    cancel_at: Option<usize>,
    /// Interleaved.
    music: Vec<f32>,
    clip: Vec<f32>,
}

fn frame(f: &Fields, key: &str) -> Option<usize> {
    f.get(key).map(|_| f.num(key).unwrap() as usize)
}

fn interleave(per_channel: &[Vec<f32>]) -> Vec<f32> {
    let n = per_channel.len();
    let mut v = vec![0f32; per_channel[0].len() * n];
    for (c, ch) in per_channel.iter().enumerate() {
        for (i, &s) in ch.iter().enumerate() {
            v[i * n + c] = s;
        }
    }
    v
}

fn case(f: &Fields) -> Case {
    let rate = f.num("rate_hz").unwrap();
    let channels = f.num("channels").unwrap() as usize;
    let frames = f.num("frames").unwrap() as usize;
    let clip_frames = f.num("clip_frames").unwrap() as usize;
    let render = |prefix: &str, n: usize| -> Vec<f32> {
        let chans: Vec<Vec<f32>> = (0..channels)
            .map(|c| signal(f.str(&format!("{prefix}.{c}")).unwrap(), n, rate).unwrap())
            .collect();
        interleave(&chans)
    };
    Case {
        params: DuckParams {
            duck_gain: f.num("duck_gain").unwrap() as f32,
            clip_gain: f.num("clip_gain").unwrap() as f32,
            duck_ramp_frames: f.num("duck_ramp_frames").unwrap() as u32,
            restore_ramp_frames: f.num("restore_ramp_frames").unwrap() as u32,
            limit: f.num("limit").unwrap() as f32,
        },
        channels,
        frames,
        start_at: f.num("start_at").unwrap() as usize,
        cancel_at: frame(f, "cancel_at"),
        music: render("music", frames),
        clip: render("clip", clip_frames),
    }
}

/// Runs a case over `music` and `clip` in blocks of at most `block` frames,
/// each event on its own frame (a block ends where an event falls). Returns
/// the output and the clip frames played.
fn run(c: &Case, music: &[f32], clip: &[f32], block: usize) -> (Vec<f32>, usize) {
    run_with(c, &c.params, music, clip, block)
}

fn run_with(
    c: &Case,
    params: &DuckParams,
    music: &[f32],
    clip: &[f32],
    block: usize,
) -> (Vec<f32>, usize) {
    let n = c.channels;
    let mut d = Duck::new(params, n).expect("the fixture's parameters are taken");
    let mut out = vec![0f32; music.len()];
    let (mut at, mut played) = (0, 0);
    while at < c.frames {
        if at == c.start_at {
            d.start();
            // The whole clip is at hand: it ends with its last frame.
            d.finish();
        }
        if Some(at) == c.cancel_at {
            d.cancel();
        }
        let mut end = (at + block).min(c.frames);
        for event in [Some(c.start_at), c.cancel_at].into_iter().flatten() {
            if event > at {
                end = end.min(event);
            }
        }
        let m = d
            .process(
                &music[at * n..end * n],
                &clip[played * n..],
                &mut out[at * n..end * n],
            )
            .expect("whole frames");
        assert_eq!(m.frames, end - at);
        played += m.clip_frames;
        at = end;
    }
    assert_eq!(d.state(), DuckState::Idle, "the run ends restored");
    (out, played)
}

/// The envelope, read off the mixer: per frame, the music's gain and the
/// clip's (channel 0 of the two probe runs; every channel is checked equal).
/// The probes run at a limit of full scale, so the fixture's limit does not
/// clamp the ones: the envelope does not depend on the limit.
fn envelope(c: &Case) -> (Vec<f32>, Vec<f32>) {
    let open = DuckParams {
        limit: 1.0,
        ..c.params
    };
    let ones = vec![1f32; c.music.len()];
    let zeros = vec![0f32; c.music.len()];
    let clip_ones = vec![1f32; c.clip.len()];
    let clip_zeros = vec![0f32; c.clip.len()];
    let (g, _) = run_with(c, &open, &ones, &clip_zeros, 4096);
    let (w, _) = run_with(c, &open, &zeros, &clip_ones, 4096);
    let first = |v: &[f32]| -> Vec<f32> {
        v.chunks(c.channels)
            .map(|fr| {
                assert!(fr.iter().all(|s| s.to_bits() == fr[0].to_bits()));
                fr[0]
            })
            .collect()
    };
    (first(&g), first(&w))
}

fn pairs(f: &Fields, at: &str, expect: &str) -> Vec<(usize, f64)> {
    if f.get(at).is_none() {
        return Vec::new();
    }
    let (a, e) = (f.list(at).unwrap(), f.list(expect).unwrap());
    assert_eq!(a.len(), e.len(), "{at} and {expect} differ in length");
    a.iter().map(|&n| n as usize).zip(e).collect()
}

/// The duck ramp reaches its target gain within the configured ramp length,
/// to the frame; a cancelled one turns round where the fixture says.
#[test]
fn the_duck_ramp_reaches_its_gain_on_the_frame() {
    let mut reached = 0;
    for (name, f) in fixtures() {
        let c = case(&f);
        let (g, w) = envelope(&c);
        let tol = f.num("tolerance").unwrap();
        assert!(
            g[..c.start_at].iter().all(|&v| v == 1.0),
            "{name}: ducked before the start"
        );
        for (n, want) in pairs(&f, "gain_at", "expect_gain") {
            assert!(
                (f64::from(g[n]) - want).abs() <= tol,
                "{name}: the music's gain at {n} is {}, want {want}",
                g[n]
            );
        }
        for (n, want) in pairs(&f, "clip_gain_at", "expect_clip_gain") {
            assert!(
                (f64::from(w[n]) - want).abs() <= tol,
                "{name}: the clip's gain at {n} is {}, want {want}",
                w[n]
            );
        }
        if let Some(at) = frame(&f, "duck_reached_at") {
            reached += 1;
            let d = c.params.duck_ramp_frames as usize;
            assert_eq!(at, c.start_at + d - 1, "{name}: the fixture's own frame");
            assert_eq!(
                g[at], c.params.duck_gain,
                "{name}: not at the duck gain on frame {at}"
            );
            assert!(
                g[at - 1] > c.params.duck_gain,
                "{name}: at the duck gain before frame {at}"
            );
            // Never below the target, and monotone on the way down.
            assert!(g.iter().all(|&v| v >= c.params.duck_gain), "{name}");
            assert!(g[c.start_at..=at].windows(2).all(|p| p[1] < p[0]), "{name}");
        } else {
            // Cancelled on the way down: the target is never reached.
            assert!(g.iter().all(|&v| v > c.params.duck_gain), "{name}");
        }
    }
    assert!(reached >= 4, "{reached} fixtures reach the duck gain");
}

/// After the restore ramp the music is its unducked samples, bit for bit, and
/// not a frame earlier; this holds for a clip that ends early and for one
/// cancelled on the way down or while it plays.
#[test]
fn the_restore_returns_the_music_bit_exact() {
    let (mut early, mut cancelled) = (0, 0);
    for (name, f) in fixtures() {
        let c = case(&f);
        let n = c.channels;
        let (out, played) = run(&c, &c.music, &c.clip, 4096);
        let at = f.num("restored_at").unwrap() as usize;
        assert!(at < c.frames, "{name}: the run is too short to restore");
        let same = |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits());
        assert!(
            same(&out[at * n..], &c.music[at * n..]),
            "{name}: not the music's samples from frame {at}"
        );
        assert!(
            same(&out[..c.start_at * n], &c.music[..c.start_at * n]),
            "{name}: not the music's samples before the start"
        );
        let (g, w) = envelope(&c);
        assert!(g[at - 1] < 1.0, "{name}: restored before frame {at}");
        assert!(
            g[at..].iter().all(|&v| v == 1.0) && w[at..].iter().all(|&v| v == 0.0),
            "{name}: not at unity from frame {at}"
        );
        assert_eq!(
            played,
            f.num("clip_frames_played").unwrap() as usize,
            "{name}: clip frames played"
        );
        // The restore's own length, to the frame: R frames after a whole duck,
        // and in proportion after one cancelled on the way down.
        let (d, r) = (
            c.params.duck_ramp_frames as usize,
            c.params.restore_ramp_frames as usize,
        );
        match c.cancel_at {
            Some(cancel) if cancel < c.start_at + d => {
                cancelled += 1;
                let down = cancel - c.start_at;
                assert_eq!(at, cancel + (down * r).div_ceil(d) - 1, "{name}");
            }
            Some(cancel) => {
                cancelled += 1;
                assert_eq!(at, cancel + r - 1, "{name}");
            }
            None => {
                let clip_frames = c.clip.len() / n;
                early += usize::from(clip_frames < 100);
                assert_eq!(at, c.start_at + d + clip_frames + r - 1, "{name}");
            }
        }
    }
    assert!(early >= 2, "{early} fixtures end early");
    assert!(cancelled >= 2, "{cancelled} fixtures are cancelled");
}

/// The mix is the ducked music plus the clip: every output sample is
/// `music * gain + clip * clip gain` at that frame's gains, bit for bit, and
/// no sample is above the limit.
#[test]
fn the_mix_is_the_ducked_music_plus_the_clip() {
    for (name, f) in fixtures() {
        let c = case(&f);
        let n = c.channels;
        let (out, played) = run(&c, &c.music, &c.clip, 4096);
        let (g, w) = envelope(&c);
        // Where the clip's frames fall: from the frame after the duck ramp, one
        // per frame, for as many as were played.
        let first = c.start_at + c.params.duck_ramp_frames as usize;
        if let Some(at) = frame(&f, "clip_first_at") {
            assert_eq!(at, first, "{name}: the fixture's own frame");
            assert!(w[at] > 0.0 && w[..at].iter().all(|&v| v == 0.0), "{name}");
        }
        let limit = c.params.limit;
        let mut mixed = 0;
        for i in 0..c.frames {
            let voice = (played > 0 && (first..first + played).contains(&i)).then(|| i - first);
            if voice.is_none() {
                assert_eq!(w[i], 0.0, "{name}: a clip gain with no clip frame at {i}");
            }
            for ch in 0..n {
                let m = c.music[i * n + ch];
                let want = match voice {
                    Some(k) => {
                        mixed += 1;
                        m * g[i] + c.clip[k * n + ch] * w[i]
                    }
                    None => m * g[i],
                };
                let want = want.clamp(-limit, limit);
                let got = out[i * n + ch];
                assert_eq!(
                    got.to_bits(),
                    want.to_bits(),
                    "{name}: frame {i} channel {ch} is {got}, want {want}"
                );
                if g[i] < 1.0 {
                    assert!(got.abs() <= limit, "{name}: {got} above the limit at {i}");
                }
            }
        }
        assert_eq!(mixed, played * n, "{name}: clip samples mixed");
        // While it holds, the gains are the parameters themselves.
        if c.cancel_at.is_none() {
            for i in first..first + played {
                assert!(
                    g[i] == c.params.duck_gain && w[i] == c.params.clip_gain,
                    "{name}"
                );
            }
        }
        if let Some(at) = frame(&f, "expect_output_0_at") {
            assert_eq!(f64::from(out[at * n]), f.num("expect_output_0").unwrap());
        }
        // Inputs inside the limit give an output inside it, everywhere.
        let inside = |v: &[f32]| v.iter().all(|s| s.abs() <= limit);
        if inside(&c.music) && inside(&c.clip) {
            assert!(inside(&out), "{name}: an output sample above the limit");
        }
    }
}

/// Click-free: neither gain moves by more than the ramp's slope bound from
/// one frame to the next (the fixture's `max_gain_step`, the depth over the
/// shorter ramp), so a steady music's output moves by no more than that
/// times its level, and a cancelled clip fades instead of stopping.
#[test]
fn no_step_exceeds_the_ramps_slope_bound() {
    for (name, f) in fixtures() {
        let c = case(&f);
        let (g, w) = envelope(&c);
        let bound = f.num("max_gain_step").unwrap();
        assert!(
            (bound - c.params.max_gain_step()).abs() < 1e-9,
            "{name}: the fixture's bound is not the parameters'"
        );
        // One f32 rounding on each side of the difference.
        let slack = f64::from(f32::EPSILON);
        let worst = |v: &[f32]| {
            v.windows(2)
                .map(|p| (f64::from(p[1]) - f64::from(p[0])).abs())
                .fold(0f64, f64::max)
        };
        let step = worst(&g);
        assert!(
            step <= bound + slack,
            "{name}: the music's gain steps {step}"
        );
        assert!(step > 0.0, "{name}: the gain never moved");
        // The clip's gain: it comes in and goes out at the clip's own edges
        // (the clip's samples start and end there), and in between it holds
        // or, cancelled, fades on the restore's slope.
        let d = c.params.duck_ramp_frames as usize;
        if let Some(cancel) = c.cancel_at.filter(|&at| at >= c.start_at + d) {
            let fade = worst(&w[cancel - 1..]);
            let slope = f64::from(c.params.clip_gain) / f64::from(c.params.restore_ramp_frames);
            assert!(
                fade <= slope + slack,
                "{name}: the cancelled clip steps {fade}, above {slope}"
            );
        }
        // The output of a steady music under a silent clip is the gain itself:
        // the same bound holds on consecutive output samples.
        let ones = vec![0.5f32; c.music.len()];
        let silent = vec![0f32; c.clip.len()];
        let (out, _) = run(&c, &ones, &silent, 4096);
        let out0: Vec<f32> = out.chunks(c.channels).map(|fr| fr[0]).collect();
        assert!(
            worst(&out0) <= 0.5 * bound + slack,
            "{name}: a steady music's output steps {}",
            worst(&out0)
        );
    }
}

/// The core reads no clock: timing is in frames only. The same frames give
/// the same bits however they are cut into blocks and whenever the calls are
/// made; a call with no frames moves nothing; and the module names no clock.
#[test]
fn the_core_reads_no_clock() {
    for (name, f) in fixtures() {
        let c = case(&f);
        let (whole, played) = run(&c, &c.music, &c.clip, usize::MAX / 2);
        for block in [1, 37, 480] {
            let (out, p) = run(&c, &c.music, &c.clip, block);
            assert_eq!(p, played, "{name}: block {block}");
            assert!(
                out.iter()
                    .zip(&whole)
                    .all(|(a, b)| a.to_bits() == b.to_bits()),
                "{name}: the output depends on the block size ({block})"
            );
        }
    }
    // Wall time passing between calls is not seen: a mixer left alone mid-ramp
    // is where it was, and calls that carry no frames do not advance it.
    let p = DuckParams {
        duck_gain: 0.25,
        clip_gain: 0.75,
        duck_ramp_frames: 8,
        restore_ramp_frames: 8,
        limit: 1.0,
    };
    let mut d = Duck::new(&p, 1).unwrap();
    d.start();
    let mut out = [0f32; 4];
    d.process(&[1.0; 4], &[], &mut out).unwrap();
    let before = d.clone();
    std::thread::sleep(std::time::Duration::from_millis(30));
    for _ in 0..1000 {
        let m = d.process(&[], &[1.0; 16], &mut []).unwrap();
        assert_eq!((m.frames, m.clip_frames), (0, 0));
    }
    assert_eq!(d, before);
    d.process(&[1.0; 4], &[], &mut out).unwrap();
    assert_eq!(out, [0.53125, 0.4375, 0.34375, 0.25]);
    // And the source: no clock type, no time module, no thread.
    let source = include_str!("../src/duck.rs");
    for word in [
        "std::time",
        "Instant",
        "SystemTime",
        "Duration",
        "std::thread",
        "sleep",
    ] {
        assert!(!source.contains(word), "duck.rs names {word}");
    }
    // Its parameters hold lengths as frame counts; milliseconds exist only in
    // the conversion that makes them (a pure function of a rate).
    assert_eq!(DuckParams::defaults(48_000).unwrap().duck_ramp_frames, 9600);
    assert_eq!(DuckParams::defaults(44_100).unwrap().duck_ramp_frames, 8820);
}
