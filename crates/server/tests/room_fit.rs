//! A recording is uploaded, fitted and applied to a room, and the correction
//! can be undone: `POST /api/room-fit` and `room_eq_undo` on the real binary
//! (`docs/decisions/0000-a-recording-is-fitted-and-not-kept.md`,
//! `docs/room-correction.md`, `docs/control-plane.md`).
//!
//! Every test runs the real `chorus-server` with a control plane and speaks
//! HTTP to it over a real socket. The recordings are the fitter's own
//! fixtures (`fixtures/roomfit/`), sent byte for byte as the files are, and
//! the filters expected are the ones `docs/room-correction.md`'s table lists
//! for each. The fixtures are recordings of a 1 s sweep with no fade-in, so
//! every upload here says `sweep_ms=1000`; one test shows what happens to
//! the same recording when the route is left to assume the 5 s sweep.
//!
//! The test names are the evidence. Nothing here is timing evidence: what is
//! graded is statuses, names, filters and bytes, and one bound ("refused
//! before it is read whole") is shown by the answer arriving while the body
//! has not been sent at all.

mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chorus_control::json::{self, Value};
use chorus_server::control::{MAX_REQUEST_BYTES, REQUEST_DEADLINE};
use chorus_server::roomfit::{CONTENT_TYPE, MAX_RECORDING_BYTES, ROUTE};
use common::RunningServer;

/// The fits `docs/room-correction.md`'s table lists (fixtures 01 to 03), as
/// the catalog spells filters.
const FIT_01: &str = concat!(
    r#"[{"freq_hz":45,"gain_db":-9.32,"q":6.409},"#,
    r#"{"freq_hz":119,"gain_db":-6.53,"q":5.135}]"#
);
const FIT_02: &str = concat!(
    r#"[{"freq_hz":38,"gain_db":-7.23,"q":7.965},"#,
    r#"{"freq_hz":94,"gain_db":-5.94,"q":3.410},"#,
    r#"{"freq_hz":152,"gain_db":1.37,"q":2.420},"#,
    r#"{"freq_hz":211,"gain_db":-8.70,"q":6.366}]"#
);
const FIT_03: &str = concat!(
    r#"[{"freq_hz":55,"gain_db":-12.00,"q":6.625},"#,
    r#"{"freq_hz":55,"gain_db":-3.06,"q":9.820},"#,
    r#"{"freq_hz":170,"gain_db":-4.97,"q":2.497}]"#
);

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/roomfit")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

fn server(extra: &[&str]) -> RunningServer {
    let mut flags = vec![
        "--source",
        "tone",
        "--serve-forever",
        "--zone",
        "living",
        "--zone",
        "kitchen",
    ];
    flags.extend_from_slice(extra);
    RunningServer::start(&flags)
}

/// One request with a body of bytes on a fresh connection: the status line
/// and the body of the answer.
fn exchange(address: &str, head: &str, body: &[u8]) -> (String, String) {
    let mut socket = TcpStream::connect(address).expect("the control plane listens");
    socket
        .set_read_timeout(Some(Duration::from_secs(120)))
        .unwrap();
    socket.write_all(head.as_bytes()).unwrap();
    // A refusal decided from the head may close the connection under a
    // large body; what matters then is the answer, which is read below.
    let _ = socket.write_all(body);
    let mut answer = String::new();
    let _ = socket.read_to_string(&mut answer);
    let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
    (
        head.lines().next().unwrap_or("").to_string(),
        body.to_string(),
    )
}

/// `POST /api/room-fit?<query>` with `recording` as its body, as the app
/// sends it: `audio/wav`, a `Content-Length`, no `Origin`.
fn upload(server: &RunningServer, query: &str, recording: &[u8]) -> (String, String) {
    exchange(
        &server.control,
        &format!(
            "POST {}?{} HTTP/1.1\r\nHost: chorus\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n",
            ROUTE,
            query,
            CONTENT_TYPE,
            recording.len()
        ),
        recording,
    )
}

/// An upload that must be fitted: the answer's `filters`, encoded.
fn fitted(server: &RunningServer, zone: &str, name: &str) -> String {
    let (status, answer) = upload(
        server,
        &format!("zone={}&sweep_ms=1000", zone),
        &fixture(name),
    );
    assert!(status.contains("200"), "{}: {} {}", name, status, answer);
    let value = json::parse(&answer).unwrap_or_else(|e| panic!("{}: {:?}", answer, e));
    assert_eq!(value.get("t").and_then(Value::as_str), Some("room_fit"));
    assert_eq!(value.get("zone").and_then(Value::as_str), Some(zone));
    assert_eq!(value.get("sweep_ms").and_then(Value::as_num), Some("1000"));
    json::write(value.get("filters").expect("a fit has filters"))
}

/// An upload that must be refused: the status, and the refusal's name (what
/// its `detail` starts with, up to the colon).
fn refused(server: &RunningServer, query: &str, recording: &[u8]) -> (String, String) {
    let (status, answer) = upload(server, query, recording);
    let value = json::parse(&answer).unwrap_or_else(|e| panic!("{} {}: {:?}", status, answer, e));
    assert_eq!(value.get("t").and_then(Value::as_str), Some("error"));
    let detail = value.get("detail").and_then(Value::as_str).unwrap();
    (status, detail.split(':').next().unwrap_or("").to_string())
}

/// The room's `room_eq` in `GET /api/state`, encoded.
fn room_eq(server: &RunningServer, zone: &str) -> String {
    let state = server.state();
    let value = json::parse(&state).unwrap_or_else(|e| panic!("{}: {:?}", state, e));
    let Some(Value::Arr(rooms)) = value.get("zones") else {
        panic!("the state has no zones: {}", state);
    };
    let room = rooms
        .iter()
        .find(|z| z.get("id").and_then(Value::as_str) == Some(zone))
        .unwrap_or_else(|| panic!("the state has no room {}", zone));
    json::write(room.get("room_eq").expect("a v2 room has room_eq"))
}

fn apply(server: &RunningServer, zone: &str, filters: &str, enabled: bool) {
    server.applied(&format!(
        r#"{{"v":2,"t":"room_eq","zone":"{}","filters":{},"enabled":{}}}"#,
        zone, filters, enabled
    ));
}

fn switch(server: &RunningServer, zone: &str, enabled: bool) {
    server.applied(&format!(
        r#"{{"v":2,"t":"room_eq","zone":"{}","enabled":{}}}"#,
        zone, enabled
    ));
}

fn undo(server: &RunningServer, zone: &str) -> (String, String) {
    server.command(&format!(
        r#"{{"v":2,"t":"room_eq_undo","zone":"{}"}}"#,
        zone
    ))
}

const NO_CORRECTION: &str = r#"{"enabled":true,"filters":[]}"#;

#[test]
fn fixtures_01_to_03_are_fitted_with_the_documented_filters_applied_and_read_back() {
    let server = server(&[]);
    assert_eq!(room_eq(&server, "living"), NO_CORRECTION);
    for (name, want) in [
        ("01-two-modes-one-null.wav", FIT_01),
        ("02-three-modes.wav", FIT_02),
        ("03-strong-mode.wav", FIT_03),
    ] {
        // The room is measured with its correction off: with one switched
        // on, the recording would be of the corrected room, and the upload
        // is refused by name with nothing changed.
        let before = room_eq(&server, "living");
        if before != NO_CORRECTION {
            let (status, why) = refused(&server, "zone=living&sweep_ms=1000", &fixture(name));
            assert!(status.contains("409"), "{}", status);
            assert_eq!(why, "correction_on");
            assert_eq!(room_eq(&server, "living"), before);
            switch(&server, "living", false);
        }
        let filters = fitted(&server, "living", name);
        assert_eq!(filters, want, "{}", name);
        // An upload changes nothing by itself.
        let unapplied = room_eq(&server, "living");
        assert!(!unapplied.contains(want), "{}: {}", name, unapplied);
        // The answer's filters are a room_eq command's filters as they are.
        apply(&server, "living", &filters, true);
        assert_eq!(
            room_eq(&server, "living"),
            format!(r#"{{"enabled":true,"filters":{},"undo":true}}"#, want),
            "{}",
            name
        );
        // The other room was never touched.
        assert_eq!(room_eq(&server, "kitchen"), NO_CORRECTION);
    }
}

#[test]
fn fixtures_04_to_07_are_refused_by_the_fitters_names_and_the_correction_is_unchanged() {
    let server = server(&[]);
    // A correction that is there, switched off for the measurement.
    apply(&server, "living", FIT_01, false);
    let before = room_eq(&server, "living");
    assert_eq!(
        before,
        format!(r#"{{"enabled":false,"filters":{},"undo":true}}"#, FIT_01)
    );
    for (name, want) in [
        ("04-too-quiet.wav", "too_quiet"),
        ("05-clipped.wav", "clipped"),
        ("06-too-short.wav", "too_short"),
        ("07-too-noisy.wav", "too_noisy"),
    ] {
        for zone in ["living", "kitchen"] {
            let (status, why) = refused(
                &server,
                &format!("zone={}&sweep_ms=1000", zone),
                &fixture(name),
            );
            assert!(status.contains("422"), "{}: {}", name, status);
            assert_eq!(why, want, "{}", name);
        }
        assert_eq!(room_eq(&server, "living"), before, "{}", name);
        assert_eq!(room_eq(&server, "kitchen"), NO_CORRECTION, "{}", name);
    }
}

#[test]
fn the_route_has_to_be_told_which_sweep_was_played() {
    let server = server(&[]);
    // Fixture 01 is a recording of a 1 s sweep. Left to assume the 5 s sweep
    // `measure_sweep` plays, the fitter finds it shorter than that sweep and
    // its response, and says so; told the truth, it fits.
    let recording = fixture("01-two-modes-one-null.wav");
    let (status, why) = refused(&server, "zone=living", &recording);
    assert!(status.contains("422"), "{}", status);
    assert_eq!(why, "too_short");
    let (status, why) = refused(
        &server,
        "zone=living&sweep_ms=5000&fade_in_ms=100",
        &recording,
    );
    assert!(status.contains("422"), "{}", status);
    assert_eq!(why, "too_short");
    assert_eq!(
        fitted(&server, "living", "01-two-modes-one-null.wav"),
        FIT_01
    );
    // And what it is told is held to its bounds and its shape.
    for query in [
        "",
        "sweep_ms=1000",
        "zone=living&sweep_ms=999",
        "zone=living&sweep_ms=10001",
        "zone=living&sweep_ms=1000&fade_in_ms=1000",
        "zone=living&sweep_ms=1000&rate_hz=44100",
    ] {
        let (status, why) = refused(&server, query, &recording);
        assert!(status.contains("400"), "{}: {}", query, status);
        assert_eq!(why, "bad_query", "{}", query);
    }
    // A room this server does not have, in the command route's words.
    let (status, answer) = upload(&server, "zone=attic&sweep_ms=1000", &recording);
    assert!(status.contains("400"), "{}", status);
    assert!(
        answer.contains(r#""field":"zone""#) && answer.contains("there is no zone 'attic'"),
        "{}",
        answer
    );
    // A body that is not a recording.
    let (status, why) = refused(&server, "zone=living&sweep_ms=1000", b"not a wav at all");
    assert!(status.contains("400"), "{}", status);
    assert_eq!(why, "not_wav");
    assert_eq!(room_eq(&server, "living"), NO_CORRECTION);
}

#[test]
fn an_undo_restores_the_earlier_correction_exactly_and_the_state_says_when_there_is_one() {
    let server = server(&[]);

    // Nothing to undo in a room that was never corrected, and the state
    // does not offer one.
    assert_eq!(room_eq(&server, "living"), NO_CORRECTION);
    let (status, answer) = undo(&server, "living");
    assert!(status.contains("400"), "{}", status);
    assert!(answer.contains("nothing-to-undo"), "{}", answer);

    // After a first apply, an undo leaves no correction.
    apply(&server, "living", FIT_01, true);
    assert_eq!(
        room_eq(&server, "living"),
        format!(r#"{{"enabled":true,"filters":{},"undo":true}}"#, FIT_01)
    );
    let (status, answer) = undo(&server, "living");
    assert!(status.contains("200"), "{} {}", status, answer);
    assert_eq!(room_eq(&server, "living"), NO_CORRECTION);
    let (status, answer) = undo(&server, "living");
    assert!(status.contains("400"), "{}", status);
    assert!(answer.contains("nothing-to-undo"), "{}", answer);

    // After an apply over an earlier correction, an undo restores exactly
    // the earlier filters and the earlier flag, whichever it was.
    for earlier_enabled in [true, false] {
        apply(&server, "living", FIT_02, earlier_enabled);
        let earlier = room_eq(&server, "living");
        assert!(
            earlier.starts_with(&format!(
                r#"{{"enabled":{},"filters":{}"#,
                earlier_enabled, FIT_02
            )),
            "{}",
            earlier
        );
        // The upload of a new measurement, applied over it.
        switch(&server, "living", false);
        let filters = fitted(&server, "living", "03-strong-mode.wav");
        switch(&server, "living", earlier_enabled);
        apply(&server, "living", &filters, true);
        assert_eq!(
            room_eq(&server, "living"),
            format!(r#"{{"enabled":true,"filters":{},"undo":true}}"#, FIT_03)
        );
        // Comparing with and without the new fit keeps the undo.
        switch(&server, "living", false);
        switch(&server, "living", true);
        assert!(room_eq(&server, "living").ends_with(r#","undo":true}"#));

        let (status, answer) = undo(&server, "living");
        assert!(status.contains("200"), "{} {}", status, answer);
        assert_eq!(
            room_eq(&server, "living"),
            format!(r#"{{"enabled":{},"filters":{}}}"#, earlier_enabled, FIT_02),
            "the earlier correction, and no second step"
        );
        let (status, _) = undo(&server, "living");
        assert!(status.contains("400"), "{}", status);
    }
    // The room beside it never had one.
    assert_eq!(room_eq(&server, "kitchen"), NO_CORRECTION);
}

/// The head of an upload with these headers and this declared length.
fn head(query: &str, headers: &str, length: Option<usize>) -> String {
    let length = match length {
        Some(n) => format!("Content-Length: {}\r\n", n),
        None => String::new(),
    };
    format!(
        "POST {}?{} HTTP/1.1\r\nHost: chorus.local:4020\r\n{}{}Connection: close\r\n\r\n",
        ROUTE, query, headers, length
    )
}

#[test]
fn a_recording_over_the_maximum_is_refused_before_it_is_read() {
    let server = server(&[]);
    assert_eq!(MAX_RECORDING_BYTES, 2 * 1024 * 1024);
    // The head alone, declaring one byte more than the maximum, and not one
    // byte of a body: a server that meant to read the body whole would wait
    // for it until the request deadline and answer 408. This one answers 413
    // from the head.
    let asked = Instant::now();
    let (status, answer) = exchange(
        &server.control,
        &head(
            "zone=living&sweep_ms=1000",
            "Content-Type: audio/wav\r\n",
            Some(MAX_RECORDING_BYTES + 1),
        ),
        b"",
    );
    assert!(status.contains("413"), "{} {}", status, answer);
    assert!(
        answer.contains(&format!("at most {} bytes", MAX_RECORDING_BYTES)),
        "{}",
        answer
    );
    assert!(
        asked.elapsed() < REQUEST_DEADLINE,
        "the refusal took {:?}, which is the deadline a body is waited for",
        asked.elapsed()
    );
    // The same with the body really sent: refused, and never fitted.
    let mut big = fixture("01-two-modes-one-null.wav");
    big.resize(MAX_RECORDING_BYTES + 1, 0);
    let (status, _) = upload(&server, "zone=living&sweep_ms=1000", &big);
    assert!(status.contains("413") || status.is_empty(), "{}", status);
    // A recording with no declared length is not read either.
    let (status, answer) = exchange(
        &server.control,
        &head(
            "zone=living&sweep_ms=1000",
            "Content-Type: audio/wav\r\n",
            None,
        ),
        b"",
    );
    assert!(status.contains("411"), "{} {}", status, answer);
    // A body of exactly the maximum is read whole and fitted: the recording
    // with bytes after its data chunk, which a WAV may have.
    big.truncate(MAX_RECORDING_BYTES);
    let (status, answer) = upload(&server, "zone=living&sweep_ms=1000", &big);
    assert!(status.contains("200"), "{} {}", status, answer);
    assert!(answer.contains(FIT_01), "{}", answer);
    assert_eq!(room_eq(&server, "living"), NO_CORRECTION);
}

#[test]
fn a_wrong_content_type_and_a_cross_origin_upload_are_refused_as_a_command_is() {
    let server = server(&[]);
    let recording = fixture("01-two-modes-one-null.wav");
    // A refusal by these rules is decided from the head, so the refused
    // requests here send the head alone (a body still on its way when the
    // server closes can cost the peer the answer, as with any early
    // refusal); the ones that are served send the recording.
    let send_with = |headers: &str, body: &[u8]| {
        exchange(
            &server.control,
            &head("zone=living&sweep_ms=1000", headers, Some(recording.len())),
            body,
        )
    };
    let send = |headers: &str| send_with(headers, b"");
    // What a cross-site page can send without a preflight, the command
    // route's own type, and no type at all: 415, as `POST /api/command`
    // answers a body that is not declared its type.
    for headers in [
        "",
        "Content-Type: text/plain\r\n",
        "Content-Type: application/x-www-form-urlencoded\r\n",
        "Content-Type: multipart/form-data; boundary=x\r\n",
        "Content-Type: application/json\r\n",
        "Content-Type: audio/wave\r\n",
    ] {
        let (status, answer) = send(headers);
        assert!(
            status.contains("415"),
            "{:?}: {} {}",
            headers,
            status,
            answer
        );
    }
    let (status, _) = server_command_with(&server, "Content-Type: text/plain\r\n");
    assert!(status.contains("415"), "{}", status);

    // An Origin that is not this server's own: 403, as a command's is.
    for origin in ["http://evil.example", "https://chorus.local:9999", "null"] {
        let (status, answer) = send(&format!(
            "Content-Type: audio/wav\r\nOrigin: {}\r\n",
            origin
        ));
        assert!(status.contains("403"), "{}: {} {}", origin, status, answer);
    }
    let (status, _) = server_command_with(
        &server,
        "Content-Type: application/json\r\nOrigin: http://evil.example\r\n",
    );
    assert!(status.contains("403"), "{}", status);
    assert_eq!(room_eq(&server, "living"), NO_CORRECTION);

    // The app's own origin, and a client that is not a browser, are served.
    for headers in [
        "Content-Type: audio/wav\r\nOrigin: http://chorus.local:4020\r\n",
        "Content-Type: audio/wav\r\nOrigin: https://chorus.local:4020\r\n",
        "Content-Type: AUDIO/WAV; charset=binary\r\n",
    ] {
        let (status, answer) = send_with(headers, &recording);
        assert!(
            status.contains("200"),
            "{:?}: {} {}",
            headers,
            status,
            answer
        );
        assert!(answer.contains(FIT_01), "{}", answer);
    }
}

/// `POST /api/command` with these headers and a small command.
fn server_command_with(server: &RunningServer, headers: &str) -> (String, String) {
    let body = r#"{"v":2,"t":"room_eq","zone":"living","enabled":false}"#;
    exchange(
        &server.control,
        &format!(
            "POST /api/command HTTP/1.1\r\nHost: chorus.local:4020\r\n{}Content-Length: {}\r\n\
             Connection: close\r\n\r\n",
            headers,
            body.len()
        ),
        body.as_bytes(),
    )
}

#[test]
fn every_other_route_keeps_its_16_kib_bound() {
    let server = server(&[]);
    assert_eq!(MAX_REQUEST_BYTES, 16 * 1024);
    // A body the upload route would take is past every other route's bound,
    // and is refused there from the head, unread, as it always was. So is
    // the upload's own path under another method, and a path that only
    // starts like it.
    let length = fixture("01-two-modes-one-null.wav").len();
    assert!(length > MAX_REQUEST_BYTES && length < MAX_RECORDING_BYTES);
    for (method, target, content_type) in [
        ("POST", "/api/command", "application/json"),
        ("POST", "/api/leaving", "application/json"),
        ("POST", "/api/room-fit-other?zone=living", "audio/wav"),
        ("POST", "/api/room-fit/x?zone=living", "audio/wav"),
        ("POST", "/api/state", "audio/wav"),
        ("PUT", "/api/room-fit?zone=living", "audio/wav"),
        ("GET", "/api/room-fit?zone=living", "audio/wav"),
        ("GET", "/api/state", "audio/wav"),
    ] {
        let asked = Instant::now();
        let (status, answer) = exchange(
            &server.control,
            &format!(
                "{} {} HTTP/1.1\r\nHost: chorus\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n",
                method, target, content_type, length
            ),
            b"",
        );
        assert!(
            status.contains("413"),
            "{} {}: {} {}",
            method,
            target,
            status,
            answer
        );
        assert!(
            answer.contains(&format!("at most {} bytes", MAX_REQUEST_BYTES)),
            "{} {}: {}",
            method,
            target,
            answer
        );
        assert!(asked.elapsed() < REQUEST_DEADLINE, "{} {}", method, target);
    }
    // A body one byte past what is left of 16 KiB is refused on the command
    // route, and the upload route's head is held to the same 16 KiB.
    let (status, _) = exchange(
        &server.control,
        &format!(
            "POST /api/command HTTP/1.1\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\n\r\n",
            MAX_REQUEST_BYTES
        ),
        b"",
    );
    assert!(status.contains("413"), "{}", status);
    let (status, _) = exchange(
        &server.control,
        &format!(
            "POST {}?zone=living HTTP/1.1\r\nContent-Type: audio/wav\r\nX-Pad: {}\r\n\
             Content-Length: 44\r\n\r\n",
            ROUTE,
            "a".repeat(MAX_REQUEST_BYTES)
        ),
        b"",
    );
    assert!(status.contains("431"), "{}", status);
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chorus-room-fit-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// Every file under `dir`, by path, with its bytes.
fn files_under(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(next) = todo.pop() {
        for entry in std::fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                todo.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                found.push((path, bytes));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_recording_is_not_written_to_disk_and_not_logged() {
    let dir = scratch("kept");
    let identity = dir.join("identity");
    let state_file = dir.join("zones.state");
    let mut server = RunningServer::start_on(
        "127.0.0.1:0",
        &["--identity-dir", identity.to_str().unwrap()],
        &[
            "--source",
            "tone",
            "--serve-forever",
            "--zone",
            "living",
            "--zone",
            "kitchen",
            "--state-file",
            state_file.to_str().unwrap(),
        ],
    );
    // A command first, so the state file is there and the directory is what
    // a running server's is.
    switch(&server, "living", false);
    let before = files_under(&dir);
    assert!(
        before.iter().any(|(path, _)| path == &state_file),
        "{:?}",
        before.iter().map(|(p, _)| p).collect::<Vec<_>>()
    );
    server.drain();
    let said_before = server.seen.len();

    // A recording that is fitted and one that is refused.
    let good = fixture("01-two-modes-one-null.wav");
    let quiet = fixture("04-too-quiet.wav");
    assert_eq!(
        fitted(&server, "living", "01-two-modes-one-null.wav"),
        FIT_01
    );
    let (_, why) = refused(&server, "zone=living&sweep_ms=1000", &quiet);
    assert_eq!(why, "too_quiet");

    // The state directory gained no file, and no file in it changed.
    let after = files_under(&dir);
    assert_eq!(
        after.iter().map(|(p, _)| p).collect::<Vec<_>>(),
        before.iter().map(|(p, _)| p).collect::<Vec<_>>(),
        "the state directory's files"
    );
    assert!(after == before, "a file in the state directory changed");
    // And nothing in it holds a stretch of either recording.
    for (path, bytes) in &after {
        for recording in [&good, &quiet] {
            let stretch = &recording[4_000..4_064];
            assert!(
                !bytes.windows(stretch.len()).any(|w| w == stretch),
                "{} holds a stretch of a recording",
                path.display()
            );
        }
    }

    // The server said one line about each upload, with counts and a name.
    server.wait_for("refused too_quiet");
    server.drain();
    let said: Vec<String> = server.seen[said_before..].to_vec();
    let about: Vec<&String> = said.iter().filter(|l| l.contains("room fit")).collect();
    assert_eq!(
        about,
        [
            "chorus-server: room fit: room 'living', a 1000 ms sweep, 76800 samples at 48000 \
             Hz, fitted 2 filters",
            "chorus-server: room fit: room 'living', a 1000 ms sweep, 76800 samples at 48000 \
             Hz, refused too_quiet",
        ],
        "{:?}",
        said
    );
    // No sample data: everything the server said while 300 KB of recordings
    // went through it is a few short lines, none of them the file's bytes
    // or a run of numbers.
    let total: usize = said.iter().map(String::len).sum();
    assert!(
        total < 2_048,
        "{} bytes were said during two uploads:\n{}",
        total,
        said.join("\n")
    );
    for line in &said {
        assert!(!line.contains("RIFF") && !line.contains("WAVE"), "{}", line);
        let numbers = line
            .split(|c: char| !(c.is_ascii_digit() || c == '-' || c == '.'))
            .filter(|t| t.chars().any(|c| c.is_ascii_digit()))
            .count();
        assert!(numbers <= 8, "a line with {} numbers: {}", numbers, line);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The grep behind the test above: the code that holds a recording opens no
/// file and prints nothing, and the route prints one line, the outcome's.
#[test]
fn the_code_that_holds_a_recording_opens_no_file_and_prints_only_the_outcomes_line() {
    let pure = include_str!("../src/roomfit.rs");
    let code = pure.split("#[cfg(test)]").next().unwrap();
    for word in [
        "std::fs",
        "File",
        "OpenOptions",
        "println!",
        "eprintln!",
        "print!",
        "dbg!",
        "{:?}",
    ] {
        assert!(
            !code
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .any(|l| l.contains(word)),
            "crates/server/src/roomfit.rs uses {}",
            word
        );
    }
    let control = include_str!("../src/control.rs");
    let start = control.find("fn serve_room_fit").expect("the route");
    let route = &control[start..];
    let route = &route[..route.find("\n}\n").expect("the route's end")];
    assert_eq!(route.matches("println!").count(), 1, "{}", route);
    assert!(route.contains(r#"println!("chorus-server: {}", outcome.line);"#));
    for word in ["std::fs", "File", "eprintln!", "dbg!", "{:?}", "write"] {
        assert_eq!(
            route.matches(word).count(),
            0,
            "the upload route uses {}",
            word
        );
    }
}
