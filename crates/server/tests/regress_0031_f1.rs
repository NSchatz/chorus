//! S0031-chorus-sync-4, refuter finding F1: `--serve-forever` means what it
//! says, on the binary the deployment actually runs.
//!
//! `--serve-forever` is a documented flag: `ServerConfig::once` is "Serve one
//! client and exit, rather than serving one client after another", and both
//! `deploy/run-server.sh` and `deploy/Dockerfile` pass it on the container's
//! command line. The first cut of the fanout server stopped reading
//! `config.once` at all, so the process served exactly one stream and exited
//! whatever the flag said. This file is the refuter's demonstration of that,
//! carried into the tree unchanged in its body so it goes on guarding the
//! behaviour rather than the moment it was found.
//!
//! It starts the real server binary with `--serve-forever` and a short finite
//! source, lets one client read the stream to its end, and then asks for a
//! second stream the way a second client would. A server that honours the flag
//! serves it; one that ignores the flag has already exited.

use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A port nothing is listening on, by binding one and letting it go.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

/// Connect, retrying until `deadline`, because the server binds after it has
/// reported its host contract.
fn connect_within(port: u16, deadline: Duration) -> Option<TcpStream> {
    let started = Instant::now();
    while started.elapsed() < deadline {
        if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
            return Some(stream);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

/// Read a whole stream to its end and report how many bytes it carried.
fn drain(mut stream: TcpStream) -> usize {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    let mut total = 0usize;
    let mut scratch = vec![0u8; 65_536];
    loop {
        match stream.read(&mut scratch) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => break,
        }
    }
    total
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn serve_forever_still_serves_a_second_client_after_the_first_stream_ends() {
    let port = free_port();
    let server = Server(
        Command::new(env!("CARGO_BIN_EXE_chorus-server"))
            .args([
                "--listen",
                &format!("127.0.0.1:{}", port),
                "--source",
                "tone",
                // A finite source, so the first stream ends cleanly, which is
                // the only case `--serve-forever` was ever about.
                "--tone-ms",
                "150",
                // This container grants no real-time priority and less locked
                // memory than the server wants; neither is what is under test.
                "--allow-non-realtime",
                "--allow-unlocked-memory",
                "--serve-forever",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server binary runs"),
    );

    let first = connect_within(port, Duration::from_secs(10))
        .expect("the first client connects to a server that has just started");
    let first_bytes = drain(first);
    assert!(
        first_bytes > 0,
        "the first client received nothing at all, so this test is not about --serve-forever"
    );

    // The stream ended. `--serve-forever` says the server serves one client
    // after another, so a second client gets a second stream.
    let second = connect_within(port, Duration::from_secs(10));
    let second = match second {
        Some(s) => s,
        None => panic!(
            "a second client could not connect after the first stream ended, so --serve-forever \
             served exactly one client and exited. ServerConfig::once documents the flag as \
             \"Serve one client and exit, rather than serving one client after another\" and \
             deploy/run-server.sh passes --serve-forever; crates/server/src/main.rs no longer \
             reads config.once anywhere"
        ),
    };
    let second_bytes = drain(second);
    assert!(
        second_bytes > 0,
        "the second client connected but was served no audio, so --serve-forever did not serve \
         one client after another"
    );

    drop(server);
}
