//! The exit codes, each graded against the fake server replaying the
//! committed vectors: 0 ok, 1 usage, 2 unreachable, 3 refused, 4 not-found.

mod support;

use std::io::ErrorKind;
use std::net::TcpListener;

use chorus_ctl::grammar::{EXIT_NOT_FOUND, EXIT_OK, EXIT_REFUSED, EXIT_UNREACHABLE, EXIT_USAGE};
use support::{closed_port, ctl, framed, vector, Fake};

#[test]
fn ok_is_0_for_a_read_and_for_an_applied_command() {
    let state = vector("v2/state-rich");
    let fake = Fake::answering(&[("200 OK", &state), ("200 OK", &state)]);
    assert_eq!(ctl(&fake.address, &["rooms", "list"]).code, EXIT_OK);
    assert_eq!(
        ctl(&fake.address, &["volume", "mute", "kitchen"]).code,
        EXIT_OK
    );
    assert_eq!(fake.requests().len(), 2);
}

#[test]
fn help_is_0_and_needs_no_server() {
    for args in [
        vec!["--help"],
        vec!["rooms", "--help"],
        vec!["help", "updates"],
    ] {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let outcome = chorus_ctl::run_with(&args, None);
        assert_eq!(outcome.code, EXIT_OK, "{:?}", args);
        assert!(
            outcome.stdout.starts_with("usage: chorusctl "),
            "{:?}",
            args
        );
        assert_eq!(outcome.stderr, "");
    }
}

#[test]
fn usage_is_1_and_nothing_is_sent() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener
        .set_nonblocking(true)
        .expect("a non-blocking listener");
    let address = listener.local_addr().expect("an address").to_string();
    for args in [
        vec!["rooms", "lst"],
        vec!["room", "list"],
        vec!["rooms", "list", "--jsn"],
        vec!["volume", "set", "kitchen", "loud"],
        vec!["updates", "install", "brick-2-0-0"],
        vec![],
    ] {
        let outcome = ctl(&address, &args);
        assert_eq!(outcome.code, EXIT_USAGE, "{:?}", args);
        assert_eq!(outcome.stdout, "", "{:?}", args);
        assert!(
            outcome.stderr.starts_with("chorusctl: usage: "),
            "{}",
            outcome.stderr
        );
    }
    match listener.accept() {
        Err(e) if e.kind() == ErrorKind::WouldBlock => {}
        other => panic!("a usage error connected to the server: {:?}", other),
    }
}

#[test]
fn usage_names_the_closest_valid_word_and_json_makes_it_one_object() {
    let outcome = ctl("127.0.0.1:9", &["rooms", "lst"]);
    assert_eq!(
        outcome.stderr,
        "chorusctl: usage: unknown verb 'lst' for 'rooms'\n  \
         did you mean 'list'? Its verbs are list, show, name\n"
    );
    let outcome = ctl("127.0.0.1:9", &["--json", "rooms", "list", "--forse"]);
    assert_eq!(outcome.code, EXIT_USAGE);
    assert_eq!(
        outcome.stderr,
        concat!(
            r#"{"error":"usage","exit":1,"detail":"unknown flag '--forse'","#,
            r#""hint":"did you mean '--force'? 'chorusctl --help' lists the flags; "#,
            r#"an operand that starts with a dash goes after a bare '--'"}"#,
            "\n"
        )
    );
}

#[test]
fn no_server_named_is_usage() {
    let args = vec!["rooms".to_string(), "list".to_string()];
    let outcome = chorus_ctl::run_with(&args, None);
    assert_eq!(outcome.code, EXIT_USAGE);
    assert!(
        outcome.stderr.contains("no server is named"),
        "{}",
        outcome.stderr
    );
    assert_eq!(chorus_ctl::run_with(&args, Some("")).code, EXIT_USAGE);
    let outcome = chorus_ctl::run_with(&args, Some("not an address"));
    assert_eq!(outcome.code, EXIT_USAGE);
    assert!(
        outcome.stderr.contains("CHORUS_SERVER holds"),
        "{}",
        outcome.stderr
    );
}

#[test]
fn the_default_server_is_used_and_the_flag_wins_over_it() {
    let state = vector("v2/state-empty");
    let fake = Fake::answering(&[("200 OK", &state), ("200 OK", &state)]);
    let args = vec!["rooms".to_string(), "list".to_string()];
    assert_eq!(
        chorus_ctl::run_with(&args, Some(&fake.address)).code,
        EXIT_OK
    );
    let mut flagged = args.clone();
    flagged.extend(["--server".to_string(), fake.address.clone()]);
    assert_eq!(
        chorus_ctl::run_with(&flagged, Some(&closed_port())).code,
        EXIT_OK
    );
    fake.requests();
}

#[test]
fn unreachable_is_2_when_nothing_listens() {
    let address = closed_port();
    let outcome = ctl(&address, &["rooms", "list"]);
    assert_eq!(outcome.code, EXIT_UNREACHABLE);
    assert_eq!(outcome.stdout, "");
    assert!(
        outcome.stderr.starts_with(&format!(
            "chorusctl: unreachable: cannot connect to {}: ",
            address
        )),
        "{}",
        outcome.stderr
    );
    let outcome = ctl(&address, &["volume", "mute", "kitchen", "--json"]);
    assert_eq!(outcome.code, EXIT_UNREACHABLE);
    assert!(
        outcome
            .stderr
            .starts_with(r#"{"error":"unreachable","exit":2,"detail":"cannot connect"#)
            && outcome
                .stderr
                .ends_with(&format!(",\"server\":\"{}\"}}\n", address)),
        "{}",
        outcome.stderr
    );
}

#[test]
fn unreachable_is_2_when_what_answers_is_not_the_control_api() {
    let state = vector("v2/state-rich");
    let cut_short = framed("200 OK", &state);
    let cut_short = &cut_short[..cut_short.len() - 40];
    for (answer, says) in [
        (String::new(), "closed without an answer"),
        ("220 mail ready\r\n\r\n".to_string(), "HTTP status line"),
        (cut_short.to_string(), "cut short"),
        (
            framed("200 OK", "<html>hello</html>"),
            "not a control catalog message",
        ),
        (
            framed("502 Bad Gateway", "upstream is down"),
            "answered HTTP 502",
        ),
        (
            framed("200 OK", r#"{"v":2,"t":"hello"}"#),
            "not a control catalog message",
        ),
    ] {
        let fake = Fake::raw(vec![answer.clone()]);
        let outcome = ctl(&fake.address, &["rooms", "list"]);
        assert_eq!(
            outcome.code, EXIT_UNREACHABLE,
            "{:?}: {}",
            answer, outcome.stderr
        );
        assert!(
            outcome.stderr.contains(says),
            "{:?}: {}",
            says,
            outcome.stderr
        );
        assert_eq!(outcome.stdout, "");
        fake.requests();
    }
}

#[test]
fn unreachable_is_2_when_the_server_never_answers() {
    // A listener that accepts (the kernel does) and never reads or writes.
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let address = listener.local_addr().expect("an address").to_string();
    let outcome = ctl(&address, &["rooms", "list", "--timeout", "1"]);
    assert_eq!(outcome.code, EXIT_UNREACHABLE, "{}", outcome.stderr);
    assert!(
        outcome.stderr.contains("did not answer within 1 s"),
        "{}",
        outcome.stderr
    );
    drop(listener);
}

#[test]
fn refused_is_3_with_the_servers_words() {
    let unknown_zone = vector("error-unknown-zone");
    let fake = Fake::answering(&[
        ("400 Bad Request", &unknown_zone),
        ("400 Bad Request", &unknown_zone),
    ]);
    let outcome = ctl(&fake.address, &["volume", "mute", "bathroom"]);
    assert_eq!(outcome.code, EXIT_REFUSED);
    assert_eq!(outcome.stdout, "");
    assert_eq!(
        outcome.stderr,
        "chorusctl: refused: there is no zone 'bathroom'; the zones configured on this server \
         are kitchen, study\n  the server answered HTTP 400 naming the field 'zone'; nothing was \
         applied\n"
    );
    let outcome = ctl(&fake.address, &["volume", "mute", "bathroom", "--json"]);
    assert_eq!(outcome.code, EXIT_REFUSED);
    assert_eq!(
        outcome.stderr,
        concat!(
            r#"{"error":"refused","exit":3,"detail":"there is no zone 'bathroom'; the zones "#,
            r#"configured on this server are kitchen, study","status":400,"field":"zone"}"#,
            "\n"
        )
    );
    // What was refused is the vector's own input, so the refusal replayed is
    // the one the real server gives this command.
    for request in fake.requests() {
        assert_eq!(
            Some(request.body),
            support::field("error-unknown-zone", "input")
        );
    }
}

#[test]
fn refused_is_3_for_every_refusal_the_server_writes() {
    for (status, name) in [
        ("426 Upgrade Required", "v2/refused-unknown-version"),
        ("400 Bad Request", "v2/error-firmware-install-unknown-image"),
        ("400 Bad Request", "v2/error-firmware-install-not-verified"),
        ("400 Bad Request", "v2/error-firmware-rescan-no-dir"),
        ("400 Bad Request", "error-volume-out-of-range"),
    ] {
        let refusal = vector(name);
        let fake = Fake::answering(&[(status, &refusal)]);
        let outcome = ctl(
            &fake.address,
            &["updates", "install", "--all", "brick-9-9-9"],
        );
        assert_eq!(outcome.code, EXIT_REFUSED, "{}: {}", name, outcome.stderr);
        assert!(
            outcome.stderr.starts_with("chorusctl: refused: "),
            "{}",
            name
        );
        fake.requests();
    }
    // The server's busy answer and its route refusals are its `error` too.
    let busy = r#"{"v":1,"t":"error","field":"","detail":"every control worker is busy"}"#;
    let fake = Fake::answering(&[("503 Service Unavailable", busy)]);
    let outcome = ctl(&fake.address, &["rooms", "list"]);
    assert_eq!(outcome.code, EXIT_REFUSED);
    assert_eq!(
        outcome.stderr,
        "chorusctl: refused: every control worker is busy\n  the server answered HTTP 503; \
         nothing was applied\n"
    );
    fake.requests();
}

#[test]
fn not_found_is_4_for_a_read_of_something_the_state_lacks() {
    let state = vector("v2/state-rich");
    for (args, says) in [
        (
            vec!["rooms", "show", "attic"],
            "there is no room 'attic'; the rooms are living, kitchen, study, bedroom",
        ),
        (
            vec!["volume", "get", "--group", "upstairs"],
            "there is no group 'upstairs' now; the groups are downstairs, live-1",
        ),
        (
            vec!["endpoints", "show", "endpoint-z"],
            "there is no speaker or endpoint 'endpoint-z'",
        ),
    ] {
        let fake = Fake::answering(&[("200 OK", &state)]);
        let outcome = ctl(&fake.address, &args);
        assert_eq!(outcome.code, EXIT_NOT_FOUND, "{:?}", args);
        assert_eq!(outcome.stdout, "");
        assert!(
            outcome.stderr.starts_with("chorusctl: not-found: "),
            "{}",
            outcome.stderr
        );
        assert!(outcome.stderr.contains(says), "{}", outcome.stderr);
        fake.requests();
    }
    let fake = Fake::answering(&[("200 OK", &state)]);
    let outcome = ctl(&fake.address, &["rooms", "show", "attic", "--json"]);
    assert_eq!(
        (outcome.code, outcome.stderr.as_str()),
        (
            EXIT_NOT_FOUND,
            "{\"error\":\"not-found\",\"exit\":4,\"detail\":\"there is no room 'attic'; the \
             rooms are living, kitchen, study, bedroom\"}\n"
        )
    );
    fake.requests();
}
