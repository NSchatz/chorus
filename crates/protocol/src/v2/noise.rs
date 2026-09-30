//! `Noise_XX_25519_ChaChaPoly_SHA256`, the v2 key exchange.
//!
//! Written from the Noise Protocol Framework specification, revision 34
//! (<https://noiseprotocol.org/noise.html>, read 2026-09-30): section 5 (the
//! CipherState, SymmetricState and HandshakeState objects), section 4.3 (HKDF),
//! section 7.5 (the XX pattern) and section 12 (the 25519, ChaChaPoly and
//! SHA256 functions). The primitives are vendored crates (BRIEF.md 3.2: crypto
//! primitives are platform); only the state machine is chorus's, and it is
//! held to the published cacophony test vector for this protocol name
//! (`crates/protocol/tests/noise_vector.rs`). Why XX and not TLS-PSK:
//! `docs/decisions/0039-the-v2-key-exchange.md`.
//!
//! ```text
//! XX:
//!   -> e
//!   <- e, ee, s, es
//!   -> s, se
//! ```
//!
//! This module holds no socket and no clock, and takes every key as an
//! argument: the caller supplies randomness, which is what lets a golden
//! vector fix the ephemeral keys.

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

/// The protocol name, which is also the initial handshake hash (it is exactly
/// 32 bytes, so section 5.2's `InitializeSymmetric` uses it unhashed).
pub const PROTOCOL_NAME: &[u8; 32] = b"Noise_XX_25519_ChaChaPoly_SHA256";

/// Bytes in a Curve25519 key.
pub const KEY_LEN: usize = 32;

/// Bytes in a ChaChaPoly authentication tag.
pub const TAG_LEN: usize = 16;

/// The largest Noise message (section 3).
pub const MAX_MESSAGE_LEN: usize = 65_535;

/// Why a handshake or transport step failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoiseError {
    /// A message is not the length this step needs.
    BadLength {
        /// Which message or field.
        what: &'static str,
        /// Bytes that arrived.
        len: usize,
    },
    /// Authenticated decryption failed: wrong key, altered bytes, or a replay.
    DecryptFailed,
    /// A Diffie-Hellman result was all zero (a low-order public key).
    WeakKey,
    /// The step was called out of order.
    OutOfOrder,
    /// A message would exceed 65535 bytes.
    TooLong,
    /// The nonce counter is exhausted; the session must end.
    NonceExhausted,
}

impl fmt::Display for NoiseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NoiseError::BadLength { what, len } => {
                write!(f, "{} has the wrong length ({} bytes)", what, len)
            }
            NoiseError::DecryptFailed => write!(f, "authenticated decryption failed"),
            NoiseError::WeakKey => write!(f, "a Diffie-Hellman result was all zero"),
            NoiseError::OutOfOrder => write!(f, "a handshake step was called out of order"),
            NoiseError::TooLong => write!(f, "a Noise message may not exceed 65535 bytes"),
            NoiseError::NonceExhausted => write!(f, "the nonce counter is exhausted"),
        }
    }
}

impl std::error::Error for NoiseError {}

/// A Curve25519 key pair.
#[derive(Clone)]
pub struct Keypair {
    secret: StaticSecret,
    /// The public half.
    pub public: [u8; KEY_LEN],
}

impl fmt::Debug for Keypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The secret half is never printed.
        f.debug_struct("Keypair")
            .field("public", &hex(&self.public))
            .finish_non_exhaustive()
    }
}

impl Keypair {
    /// The key pair whose secret is these 32 bytes (clamped by X25519 itself).
    pub fn from_secret(secret: [u8; KEY_LEN]) -> Keypair {
        let secret = StaticSecret::from(secret);
        let public = PublicKey::from(&secret).to_bytes();
        Keypair { secret, public }
    }

    /// The secret half, for storing it. Handle with care.
    pub fn secret_bytes(&self) -> [u8; KEY_LEN] {
        self.secret.to_bytes()
    }

    fn dh(&self, remote: &[u8; KEY_LEN]) -> Result<[u8; KEY_LEN], NoiseError> {
        let shared = self.secret.diffie_hellman(&PublicKey::from(*remote));
        if !shared.was_contributory() {
            return Err(NoiseError::WeakKey);
        }
        Ok(shared.to_bytes())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hmac_sha256(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as hmac::KeyInit>::new_from_slice(key)
        .expect("HMAC takes a key of any length");
    for part in parts {
        mac.update(part);
    }
    mac.finalize().into_bytes().into()
}

/// Section 4.3: HKDF with two outputs.
fn hkdf2(chaining_key: &[u8; 32], ikm: &[u8]) -> ([u8; 32], [u8; 32]) {
    let temp = hmac_sha256(chaining_key, &[ikm]);
    let out1 = hmac_sha256(&temp, &[&[0x01]]);
    let out2 = hmac_sha256(&temp, &[&out1, &[0x02]]);
    (out1, out2)
}

/// Section 5.1: a key and a nonce.
#[derive(Clone)]
pub struct CipherState {
    key: Option<[u8; 32]>,
    nonce: u64,
}

impl fmt::Debug for CipherState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CipherState")
            .field("has_key", &self.key.is_some())
            .field("nonce", &self.nonce)
            .finish()
    }
}

impl CipherState {
    fn new(key: Option<[u8; 32]>) -> CipherState {
        CipherState { key, nonce: 0 }
    }

    /// The nonce the next message will use.
    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    /// Section 12.3: 32 bits of zeros, then the 64-bit nonce little-endian.
    fn nonce_bytes(n: u64) -> [u8; 12] {
        let mut out = [0u8; 12];
        out[4..].copy_from_slice(&n.to_le_bytes());
        out
    }

    /// `EncryptWithAd`. Without a key, the plaintext is returned unchanged.
    pub fn encrypt_with_ad(&mut self, ad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        let Some(key) = self.key else {
            return Ok(plaintext.to_vec());
        };
        // Section 5.1: 2^64 - 1 is reserved; a counter that reached it is spent.
        if self.nonce == u64::MAX {
            return Err(NoiseError::NonceExhausted);
        }
        if plaintext.len() + TAG_LEN > MAX_MESSAGE_LEN {
            return Err(NoiseError::TooLong);
        }
        let cipher = ChaCha20Poly1305::new(&Key::from(key));
        let nonce = Self::nonce_bytes(self.nonce);
        let out = cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad: ad,
                },
            )
            .map_err(|_| NoiseError::TooLong)?;
        self.nonce += 1;
        Ok(out)
    }

    /// `DecryptWithAd`. The nonce advances only on success.
    pub fn decrypt_with_ad(&mut self, ad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        let Some(key) = self.key else {
            return Ok(ciphertext.to_vec());
        };
        if self.nonce == u64::MAX {
            return Err(NoiseError::NonceExhausted);
        }
        if ciphertext.len() < TAG_LEN {
            return Err(NoiseError::BadLength {
                what: "ciphertext",
                len: ciphertext.len(),
            });
        }
        let cipher = ChaCha20Poly1305::new(&Key::from(key));
        let nonce = Self::nonce_bytes(self.nonce);
        let out = cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: ciphertext,
                    aad: ad,
                },
            )
            .map_err(|_| NoiseError::DecryptFailed)?;
        self.nonce += 1;
        Ok(out)
    }
}

/// Section 5.2.
#[derive(Clone)]
struct SymmetricState {
    cipher: CipherState,
    chaining_key: [u8; 32],
    hash: [u8; 32],
}

impl SymmetricState {
    fn new() -> SymmetricState {
        SymmetricState {
            cipher: CipherState::new(None),
            chaining_key: *PROTOCOL_NAME,
            hash: *PROTOCOL_NAME,
        }
    }

    fn mix_hash(&mut self, data: &[u8]) {
        let mut h = Sha256::new();
        h.update(self.hash);
        h.update(data);
        self.hash = h.finalize().into();
    }

    fn mix_key(&mut self, ikm: &[u8]) {
        let (ck, k) = hkdf2(&self.chaining_key, ikm);
        self.chaining_key = ck;
        self.cipher = CipherState::new(Some(k));
    }

    fn encrypt_and_hash(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        let ct = self.cipher.encrypt_with_ad(&self.hash, plaintext)?;
        self.mix_hash(&ct);
        Ok(ct)
    }

    fn decrypt_and_hash(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        let pt = self.cipher.decrypt_with_ad(&self.hash, ciphertext)?;
        self.mix_hash(ciphertext);
        Ok(pt)
    }

    fn split(&self) -> (CipherState, CipherState) {
        let (k1, k2) = hkdf2(&self.chaining_key, &[]);
        (CipherState::new(Some(k1)), CipherState::new(Some(k2)))
    }

    /// Bytes the payload cipher adds: the tag, once a key exists.
    fn overhead(&self) -> usize {
        if self.cipher.key.is_some() {
            TAG_LEN
        } else {
            0
        }
    }
}

/// The finished handshake: two one-way ciphers and the handshake hash.
#[derive(Debug)]
pub struct Transport {
    /// Encrypts what this side sends.
    pub send: CipherState,
    /// Decrypts what this side receives.
    pub receive: CipherState,
    /// The handshake hash, `h` after the last message (section 11.2's
    /// channel binding value).
    pub handshake_hash: [u8; 32],
    /// The other side's long-term public key, authenticated by the handshake.
    pub remote_static: [u8; KEY_LEN],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Message1,
    Message2,
    Message3,
    Done,
}

/// The initiator (the endpoint) of an XX handshake.
pub struct Initiator {
    symmetric: SymmetricState,
    s: Keypair,
    e: Keypair,
    re: Option<[u8; KEY_LEN]>,
    rs: Option<[u8; KEY_LEN]>,
    step: Step,
}

impl Initiator {
    /// A new handshake with this prologue, long-term key and ephemeral key.
    pub fn new(prologue: &[u8], s: Keypair, e: Keypair) -> Initiator {
        let mut symmetric = SymmetricState::new();
        symmetric.mix_hash(prologue);
        Initiator {
            symmetric,
            s,
            e,
            re: None,
            rs: None,
            step: Step::Message1,
        }
    }

    /// `-> e`, then the payload (in the clear: no key exists yet).
    pub fn write_message1(&mut self, payload: &[u8]) -> Result<Vec<u8>, NoiseError> {
        if self.step != Step::Message1 {
            return Err(NoiseError::OutOfOrder);
        }
        let mut out = self.e.public.to_vec();
        self.symmetric.mix_hash(&self.e.public);
        out.extend(self.symmetric.encrypt_and_hash(payload)?);
        self.step = Step::Message2;
        Ok(out)
    }

    /// `<- e, ee, s, es`; returns the responder's payload.
    pub fn read_message2(&mut self, message: &[u8]) -> Result<Vec<u8>, NoiseError> {
        if self.step != Step::Message2 {
            return Err(NoiseError::OutOfOrder);
        }
        if message.len() < KEY_LEN + KEY_LEN + TAG_LEN + TAG_LEN {
            return Err(NoiseError::BadLength {
                what: "message 2",
                len: message.len(),
            });
        }
        let re = key_at(message, 0);
        self.symmetric.mix_hash(&re);
        self.symmetric.mix_key(&self.e.dh(&re)?);
        let rs_bytes = self
            .symmetric
            .decrypt_and_hash(&message[KEY_LEN..KEY_LEN * 2 + TAG_LEN])?;
        let rs = key_at(&rs_bytes, 0);
        self.symmetric.mix_key(&self.e.dh(&rs)?);
        let payload = self
            .symmetric
            .decrypt_and_hash(&message[KEY_LEN * 2 + TAG_LEN..])?;
        self.re = Some(re);
        self.rs = Some(rs);
        self.step = Step::Message3;
        Ok(payload)
    }

    /// The responder's long-term key, known after message 2.
    pub fn remote_static(&self) -> Option<[u8; KEY_LEN]> {
        self.rs
    }

    /// `-> s, se`, then the payload; returns message 3 and the transport.
    pub fn write_message3(mut self, payload: &[u8]) -> Result<(Vec<u8>, Transport), NoiseError> {
        if self.step != Step::Message3 {
            return Err(NoiseError::OutOfOrder);
        }
        let re = self.re.ok_or(NoiseError::OutOfOrder)?;
        let mut out = self.symmetric.encrypt_and_hash(&self.s.public)?;
        self.symmetric.mix_key(&self.s.dh(&re)?);
        out.extend(self.symmetric.encrypt_and_hash(payload)?);
        if out.len() > MAX_MESSAGE_LEN {
            return Err(NoiseError::TooLong);
        }
        self.step = Step::Done;
        let (c1, c2) = self.symmetric.split();
        Ok((
            out,
            Transport {
                send: c1,
                receive: c2,
                handshake_hash: self.symmetric.hash,
                remote_static: self.rs.ok_or(NoiseError::OutOfOrder)?,
            },
        ))
    }
}

/// The responder (the server) of an XX handshake.
pub struct Responder {
    symmetric: SymmetricState,
    s: Keypair,
    e: Keypair,
    re: Option<[u8; KEY_LEN]>,
    step: Step,
}

impl Responder {
    /// A new handshake with this prologue, long-term key and ephemeral key.
    pub fn new(prologue: &[u8], s: Keypair, e: Keypair) -> Responder {
        let mut symmetric = SymmetricState::new();
        symmetric.mix_hash(prologue);
        Responder {
            symmetric,
            s,
            e,
            re: None,
            step: Step::Message1,
        }
    }

    /// `-> e`; returns the initiator's payload.
    pub fn read_message1(&mut self, message: &[u8]) -> Result<Vec<u8>, NoiseError> {
        if self.step != Step::Message1 {
            return Err(NoiseError::OutOfOrder);
        }
        if message.len() < KEY_LEN {
            return Err(NoiseError::BadLength {
                what: "message 1",
                len: message.len(),
            });
        }
        let re = key_at(message, 0);
        self.symmetric.mix_hash(&re);
        let payload = self.symmetric.decrypt_and_hash(&message[KEY_LEN..])?;
        self.re = Some(re);
        self.step = Step::Message2;
        Ok(payload)
    }

    /// `<- e, ee, s, es`, then the payload.
    pub fn write_message2(&mut self, payload: &[u8]) -> Result<Vec<u8>, NoiseError> {
        if self.step != Step::Message2 {
            return Err(NoiseError::OutOfOrder);
        }
        let re = self.re.ok_or(NoiseError::OutOfOrder)?;
        let mut out = self.e.public.to_vec();
        self.symmetric.mix_hash(&self.e.public);
        self.symmetric.mix_key(&self.e.dh(&re)?);
        out.extend(self.symmetric.encrypt_and_hash(&self.s.public)?);
        self.symmetric.mix_key(&self.s.dh(&re)?);
        out.extend(self.symmetric.encrypt_and_hash(payload)?);
        if out.len() > MAX_MESSAGE_LEN {
            return Err(NoiseError::TooLong);
        }
        self.step = Step::Message3;
        Ok(out)
    }

    /// `-> s, se`; returns the initiator's payload and the transport. The
    /// initiator's long-term key is `Transport::remote_static`, and it is the
    /// caller's to check against its pin before sending anything.
    pub fn read_message3(mut self, message: &[u8]) -> Result<(Vec<u8>, Transport), NoiseError> {
        if self.step != Step::Message3 {
            return Err(NoiseError::OutOfOrder);
        }
        if message.len() < KEY_LEN + TAG_LEN + self.symmetric.overhead() {
            return Err(NoiseError::BadLength {
                what: "message 3",
                len: message.len(),
            });
        }
        let rs_bytes = self
            .symmetric
            .decrypt_and_hash(&message[..KEY_LEN + TAG_LEN])?;
        let rs = key_at(&rs_bytes, 0);
        self.symmetric.mix_key(&self.e.dh(&rs)?);
        let payload = self
            .symmetric
            .decrypt_and_hash(&message[KEY_LEN + TAG_LEN..])?;
        self.step = Step::Done;
        let (c1, c2) = self.symmetric.split();
        Ok((
            payload,
            Transport {
                send: c2,
                receive: c1,
                handshake_hash: self.symmetric.hash,
                remote_static: rs,
            },
        ))
    }
}

fn key_at(bytes: &[u8], at: usize) -> [u8; KEY_LEN] {
    let mut k = [0u8; KEY_LEN];
    k.copy_from_slice(&bytes[at..at + KEY_LEN]);
    k
}

/// A short, stable fingerprint of a public key for people to compare: the
/// first 8 bytes of its SHA-256, as four colon-separated groups.
pub fn fingerprint(public: &[u8; KEY_LEN]) -> String {
    let digest = Sha256::digest(public);
    digest[..8]
        .chunks(2)
        .map(|c| format!("{:02x}{:02x}", c[0], c[1]))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kp(b: u8) -> Keypair {
        Keypair::from_secret([b; 32])
    }

    #[test]
    fn a_handshake_agrees_on_keys_and_hash_both_ways() {
        let mut i = Initiator::new(b"p", kp(1), kp(2));
        let mut r = Responder::new(b"p", kp(3), kp(4));
        let m1 = i.write_message1(b"").unwrap();
        assert_eq!(r.read_message1(&m1).unwrap(), b"");
        let m2 = r.write_message2(b"server").unwrap();
        assert_eq!(i.read_message2(&m2).unwrap(), b"server");
        let (m3, mut ti) = i.write_message3(b"endpoint").unwrap();
        let (p3, mut tr) = r.read_message3(&m3).unwrap();
        assert_eq!(p3, b"endpoint");
        assert_eq!(ti.handshake_hash, tr.handshake_hash);
        assert_eq!(tr.remote_static, kp(1).public);
        assert_eq!(ti.remote_static, kp(3).public);
        let ct = ti.send.encrypt_with_ad(&[], b"up").unwrap();
        assert_eq!(tr.receive.decrypt_with_ad(&[], &ct).unwrap(), b"up");
        let ct = tr.send.encrypt_with_ad(&[], b"down").unwrap();
        assert_eq!(ti.receive.decrypt_with_ad(&[], &ct).unwrap(), b"down");
    }

    #[test]
    fn a_different_prologue_fails_the_handshake() {
        let mut i = Initiator::new(b"chorus", kp(1), kp(2));
        let mut r = Responder::new(b"other", kp(3), kp(4));
        let m1 = i.write_message1(b"").unwrap();
        r.read_message1(&m1).unwrap();
        let m2 = r.write_message2(b"").unwrap();
        assert_eq!(i.read_message2(&m2), Err(NoiseError::DecryptFailed));
    }

    #[test]
    fn a_replayed_or_altered_record_does_not_decrypt() {
        let mut i = Initiator::new(b"", kp(1), kp(2));
        let mut r = Responder::new(b"", kp(3), kp(4));
        r.read_message1(&i.write_message1(b"").unwrap()).unwrap();
        i.read_message2(&r.write_message2(b"").unwrap()).unwrap();
        let (m3, mut ti) = i.write_message3(b"").unwrap();
        let (_, mut tr) = r.read_message3(&m3).unwrap();
        let ct = ti.send.encrypt_with_ad(&[], b"one").unwrap();
        assert!(tr.receive.decrypt_with_ad(&[], &ct).is_ok());
        assert_eq!(
            tr.receive.decrypt_with_ad(&[], &ct),
            Err(NoiseError::DecryptFailed)
        );
        let mut altered = ti.send.encrypt_with_ad(&[], b"two").unwrap();
        altered[0] ^= 1;
        assert_eq!(
            tr.receive.decrypt_with_ad(&[], &altered),
            Err(NoiseError::DecryptFailed)
        );
    }
}
