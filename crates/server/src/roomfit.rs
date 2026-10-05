//! A recording of the measurement sweep, fitted (the recording half of the
//! phone-microphone measurement, `docs/room-correction.md`,
//! `docs/decisions/0000-a-recording-is-fitted-and-not-kept.md`).
//!
//! `POST /api/room-fit?zone=<room>` takes one mono recording of the sweep a
//! room played (`measure_sweep`), runs the fitter's own check and fit on it
//! (`chorus_dsp::roomfit`), and answers with the filters or with the
//! fitter's refusal by its name. Nothing here changes a room: the filters
//! are applied with the catalog's `room_eq`, which this route's answer is
//! written to be posted as, and taken back with `room_eq_undo`.
//!
//! This file is the pure half: the route's numbers, what its query says
//! about the sweep, the WAV it reads and the answer it writes. The socket
//! half (the bound checked before the body is read, the POST rules) is
//! `crate::control`, beside the other routes.
//!
//! **The recording is never stored.** It is read into memory, turned into
//! samples, fitted and dropped when the answer is written. No function here
//! opens a file, and none prints: the one line the server says about an
//! upload is written by the caller from [`Outcome::line`], which carries
//! counts and a name and no sample.

use chorus_control::catalog::{is_identifier, Refusal, CATALOG_VERSION};
use chorus_dsp::roomfit::{filter_json, fit_recording, FitConfig, RoomFitError, Sweep, Target};

/// The route.
pub const ROUTE: &str = "/api/room-fit";

/// The one content type a recording is sent as. It is not one of the three a
/// cross-site page can send without a preflight (`text/plain`,
/// `application/x-www-form-urlencoded`, `multipart/form-data`; the Fetch
/// standard's CORS-safelisted request headers), so the rule the command
/// route's `application/json` enforces holds here too.
pub const CONTENT_TYPE: &str = "audio/wav";

/// The most a recording's body may be, bytes. 2 MiB is 21.8 s of 48 kHz
/// 16-bit mono, three times the 6.5 s `measure_sweep` plays (0.5 s of
/// silence, the 5 s sweep, 1 s of silence: 624 KB), so a recording started
/// early and stopped late fits and nothing much longer does. ASSUMED value.
pub const MAX_RECORDING_BYTES: usize = 2 * 1024 * 1024;

/// The one sample rate a recording is accepted at, Hz: the rate every
/// fixture the fitter is held to was made at. A recording at another rate is
/// refused by name (`unsupported_rate`) rather than fitted on a path no test
/// holds; the recorder resamples.
pub const RATE_HZ: u32 = 48_000;

/// The bounds of `sweep_ms`, the sweep's length. The low end is the
/// fixtures' 1 s sweep; the high end keeps the sweep inside the body bound
/// with its response window. ASSUMED values.
pub const SWEEP_MS: (u32, u32) = (1_000, 10_000);

/// The most `fade_in_ms` may be, and it is always shorter than the sweep.
pub const FADE_IN_MS_MAX: u32 = 1_000;

/// Which sweep a recording is of: what the fitter deconvolves with. The
/// frequencies and the level are `Sweep::recommended`'s, which is what
/// `measure_sweep` plays; the length and the fade-in are what the route is
/// told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SweepSpec {
    /// The sweep's length, ms.
    pub sweep_ms: u32,
    /// Its fade-in, ms.
    pub fade_in_ms: u32,
}

impl SweepSpec {
    /// The sweep `measure_sweep` plays: 5 s with a 0.1 s fade-in.
    pub fn recommended() -> SweepSpec {
        let sweep = Sweep::recommended(1_000);
        SweepSpec {
            sweep_ms: sweep.samples as u32,
            fade_in_ms: sweep.fade_in_samples as u32,
        }
    }

    /// The fitter's sweep at `rate_hz`. Both lengths are whole samples at
    /// any rate that is a whole number of kHz; [`RATE_HZ`] is.
    pub fn sweep(&self, rate_hz: u32) -> Sweep {
        let samples = |ms: u32| (u64::from(ms) * u64::from(rate_hz) / 1_000) as usize;
        Sweep {
            samples: samples(self.sweep_ms),
            fade_in_samples: samples(self.fade_in_ms),
            ..Sweep::recommended(rate_hz)
        }
    }
}

/// Why an upload is refused, with the HTTP status it is answered with. The
/// body is the catalog's `error` message at version 2, its `detail` starting
/// with the refusal's name and a colon, as a command's refusal by name does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The status line's code and phrase.
    pub status: &'static str,
    /// The refusal's name: the fitter's own for a recording it refuses.
    pub name: &'static str,
    /// The `error` message's `field`.
    pub field: &'static str,
    /// The `error` message's `detail`, name first.
    pub detail: String,
}

impl Refused {
    fn new(status: &'static str, name: &'static str, field: &'static str, why: String) -> Refused {
        Refused {
            status,
            name,
            field,
            detail: format!("{}: {}", name, why),
        }
    }

    /// The query is not one this route reads.
    fn query(field: &'static str, why: String) -> Refused {
        Refused::new("400 Bad Request", "bad_query", field, why)
    }

    /// The body is not a WAV this route reads.
    fn wav(why: String) -> Refused {
        Refused::new("400 Bad Request", "not_wav", "recording", why)
    }

    /// The fitter's refusal, under the fitter's name and in its words.
    pub fn fitter(error: &RoomFitError) -> Refused {
        let (status, field) = match error {
            // Not a bad recording: the sweep the query described cannot be
            // fitted with (it does not cover the fit band).
            RoomFitError::BadConfig(_) => ("400 Bad Request", "sweep_ms"),
            _ => ("422 Unprocessable Content", "recording"),
        };
        Refused {
            status,
            name: error.name(),
            field,
            // The fitter's Display starts with its name and a colon already.
            detail: error.to_string(),
        }
    }

    /// The room is not one of this server's.
    pub fn unknown_room(zone: &str, rooms: &str) -> Refused {
        Refused {
            status: "400 Bad Request",
            name: "zone",
            field: "zone",
            detail: format!(
                "there is no zone '{}'; the zones configured on this server are {}",
                zone, rooms
            ),
        }
    }

    /// The room's correction is switched on, so the sweep it played was
    /// already corrected and a fit of it would not be the room's.
    pub fn correction_on(zone: &str) -> Refused {
        Refused::new(
            "409 Conflict",
            "correction_on",
            "zone",
            format!(
                "room '{}' has its correction switched on, so a sweep it plays is the \
                 corrected room and a fit of that recording would replace the correction \
                 with one for what is left; switch it off (room_eq with enabled false), \
                 measure, and apply the new filters",
                zone
            ),
        )
    }

    /// Another recording is being fitted.
    pub fn busy() -> Refused {
        Refused::new(
            "503 Service Unavailable",
            "busy",
            "",
            "this server fits one recording at a time and is fitting one now; send this \
             one again when that answer is back"
                .to_string(),
        )
    }

    /// The answer's body.
    pub fn encode(&self) -> String {
        Refusal::rejected(self.field, self.detail.clone())
            .at(CATALOG_VERSION)
            .encode()
    }
}

/// What the route's query says: the room, and which sweep was played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    /// The room the recording was made in.
    pub zone: String,
    /// The sweep it is a recording of.
    pub sweep: SweepSpec,
}

/// Read the route's query out of the request target (`/api/room-fit?...`).
///
/// `zone` is required. `sweep_ms` and `fade_in_ms` say which sweep was
/// played; without them it is the one `measure_sweep` plays. A member this
/// route does not have, one given twice, and a number outside its bounds are
/// refused by name: a recording fitted against the wrong sweep gives filters
/// for no room, so nothing here is guessed.
pub fn parse_query(target: &str) -> Result<Query, Refused> {
    let query = target.split_once('?').map_or("", |(_, q)| q);
    let mut zone = None;
    let mut sweep_ms = None;
    let mut fade_in_ms = None;
    let whole = |field: &'static str, text: &str| -> Result<u32, Refused> {
        match text.parse::<u32>() {
            Ok(n) if n.to_string() == text => Ok(n),
            _ => Err(Refused::query(
                field,
                format!("{} = '{}' is not a whole number of ms", field, text),
            )),
        }
    };
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let twice = |field: &'static str| {
            Refused::query(field, format!("'{}' is given more than once", field))
        };
        match key {
            "zone" => {
                if zone.replace(value.to_string()).is_some() {
                    return Err(twice("zone"));
                }
            }
            "sweep_ms" => {
                if sweep_ms.replace(whole("sweep_ms", value)?).is_some() {
                    return Err(twice("sweep_ms"));
                }
            }
            "fade_in_ms" => {
                if fade_in_ms.replace(whole("fade_in_ms", value)?).is_some() {
                    return Err(twice("fade_in_ms"));
                }
            }
            _ => {
                return Err(Refused::query(
                    "",
                    "this route's query has zone, sweep_ms and fade_in_ms, and this one has \
                     another member"
                        .to_string(),
                ))
            }
        }
    }
    let zone = zone.ok_or_else(|| {
        Refused::query(
            "zone",
            "the query names no room: zone=<room> is required".to_string(),
        )
    })?;
    if !is_identifier(&zone) {
        return Err(Refused::query(
            "zone",
            "zone is not a room's identifier".to_string(),
        ));
    }
    let recommended = SweepSpec::recommended();
    let sweep = SweepSpec {
        sweep_ms: sweep_ms.unwrap_or(recommended.sweep_ms),
        // A sweep whose length is given and whose fade-in is not has none:
        // only the recommended sweep's fade-in is known without being said.
        fade_in_ms: fade_in_ms.unwrap_or(if sweep_ms.is_none() {
            recommended.fade_in_ms
        } else {
            0
        }),
    };
    if sweep.sweep_ms < SWEEP_MS.0 || sweep.sweep_ms > SWEEP_MS.1 {
        return Err(Refused::query(
            "sweep_ms",
            format!(
                "sweep_ms = {} is outside {} to {}",
                sweep.sweep_ms, SWEEP_MS.0, SWEEP_MS.1
            ),
        ));
    }
    if sweep.fade_in_ms > FADE_IN_MS_MAX || sweep.fade_in_ms >= sweep.sweep_ms {
        return Err(Refused::query(
            "fade_in_ms",
            format!(
                "fade_in_ms = {} is outside 0 to {} or not shorter than the sweep",
                sweep.fade_in_ms, FADE_IN_MS_MAX
            ),
        ));
    }
    Ok(Query { zone, sweep })
}

/// The samples of a recording (full scale = 1) and its rate.
///
/// The body is a RIFF WAVE file: a `fmt ` chunk saying integer PCM, one
/// channel, 16 bits, and a `data` chunk; any other chunk (a `LIST`, a
/// `fact`) is stepped over. Every length is checked against the bytes there
/// are before it is used, so no body can make this read past its end.
pub fn parse_wav(bytes: &[u8]) -> Result<(u32, Vec<f32>), Refused> {
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(Refused::wav("the body is not a RIFF WAVE file".to_string()));
    }
    let mut rate = None;
    let mut at = 12usize;
    while bytes.len() - at >= 8 {
        let id = &bytes[at..at + 4];
        let len = u32_at(at + 4) as usize;
        let start = at + 8;
        if len > bytes.len() - start {
            return Err(Refused::wav(format!(
                "a chunk says it is {} bytes and the body has {} left",
                len,
                bytes.len() - start
            )));
        }
        if id == b"fmt " {
            if len < 16 {
                return Err(Refused::wav("the fmt chunk is too short".to_string()));
            }
            let (format, channels, bits) = (u16_at(start), u16_at(start + 2), u16_at(start + 14));
            if format != 1 || channels != 1 || bits != 16 {
                return Err(Refused::wav(format!(
                    "a recording is integer PCM (format 1), one channel, 16 bits, and this \
                     one is format {}, {} channels, {} bits",
                    format, channels, bits
                )));
            }
            rate = Some(u32_at(start + 4));
        } else if id == b"data" {
            let Some(rate) = rate else {
                return Err(Refused::wav(
                    "the data chunk comes before the fmt chunk".to_string(),
                ));
            };
            if rate != RATE_HZ {
                return Err(Refused::new(
                    "400 Bad Request",
                    "unsupported_rate",
                    "recording",
                    format!(
                        "a recording is {} Hz and this one says {} Hz; resample it first",
                        RATE_HZ, rate
                    ),
                ));
            }
            let samples = bytes[start..start + len]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| f32::from(i16::from_le_bytes(*c)) / 32_768.0)
                .collect();
            return Ok((rate, samples));
        }
        // A chunk is padded to an even length.
        at = start + len + (len & 1);
        if at > bytes.len() {
            break;
        }
    }
    Err(Refused::wav("the file has no data chunk".to_string()))
}

/// What an upload came to: the answer, and the one line said about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The status line's code and phrase.
    pub status: &'static str,
    /// The answer's body.
    pub body: String,
    /// What the server says about it on its output: the room, how much was
    /// sent, and how it ended. Counts and a name; never a sample.
    pub line: String,
}

/// Fit the recording in `body` for `query`'s room and write the answer.
///
/// On a fit: `{"v":2,"t":"room_fit","zone":...,"sweep_ms":...,"filters":[...],
/// "rms_before_db":...,"rms_after_db":...}`, the filters spelled as the
/// catalog's `room_eq` spells them (the fitter's own `filter_json`), so the
/// answer's `zone` and `filters` are a `room_eq` command's. A room with
/// nothing to correct answers with no filters.
pub fn fit(query: &Query, body: &[u8]) -> Outcome {
    let outcome = |status, body: String, how: String| Outcome {
        status,
        body,
        line: format!(
            "room fit: room '{}', a {} ms sweep, {}",
            query.zone, query.sweep.sweep_ms, how
        ),
    };
    let refused = |r: Refused, sent: String| {
        outcome(
            r.status,
            r.encode(),
            format!("{}, refused {}", sent, r.name),
        )
    };
    let (rate, samples) = match parse_wav(body) {
        Ok(read) => read,
        Err(r) => return refused(r, format!("{} bytes", body.len())),
    };
    let sent = format!("{} samples at {} Hz", samples.len(), rate);
    match fit_recording(
        &samples,
        &query.sweep.sweep(rate),
        &Target::flat(),
        &FitConfig::default(),
    ) {
        Ok(fit) => {
            let filters = fit
                .filters
                .iter()
                .map(filter_json)
                .collect::<Vec<_>>()
                .join(",");
            outcome(
                "200 OK",
                format!(
                    "{{\"v\":{},\"t\":\"room_fit\",\"zone\":\"{}\",\"sweep_ms\":{},\
                     \"filters\":[{}],\"rms_before_db\":{:.2},\"rms_after_db\":{:.2}}}",
                    CATALOG_VERSION,
                    query.zone,
                    query.sweep.sweep_ms,
                    filters,
                    fit.rms_before_db(),
                    fit.rms_after_db()
                ),
                format!("{}, fitted {} filters", sent, fit.filters.len()),
            )
        }
        Err(e) => refused(Refused::fitter(&e), sent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chorus_dsp::roomfit::synthetic::wav_bytes;

    #[test]
    fn without_a_word_about_the_sweep_it_is_the_one_measure_sweep_plays() {
        let q = parse_query("/api/room-fit?zone=living").unwrap();
        assert_eq!(q.zone, "living");
        assert_eq!(q.sweep, SweepSpec::recommended());
        assert_eq!((q.sweep.sweep_ms, q.sweep.fade_in_ms), (5_000, 100));
        assert_eq!(q.sweep.sweep(RATE_HZ), Sweep::recommended(RATE_HZ));
    }

    #[test]
    fn a_sweep_whose_length_is_given_has_the_fade_in_it_is_given_or_none() {
        let q = parse_query("/api/room-fit?zone=living&sweep_ms=1000").unwrap();
        assert_eq!((q.sweep.sweep_ms, q.sweep.fade_in_ms), (1_000, 0));
        let sweep = q.sweep.sweep(RATE_HZ);
        assert_eq!((sweep.samples, sweep.fade_in_samples), (48_000, 0));
        assert_eq!(
            (sweep.f1_hz, sweep.f2_hz, sweep.amplitude),
            (10.0, 20_000.0, 0.5)
        );
        let q = parse_query("/api/room-fit?sweep_ms=5000&fade_in_ms=100&zone=a").unwrap();
        assert_eq!(q.sweep, SweepSpec::recommended());
    }

    #[test]
    fn a_query_this_route_cannot_read_is_refused_by_name_and_field() {
        for (target, field) in [
            ("/api/room-fit", "zone"),
            ("/api/room-fit?sweep_ms=1000", "zone"),
            ("/api/room-fit?zone=", "zone"),
            ("/api/room-fit?zone=Not%20One", "zone"),
            ("/api/room-fit?zone=a&zone=b", "zone"),
            ("/api/room-fit?zone=a&sweep_ms=999", "sweep_ms"),
            ("/api/room-fit?zone=a&sweep_ms=10001", "sweep_ms"),
            ("/api/room-fit?zone=a&sweep_ms=1e3", "sweep_ms"),
            ("/api/room-fit?zone=a&sweep_ms=+1000", "sweep_ms"),
            (
                "/api/room-fit?zone=a&sweep_ms=1000&sweep_ms=1000",
                "sweep_ms",
            ),
            ("/api/room-fit?zone=a&fade_in_ms=1001", "fade_in_ms"),
            (
                "/api/room-fit?zone=a&sweep_ms=1000&fade_in_ms=1000",
                "fade_in_ms",
            ),
            ("/api/room-fit?zone=a&fade_in_ms=-1", "fade_in_ms"),
            ("/api/room-fit?zone=a&rate=48000", ""),
        ] {
            let refused = parse_query(target).unwrap_err();
            assert_eq!(refused.status, "400 Bad Request", "{}", target);
            assert_eq!(refused.name, "bad_query", "{}", target);
            assert_eq!(refused.field, field, "{}", target);
            assert!(refused.detail.starts_with("bad_query: "), "{}", target);
        }
    }

    #[test]
    fn a_wav_is_read_through_chunks_it_does_not_know_and_never_past_its_end() {
        let pcm = [0i16, 16_384, -32_768, 32_767];
        let plain = wav_bytes(RATE_HZ, &pcm);
        let want = vec![0.0f32, 0.5, -1.0, 32_767.0 / 32_768.0];
        assert_eq!(parse_wav(&plain).unwrap(), (RATE_HZ, want.clone()));

        // A LIST chunk of odd length (padded) between fmt and data.
        let mut listed = plain[..36].to_vec();
        listed.extend_from_slice(b"LIST");
        listed.extend_from_slice(&3u32.to_le_bytes());
        listed.extend_from_slice(b"abc\0");
        listed.extend_from_slice(&plain[36..]);
        assert_eq!(parse_wav(&listed).unwrap(), (RATE_HZ, want));

        let name = |bytes: &[u8]| parse_wav(bytes).unwrap_err().name;
        assert_eq!(name(b""), "not_wav");
        assert_eq!(name(b"RIFF\0\0\0\0WAVE"), "not_wav");
        assert_eq!(name(&plain[..40]), "not_wav");
        // Every truncation is a refusal and none is a panic.
        for cut in 0..plain.len() {
            assert!(parse_wav(&plain[..cut]).is_err(), "cut at {}", cut);
        }
        // A data chunk that claims more than there is.
        let mut long = plain.clone();
        long[40..44].copy_from_slice(&1_000u32.to_le_bytes());
        assert_eq!(name(&long), "not_wav");
        // Stereo, 8-bit and float are not recordings.
        for (at, value) in [(22usize, 2u16), (34, 8), (20, 3)] {
            let mut other = plain.clone();
            other[at..at + 2].copy_from_slice(&value.to_le_bytes());
            assert_eq!(name(&other), "not_wav", "byte {}", at);
        }
        assert_eq!(name(&wav_bytes(44_100, &pcm)), "unsupported_rate");
    }

    #[test]
    fn a_refusal_is_the_catalogs_error_with_the_name_first() {
        let refused = Refused::fitter(&RoomFitError::TooQuiet {
            peak_dbfs: -65.2,
            min_dbfs: -40.0,
        });
        assert_eq!(refused.status, "422 Unprocessable Content");
        assert_eq!(refused.name, "too_quiet");
        let body = refused.encode();
        assert!(
            body.starts_with(r#"{"v":2,"t":"error","field":"recording","detail":"too_quiet: "#),
            "{}",
            body
        );
        assert_eq!(Refused::busy().status, "503 Service Unavailable");
        assert!(Refused::correction_on("living")
            .encode()
            .contains(r#""field":"zone","detail":"correction_on: room 'living'"#));
    }

    #[test]
    fn the_line_said_about_an_upload_carries_counts_and_a_name_only() {
        let query = parse_query("/api/room-fit?zone=living&sweep_ms=1000").unwrap();
        let silence = wav_bytes(RATE_HZ, &[0i16; 100]);
        let outcome = fit(&query, &silence);
        assert_eq!(
            outcome.line,
            "room fit: room 'living', a 1000 ms sweep, 100 samples at 48000 Hz, refused \
             too_short"
        );
        assert_eq!(outcome.status, "422 Unprocessable Content");
        let outcome = fit(&query, b"not a wav");
        assert_eq!(
            outcome.line,
            "room fit: room 'living', a 1000 ms sweep, 9 bytes, refused not_wav"
        );
    }

    #[test]
    fn the_longest_sweep_and_its_response_fit_inside_the_body_bound() {
        let longest = SweepSpec {
            sweep_ms: SWEEP_MS.1,
            fade_in_ms: 0,
        }
        .sweep(RATE_HZ);
        // Two bytes a sample, a second of room after the sweep, the header.
        assert!((longest.samples + RATE_HZ as usize) * 2 + 44 <= MAX_RECORDING_BYTES);
    }
}
