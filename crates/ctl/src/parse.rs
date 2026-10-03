//! The command line, read into what to do.
//!
//! Hand-written, as every chorus binary's parser is: flags are `--flag value`
//! or `--flag=value` and may stand anywhere; the first two words are the noun
//! and the verb, looked up in [`crate::grammar`]; the rest are the verb's
//! operands. A word that starts with `-` and a digit is an operand (a negative
//! step), and everything after a bare `--` is an operand.
//!
//! A mutating verb becomes a typed catalog [`Command`]; the bytes sent are that
//! command's own `encode()`, so they are the catalog's by construction.

use std::time::Duration;

use chorus_control::rooms::{
    InputId, InputLabel, InputRole, PlaybackAction, Source, StoredKind, StoredSource,
};
use chorus_control::{Command, Volume};

use crate::grammar::{
    self, closest, usage_line, Flag, Noun, Verb, DEFAULT_TIMEOUT_S, FLAGS, NOUNS, SERVER_VARIABLE,
};

/// A room, or a group, a volume verb acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A room, by id.
    Room(String),
    /// A group that exists now, by id.
    Group(String),
}

/// A part of the state to print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    /// Every room.
    Rooms,
    /// One room.
    Room(String),
    /// The groups and the saved groups.
    Groups,
    /// The volume of a room or a group.
    Volume(Target),
    /// The inputs.
    Inputs,
    /// The labelled inputs.
    InputLabels,
    /// The stored sources.
    Sources,
    /// The speakers and the endpoints.
    Endpoints,
    /// One speaker or endpoint.
    Endpoint(String),
    /// The staged firmware images.
    Images,
    /// Each speaker's firmware.
    Updates,
    /// The Soloist receivers.
    Soloist,
}

/// What one invocation does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Print this text and exit 0.
    Help(String),
    /// Read the state and print a part of it.
    Read(View),
    /// Send a command; on success print `shows` from the state it answers.
    Send {
        /// The catalog command.
        command: Command,
        /// The part of the new state a person is shown.
        shows: View,
    },
}

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// `--server`, where given.
    pub server: Option<String>,
    /// `--json`.
    pub json: bool,
    /// `--timeout`.
    pub timeout: Duration,
    /// What to do.
    pub action: Action,
}

/// A command line that is not valid: what is wrong, and the fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    /// What is wrong.
    pub detail: String,
    /// What to type instead.
    pub hint: String,
}

fn usage(detail: String, hint: String) -> Usage {
    Usage { detail, hint }
}

type Given = Vec<(&'static Flag, Option<String>)>;

fn take(given: &mut Given, name: &str) -> Option<Option<String>> {
    let at = given.iter().position(|(f, _)| f.name == name)?;
    Some(given.remove(at).1)
}

/// Split the arguments into flags and words.
fn split(args: &[String]) -> Result<(Given, Vec<String>), Usage> {
    let mut given: Given = Vec::new();
    let mut words = Vec::new();
    let mut only_words = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if only_words {
            words.push(arg.clone());
            continue;
        }
        if arg == "--" {
            only_words = true;
            continue;
        }
        let (name, inline) = if arg == "-h" {
            ("--help".to_string(), None)
        } else if arg.starts_with("--") {
            match arg.split_once('=') {
                Some((name, value)) => (name.to_string(), Some(value.to_string())),
                None => (arg.clone(), None),
            }
        } else if arg.len() > 1
            && arg.starts_with('-')
            && !arg[1..].starts_with(|c: char| c.is_ascii_digit())
        {
            (arg.clone(), None)
        } else {
            words.push(arg.clone());
            continue;
        };
        let Some(flag) = grammar::flag(&name) else {
            let near = closest(&name, FLAGS.iter().map(|f| f.name)).unwrap_or("--help");
            return Err(usage(
                format!("unknown flag '{}'", name),
                format!(
                    "did you mean '{}'? 'chorusctl --help' lists the flags; an operand that \
                     starts with a dash goes after a bare '--'",
                    near
                ),
            ));
        };
        if given.iter().any(|(f, _)| f.name == flag.name) {
            return Err(usage(
                format!("'{}' is given twice", flag.name),
                "give it once".to_string(),
            ));
        }
        let value = match (flag.value, inline) {
            (Some(_), Some(value)) => Some(value),
            (Some(what), None) => match it.next() {
                Some(value) => Some(value.clone()),
                None => {
                    return Err(usage(
                        format!("'{}' needs a value", flag.name),
                        format!("write '{} {}'", flag.name, what),
                    ))
                }
            },
            (None, Some(_)) => {
                return Err(usage(
                    format!("'{}' takes no value", flag.name),
                    format!("write '{}' alone", flag.name),
                ))
            }
            (None, None) => None,
        };
        given.push((flag, value));
    }
    Ok((given, words))
}

/// The control address in the one form the client uses: `host:port`.
///
/// `http://host:port/` is accepted and reduced to it, because that is what a
/// person copies out of a browser. Anything else with a scheme or a path is
/// refused: there is no TLS and no path prefix to speak.
pub fn server_address(text: &str) -> Result<String, Usage> {
    let hint = format!(
        "give the server's --control-listen address as host:port, for example \
         '--server 127.0.0.1:8080' (or set {})",
        SERVER_VARIABLE
    );
    let bare = text.strip_prefix("http://").unwrap_or(text);
    let bare = bare.strip_suffix('/').unwrap_or(bare);
    let port = bare
        .rsplit_once(':')
        .map(|(host, port)| (host, port.parse::<u16>()));
    match port {
        Some((host, Ok(_)))
            if !host.is_empty()
                && !bare.contains('/')
                && !bare.chars().any(|c| c.is_whitespace() || c.is_control()) =>
        {
            Ok(bare.to_string())
        }
        _ => Err(usage(
            format!("'{}' is not a host:port control address", text),
            hint,
        )),
    }
}

fn volume(what: &str, text: &str, line: &str) -> Result<Volume, Usage> {
    Volume::parse(text).ok_or_else(|| {
        usage(
            format!(
                "'{}' is not a {}: it is 0 to 1 with at most three decimals, for example 0.350",
                text, what
            ),
            line.to_string(),
        )
    })
}

fn step(text: &str, line: &str) -> Result<i32, Usage> {
    match text.parse::<i32>() {
        Ok(step) if (-1000..=1000).contains(&step) => Ok(step),
        _ => Err(usage(
            format!(
                "'{}' is not a step: it is whole thousandths, -1000 to 1000, for example -50",
                text
            ),
            line.to_string(),
        )),
    }
}

/// Exactly `N` operands, or a usage error that says what the verb takes.
fn exactly<const N: usize>(operands: &[String], line: &str) -> Result<[String; N], Usage> {
    <[String; N]>::try_from(operands.to_vec()).map_err(|_| {
        usage(
            format!(
                "{} operand{} given where {} {} expected",
                operands.len(),
                if operands.len() == 1 { "" } else { "s" },
                N,
                if N == 1 { "is" } else { "are" }
            ),
            line.to_string(),
        )
    })
}

/// The room or `--group` a volume verb names, and the operands after it.
fn target(
    given: &mut Given,
    operands: &[String],
    line: &str,
) -> Result<(Target, Vec<String>), Usage> {
    if let Some(group) = take(given, "--group") {
        return Ok((Target::Group(group.unwrap_or_default()), operands.to_vec()));
    }
    match operands.split_first() {
        Some((room, rest)) => Ok((Target::Room(room.clone()), rest.to_vec())),
        None => Err(usage(
            "no room and no --group is named".to_string(),
            line.to_string(),
        )),
    }
}

fn build(
    noun: &Noun,
    verb: &Verb,
    given: &mut Given,
    operands: &[String],
) -> Result<Action, Usage> {
    let line = format!("usage: {}", usage_line(noun, verb));
    let line = line.as_str();
    let send = |command: Command, shows: View| Ok(Action::Send { command, shows });
    match (noun.name, verb.name) {
        ("rooms", "list") => exactly::<0>(operands, line).map(|_| Action::Read(View::Rooms)),
        ("rooms", "show") => {
            let [room] = exactly(operands, line)?;
            Ok(Action::Read(View::Room(room)))
        }
        ("rooms", "name") => {
            let [zone, name] = exactly(operands, line)?;
            let shows = View::Room(zone.clone());
            send(Command::Name { zone, name }, shows)
        }
        ("groups", "list") => exactly::<0>(operands, line).map(|_| Action::Read(View::Groups)),
        ("groups", "join") => {
            let [zone, target] = exactly(operands, line)?;
            send(Command::Join { zone, target }, View::Groups)
        }
        ("groups", "leave") => {
            let [zone] = exactly(operands, line)?;
            send(Command::Ungroup { zone }, View::Groups)
        }
        ("groups", "save") => match operands {
            [group, name, zones @ ..] if zones.len() >= 2 => send(
                Command::GroupSave {
                    group: group.clone(),
                    name: name.clone(),
                    zones: zones.to_vec(),
                },
                View::Groups,
            ),
            _ => Err(usage(
                "a saved group is an id, a name and two or more rooms".to_string(),
                line.to_string(),
            )),
        },
        ("groups", "delete") => {
            let [group] = exactly(operands, line)?;
            send(Command::GroupDelete { group }, View::Groups)
        }
        ("groups", "take") => {
            let [target] = exactly(operands, line)?;
            let source = match take(given, "--source") {
                Some(text) => {
                    let text = text.unwrap_or_default();
                    Some(Source::parse(&text).ok_or_else(|| {
                        usage(
                            format!(
                                "'{}' is not a source: it is stream, none, \
                                 line-in:<endpoint>/<input>, chime:<name> or player:<id>",
                                text
                            ),
                            line.to_string(),
                        )
                    })?)
                }
                None => None,
            };
            send(Command::Take { target, source }, View::Groups)
        }
        ("volume", "get") => {
            let (target, rest) = target(given, operands, line)?;
            exactly::<0>(&rest, line)?;
            Ok(Action::Read(View::Volume(target)))
        }
        ("volume", "set") => {
            let (target, rest) = target(given, operands, line)?;
            let [text] = exactly(&rest, line)?;
            let volume = volume("volume", &text, line)?;
            let command = match target.clone() {
                Target::Room(zone) => Command::Volume { zone, volume },
                Target::Group(group) => Command::GroupVolume { group, volume },
            };
            send(command, View::Volume(target))
        }
        ("volume", "step") => {
            let (target, rest) = target(given, operands, line)?;
            let [text] = exactly(&rest, line)?;
            let step = step(&text, line)?;
            let command = match target.clone() {
                Target::Room(zone) => Command::VolumeStep { zone, step },
                Target::Group(group) => Command::GroupVolumeStep { group, step },
            };
            send(command, View::Volume(target))
        }
        ("volume", "mute") | ("volume", "unmute") => {
            let [zone] = exactly(operands, line)?;
            let shows = View::Volume(Target::Room(zone.clone()));
            let muted = verb.name == "mute";
            send(Command::Mute { zone, muted }, shows)
        }
        ("volume", "limit") => {
            let [zone, text] = exactly(operands, line)?;
            let limit = volume("limit", &text, line)?;
            let shows = View::Volume(Target::Room(zone.clone()));
            send(Command::Limit { zone, limit }, shows)
        }
        ("inputs", "list") => exactly::<0>(operands, line).map(|_| Action::Read(View::Inputs)),
        ("inputs", "select") => {
            let [input, target] = exactly(operands, line)?;
            let input = InputId::parse(&input).ok_or_else(|| {
                usage(
                    format!(
                        "'{}' is not an input: it is <endpoint>/<input>, as 'chorusctl inputs \
                         list' prints it",
                        input
                    ),
                    line.to_string(),
                )
            })?;
            let source = Some(Source::LineIn(input));
            send(Command::Take { target, source }, View::Groups)
        }
        ("inputs", "labels") => {
            exactly::<0>(operands, line).map(|_| Action::Read(View::InputLabels))
        }
        ("inputs", "label") | ("inputs", "unlabel") => {
            let (input, role, name) = if verb.name == "label" {
                let [input, role, name] = exactly(operands, line)?;
                let role = InputRole::parse(&role).ok_or_else(|| {
                    usage(
                        format!("'{}' is not a role: it is line-in or streamer", role),
                        line.to_string(),
                    )
                })?;
                if name.is_empty() {
                    return Err(usage(
                        "a label has a name; 'chorusctl inputs unlabel' removes one".to_string(),
                        line.to_string(),
                    ));
                }
                (input, role, name)
            } else {
                let [input] = exactly(operands, line)?;
                (input, InputRole::LineIn, String::new())
            };
            let input = InputId::parse(&input).ok_or_else(|| {
                usage(
                    format!(
                        "'{}' is not an input: it is <endpoint>/<input>, as 'chorusctl inputs \
                         list' prints it",
                        input
                    ),
                    line.to_string(),
                )
            })?;
            send(
                Command::InputLabel(InputLabel { input, name, role }),
                View::InputLabels,
            )
        }
        ("sources", "list") => exactly::<0>(operands, line).map(|_| Action::Read(View::Sources)),
        ("sources", "store") => {
            let [id, kind, name, value] = exactly(operands, line)?;
            let kind = StoredKind::parse(&kind).ok_or_else(|| {
                usage(
                    format!(
                        "'{}' is not a kind of stored source: it is url or spotify",
                        kind
                    ),
                    line.to_string(),
                )
            })?;
            if let Some(problem) = StoredSource::value_problem(kind, &value) {
                return Err(usage(problem, line.to_string()));
            }
            send(
                Command::SourceStore(StoredSource {
                    id,
                    kind,
                    value,
                    name,
                }),
                View::Sources,
            )
        }
        ("sources", "forget") => {
            let [id] = exactly(operands, line)?;
            send(Command::SourceForget { id }, View::Sources)
        }
        ("endpoints", "list") => {
            exactly::<0>(operands, line).map(|_| Action::Read(View::Endpoints))
        }
        ("endpoints", "show") => {
            let [id] = exactly(operands, line)?;
            Ok(Action::Read(View::Endpoint(id)))
        }
        ("endpoints", "name") => {
            let [speaker, name] = exactly(operands, line)?;
            let shows = View::Endpoint(speaker.clone());
            send(Command::SpeakerName { speaker, name }, shows)
        }
        ("endpoints", "room") => {
            let none = take(given, "--none").is_some();
            let (speaker, room) = match (none, operands) {
                (true, [speaker]) => (speaker.clone(), None),
                (false, [speaker, room]) => (speaker.clone(), Some(room.clone())),
                _ => {
                    return Err(usage(
                        "a speaker goes to one room, or to none with --none".to_string(),
                        line.to_string(),
                    ))
                }
            };
            let shows = View::Endpoint(speaker.clone());
            send(Command::SpeakerRoom { speaker, room }, shows)
        }
        ("endpoints", "forget") => {
            let [speaker] = exactly(operands, line)?;
            send(Command::SpeakerForget { speaker }, View::Endpoints)
        }
        ("soloist", "status") => exactly::<0>(operands, line).map(|_| Action::Read(View::Soloist)),
        ("soloist", "restart") => {
            exactly::<0>(operands, line)?;
            send(Command::SoloistRestart, View::Soloist)
        }
        ("soloist", "pause" | "resume" | "next" | "previous") => {
            let [target] = exactly(operands, line)?;
            let action = PlaybackAction::parse(verb.name).expect("the verbs are the actions");
            send(Command::Playback { target, action }, View::Soloist)
        }
        ("updates", "list") => exactly::<0>(operands, line).map(|_| Action::Read(View::Images)),
        ("updates", "status") => exactly::<0>(operands, line).map(|_| Action::Read(View::Updates)),
        ("updates", "install") => {
            let all = take(given, "--all").is_some();
            let force = take(given, "--force").is_some();
            let (speaker, image) = match (all, operands) {
                (true, [image]) => (None, image.clone()),
                (false, [speaker, image]) => (Some(speaker.clone()), image.clone()),
                _ => {
                    return Err(usage(
                        "an install names one speaker and an image, or --all and an image"
                            .to_string(),
                        line.to_string(),
                    ))
                }
            };
            send(
                Command::FirmwareInstall {
                    speaker,
                    image,
                    force,
                },
                View::Updates,
            )
        }
        ("updates", "cancel") => {
            let [speaker] = exactly(operands, line)?;
            send(Command::FirmwareCancel { speaker }, View::Updates)
        }
        ("updates", "rescan") => {
            exactly::<0>(operands, line)?;
            send(Command::FirmwareRescan, View::Images)
        }
        // The table and this match are held together by the test
        // `every_verb_in_the_table_parses_with_its_example`.
        _ => Err(usage(
            format!("'{} {}' is not implemented", noun.name, verb.name),
            format!("see 'chorusctl {} --help'", noun.name),
        )),
    }
}

/// Read a command line (without the program's name).
pub fn parse(args: &[String]) -> Result<Invocation, Usage> {
    let (mut given, words) = split(args)?;
    let json = take(&mut given, "--json").is_some();
    let help = take(&mut given, "--help").is_some();
    let server = match take(&mut given, "--server") {
        Some(text) => Some(server_address(&text.unwrap_or_default())?),
        None => None,
    };
    let timeout = match take(&mut given, "--timeout") {
        Some(text) => {
            let text = text.unwrap_or_default();
            match text.parse::<u64>() {
                Ok(seconds) if (1..=60).contains(&seconds) => Duration::from_secs(seconds),
                _ => {
                    return Err(usage(
                        format!("'{}' is not a timeout: it is whole seconds, 1 to 60", text),
                        "for example '--timeout 10'".to_string(),
                    ))
                }
            }
        }
        None => Duration::from_secs(DEFAULT_TIMEOUT_S),
    };
    let invocation = |action| Invocation {
        server: server.clone(),
        json,
        timeout,
        action,
    };

    let mut words = words.as_slice();
    let asked_help = help || words.first().is_some_and(|w| w == "help");
    if words.first().is_some_and(|w| w == "help") {
        words = &words[1..];
    }
    let Some(noun_word) = words.first() else {
        if asked_help {
            return Ok(invocation(Action::Help(grammar::global_help())));
        }
        return Err(usage(
            "no command is given".to_string(),
            "'chorusctl --help' lists every noun and verb".to_string(),
        ));
    };
    let nouns = || NOUNS.iter().map(|n| n.name);
    let Some(noun) = grammar::noun(noun_word) else {
        return Err(usage(
            format!("unknown noun '{}'", noun_word),
            format!(
                "did you mean '{}'? The nouns are {}",
                closest(noun_word, nouns()).unwrap_or("rooms"),
                nouns().collect::<Vec<_>>().join(", ")
            ),
        ));
    };
    if asked_help {
        return Ok(invocation(Action::Help(grammar::noun_help(noun))));
    }
    let verbs = || noun.verbs.iter().map(|v| v.name);
    let listed = verbs().collect::<Vec<_>>().join(", ");
    let Some(verb_word) = words.get(1) else {
        return Err(usage(
            format!("'{}' needs a verb", noun.name),
            format!(
                "its verbs are {}; see 'chorusctl {} --help'",
                listed, noun.name
            ),
        ));
    };
    let Some(verb) = noun.verbs.iter().find(|v| v.name == verb_word) else {
        return Err(usage(
            format!("unknown verb '{}' for '{}'", verb_word, noun.name),
            format!(
                "did you mean '{}'? Its verbs are {}",
                closest(verb_word, verbs()).unwrap_or("list"),
                listed
            ),
        ));
    };
    if let Some((flag, _)) = given.iter().find(|(f, _)| !verb.flags.contains(&f.name)) {
        return Err(usage(
            format!(
                "'{}' does not apply to '{} {}'",
                flag.name, noun.name, verb.name
            ),
            format!("usage: {}", usage_line(noun, verb)),
        ));
    }
    let action = build(noun, verb, &mut given, &words[2..])?;
    Ok(invocation(action))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    fn sent(line: &str) -> String {
        match parse(&args(line))
            .unwrap_or_else(|e| panic!("{}: {:?}", line, e))
            .action
        {
            Action::Send { command, .. } => command.encode(),
            other => panic!("{} does not send: {:?}", line, other),
        }
    }

    fn refused(line: &str) -> Usage {
        parse(&args(line)).expect_err(line)
    }

    #[test]
    fn every_verb_in_the_table_parses_with_its_example() {
        for noun in NOUNS {
            for verb in noun.verbs {
                let line = format!("{} {} {}", noun.name, verb.name, verb.example);
                let parsed = parse(&args(&line)).unwrap_or_else(|e| panic!("{}: {:?}", line, e));
                match parsed.action {
                    Action::Send { command, .. } => assert!(
                        verb.sends.contains(&command.type_name()),
                        "{} sends {}, which its table row does not name",
                        line,
                        command.type_name()
                    ),
                    Action::Read(_) => assert!(verb.sends.is_empty(), "{} only reads", line),
                    Action::Help(_) => panic!("{} printed the help", line),
                }
            }
        }
    }

    #[test]
    fn flags_stand_anywhere_and_take_either_spelling() {
        let a = parse(&args("--server 127.0.0.1:8080 --json rooms list")).unwrap();
        let b = parse(&args("rooms --json list --server=127.0.0.1:8080")).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.server.as_deref(), Some("127.0.0.1:8080"));
        assert!(a.json);
        assert_eq!(a.timeout, Duration::from_secs(DEFAULT_TIMEOUT_S));
        let c = parse(&args(
            "--timeout=9 --server http://127.0.0.1:8080/ rooms list",
        ))
        .unwrap();
        assert_eq!(c.server.as_deref(), Some("127.0.0.1:8080"));
        assert_eq!(c.timeout, Duration::from_secs(9));
    }

    #[test]
    fn a_negative_step_and_a_word_after_two_dashes_are_operands() {
        assert_eq!(
            sent("volume step kitchen -50"),
            r#"{"v":2,"t":"volume_step","zone":"kitchen","step":-50}"#
        );
        assert_eq!(
            sent("rooms name kitchen -- --quiet"),
            r#"{"v":1,"t":"name","zone":"kitchen","name":"--quiet"}"#
        );
    }

    #[test]
    fn unmute_is_mute_false_and_none_is_null() {
        assert_eq!(
            sent("volume unmute kitchen"),
            r#"{"v":1,"t":"mute","zone":"kitchen","muted":false}"#
        );
        assert_eq!(
            sent("endpoints room chorus-0123456789ab --none"),
            r#"{"v":2,"t":"speaker_room","speaker":"chorus-0123456789ab","room":null}"#
        );
    }

    #[test]
    fn an_unknown_flag_noun_or_verb_names_the_closest_valid_one() {
        let e = refused("--jsn rooms list");
        assert!(e.detail.contains("unknown flag '--jsn'"), "{:?}", e);
        assert!(e.hint.contains("did you mean '--json'"), "{:?}", e);
        let e = refused("-j rooms list");
        assert!(e.detail.contains("unknown flag '-j'"), "{:?}", e);
        let e = refused("room list");
        assert!(e.detail.contains("unknown noun 'room'"), "{:?}", e);
        assert!(e.hint.contains("did you mean 'rooms'"), "{:?}", e);
        let e = refused("updates instal x y");
        assert!(
            e.detail.contains("unknown verb 'instal' for 'updates'"),
            "{:?}",
            e
        );
        assert!(e.hint.contains("did you mean 'install'"), "{:?}", e);
    }

    #[test]
    fn what_is_not_a_valid_command_line_is_refused_before_anything_is_sent() {
        for line in [
            "",
            "rooms",
            "rooms list extra",
            "rooms show",
            "rooms name kitchen",
            "rooms list --force",
            "rooms list --json --json",
            "rooms list --json=yes",
            "rooms list --server",
            "rooms list --server nowhere",
            "rooms list --server https://127.0.0.1:8080",
            "rooms list --server 127.0.0.1:8080/api",
            "rooms list --timeout 0",
            "rooms list --timeout soon",
            "groups save downstairs Downstairs kitchen",
            "groups take kitchen --source radio",
            "volume get",
            "volume set kitchen",
            "volume set kitchen 1.5",
            "volume set kitchen 0.1234",
            "volume set kitchen loud",
            "volume set --group downstairs kitchen 0.4",
            "volume step kitchen 1001",
            "volume step kitchen 0.5",
            "volume mute --group downstairs",
            "volume limit kitchen",
            "inputs select line-1 kitchen",
            "inputs label line-1 streamer Streamer",
            "inputs label endpoint-c/line-1 microphone Mic",
            "inputs unlabel endpoint-c/line-1 now",
            "sources store radio ftp Radio ftp://radio.example/a",
            "sources store radio url Radio file:///etc/passwd",
            "sources store wake spotify Wake https://open.spotify.example/playlist/a",
            "sources forget",
            "endpoints room chorus-0123456789ab",
            "endpoints room chorus-0123456789ab kitchen --none",
            "updates install brick-2-0-0",
            "updates install --all chorus-0123456789ab brick-2-0-0",
            "updates rescan now",
            "help nothing",
        ] {
            let e = refused(line);
            assert!(
                !e.detail.is_empty() && !e.hint.is_empty(),
                "{}: {:?}",
                line,
                e
            );
        }
    }

    #[test]
    fn help_is_global_or_one_nouns() {
        for line in ["--help", "-h", "help"] {
            match parse(&args(line)).unwrap().action {
                Action::Help(text) => assert_eq!(text, grammar::global_help()),
                other => panic!("{}: {:?}", line, other),
            }
        }
        for line in [
            "volume --help",
            "help volume",
            "volume set -h",
            "--help volume",
        ] {
            match parse(&args(line)).unwrap().action {
                Action::Help(text) => {
                    assert_eq!(text, grammar::noun_help(grammar::noun("volume").unwrap()))
                }
                other => panic!("{}: {:?}", line, other),
            }
        }
    }
}
