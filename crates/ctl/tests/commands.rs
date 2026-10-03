//! Every command chorusctl sends is the catalog's vector for the same
//! arguments, byte for byte.
//!
//! Each case takes its arguments from a committed vector's `.fields`, runs
//! chorusctl against the fake server, and compares the body that arrived on
//! the socket with the vector's `.json`. The grammar table says which catalog
//! messages each verb sends; this file is held to covering every one of them.

mod support;

use chorus_ctl::grammar::NOUNS;
use support::{ctl, field, vector, Fake};

/// One case: the vector, the noun and verb, and the arguments built from the
/// vector's fields.
struct Case {
    vector: &'static str,
    noun: &'static str,
    verb: &'static str,
    args: Vec<String>,
}

fn case(vector: &'static str, noun: &'static str, verb: &'static str, shape: &[&str]) -> Case {
    // A word of the shape that names a field is that field's value; any other
    // word (a flag) is itself.
    let args = shape
        .iter()
        .flat_map(|word| match word.strip_prefix('*') {
            // `*zones`: a comma-separated field, one operand per item.
            Some(key) => field(vector, key)
                .unwrap_or_else(|| panic!("{}.fields has '{}'", vector, key))
                .split(',')
                .map(str::to_string)
                .collect::<Vec<_>>(),
            None => vec![field(vector, word).unwrap_or_else(|| word.to_string())],
        })
        .collect();
    Case {
        vector,
        noun,
        verb,
        args,
    }
}

fn cases() -> Vec<Case> {
    let mut cases = vec![
        case("name", "rooms", "name", &["zone", "name"]),
        case("ungroup", "groups", "leave", &["zone"]),
        case("volume", "volume", "set", &["zone", "volume"]),
        case("mute", "volume", "mute", &["zone"]),
        case("v2/join", "groups", "join", &["zone", "target"]),
        case(
            "v2/group_save",
            "groups",
            "save",
            &["group", "name", "*zones"],
        ),
        case("v2/group_delete", "groups", "delete", &["group"]),
        case("v2/take", "groups", "take", &["target"]),
        case(
            "v2/take-source",
            "groups",
            "take",
            &["target", "--source", "source"],
        ),
        case(
            "v2/group_volume",
            "volume",
            "set",
            &["--group", "group", "volume"],
        ),
        case(
            "v2/group_volume_step",
            "volume",
            "step",
            &["--group", "group", "step"],
        ),
        case("v2/volume_step", "volume", "step", &["zone", "step"]),
        case("v2/limit", "volume", "limit", &["zone", "limit"]),
        case("v2/speaker_name", "endpoints", "name", &["speaker", "name"]),
        case("v2/speaker_room", "endpoints", "room", &["speaker", "room"]),
        case(
            "v2/speaker_room-none",
            "endpoints",
            "room",
            &["speaker", "--none"],
        ),
        case("v2/speaker_forget", "endpoints", "forget", &["speaker"]),
        case(
            "v2/firmware_install",
            "updates",
            "install",
            &["speaker", "image"],
        ),
        case(
            "v2/firmware_install-all",
            "updates",
            "install",
            &["--all", "image"],
        ),
        case(
            "v2/firmware_install-force",
            "updates",
            "install",
            &["speaker", "image", "--force"],
        ),
        case("v2/firmware_cancel", "updates", "cancel", &["speaker"]),
        case("v2/firmware_rescan", "updates", "rescan", &[]),
        case(
            "v2/source_store",
            "sources",
            "store",
            &["id", "kind", "name", "value"],
        ),
        case(
            "v2/source_store-spotify",
            "sources",
            "store",
            &["id", "kind", "name", "value"],
        ),
        case("v2/source_forget", "sources", "forget", &["id"]),
        case(
            "v2/input_label",
            "inputs",
            "label",
            &["input", "role", "name"],
        ),
        case("v2/input_label-clear", "inputs", "unlabel", &["input"]),
        case("v2/soloist_restart", "soloist", "restart", &[]),
        case("v2/playback", "soloist", "pause", &["target"]),
        case("v2/playback-resume", "soloist", "resume", &["target"]),
        case("v2/playback-next", "soloist", "next", &["target"]),
        case("v2/playback-previous", "soloist", "previous", &["target"]),
    ];
    // `inputs select <input> <target>` is `take` with a line-in source: the
    // input is the vector's source without its `line-in:` prefix.
    let source = field("v2/take-source", "source").expect("take-source has a source");
    let input = source.strip_prefix("line-in:").expect("a line-in source");
    cases.push(Case {
        vector: "v2/take-source",
        noun: "inputs",
        verb: "select",
        args: vec![
            input.to_string(),
            field("v2/take-source", "target").expect("a target"),
        ],
    });
    cases
}

#[test]
fn every_command_sent_is_byte_equal_to_its_catalog_vector() {
    let state = vector("v2/state-rich");
    for case in cases() {
        let fake = Fake::answering(&[("200 OK", &state)]);
        let mut args = vec![case.noun, case.verb];
        args.extend(case.args.iter().map(String::as_str));
        let outcome = ctl(&fake.address, &args);
        assert_eq!(outcome.code, 0, "{:?}: {}", args, outcome.stderr);
        let requests = fake.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].body,
            vector(case.vector),
            "chorusctl {:?} against fixtures/control/{}.json",
            args,
            case.vector
        );
        let head = &requests[0].head;
        assert!(
            head.starts_with("POST /api/command HTTP/1.1\r\n"),
            "{}",
            head
        );
        assert!(
            head.contains("\r\nContent-Type: application/json"),
            "{}",
            head
        );
        assert!(!head.to_ascii_lowercase().contains("origin:"), "{}", head);
    }
}

#[test]
fn every_message_a_verb_sends_has_a_vector_case_and_every_case_is_in_the_table() {
    let cases = cases();
    for noun in NOUNS {
        for verb in noun.verbs {
            for sends in verb.sends {
                assert!(
                    cases.iter().any(|c| c.noun == noun.name
                        && c.verb == verb.name
                        && field(c.vector, "message_type").as_deref() == Some(*sends))
                        // `unmute` is `mute` with `muted` false; the catalog's one
                        // `mute` vector is the true one, and the parser's own test
                        // holds the false one to `Command::encode()`.
                        || (noun.name, verb.name) == ("volume", "unmute"),
                    "'{} {}' sends {} and no case holds it to a vector",
                    noun.name,
                    verb.name,
                    sends
                );
            }
        }
    }
    for case in &cases {
        let verb = NOUNS
            .iter()
            .find(|n| n.name == case.noun)
            .and_then(|n| n.verbs.iter().find(|v| v.name == case.verb))
            .unwrap_or_else(|| panic!("{} {} is in the table", case.noun, case.verb));
        let sent = field(case.vector, "message_type").expect("a message_type");
        assert!(verb.sends.contains(&sent.as_str()), "{}", case.vector);
    }
}

#[test]
fn a_read_verb_gets_the_state_and_sends_no_command() {
    let state = vector("v2/state-rich");
    for args in [
        vec!["rooms", "list"],
        vec!["rooms", "show", "kitchen"],
        vec!["groups", "list"],
        vec!["volume", "get", "kitchen"],
        vec!["inputs", "list"],
        vec!["inputs", "labels"],
        vec!["sources", "list"],
        vec!["endpoints", "list"],
        vec!["endpoints", "show", "endpoint-a"],
        vec!["updates", "list"],
        vec!["updates", "status"],
        vec!["soloist", "status"],
    ] {
        let fake = Fake::answering(&[("200 OK", &state)]);
        let outcome = ctl(&fake.address, &args);
        assert_eq!(outcome.code, 0, "{:?}: {}", args, outcome.stderr);
        let requests = fake.requests();
        assert!(
            requests[0].head.starts_with("GET /api/state HTTP/1.1\r\n"),
            "{:?}: {}",
            args,
            requests[0].head
        );
        assert_eq!(requests[0].body, "");
    }
}
