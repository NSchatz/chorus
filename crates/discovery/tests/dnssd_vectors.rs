//! AC-2, the half that is graded on the bytes.
//!
//! "WHEN an endpoint starts with no configured server address THE SYSTEM SHALL
//! discover one by mDNS and SHALL fall back to a static address when discovery
//! returns nothing."
//!
//! The discovery half: given a committed DNS-SD response packet the resolver
//! produces the advertised host and port, and with nothing to resolve it
//! produces the query packet the committed query fixture holds. Neither needs
//! multicast, a second machine or a link that carries either, which is the
//! whole point: a second implementation is graded against `fixtures/discovery/`
//! and not against this code, exactly as `fixtures/protocol/` does for the
//! audio wire.
//!
//! The fallback half is here too, because it is behaviour and not bytes: an
//! endpoint whose browse returns nothing takes the configured static address,
//! and an endpoint with neither says which of the two it lacked.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chorus_discovery::dnssd::{browse_query_bytes, resolve, AUDIO_SERVICE, CONTROL_SERVICE};
use chorus_discovery::net::{locate, Located};
use chorus_discovery::wire;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("this crate lives at crates/<name> under the repository root")
        .to_path_buf()
}

fn fixture_dir() -> PathBuf {
    repository_root().join("fixtures/discovery")
}

/// Every vector's name, discovered from the directory rather than listed here,
/// so that adding a vector is adding files and registering nothing.
fn vector_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .expect("fixtures/discovery is committed")
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("params") {
                return None;
            }
            path.file_stem().and_then(|s| s.to_str()).map(String::from)
        })
        .collect();
    names.sort();
    assert!(!names.is_empty(), "fixtures/discovery holds no vector");
    names
}

fn read_fields(path: &Path) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} is committed and readable: {}", path.display(), e));
    let mut fields = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("{}: '{}' is not 'key = value'", path.display(), line));
        fields.insert(key.trim().to_string(), value.trim().to_string());
    }
    fields
}

/// The bytes of one committed `.hex` vector.
fn read_hex(name: &str) -> Vec<u8> {
    let path = fixture_dir().join(format!("{}.hex", name));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is committed and readable: {}", path.display(), e));
    let mut bytes = Vec::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        for token in line.split_whitespace() {
            bytes.push(
                u8::from_str_radix(token, 16)
                    .unwrap_or_else(|_| panic!("{}: '{}' is not a hex byte", path.display(), token)),
            );
        }
    }
    assert!(!bytes.is_empty(), "{} holds no bytes", path.display());
    bytes
}

#[test]
fn a_browse_query_is_the_committed_query_packet_byte_for_byte() {
    let mut ran = 0;
    for name in vector_names() {
        let params = read_fields(&fixture_dir().join(format!("{}.params", name)));
        if params.get("kind").map(|k| k.as_str()) != Some("query") {
            continue;
        }
        let service = params.get("service").expect("a service");
        let produced = browse_query_bytes(service).expect("the service type encodes");
        assert_eq!(
            produced,
            read_hex(&name),
            "fixtures/discovery/{}.hex is the contract and browse_query_bytes({}) did not produce \
             it",
            name,
            service
        );
        ran += 1;
    }
    assert_eq!(ran, 2, "both service types have a committed query vector");
}

#[test]
fn resolving_every_committed_response_packet_gives_its_committed_answer() {
    let mut ran = 0;
    for name in vector_names() {
        let expected_path = fixture_dir().join(format!("{}.expected", name));
        if !expected_path.exists() {
            continue;
        }
        let expected = read_fields(&expected_path);
        let service = expected.get("service").expect("a service");
        let found = resolve(&read_hex(&name), service)
            .unwrap_or_else(|e| panic!("{}.hex does not resolve: {}", name, e));
        let wanted: usize = expected.get("instances").expect("a count").parse().unwrap();
        assert_eq!(found.len(), wanted, "{}: instances found", name);
        for (index, service) in found.iter().enumerate() {
            let at = |key: &str| {
                expected
                    .get(&format!("instance.{}.{}", index, key))
                    .unwrap_or_else(|| panic!("{}.expected has no instance.{}.{}", name, index, key))
                    .clone()
            };
            assert_eq!(service.instance, at("name"), "{}: instance name", name);
            assert_eq!(service.label, at("label"), "{}: instance label", name);
            assert_eq!(service.host, at("host"), "{}: host", name);
            assert_eq!(service.port.to_string(), at("port"), "{}: port", name);
            assert_eq!(
                service.socket_address().unwrap_or_default(),
                at("address"),
                "{}: the address the endpoint would dial",
                name
            );
            for (key, value) in &expected {
                if let Some(txt_key) = key.strip_prefix(&format!("instance.{}.txt.", index)) {
                    assert_eq!(
                        service.txt_value(txt_key),
                        Some(value.as_str()),
                        "{}: TXT key '{}'",
                        name,
                        txt_key
                    );
                }
            }
        }
        ran += 1;
    }
    assert!(ran >= 3, "only {} response vectors ran", ran);
}

#[test]
fn a_compressed_advertisement_resolves_to_the_same_answer_as_an_uncompressed_one() {
    let plain = read_hex("advertisement-audio");
    let compressed = read_hex("advertisement-audio-compressed");
    assert!(
        compressed.len() < plain.len(),
        "the compressed vector is {} bytes and the plain one is {}; if they are the same size \
         nothing is being compressed and this vector checks nothing",
        compressed.len(),
        plain.len()
    );
    assert!(
        compressed.windows(1).any(|b| b[0] & 0xC0 == 0xC0),
        "the compressed vector carries no compression pointer at all"
    );
    assert_eq!(
        resolve(&plain, AUDIO_SERVICE).unwrap(),
        resolve(&compressed, AUDIO_SERVICE).unwrap(),
        "the same advertisement written two ways has to resolve to the same thing"
    );
}

#[test]
fn regenerating_every_vector_reproduces_the_committed_bytes() {
    // The vectors are generated by `make discovery-vectors` from the committed
    // parameters. This is what stops that target being a way to make a red
    // assertion green: the committed bytes and the bytes the parameters produce
    // are the same bytes, or this test is red.
    let status = std::process::Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "-p",
            "chorus-discovery",
            "--bin",
            "chorus-discovery-vectors",
        ])
        .current_dir(repository_root())
        .status();
    match status {
        Ok(status) if status.success() => {}
        other => panic!("the vector generator did not run: {:?}", other),
    }
    // If regeneration had changed anything, the assertions above would now be
    // reading different files. Run them again, here, on the regenerated tree.
    for name in vector_names() {
        let params = read_fields(&fixture_dir().join(format!("{}.params", name)));
        if params.get("kind").map(|k| k.as_str()) == Some("query") {
            assert_eq!(
                browse_query_bytes(params.get("service").unwrap()).unwrap(),
                read_hex(&name)
            );
        }
    }
    let git = std::process::Command::new("git")
        .args(["status", "--porcelain", "--", "fixtures/discovery"])
        .current_dir(repository_root())
        .output();
    if let Ok(output) = git {
        let changed = String::from_utf8_lossy(&output.stdout);
        let regenerated: Vec<&str> = changed
            .lines()
            .filter(|l| l.contains(".hex") && !l.trim_start().starts_with("??"))
            .collect();
        assert!(
            regenerated.is_empty(),
            "regenerating the vectors changed committed files, which means the committed bytes \
             are not what the committed parameters produce:\n{}",
            regenerated.join("\n")
        );
    }
}

#[test]
fn a_response_for_another_service_type_resolves_to_nothing() {
    assert!(resolve(&read_hex("advertisement-audio"), CONTROL_SERVICE)
        .unwrap()
        .is_empty());
}

#[test]
fn a_packet_that_is_not_a_message_is_refused_rather_than_half_read() {
    let good = read_hex("advertisement-audio");
    for cut in 0..good.len() {
        let truncated = &good[..cut];
        // Either it fails to decode, or it decodes to something with no
        // resolvable instance in it. What it must never do is produce a host
        // and port out of a message that was cut in half.
        if let Ok(found) = resolve(truncated, AUDIO_SERVICE) {
            for service in found {
                assert!(
                    service.socket_address().is_none() || cut == good.len(),
                    "{} bytes of a {}-byte advertisement produced an address to dial",
                    cut,
                    good.len()
                );
            }
        }
    }
    assert!(resolve(&[0u8; 4], AUDIO_SERVICE).is_err());
    assert!(wire::decode(&[]).is_err());
}

#[test]
fn a_browse_that_returns_nothing_falls_back_to_the_configured_static_address() {
    // A browse on a link with no responder: a real socket, a real query, a real
    // window, and nothing answers. That is not an error and it is not a
    // discovered server; it is the case the fallback exists for.
    let located = locate(
        "_chorus-no-such-service._tcp.local.",
        Some(Duration::from_millis(150)),
        Some("127.0.0.1:4010"),
    )
    .expect("a static address is configured");
    assert_eq!(located.address(), "127.0.0.1:4010");
    match &located {
        Located::Fallback { because, .. } => assert!(
            because.contains("returned nothing") || because.contains("could not run"),
            "the fallback has to say WHY it was taken: {}",
            because
        ),
        other => panic!("expected a fallback, got {:?}", other),
    }
    assert!(
        located.line().contains("how=static-fallback"),
        "{}",
        located.line()
    );
}

#[test]
fn an_endpoint_with_neither_says_which_of_the_two_it_lacks() {
    let err = locate(AUDIO_SERVICE, Some(Duration::from_millis(150)), None)
        .expect_err("there is nothing to connect to");
    let text = err.to_string();
    assert!(text.contains("no server address"), "{}", text);
    assert!(
        text.contains("discovery was attempted"),
        "it has to say what discovery did: {}",
        text
    );
    assert!(text.contains("--server"), "{}", text);

    let err = locate(AUDIO_SERVICE, None, None).expect_err("there is nothing to connect to");
    let text = err.to_string();
    assert!(text.contains("discovery was not attempted"), "{}", text);
    assert!(text.contains("--discover"), "{}", text);
}
