//! ICY radio streams from an in-test server: the metadata blocks come out of
//! the audio at several block spacings, with blocks split across writes and
//! zero-length blocks, and the latest title is exposed.

mod common;

use std::io::Read;
use std::thread;
use std::time::Duration;

use chorus_fetch::{open, IcyInfo};
use common::{pattern, policy, read_all, Server};

/// A metadata block: the length byte, then the text padded to 16s.
fn block(text: &str) -> Vec<u8> {
    let units = text.len().div_ceil(16);
    let mut out = vec![units as u8];
    out.extend_from_slice(text.as_bytes());
    out.resize(1 + units * 16, 0);
    out
}

/// Serves `audio` as an ICY stream with a block every `metaint` bytes, taken
/// in turn from `titles` (an empty one is the zero-length block). Every block
/// is written in two halves with a pause between, so it is split across the
/// client's reads.
fn radio(
    status_line: &'static str,
    audio: Vec<u8>,
    metaint: usize,
    titles: Vec<&'static str>,
) -> Server {
    Server::start(move |_, out| {
        let _ = write!(
            out,
            "{status_line}\r\nicy-name: Example Radio\r\nicy-genre: Test Tones\r\nicy-br: 128\r\n\
             icy-metaint: {metaint}\r\nContent-Type: audio/mpeg\r\n\r\n"
        );
        for (i, chunk) in audio.chunks(metaint).enumerate() {
            let _ = out.write_all(chunk);
            if chunk.len() < metaint {
                break;
            }
            let block = block(titles[i % titles.len()]);
            let (a, b) = block.split_at(block.len() / 2);
            let _ = out.write_all(a);
            let _ = out.flush();
            if i < 4 {
                thread::sleep(Duration::from_millis(15));
            }
            let _ = out.write_all(b);
        }
    })
}

#[test]
fn metadata_is_stripped_at_several_block_spacings() {
    let titles = vec![
        "StreamTitle='First Artist - First Song';",
        "",
        "StreamTitle='Second - It's a Song';StreamUrl='http://radio.example/now';",
        "",
    ];
    for metaint in [1usize, 16, 100, 1000, 8192, 16000] {
        let audio = pattern(metaint * 6 + metaint / 3, metaint as u8);
        let server = radio("HTTP/1.0 200 OK", audio.clone(), metaint, titles.clone());
        let mut stream = open(&server.url("/radio"), &policy()).unwrap();
        let opened = stream.opened().clone();
        assert_eq!(
            opened.icy,
            Some(IcyInfo {
                name: Some("Example Radio".into()),
                bitrate_kbps: Some(128),
                genre: Some("Test Tones".into()),
                metaint: Some(metaint),
            })
        );
        assert_eq!(opened.byte_len, None);
        assert!(!opened.seekable);
        assert_eq!(opened.content_type.as_deref(), Some("audio/mpeg"));
        assert_eq!(read_all(&mut stream).unwrap(), audio, "metaint {metaint}");
        // Six blocks: titles 0, 1 (empty), 2, 3 (empty), 0, 1 (empty).
        assert_eq!(
            stream.stream_title().as_deref(),
            Some("First Artist - First Song"),
            "metaint {metaint}"
        );
    }
}

#[test]
fn the_shoutcast_status_line_is_accepted() {
    let audio = pattern(5000, 9);
    let server = radio(
        "ICY 200 OK",
        audio.clone(),
        1024,
        vec!["StreamTitle='On Air';"],
    );
    let mut stream = open(&server.url("/;stream.mp3"), &policy()).unwrap();
    assert_eq!(stream.opened().icy.as_ref().unwrap().metaint, Some(1024));
    assert_eq!(read_all(&mut stream).unwrap(), audio);
    assert_eq!(stream.stream_title().as_deref(), Some("On Air"));
}

#[test]
fn the_title_follows_the_stream() {
    let audio = pattern(350, 7);
    let server = radio(
        "ICY 200 OK",
        audio.clone(),
        100,
        vec!["StreamTitle='One';", "", "StreamTitle='Three';"],
    );
    let mut stream = open(&server.url("/radio"), &policy()).unwrap();
    assert_eq!(stream.stream_title(), None, "no block read yet");
    let mut buf = [0u8; 50];
    let mut got = Vec::new();
    let mut seen = Vec::new();
    loop {
        let n = stream.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        got.extend_from_slice(&buf[..n]);
        let title = stream.stream_title();
        if seen.last() != Some(&title) {
            seen.push(title);
        }
    }
    assert_eq!(got, audio);
    // The zero-length block between them changes nothing.
    assert_eq!(
        seen,
        [None, Some("One".to_string()), Some("Three".to_string())]
    );
}

#[test]
fn icy_headers_without_metaint_leave_the_body_alone() {
    let audio = pattern(4000, 8);
    let served = audio.clone();
    let server = Server::start(move |_, out| {
        let _ = out.write_all(b"ICY 200 OK\r\nicy-name: Plain Station\r\nicy-br: 64, 64\r\n\r\n");
        let _ = out.write_all(&served);
    });
    let mut stream = open(&server.url("/"), &policy()).unwrap();
    assert_eq!(
        stream.opened().icy,
        Some(IcyInfo {
            name: Some("Plain Station".into()),
            bitrate_kbps: Some(64),
            genre: None,
            metaint: None,
        })
    );
    assert_eq!(read_all(&mut stream).unwrap(), audio);
    assert_eq!(stream.stream_title(), None);
}

#[test]
fn a_metaint_that_is_not_a_number_is_malformed() {
    let server = Server::start(|_, out| {
        let _ = out.write_all(b"ICY 200 OK\r\nicy-metaint: lots\r\n\r\n");
    });
    assert!(matches!(
        open(&server.url("/"), &policy()),
        Err(chorus_fetch::FetchError::Malformed(_))
    ));
}
