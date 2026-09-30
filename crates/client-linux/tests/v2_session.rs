//! The client's side of protocol v2 against servers that are not v2 servers.

use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::session::{self, EndpointIdentity, SessionRefusal, HANDSHAKE_TIMEOUT};

#[test]
fn a_server_that_never_answers_the_handshake_is_refused_as_one_that_may_speak_protocol_v1() {
    // What a v1 server does with handshake_init: it reads the frame, finds no
    // type it knows, and goes on waiting. This listener does exactly that.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let silent = thread::spawn(move || {
        let (mut peer, _) = listener.accept().unwrap();
        let mut sink = Vec::new();
        let _ = peer.read_to_end(&mut sink);
        sink.len()
    });

    let stream = TcpStream::connect(address).unwrap();
    let mut me = EndpointIdentity::ephemeral("v1-probe").unwrap();
    let started = Instant::now();
    let refusal = match session::open(stream, &mut me, &ClientConfig::default()) {
        Err(e) => e,
        Ok(_) => panic!("a session opened with a server that never answered"),
    };
    let waited = started.elapsed();
    assert!(
        matches!(refusal, SessionRefusal::MaySpeakV1 { .. }),
        "{:?}",
        refusal
    );
    let text = refusal.to_string();
    assert!(
        text.contains("may speak chorus protocol v1"),
        "the refusal names v1: {}",
        text
    );
    assert_eq!(refusal.reason(), "server-may-speak-v1");
    assert!(
        waited >= HANDSHAKE_TIMEOUT - Duration::from_millis(100)
            && waited < HANDSHAKE_TIMEOUT + Duration::from_secs(2),
        "refused at the handshake timeout, not before and not long after: {:?}",
        waited
    );
    let sent = silent.join().unwrap();
    assert!(sent > 0, "the handshake_init went out");
    println!("refused after {:?}: {}", waited, text);
}
