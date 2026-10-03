//! `chorusctl`: a command line over the chorus control API.
//!
//! The control API is `docs/control-plane.md`: `GET /api/state` answers the
//! state message and `POST /api/command` takes one catalog message and answers
//! the new state, or the server's `error` or `refused`. `chorusctl` is a thin
//! client of exactly that: it has no state of its own, decides nothing the
//! server decides, and every command it sends is a catalog [`Command`]'s own
//! `encode()`, held byte for byte to the vectors under `fixtures/control`.
//!
//! # Grammar
//!
//! ```text
//! chorusctl [--server <host:port>] [--json] [--timeout <seconds>] <noun> <verb> [args]
//! ```
//!
//! Seven nouns, one per area: `rooms`, `groups`, `volume`, `inputs`, `sources`,
//! `endpoints`, `updates`. [`grammar`] is the one table of them, their verbs,
//! the flags and the exit codes; the parser, `--help` (global and per noun)
//! and the tests all read it. `docs/chorusctl.md` is the page for a person.
//!
//! `--server` is the server's `--control-listen` address, and `CHORUS_SERVER`
//! supplies it when the flag is absent. There is no default address, because
//! the server's control plane has no default port.
//!
//! # Output
//!
//! A read verb prints a part of the state; a mutating verb prints the part of
//! the new state it changed. With `--json` a read verb prints that part as
//! the server's own JSON, a mutating verb prints the whole state message the
//! server answered, and an error is one JSON object on stderr:
//! `{"error":"<name>","exit":<code>,"detail":"..."}` plus `hint` (usage),
//! `server` (unreachable) or `status` and `field` (refused).
//!
//! # Exit codes, which are the contract a script grades
//!
//! | code | name | meaning |
//! |---|---|---|
//! | 0 | ok | the command was applied, or the state was read and printed |
//! | 1 | usage | the command line is not valid; nothing was sent |
//! | 2 | unreachable | no answer from a chorus server: connect, timeout or a bad HTTP answer |
//! | 3 | refused | the server answered and refused (its `error` or `refused` message) |
//! | 4 | not-found | the server answered; what a read verb named is not in its state |
//!
//! # Firmware installs
//!
//! `updates install` sends the catalog's `firmware_install` and nothing else.
//! Whether an image may go to a speaker is the server's decision (its guard
//! refuses a transfer to a peer that is not loopback unless the owner is at
//! the bench, `docs/conventions.md` rule 20). `chorusctl` neither reads nor
//! sets the owner-at-bench variable and has no flag that reaches around the
//! server: a refusal is exit 3 with the server's words.

#![forbid(unsafe_code)]

pub mod client;
pub mod grammar;
pub mod parse;
pub mod render;

use chorus_control::json::{self, Value};

use client::Failure;
use grammar::{
    EXIT_CODES, EXIT_NOT_FOUND, EXIT_OK, EXIT_REFUSED, EXIT_UNREACHABLE, EXIT_USAGE,
    SERVER_VARIABLE,
};
use parse::{Action, Usage};

/// What one run of `chorusctl` did: its exit code and what it printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The exit code, one of [`grammar::EXIT_CODES`].
    pub code: u8,
    /// What goes to stdout.
    pub stdout: String,
    /// What goes to stderr.
    pub stderr: String,
}

/// One failure, written for a person or as one JSON object.
fn failed(as_json: bool, code: u8, detail: &str, fix: &str, extra: Vec<(&str, Value)>) -> Outcome {
    let name = EXIT_CODES
        .iter()
        .find(|e| e.code == code)
        .map_or("error", |e| e.name);
    let stderr = if as_json {
        let mut members = vec![
            ("error".to_string(), Value::text(name)),
            ("exit".to_string(), Value::int(i64::from(code))),
            ("detail".to_string(), Value::text(detail)),
        ];
        members.extend(extra.into_iter().map(|(k, v)| (k.to_string(), v)));
        format!("{}\n", json::write(&Value::Obj(members)))
    } else {
        format!("chorusctl: {}: {}\n  {}\n", name, detail, fix)
    };
    Outcome {
        code,
        stdout: String::new(),
        stderr,
    }
}

fn usage(as_json: bool, error: &Usage) -> Outcome {
    failed(
        as_json,
        EXIT_USAGE,
        &error.detail,
        &error.hint,
        vec![("hint", Value::text(&error.hint))],
    )
}

/// Run `chorusctl` with these arguments (without the program's name), taking
/// `--server`'s default from the `CHORUS_SERVER` variable.
pub fn run(args: &[String]) -> Outcome {
    let default = std::env::var(SERVER_VARIABLE).ok();
    run_with(args, default.as_deref())
}

/// [`run`], with the default server given rather than read from the
/// environment: the whole program as a function of its inputs.
pub fn run_with(args: &[String], default_server: Option<&str>) -> Outcome {
    // An error is JSON when the caller asked for JSON, including the error
    // of a command line that could not be read past that flag.
    let as_json = args
        .iter()
        .take_while(|a| a.as_str() != "--")
        .any(|a| a == "--json");
    let invocation = match parse::parse(args) {
        Ok(invocation) => invocation,
        Err(error) => return usage(as_json, &error),
    };
    let (command, view) = match &invocation.action {
        Action::Help(text) => {
            return Outcome {
                code: EXIT_OK,
                stdout: text.clone(),
                stderr: String::new(),
            }
        }
        Action::Read(view) => (None, view),
        Action::Send { command, shows } => (Some(command.encode()), shows),
    };
    let server = match (&invocation.server, default_server.filter(|s| !s.is_empty())) {
        (Some(server), _) => server.clone(),
        (None, Some(default)) => match parse::server_address(default) {
            Ok(server) => server,
            Err(error) => {
                return usage(
                    as_json,
                    &Usage {
                        detail: format!("{} holds {}", SERVER_VARIABLE, error.detail),
                        hint: error.hint,
                    },
                )
            }
        },
        (None, None) => {
            return usage(
                as_json,
                &Usage {
                    detail: "no server is named".to_string(),
                    hint: format!(
                        "give the server's --control-listen address with '--server host:port' \
                         or in {}",
                        SERVER_VARIABLE
                    ),
                },
            )
        }
    };
    let answered = match &command {
        Some(body) => client::command(&server, body, invocation.timeout),
        None => client::state(&server, invocation.timeout),
    };
    let (text, state) = match answered {
        Ok(state) => state,
        Err(Failure::Unreachable(detail)) => {
            return failed(
                as_json,
                EXIT_UNREACHABLE,
                &detail,
                &format!(
                    "check --server (or {}) and that chorus-server runs with --control-listen",
                    SERVER_VARIABLE
                ),
                vec![("server", Value::text(&server))],
            )
        }
        Err(Failure::Refused {
            status,
            field,
            detail,
            ..
        }) => {
            let fix = if field.is_empty() {
                format!("the server answered HTTP {}; nothing was applied", status)
            } else {
                format!(
                    "the server answered HTTP {} naming the field '{}'; nothing was applied",
                    status, field
                )
            };
            return failed(
                as_json,
                EXIT_REFUSED,
                &detail,
                &fix,
                vec![
                    ("status", Value::int(i64::from(status))),
                    ("field", Value::text(&field)),
                ],
            );
        }
    };
    // A mutating verb with --json prints what the server answered, as it
    // answered it.
    if command.is_some() && invocation.json {
        return Outcome {
            code: EXIT_OK,
            stdout: format!("{}\n", text),
            stderr: String::new(),
        };
    }
    match render::render(&state, view, invocation.json) {
        Ok(stdout) => Outcome {
            code: EXIT_OK,
            stdout,
            stderr: String::new(),
        },
        // The command applied; the thing it named is simply not in the part
        // shown any more (a forgotten speaker, a deleted group).
        Err(_) if command.is_some() => Outcome {
            code: EXIT_OK,
            stdout: "ok\n".to_string(),
            stderr: String::new(),
        },
        Err(detail) => failed(
            as_json,
            EXIT_NOT_FOUND,
            &detail,
            "the server answered; check the name against the list verb of the same noun",
            Vec::new(),
        ),
    }
}
