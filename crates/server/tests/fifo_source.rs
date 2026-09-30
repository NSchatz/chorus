//! Audit B-2: a named pipe is a development input, not a file.
//!
//! The real `chorus-server` binary is started on a real `mkfifo` pipe with no
//! writer attached, and a real protocol v2 endpoint session reads what it
//! serves. Then a player writes, closes, and a second player opens the same
//! pipe and writes again. What must hold, read off the chunks the endpoint
//! received and nothing else:
//!
//! - the supervisor was not blocked by a pipe with no writer: chunks arrive
//!   before anything has written, and they are silence;
//! - both players' audio arrives, in order, and between them the stream is
//!   silence at the chunk cadence rather than an end;
//! - no `stream_end` is ever sent and the sequence numbers are one unbroken
//!   run, so the endpoint never left the stream.

mod common;

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use chorus_client_linux::session::Session;
use chorus_protocol::{decode_frame, FrameOutcome, Message};

const FRAME_LEN: usize = 4; // stereo pcm_s16le
const CHUNK_BYTES: usize = 960 * FRAME_LEN; // 20 ms at 48 kHz
const PLAYER_CHUNKS: usize = 8;

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().unwrap().port()
}

fn a_real_fifo() -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("chorus-fifo-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let path = dir.join("pcm.fifo");
    let made = Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("mkfifo runs");
    assert!(made.success(), "mkfifo made {}", path.display());
    (dir, path)
}

/// A player's audio: never zero, so it cannot be mistaken for the silence
/// between players, and different per player.
fn player_pcm(seed: u32) -> Vec<u8> {
    let mut x = seed;
    (0..PLAYER_CHUNKS * CHUNK_BYTES)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((x >> 24) as u8) | 1
        })
        .collect()
}

/// Every chunk the endpoint received, and whether a `stream_end` came.
#[derive(Default)]
struct Heard {
    chunks: Vec<(u32, Vec<u8>)>,
    ended: bool,
    pending: Vec<u8>,
}

impl Heard {
    /// Read and decode until `enough` says stop or `limit` passes.
    fn listen(&mut self, session: &mut Session, limit: Duration, enough: impl Fn(&Heard) -> bool) {
        let deadline = Instant::now() + limit;
        let mut scratch = vec![0u8; 65_536];
        while Instant::now() < deadline && !enough(self) {
            let n = match session.reader.read(&mut scratch) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => panic!("the endpoint's session broke: {}", e),
            };
            self.pending.extend_from_slice(&scratch[..n]);
            let mut at = 0usize;
            loop {
                let result = decode_frame(&self.pending[at..]);
                if result.consumed == 0 {
                    break;
                }
                at += result.consumed;
                match result.outcome {
                    FrameOutcome::Decoded(Message::AudioChunk(c)) => {
                        self.chunks.push((c.sequence, c.audio_data))
                    }
                    FrameOutcome::Decoded(Message::StreamEnd(_)) => self.ended = true,
                    _ => {}
                }
            }
            self.pending.drain(..at);
        }
    }

    fn audio(&self) -> Vec<u8> {
        self.chunks
            .iter()
            .flat_map(|(_, d)| d.iter().copied())
            .collect()
    }

    fn silent_chunks_after(&self, from: usize) -> usize {
        self.chunks[from..]
            .iter()
            .filter(|(_, d)| d.iter().all(|b| *b == 0))
            .count()
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn play(path: &PathBuf, pcm: &[u8]) {
    let mut writer = OpenOptions::new()
        .write(true)
        .open(path)
        .expect("a player opens the pipe for writing");
    writer.write_all(pcm).expect("the player writes");
    // Dropped here: the player closes its end, as one that pauses does.
}

#[test]
fn a_fifo_whose_player_closes_and_returns_keeps_every_endpoint_on_one_stream() {
    let (dir, path) = a_real_fifo();
    let port = free_port();
    let child = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
        .args([
            "--listen",
            &format!("127.0.0.1:{}", port),
            "--allow-non-realtime",
            "--allow-unlocked-memory",
            "--ephemeral-identity",
            "--source",
            &format!("fifo:{}", path.display()),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the server binary runs");
    let mut server = Server(child);
    let stdout = server.0.stdout.take().expect("the server's stdout");
    let mut lines = BufReader::new(stdout).lines();
    loop {
        match lines.next() {
            Some(Ok(line)) if line.contains("listening on=") => break,
            Some(Ok(_)) => {}
            _ => panic!("the server stopped before it listened"),
        }
    }
    // Keep reading, so the server never blocks on a full stdout pipe.
    std::thread::spawn(move || for _ in lines.map_while(Result::ok) {});

    let mut session = common::v2_client(("127.0.0.1", port), Duration::from_secs(5));
    let mut heard = Heard::default();

    // Nobody has opened the pipe for writing: the stream runs anyway, on
    // silence, so neither the supervisor nor the emitter waited on a player.
    heard.listen(&mut session, Duration::from_secs(10), |h| {
        h.chunks.len() >= 10
    });
    assert!(
        heard.chunks.len() >= 10,
        "chunks flow with no writer attached"
    );
    assert_eq!(
        heard.silent_chunks_after(0),
        heard.chunks.len(),
        "and they are silence"
    );

    // The first player plays and closes.
    let first = player_pcm(0x2545_f491);
    play(&path, &first);
    heard.listen(&mut session, Duration::from_secs(10), |h| {
        find(&h.audio(), &first).is_some()
    });
    let first_at = find(&heard.audio(), &first).expect("the first player's audio arrived whole");

    // With nobody writing, the stream carries on as silence at the cadence.
    let mark = heard.chunks.len();
    let quiet_started = Instant::now();
    heard.listen(&mut session, Duration::from_secs(10), |h| {
        h.silent_chunks_after(mark) >= 15
    });
    let quiet = quiet_started.elapsed();
    assert!(
        heard.silent_chunks_after(mark) >= 15,
        "silence after the writer closed"
    );
    assert!(!heard.ended, "a closed writer did not end the stream");
    // 15 chunks of 20 ms are 300 ms of stream; paced, they cannot all arrive
    // in much less than that (the bound leaves room for queued chunks).
    assert!(
        quiet >= Duration::from_millis(150),
        "silence came at the chunk cadence, not as fast as it could be made: {:?}",
        quiet
    );

    // A second player opens the same pipe and plays.
    let second = player_pcm(0x0bad_cafe);
    play(&path, &second);
    heard.listen(&mut session, Duration::from_secs(10), |h| {
        find(&h.audio(), &second).is_some()
    });
    let second_at = find(&heard.audio(), &second).expect("the second player's audio arrived whole");
    assert!(
        second_at > first_at + first.len(),
        "in order, with silence between"
    );

    assert!(!heard.ended, "no stream_end was ever sent");
    for pair in heard.chunks.windows(2) {
        assert_eq!(
            pair[1].0,
            pair[0].0.wrapping_add(1),
            "one unbroken run of sequence numbers: the endpoint never left the stream"
        );
    }
    assert!(
        heard.chunks.iter().all(|(_, d)| d.len() == CHUNK_BYTES),
        "every chunk whole"
    );
    let _ = std::fs::remove_dir_all(dir);
}
