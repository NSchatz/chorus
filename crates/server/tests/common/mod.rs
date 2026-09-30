//! What the server's end-to-end tests share: an endpoint that speaks
//! protocol v2 through the Linux client's own session code.
//!
//! Every audio connection to `chorus-server` is an encrypted v2 session, so a
//! test that wants audio off the real binary opens one exactly the way the
//! shipped client does (`chorus_client_linux::session::open`), and then reads
//! the v1 frames the session carries, unchanged.

#![allow(dead_code)]

use std::io::Read;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::session::{self, EndpointIdentity, Session};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// An endpoint id no other test endpoint in this process uses, so a fresh
/// ephemeral key is never presented under an id already pinned to another.
pub fn fresh_id(stem: &str) -> String {
    format!(
        "{}-{}-{}",
        stem,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Connect to `address` and open a v2 session as a fresh endpoint, with a
/// read timeout of `read_timeout` once the session is up.
pub fn v2_client<A: ToSocketAddrs>(address: A, read_timeout: Duration) -> Session {
    let stream = TcpStream::connect(address).expect("the server is listening");
    stream
        .set_read_timeout(Some(read_timeout))
        .expect("a read timeout");
    let mut me = EndpointIdentity::ephemeral(&fresh_id("test-endpoint")).expect("an identity");
    session::open(stream, &mut me, &ClientConfig::default())
        .unwrap_or_else(|e| panic!("the v2 session opens: {}", e))
}

/// Read the session's audio to its end and report how many plaintext bytes
/// (v1 frames) it carried.
pub fn drain(session: &mut Session) -> usize {
    let mut total = 0usize;
    let mut scratch = vec![0u8; 65_536];
    loop {
        match session.reader.read(&mut scratch) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => break,
        }
    }
    total
}
