//! A throwaway certificate authority for the HTTPS tests, made at run time:
//! Ed25519 keys from ring, and just enough DER to write an X.509 v3
//! certificate (RFC 5280 section 4.1, https://www.rfc-editor.org/rfc/rfc5280,
//! read 2026-10-03; Ed25519 identifiers from RFC 8410 section 3). Nothing
//! here is committed as a key, and nothing made here outlives the test.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    match content.len() {
        n if n < 0x80 => out.push(n as u8),
        n if n < 0x100 => out.extend_from_slice(&[0x81, n as u8]),
        n => out.extend_from_slice(&[0x82, (n >> 8) as u8, n as u8]),
    }
    out.extend_from_slice(content);
    out
}

fn seq(parts: &[&[u8]]) -> Vec<u8> {
    der(0x30, &parts.concat())
}

/// id-Ed25519, 1.3.101.112 (RFC 8410 section 3): the algorithm identifier
/// has no parameters.
fn ed25519() -> Vec<u8> {
    seq(&[&der(0x06, &[0x2b, 0x65, 0x70])])
}

/// A Name with one commonName (2.5.4.3).
fn name(common_name: &str) -> Vec<u8> {
    let attribute = seq(&[
        &der(0x06, &[0x55, 0x04, 0x03]),
        &der(0x0c, common_name.as_bytes()),
    ]);
    seq(&[&der(0x31, &attribute)])
}

fn bit_string(bytes: &[u8]) -> Vec<u8> {
    let mut content = vec![0u8];
    content.extend_from_slice(bytes);
    der(0x03, &content)
}

fn extension(oid: &[u8], critical: bool, value: &[u8]) -> Vec<u8> {
    let oid = der(0x06, oid);
    let value = der(0x04, value);
    if critical {
        seq(&[&oid, &[0x01, 0x01, 0xff], &value])
    } else {
        seq(&[&oid, &value])
    }
}

static SERIAL: AtomicU8 = AtomicU8::new(1);

fn certificate(
    issuer: &[u8],
    issuer_key: &Ed25519KeyPair,
    subject: &[u8],
    subject_key: &Ed25519KeyPair,
    extensions: &[Vec<u8>],
) -> Vec<u8> {
    let serial = SERIAL.fetch_add(1, Ordering::SeqCst) & 0x7f;
    // Valid from 2020 to the end of 2099: UTCTime before 2050, GeneralizedTime
    // after (RFC 5280 section 4.1.2.5). Fixed dates, so the test reads no clock.
    let validity = seq(&[&der(0x17, b"200101000000Z"), &der(0x18, b"20991231235959Z")]);
    let spki = seq(&[&ed25519(), &bit_string(subject_key.public_key().as_ref())]);
    let tbs = seq(&[
        &der(0xa0, &der(0x02, &[2])),
        &der(0x02, &[serial.max(1)]),
        &ed25519(),
        issuer,
        &validity,
        subject,
        &spki,
        &der(0xa3, &der(0x30, &extensions.concat())),
    ]);
    let signature = issuer_key.sign(&tbs);
    seq(&[&tbs, &ed25519(), &bit_string(signature.as_ref())])
}

fn new_key() -> (Vec<u8>, Ed25519KeyPair) {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).expect("a key");
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("the key just made");
    (pkcs8.as_ref().to_vec(), pair)
}

/// A self-signed root that issues server certificates.
pub struct Authority {
    name: Vec<u8>,
    key: Ed25519KeyPair,
    cert: Vec<u8>,
}

impl Authority {
    pub fn new(common_name: &str) -> Authority {
        let (_, key) = new_key();
        let name = name(common_name);
        // basicConstraints (2.5.29.19), critical, cA TRUE.
        let ca = extension(&[0x55, 0x1d, 0x13], true, &seq(&[&[0x01, 0x01, 0xff]]));
        let cert = certificate(&name, &key, &name, &key, &[ca]);
        Authority { name, key, cert }
    }

    /// A server configuration whose certificate names `dns` names and IPv4
    /// `ips` in its subjectAltName (2.5.29.17).
    pub fn server(&self, dns: &[&str], ips: &[[u8; 4]]) -> Arc<rustls::ServerConfig> {
        let (pkcs8, key) = new_key();
        let mut names = Vec::new();
        for d in dns {
            names.extend_from_slice(&der(0x82, d.as_bytes()));
        }
        for ip in ips {
            names.extend_from_slice(&der(0x87, ip));
        }
        let san = extension(&[0x55, 0x1d, 0x11], false, &der(0x30, &names));
        let cert = certificate(
            &self.name,
            &self.key,
            &name("chorus test server"),
            &key,
            &[san],
        );
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("protocol versions")
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(cert)],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pkcs8)),
            )
            .expect("the server certificate");
        Arc::new(config)
    }

    /// The root as a PEM bundle in a file of its own under the temporary
    /// directory; the caller removes it.
    pub fn bundle(&self, label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "chorus-fetch-test-{}-{label}.pem",
            std::process::id()
        ));
        std::fs::write(&path, pem(&self.cert)).expect("write the test bundle");
        path
    }
}

/// A certificate as PEM (RFC 7468): base64 in lines of 64.
fn pem(der: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::new();
    for chunk in der.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                encoded.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                encoded.push('=');
            }
        }
    }
    let mut out = String::from("-----BEGIN CERTIFICATE-----\n");
    for line in encoded.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(line).expect("base64 is ASCII"));
        out.push('\n');
    }
    out.push_str("-----END CERTIFICATE-----\n");
    out
}
