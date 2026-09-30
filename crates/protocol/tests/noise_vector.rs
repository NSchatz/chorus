//! The Noise state machine against the published test vector.
//!
//! `fixtures/protocol/v2/noise/cacophony_xx.fields` is cacophony's vector for
//! `Noise_XX_25519_ChaChaPoly_SHA256`, unchanged but for layout. Both roles are
//! driven with its fixed keys, and every byte they produce must be the
//! published byte: the three handshake messages, the handshake hash, and the
//! three transport messages.

mod common;

use chorus_protocol::v2::noise::{Initiator, Keypair, Responder, PROTOCOL_NAME};

use common::{fixture_dir, Fields};

fn key(fields: &Fields, name: &str) -> Keypair {
    let bytes = fields.bytes(name);
    let mut k = [0u8; 32];
    k.copy_from_slice(&bytes);
    Keypair::from_secret(k)
}

#[test]
fn noise_xx_reproduces_the_published_cacophony_vector_in_both_roles() {
    let path = fixture_dir().join("v2/noise/cacophony_xx.fields");
    let text = std::fs::read_to_string(&path).expect("the committed vector is readable");
    let f = Fields::parse("cacophony_xx.fields", &text);
    assert_eq!(f.str("protocol_name").as_bytes(), PROTOCOL_NAME);
    let prologue = f.bytes("prologue");
    let msg = |i: usize, part: &str| f.bytes(&format!("message_{}_{}", i, part));

    let mut initiator =
        Initiator::new(&prologue, key(&f, "init_static"), key(&f, "init_ephemeral"));
    let mut responder =
        Responder::new(&prologue, key(&f, "resp_static"), key(&f, "resp_ephemeral"));

    let m0 = initiator.write_message1(&msg(0, "payload")).unwrap();
    assert_eq!(m0, msg(0, "ciphertext"), "message 0 (-> e)");
    assert_eq!(responder.read_message1(&m0).unwrap(), msg(0, "payload"));

    let m1 = responder.write_message2(&msg(1, "payload")).unwrap();
    assert_eq!(m1, msg(1, "ciphertext"), "message 1 (<- e, ee, s, es)");
    assert_eq!(initiator.read_message2(&m1).unwrap(), msg(1, "payload"));

    let (m2, mut ti) = initiator.write_message3(&msg(2, "payload")).unwrap();
    assert_eq!(m2, msg(2, "ciphertext"), "message 2 (-> s, se)");
    let (p2, mut tr) = responder.read_message3(&m2).unwrap();
    assert_eq!(p2, msg(2, "payload"));

    let hash = f.bytes("handshake_hash");
    assert_eq!(
        ti.handshake_hash.to_vec(),
        hash,
        "the initiator's handshake hash"
    );
    assert_eq!(
        tr.handshake_hash.to_vec(),
        hash,
        "the responder's handshake hash"
    );
    assert_eq!(tr.remote_static, key(&f, "init_static").public);
    assert_eq!(ti.remote_static, key(&f, "resp_static").public);

    // Transport: senders keep alternating, so 3 and 5 are the responder's.
    for i in 3..6 {
        let (sender, receiver) = if i % 2 == 1 {
            (&mut tr.send, &mut ti.receive)
        } else {
            (&mut ti.send, &mut tr.receive)
        };
        let ct = sender.encrypt_with_ad(&[], &msg(i, "payload")).unwrap();
        assert_eq!(ct, msg(i, "ciphertext"), "transport message {}", i);
        assert_eq!(
            receiver.decrypt_with_ad(&[], &ct).unwrap(),
            msg(i, "payload")
        );
    }
}
