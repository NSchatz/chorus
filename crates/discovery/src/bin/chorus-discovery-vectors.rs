//! Regenerate the committed DNS-SD packet vectors from their committed
//! parameters.
//!
//! The same shape as `make sync-vectors` and `make measure-fixtures`, and for
//! the same reason: `crates/discovery/tests/dnssd_vectors.rs` asserts that
//! regenerating reproduces every committed `.hex` byte for byte, so this
//! binary is for changing a fixture's parameters and NEVER for making a red
//! assertion green.
//!
//!     make discovery-vectors
//!
//! One fixture is not a straight encode: `compress = 1` writes the same
//! logical message with DNS name compression, which every real responder emits
//! and which the resolver therefore has to follow. The encoder in
//! `crates/discovery/src/wire.rs` deliberately never compresses - one message,
//! one spelling - so the compressed vector is built here, by hand, out of the
//! same records.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use chorus_discovery::dnssd::{browse_query_bytes, Advertisement};
use chorus_discovery::wire::{self, Name, Rdata, Record};

fn main() -> std::process::ExitCode {
    let root = repository_root();
    let dir = root.join("fixtures/discovery");
    let mut names: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("params"))
            .collect(),
        Err(e) => {
            eprintln!("chorus-discovery-vectors: {}: {}", dir.display(), e);
            return std::process::ExitCode::from(2);
        }
    };
    names.sort();
    if names.is_empty() {
        eprintln!("chorus-discovery-vectors: {} holds no .params file", dir.display());
        return std::process::ExitCode::from(2);
    }
    for params in names {
        let stem = params.file_stem().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&params).expect("a readable .params");
        let fields = parse(&text);
        let bytes = match render(&fields) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("chorus-discovery-vectors: {}: {}", stem, e);
                return std::process::ExitCode::from(2);
            }
        };
        let hex = as_hex(&stem, &fields, &bytes);
        let out = dir.join(format!("{}.hex", stem));
        std::fs::write(&out, hex).expect("a writable fixture directory");
        println!(
            "chorus-discovery-vectors: {} ({} bytes)",
            out.file_name().unwrap().to_string_lossy(),
            bytes.len()
        );
    }
    std::process::ExitCode::SUCCESS
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("this crate lives at crates/<name> under the repository root")
        .to_path_buf()
}

fn parse(text: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    fields
}

fn get<'a>(fields: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    fields
        .get(key)
        .unwrap_or_else(|| panic!("the .params file has no '{}'", key))
}

/// Build the packet a `.params` file describes.
pub fn render(fields: &BTreeMap<String, String>) -> Result<Vec<u8>, String> {
    match get(fields, "kind") {
        "query" => browse_query_bytes(get(fields, "service")).map_err(|e| e.to_string()),
        "response" => {
            let advertisement = Advertisement {
                instance: get(fields, "instance").to_string(),
                service: get(fields, "service").to_string(),
                host: get(fields, "host").to_string(),
                port: get(fields, "port").parse().map_err(|_| "the port is not a number")?,
                addresses: fields
                    .iter()
                    .filter(|(k, _)| k.starts_with("address."))
                    .map(|(_, v)| v.parse::<IpAddr>().map_err(|_| format!("'{}' is not an address", v)))
                    .collect::<Result<Vec<_>, _>>()?,
                txt: fields
                    .iter()
                    .filter(|(k, _)| k.starts_with("txt."))
                    .map(|(_, v)| match v.split_once('=') {
                        Some((k, value)) => (k.to_string(), value.to_string()),
                        None => (v.to_string(), String::new()),
                    })
                    .collect(),
            };
            let message = advertisement.response().map_err(|e| e.to_string())?;
            if fields.get("compress").map(|s| s.as_str()) == Some("1") {
                Ok(compressed(&message))
            } else {
                wire::encode(&message).map_err(|e| e.to_string())
            }
        }
        other => Err(format!("'{}' is not a kind of packet this makes", other)),
    }
}

/// The same message, written with name compression.
///
/// Every name written out in full is remembered at the offset it started, and
/// a name already seen is written as a two-byte pointer to it. That is exactly
/// what a real responder does, and it is the reason the decoder has to follow
/// one.
fn compressed(message: &wire::Message) -> Vec<u8> {
    let mut out = Vec::new();
    let mut seen: Vec<(String, usize)> = Vec::new();
    out.extend_from_slice(&message.id.to_be_bytes());
    out.extend_from_slice(&message.flags.to_be_bytes());
    out.extend_from_slice(&(message.questions.len() as u16).to_be_bytes());
    out.extend_from_slice(&(message.answers.len() as u16).to_be_bytes());
    out.extend_from_slice(&(message.authorities.len() as u16).to_be_bytes());
    out.extend_from_slice(&(message.additionals.len() as u16).to_be_bytes());
    for question in &message.questions {
        put_name(&question.name, &mut out, &mut seen);
        out.extend_from_slice(&question.qtype.to_be_bytes());
        out.extend_from_slice(&question.qclass.to_be_bytes());
    }
    for record in message.records() {
        put_record(record, &mut out, &mut seen);
    }
    out
}

fn put_record(record: &Record, out: &mut Vec<u8>, seen: &mut Vec<(String, usize)>) {
    put_name(&record.name, out, seen);
    out.extend_from_slice(&record.rtype.to_be_bytes());
    out.extend_from_slice(&record.class.to_be_bytes());
    out.extend_from_slice(&record.ttl.to_be_bytes());
    // The length is written once the data is, because a compressed name inside
    // the data has to be built at its final offset.
    let length_at = out.len();
    out.extend_from_slice(&[0, 0]);
    let from = out.len();
    match &record.rdata {
        Rdata::Ptr(name) => put_name(name, out, seen),
        Rdata::Srv {
            priority,
            weight,
            port,
            target,
        } => {
            out.extend_from_slice(&priority.to_be_bytes());
            out.extend_from_slice(&weight.to_be_bytes());
            out.extend_from_slice(&port.to_be_bytes());
            // RFC 2782's SRV target is NOT compressed by most responders; it is
            // written out here for the same reason, and because a decoder that
            // handles the compressed PTR is already shown to follow pointers.
            put_uncompressed(target, out, seen);
        }
        Rdata::Txt(strings) => {
            for string in strings {
                out.push(string.len() as u8);
                out.extend_from_slice(string);
            }
            if strings.is_empty() {
                out.push(0);
            }
        }
        Rdata::A(address) => out.extend_from_slice(&address.octets()),
        Rdata::Aaaa(address) => out.extend_from_slice(&address.octets()),
        Rdata::Other(bytes) => out.extend_from_slice(bytes),
    }
    let length = (out.len() - from) as u16;
    out[length_at..length_at + 2].copy_from_slice(&length.to_be_bytes());
}

fn put_name(name: &Name, out: &mut Vec<u8>, seen: &mut Vec<(String, usize)>) {
    let key = name.dotted().to_ascii_lowercase();
    if let Some((_, at)) = seen.iter().find(|(k, _)| *k == key) {
        let pointer = 0xC000u16 | *at as u16;
        out.extend_from_slice(&pointer.to_be_bytes());
        return;
    }
    put_uncompressed(name, out, seen);
}

/// Write a name out in full, remembering it and every suffix of it, so a later
/// name can point at the part it shares.
fn put_uncompressed(name: &Name, out: &mut Vec<u8>, seen: &mut Vec<(String, usize)>) {
    let labels = name.labels();
    for skipped in 0..labels.len() {
        let suffix = Name::from_labels(labels[skipped..].to_vec());
        let key = suffix.dotted().to_ascii_lowercase();
        if !seen.iter().any(|(k, _)| *k == key) {
            seen.push((key, out.len()));
        }
        let label = &labels[skipped];
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
}

/// The `.hex` file: a comment header saying what this is, then the bytes.
fn as_hex(name: &str, fields: &BTreeMap<String, String>, bytes: &[u8]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# chorus DNS-SD vector: {}.\n", name));
    out.push_str("#\n");
    out.push_str("# GENERATED from the .params file beside it by `make discovery-vectors`, and\n");
    out.push_str("# asserted byte for byte by crates/discovery/tests/dnssd_vectors.rs. Change the\n");
    out.push_str("# parameters, never this file, and never to make a red assertion green.\n");
    out.push_str("#\n");
    for (key, value) in fields {
        out.push_str(&format!("# {} = {}\n", key, value));
    }
    out.push_str("#\n");
    out.push_str("# Format: whitespace separated hex bytes, '#' starts a comment.\n");
    out.push('\n');
    for (index, chunk) in bytes.chunks(16).enumerate() {
        out.push_str(&format!("# offset {:#06x}\n", index * 16));
        let line: Vec<String> = chunk.iter().map(|b| format!("{:02X}", b)).collect();
        out.push_str(&line.join(" "));
        out.push('\n');
    }
    out
}
