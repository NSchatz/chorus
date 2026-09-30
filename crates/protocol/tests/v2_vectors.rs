//! The protocol v2 golden vectors, both directions, and the session on the wire.
//!
//! Every v2 message type has at least one committed vector under
//! `fixtures/protocol/v2/`; the encoder must produce its bytes and the decoder
//! must recover its fields. The session vectors are then reproduced by the
//! real handshake: a server and an endpoint driven with the vectors' test keys
//! put exactly the committed frames on the wire, a v1 peer is refused with
//! exactly the committed refusal, and a changed endpoint key is refused with
//! exactly the committed refusal and surfaced to the server's caller.

mod common;

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::thread;

use chorus_protocol::v2::adoption::{PinStore, Verdict};
use chorus_protocol::v2::noise::fingerprint;
use chorus_protocol::v2::session::{
    accept, connect, Identity, SecureReader, SecureWriter, SessionError,
};
use chorus_protocol::v2::{decode_frame, encode, Message, Outcome, Type};
use chorus_protocol::MessageType;

use common::v2::{keypair, load_v2, v2_stems};
use common::{fixture_dir, load_vector};

#[test]
fn every_v2_message_type_has_a_committed_vector() {
    let mut by_type: BTreeMap<Type, Vec<String>> = BTreeMap::new();
    for stem in v2_stems() {
        let v = load_v2(&stem);
        by_type
            .entry(v.message.message_type())
            .or_default()
            .push(stem);
    }
    for t in Type::ALL {
        if t.is_v1() {
            // Carried into v2 unchanged: their vectors are the v1 ones.
            let v1 = MessageType::from_name(t.name()).expect("a v1 type has a v1 name");
            assert!(!load_vector(v1).frame.is_empty());
            continue;
        }
        let stems = by_type
            .get(&t)
            .unwrap_or_else(|| panic!("{} has no vector in fixtures/protocol/v2", t.name()));
        println!("{:<20} {}", t.name(), stems.join(" "));
    }
    let v2_count: usize = by_type.values().map(Vec::len).sum();
    println!(
        "v2 vectors: {} in fixtures/protocol/v2 for {} new types, plus 3 v1 vectors for the 3 carried types",
        v2_count,
        Type::ALL.iter().filter(|t| !t.is_v1()).count()
    );
}

#[test]
fn the_encoder_reproduces_every_v2_vector_byte_for_byte() {
    for stem in v2_stems() {
        let v = load_v2(&stem);
        let encoded = encode(&v.message).unwrap_or_else(|e| panic!("{}: must encode: {}", stem, e));
        assert_eq!(
            encoded, v.frame,
            "{}: encoder output differs from the committed vector",
            stem
        );
    }
}

#[test]
fn the_decoder_recovers_every_v2_vector_field_for_field() {
    for stem in v2_stems() {
        let v = load_v2(&stem);
        let d = decode_frame(&v.frame);
        assert_eq!(
            d.consumed,
            v.frame.len(),
            "{}: the vector is exactly one frame",
            stem
        );
        assert_eq!(d.outcome, Outcome::Decoded(v.message.clone()), "{}", stem);
    }
}

#[test]
fn the_three_v1_vectors_are_v2_messages_unchanged() {
    for t in MessageType::ALL {
        let v = load_vector(t);
        match decode_frame(&v.frame).outcome {
            Outcome::Decoded(m) => {
                assert_eq!(m.message_type().name(), t.name());
                assert_eq!(
                    encode(&m).unwrap(),
                    v.frame,
                    "{} re-encodes to its v1 bytes",
                    t.name()
                );
            }
            other => panic!("{}: {:?}", t.name(), other),
        }
    }
}

/// A stream that records everything written through it.
struct Tap {
    inner: UnixStream,
    written: Arc<Mutex<Vec<u8>>>,
}

impl Read for Tap {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for Tap {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.written.lock().unwrap().extend_from_slice(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn tapped(s: UnixStream) -> (Tap, Arc<Mutex<Vec<u8>>>) {
    let written = Arc::new(Mutex::new(Vec::new()));
    (
        Tap {
            inner: s,
            written: Arc::clone(&written),
        },
        written,
    )
}

fn frames(stems: &[&str]) -> Vec<u8> {
    stems.iter().flat_map(|s| load_v2(s).frame).collect()
}

#[test]
fn the_handshake_and_the_first_record_on_the_wire_are_the_committed_vectors() {
    let keys = load_v2("handshake_init").fields;
    let server_identity = Identity {
        id: keys.str("server_id").to_string(),
        keypair: keypair(&keys, "server_static"),
    };
    let endpoint_identity = Identity {
        id: keys.str("endpoint_id").to_string(),
        keypair: keypair(&keys, "endpoint_static"),
    };
    let (a, b) = UnixStream::pair().unwrap();
    let (mut server_end, server_wrote) = tapped(a);
    let (mut endpoint_end, endpoint_wrote) = tapped(b);

    let server_ephemeral = keypair(&keys, "server_ephemeral");
    let server = thread::spawn(move || {
        let mut pins = PinStore::new();
        let est = accept(
            &mut server_end,
            &server_identity,
            server_ephemeral,
            |id, k| pins.check(id, k),
        )
        .unwrap();
        assert_eq!(
            est.verdict,
            Verdict::Adopted,
            "a new endpoint id is adopted on first use"
        );
        let mut reader = SecureReader::new(server_end, est.opener);
        let first = reader.next_message().unwrap();
        let second = reader.next_message().unwrap();
        (est.peer_id, est.handshake_hash, first, second)
    });
    let est = connect(
        &mut endpoint_end,
        &endpoint_identity,
        keypair(&keys, "endpoint_ephemeral"),
        |_, _| Verdict::Adopted,
    )
    .unwrap();
    assert_eq!(est.peer_id, "chorus-server-test");
    let mut writer = SecureWriter::new(endpoint_end, est.sealer);
    let record = load_v2("secure_record");
    writer.write_all(&record.fields.bytes("plaintext")).unwrap();
    writer.flush().unwrap();
    let (peer_id, server_hash, first, second) = server.join().unwrap();

    assert_eq!(peer_id, "chorus-endpoint-test");
    assert_eq!(server_hash, est.handshake_hash);
    assert_eq!(first, load_v2("hello_endpoint").message);
    assert_eq!(second, load_v2("capabilities").message);
    assert_eq!(
        *endpoint_wrote.lock().unwrap(),
        frames(&["handshake_init", "handshake_finish", "secure_record"]),
        "the endpoint wrote handshake_init, handshake_finish and then secure_record, byte for byte"
    );
    assert_eq!(
        *server_wrote.lock().unwrap(),
        frames(&["handshake_response"]),
        "the server wrote handshake_response, byte for byte"
    );
}

#[test]
fn a_v1_peer_is_refused_by_name_with_the_committed_refusal() {
    let keys = load_v2("handshake_init").fields;
    let me = Identity {
        id: keys.str("server_id").to_string(),
        keypair: keypair(&keys, "server_static"),
    };
    let (a, mut peer) = UnixStream::pair().unwrap();
    let (mut server_end, server_wrote) = tapped(a);
    // A v1 client opens with a time_sync request.
    peer.write_all(&load_vector(MessageType::TimeSync).frame)
        .unwrap();
    let result = accept(
        &mut server_end,
        &me,
        keypair(&keys, "server_ephemeral"),
        |_, _| Verdict::Known,
    );
    match result {
        Err(SessionError::PeerSpeaksV1 { first }) => assert_eq!(first, "time_sync"),
        other => panic!(
            "a v1 peer must be refused by name, got {:?}",
            other.map(|e| e.peer_id)
        ),
    }
    assert_eq!(
        *server_wrote.lock().unwrap(),
        load_v2("session_refused_v1_peer").frame
    );
}

#[test]
fn an_endpoint_whose_key_changed_is_refused_and_surfaced() {
    let keys = load_v2("handshake_init").fields;
    let refusal = load_v2("session_refused_key_changed");
    let server_identity = Identity {
        id: keys.str("server_id").to_string(),
        keypair: keypair(&keys, "server_static"),
    };
    let endpoint_id = keys.str("endpoint_id").to_string();
    let adopted_key = keypair(&keys, "endpoint_static");
    // The endpoint was adopted with its test key; it now presents another.
    let impostor = Identity {
        id: endpoint_id.clone(),
        keypair: keypair(&refusal.fields, "offered_static"),
    };
    let mut pins = PinStore::new();
    assert_eq!(
        pins.check(&endpoint_id, &adopted_key.public),
        Verdict::Adopted
    );
    let pins = Arc::new(Mutex::new(pins));

    let (a, b) = UnixStream::pair().unwrap();
    let (mut server_end, server_wrote) = tapped(a);
    let mut endpoint_end = b;
    let server_pins = Arc::clone(&pins);
    let server_ephemeral = keypair(&keys, "server_ephemeral");
    let server = thread::spawn(move || {
        accept(
            &mut server_end,
            &server_identity,
            server_ephemeral,
            |id, k| server_pins.lock().unwrap().check(id, k),
        )
    });
    let est = connect(
        &mut endpoint_end,
        &impostor,
        keypair(&keys, "endpoint_ephemeral"),
        |_, _| Verdict::Known,
    )
    .expect("the endpoint cannot tell yet: the server decides after message 3");

    // Surfaced to the server's caller, by name, with both fingerprints.
    let change = match server.join().unwrap() {
        Err(SessionError::KeyChanged(change)) => change,
        other => panic!(
            "a changed key must be refused, got {:?}",
            other.map(|e| e.peer_id)
        ),
    };
    assert_eq!(change.id, endpoint_id);
    assert_eq!(change.pinned, fingerprint(&adopted_key.public));
    assert_eq!(change.offered, fingerprint(&impostor.keypair.public));
    // Recorded in the store, and the pin did not move.
    let pins = pins.lock().unwrap();
    assert_eq!(pins.key_changes(), std::slice::from_ref(&change));
    assert_eq!(pins.pinned(&endpoint_id), Some(adopted_key.public));
    // On the wire: the committed refusal right after handshake_response.
    let wrote = server_wrote.lock().unwrap().clone();
    let response_len = 3 + u16::from_be_bytes([wrote[1], wrote[2]]) as usize;
    assert_eq!(
        wrote[response_len..],
        refusal.frame[..],
        "the server sent exactly the committed key_changed refusal"
    );
    // And the endpoint surfaces the refusal when it reads.
    let mut reader = SecureReader::new(endpoint_end, est.opener);
    let err = reader.next_message().expect_err("the session was refused");
    assert_eq!(err.kind(), io::ErrorKind::ConnectionRefused);
    assert!(err.to_string().contains("key_changed"), "{}", err);
    println!("surfaced: {}", change);
}

#[test]
fn a_secure_record_carries_only_whole_frames_and_unknown_types_inside_are_skipped() {
    let keys = load_v2("handshake_init").fields;
    let server_identity = Identity {
        id: "s".to_string(),
        keypair: keypair(&keys, "server_static"),
    };
    let endpoint_identity = Identity {
        id: "e".to_string(),
        keypair: keypair(&keys, "endpoint_static"),
    };
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let se = keypair(&keys, "server_ephemeral");
    let server = thread::spawn(move || {
        let est = accept(&mut a, &server_identity, se, |_, _| Verdict::Known).unwrap();
        let mut reader = SecureReader::new(a, est.opener);
        let mut plain = Vec::new();
        let first = reader.next_message().unwrap();
        reader.read_to_end(&mut plain).unwrap();
        (first, plain, reader.skipped())
    });
    let est = connect(
        &mut b,
        &endpoint_identity,
        keypair(&keys, "endpoint_ephemeral"),
        |_, _| Verdict::Known,
    )
    .unwrap();
    let mut w = SecureWriter::new(b, est.sealer);
    let time_sync = load_vector(MessageType::TimeSync).frame;
    // A v1 frame split across two writes, an unassigned type, then a v2 message.
    w.write_all(&time_sync[..5]).unwrap();
    assert_eq!(w.records(), 0, "half a frame is never sealed");
    w.write_all(&time_sync[5..]).unwrap();
    w.write_all(&[0x7E, 0x00, 0x01, 0xAA]).unwrap();
    w.write_all(&load_v2("telemetry").frame).unwrap();
    drop(w);
    let (first, plain, skipped) = server.join().unwrap();
    assert_eq!(first, load_v2("telemetry").message);
    assert_eq!(
        plain, time_sync,
        "the v1 frame comes out of the reader byte for byte"
    );
    assert_eq!(
        skipped, 1,
        "the unassigned type inside a record was stepped over"
    );
}

#[test]
fn the_v2_fixtures_are_not_rewritten_by_the_suite() {
    let snapshot = || {
        let mut out = Vec::new();
        for dir in [fixture_dir().join("v2"), fixture_dir().join("v2/noise")] {
            for e in std::fs::read_dir(dir).unwrap() {
                let e = e.unwrap();
                let m = e.metadata().unwrap();
                if m.is_file() {
                    out.push((e.path(), m.len(), m.modified().unwrap()));
                }
            }
        }
        out.sort();
        out
    };
    let before = snapshot();
    for stem in v2_stems() {
        let v = load_v2(&stem);
        assert_eq!(encode(&v.message).unwrap(), v.frame);
    }
    assert_eq!(before, snapshot());
    let _ = Message::OutputDelay(chorus_protocol::v2::OutputDelay { delay_ns: 0 });
}
