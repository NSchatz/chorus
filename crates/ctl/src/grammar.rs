//! The one table: every noun, verb, flag and exit code `chorusctl` has.
//!
//! The parser looks commands up here, `--help` (global and per noun) is
//! printed from here, and the tests walk it: every verb's example parses, and
//! every catalog message a verb says it sends is held to that message's
//! committed vector under `fixtures/control`.

/// A flag, global or a verb's own.
#[derive(Debug)]
pub struct Flag {
    /// Its spelling, with the two dashes.
    pub name: &'static str,
    /// What its value is called in the help, or `None` for a flag with none.
    pub value: Option<&'static str>,
    /// One line for the help.
    pub about: &'static str,
    /// Whether it applies to every command.
    pub global: bool,
}

/// A verb of a noun.
#[derive(Debug)]
pub struct Verb {
    /// Its name.
    pub name: &'static str,
    /// Its operands and its own flags, as the help writes them.
    pub operands: &'static str,
    /// The non-global flags it accepts.
    pub flags: &'static [&'static str],
    /// One line for the help.
    pub about: &'static str,
    /// The catalog message types it can send (empty: it only reads the state).
    pub sends: &'static [&'static str],
    /// Arguments that make a valid invocation, for the help and the tests.
    pub example: &'static str,
}

/// A noun: one area of the control API.
#[derive(Debug)]
pub struct Noun {
    /// Its name.
    pub name: &'static str,
    /// One line for the help.
    pub about: &'static str,
    /// Its verbs.
    pub verbs: &'static [Verb],
}

/// An exit code and what it means.
#[derive(Debug)]
pub struct ExitCode {
    /// The code.
    pub code: u8,
    /// Its short name, which is also the `error` member of a `--json` error.
    pub name: &'static str,
    /// What it means.
    pub about: &'static str,
}

/// The command applied, or the state was read and printed.
pub const EXIT_OK: u8 = 0;
/// The command line is not one `chorusctl` understands; nothing was sent.
pub const EXIT_USAGE: u8 = 1;
/// The server could not be reached, or what answered is not a chorus server.
pub const EXIT_UNREACHABLE: u8 = 2;
/// The server answered and refused the command.
pub const EXIT_REFUSED: u8 = 3;
/// The server answered, and the thing a read verb named is not in its state.
pub const EXIT_NOT_FOUND: u8 = 4;

/// Every exit code, which is the contract a script grades.
pub const EXIT_CODES: &[ExitCode] = &[
    ExitCode {
        code: EXIT_OK,
        name: "ok",
        about: "the command was applied, or the state was read and printed",
    },
    ExitCode {
        code: EXIT_USAGE,
        name: "usage",
        about: "the command line is not valid; nothing was sent",
    },
    ExitCode {
        code: EXIT_UNREACHABLE,
        name: "unreachable",
        about: "no answer from a chorus server: connect, timeout or a bad HTTP answer",
    },
    ExitCode {
        code: EXIT_REFUSED,
        name: "refused",
        about: "the server answered and refused (its `error` or `refused` message)",
    },
    ExitCode {
        code: EXIT_NOT_FOUND,
        name: "not-found",
        about: "the server answered; what a read verb named is not in its state",
    },
];

/// The variable that supplies `--server` when the flag is absent.
pub const SERVER_VARIABLE: &str = "CHORUS_SERVER";

/// The default of `--timeout`, in seconds: the server's own bound on a whole
/// request (`REQUEST_DEADLINE` in `crates/server/src/control.rs`).
pub const DEFAULT_TIMEOUT_S: u64 = 5;

/// Every flag.
pub const FLAGS: &[Flag] = &[
    Flag {
        name: "--server",
        value: Some("<host:port>"),
        about: "the server's control address (its --control-listen); default: $CHORUS_SERVER",
        global: true,
    },
    Flag {
        name: "--json",
        value: None,
        about: "print the server's own JSON; an error is one JSON object on stderr",
        global: true,
    },
    Flag {
        name: "--timeout",
        value: Some("<seconds>"),
        about: "how long to wait for the server, 1 to 60; default 5",
        global: true,
    },
    Flag {
        name: "--help",
        value: None,
        about: "this text; after a noun, that noun's verbs (also -h, and 'help <noun>')",
        global: true,
    },
    Flag {
        name: "--group",
        value: Some("<group>"),
        about: "volume get, set, step: act on a group instead of a room",
        global: false,
    },
    Flag {
        name: "--source",
        value: Some("<source>"),
        about: "groups take: stream, none, line-in:<endpoint>/<input>, chime:<name> or player:<id>",
        global: false,
    },
    Flag {
        name: "--none",
        value: None,
        about: "endpoints room: take the speaker out of every room",
        global: false,
    },
    Flag {
        name: "--all",
        value: None,
        about: "updates install: every present speaker of the image's board not running it",
        global: false,
    },
    Flag {
        name: "--force",
        value: None,
        about: "updates install: install even the version the speaker already runs",
        global: false,
    },
];

/// Every noun with its verbs.
pub const NOUNS: &[Noun] = &[
    Noun {
        name: "rooms",
        about: "the rooms (the catalog's zones)",
        verbs: &[
            Verb {
                name: "list",
                operands: "",
                flags: &[],
                about: "every room: group, volume, mute, limit, speakers present",
                sends: &[],
                example: "",
            },
            Verb {
                name: "show",
                operands: "<room>",
                flags: &[],
                about: "one room in full",
                sends: &[],
                example: "kitchen",
            },
            Verb {
                name: "name",
                operands: "<room> <name>",
                flags: &[],
                about: "give a room its display name",
                sends: &["name"],
                example: "kitchen Kitchen",
            },
        ],
    },
    Noun {
        name: "groups",
        about: "what plays together: live groups and saved groups",
        verbs: &[
            Verb {
                name: "list",
                operands: "",
                flags: &[],
                about: "the groups that exist now, and the saved groups",
                sends: &[],
                example: "",
            },
            Verb {
                name: "join",
                operands: "<room> <target>",
                flags: &[],
                about: "put a room into the group a room or a group is in",
                sends: &["join"],
                example: "kitchen study",
            },
            Verb {
                name: "leave",
                operands: "<room>",
                flags: &[],
                about: "take a room out of its group; it plays alone",
                sends: &["ungroup"],
                example: "kitchen",
            },
            Verb {
                name: "save",
                operands: "<group> <name> <room> <room>...",
                flags: &[],
                about: "save, or replace, a named group of two or more rooms",
                sends: &["group_save"],
                example: "downstairs Downstairs kitchen living",
            },
            Verb {
                name: "delete",
                operands: "<group>",
                flags: &[],
                about: "forget a saved group; its rooms stay where they are",
                sends: &["group_delete"],
                example: "downstairs",
            },
            Verb {
                name: "take",
                operands: "<target> [--source <source>]",
                flags: &["--source"],
                about: "play in a room, a saved group or a live group, alone",
                sends: &["take"],
                example: "downstairs --source stream",
            },
        ],
    },
    Noun {
        name: "volume",
        about: "volume, mute and limit of a room, or the volume of a group",
        verbs: &[
            Verb {
                name: "get",
                operands: "<room> | --group <group>",
                flags: &["--group"],
                about: "the volume now",
                sends: &[],
                example: "kitchen",
            },
            Verb {
                name: "set",
                operands: "(<room> | --group <group>) <volume>",
                flags: &["--group"],
                about: "set it: 0 to 1, at most three decimals (0.350)",
                sends: &["volume", "group_volume"],
                example: "kitchen 0.350",
            },
            Verb {
                name: "step",
                operands: "(<room> | --group <group>) <step>",
                flags: &["--group"],
                about: "move it by signed thousandths, -1000 to 1000 (-50)",
                sends: &["volume_step", "group_volume_step"],
                example: "kitchen -50",
            },
            Verb {
                name: "mute",
                operands: "<room>",
                flags: &[],
                about: "mute a room",
                sends: &["mute"],
                example: "kitchen",
            },
            Verb {
                name: "unmute",
                operands: "<room>",
                flags: &[],
                about: "unmute a room",
                sends: &["mute"],
                example: "kitchen",
            },
            Verb {
                name: "limit",
                operands: "<room> <limit>",
                flags: &[],
                about: "set a room's maximum volume, written as a volume",
                sends: &["limit"],
                example: "kitchen 0.600",
            },
        ],
    },
    Noun {
        name: "inputs",
        about: "the line inputs endpoints offer",
        verbs: &[
            Verb {
                name: "list",
                operands: "",
                flags: &[],
                about: "every input offered now, and the groups playing it",
                sends: &[],
                example: "",
            },
            Verb {
                name: "select",
                operands: "<input> <target>",
                flags: &[],
                about: "play an input (<endpoint>/<input>) in a room or a group",
                sends: &["take"],
                example: "endpoint-c/line-1 downstairs",
            },
        ],
    },
    Noun {
        name: "endpoints",
        about: "the adopted speakers and the endpoints attached to rooms",
        verbs: &[
            Verb {
                name: "list",
                operands: "",
                flags: &[],
                about: "every speaker and endpoint, and any changed key",
                sends: &[],
                example: "",
            },
            Verb {
                name: "show",
                operands: "<id>",
                flags: &[],
                about: "one speaker or endpoint in full",
                sends: &[],
                example: "chorus-0123456789ab",
            },
            Verb {
                name: "name",
                operands: "<speaker> <name>",
                flags: &[],
                about: "name an adopted speaker",
                sends: &["speaker_name"],
                example: "chorus-0123456789ab Kitchen-left",
            },
            Verb {
                name: "room",
                operands: "<speaker> (<room> | --none)",
                flags: &["--none"],
                about: "assign a speaker to a room, or to none",
                sends: &["speaker_room"],
                example: "chorus-0123456789ab kitchen",
            },
            Verb {
                name: "forget",
                operands: "<speaker>",
                flags: &[],
                about: "forget a speaker and its pinned key; it is adopted afresh",
                sends: &["speaker_forget"],
                example: "chorus-0123456789ab",
            },
        ],
    },
    Noun {
        name: "updates",
        about: "firmware images staged on the server, and installs",
        verbs: &[
            Verb {
                name: "list",
                operands: "",
                flags: &[],
                about: "the staged images and the server's verdict on each",
                sends: &[],
                example: "",
            },
            Verb {
                name: "status",
                operands: "",
                flags: &[],
                about: "what each speaker runs, and its install's progress",
                sends: &[],
                example: "",
            },
            Verb {
                name: "install",
                operands: "(<speaker> | --all) <image> [--force]",
                flags: &["--all", "--force"],
                about: "install a staged image, if the server allows it",
                sends: &["firmware_install"],
                example: "chorus-0123456789ab brick-2-0-0",
            },
            Verb {
                name: "cancel",
                operands: "<speaker>",
                flags: &[],
                about: "abandon a speaker's install that is not yet verified",
                sends: &["firmware_cancel"],
                example: "chorus-0123456789ab",
            },
            Verb {
                name: "rescan",
                operands: "",
                flags: &[],
                about: "have the server read its firmware directory again",
                sends: &["firmware_rescan"],
                example: "",
            },
        ],
    },
];

/// The noun with this name.
pub fn noun(name: &str) -> Option<&'static Noun> {
    NOUNS.iter().find(|n| n.name == name)
}

/// The flag with this spelling.
pub fn flag(name: &str) -> Option<&'static Flag> {
    FLAGS.iter().find(|f| f.name == name)
}

/// The usage line of one verb, as the help and a usage error write it.
pub fn usage_line(noun: &Noun, verb: &Verb) -> String {
    let mut line = format!("chorusctl {} {}", noun.name, verb.name);
    if !verb.operands.is_empty() {
        line.push(' ');
        line.push_str(verb.operands);
    }
    line
}

const USAGE: &str =
    "usage: chorusctl [--server <host:port>] [--json] [--timeout <seconds>] <noun> <verb> [args]";

fn verb_lines(out: &mut String, noun: &Noun, indent: &str) {
    let width = noun
        .verbs
        .iter()
        .map(|v| v.name.len() + 1 + v.operands.len())
        .max()
        .unwrap_or(0);
    for verb in noun.verbs {
        let left = format!("{} {}", verb.name, verb.operands);
        out.push_str(&format!(
            "{}{:<width$}  {}\n",
            indent,
            left.trim_end(),
            verb.about,
            width = width
        ));
    }
}

fn flag_lines(out: &mut String, flags: &[&Flag]) {
    let spelled: Vec<String> = flags
        .iter()
        .map(|f| match f.value {
            Some(value) => format!("{} {}", f.name, value),
            None => f.name.to_string(),
        })
        .collect();
    let width = spelled.iter().map(String::len).max().unwrap_or(0);
    for (flag, left) in flags.iter().zip(&spelled) {
        out.push_str(&format!(
            "  {:<width$}  {}\n",
            left,
            flag.about,
            width = width
        ));
    }
}

fn exit_lines(out: &mut String) {
    out.push_str("\nexit codes:\n");
    for exit in EXIT_CODES {
        out.push_str(&format!("  {}  {}: {}\n", exit.code, exit.name, exit.about));
    }
}

/// `chorusctl --help`: every noun and verb, the flags and the exit codes.
pub fn global_help() -> String {
    let mut out = String::new();
    out.push_str(USAGE);
    out.push_str("\n\nchorusctl drives a chorus server through its control API.\n");
    for noun in NOUNS {
        out.push_str(&format!("\n{}: {}\n", noun.name, noun.about));
        verb_lines(&mut out, noun, "  ");
    }
    out.push_str("\nflags:\n");
    flag_lines(&mut out, &FLAGS.iter().collect::<Vec<_>>());
    exit_lines(&mut out);
    out.push_str("\nThe grammar, the --json shapes and the exit codes: docs/chorusctl.md\n");
    out
}

/// `chorusctl <noun> --help`: that noun's verbs, each with an example.
pub fn noun_help(noun: &Noun) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "usage: chorusctl [flags] {} <verb> [args]\n\n{}: {}\n",
        noun.name, noun.name, noun.about
    ));
    verb_lines(&mut out, noun, "  ");
    out.push_str("\nexamples:\n");
    for verb in noun.verbs {
        let line = format!("chorusctl {} {} {}", noun.name, verb.name, verb.example);
        out.push_str(&format!("  {}\n", line.trim_end()));
    }
    let own: Vec<&Flag> = FLAGS
        .iter()
        .filter(|f| f.global || noun.verbs.iter().any(|v| v.flags.contains(&f.name)))
        .collect();
    out.push_str("\nflags:\n");
    flag_lines(&mut out, &own);
    exit_lines(&mut out);
    out
}

/// The candidate closest to `word` by edit distance, the first on a tie.
pub fn closest<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    candidates
        .into_iter()
        .map(|c| (distance(word, c), c))
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

/// Levenshtein distance over characters.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitute = diagonal + usize::from(ca != cb);
            diagonal = row[j + 1];
            row[j + 1] = substitute.min(row[j] + 1).min(diagonal + 1);
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_closest_candidate_is_named() {
        assert_eq!(closest("lst", ["list", "show", "name"]), Some("list"));
        assert_eq!(
            closest("--jsn", FLAGS.iter().map(|f| f.name)),
            Some("--json")
        );
        assert_eq!(closest("room", NOUNS.iter().map(|n| n.name)), Some("rooms"));
        assert_eq!(distance("kitten", "sitting"), 3);
    }

    #[test]
    fn every_flag_a_verb_names_is_a_flag_that_is_not_global() {
        for noun in NOUNS {
            for verb in noun.verbs {
                for name in verb.flags {
                    let flag = flag(name).unwrap_or_else(|| panic!("{} is in FLAGS", name));
                    assert!(!flag.global, "{} is global", name);
                }
            }
        }
    }

    #[test]
    fn the_help_names_every_noun_verb_flag_and_exit_code() {
        let help = global_help();
        for noun in NOUNS {
            assert!(help.contains(&format!("\n{}: ", noun.name)));
            let own = noun_help(noun);
            for verb in noun.verbs {
                let left = format!("  {} {}", verb.name, verb.operands);
                assert!(help.contains(left.trim_end()), "{}", left);
                assert!(own.contains(left.trim_end()), "{}", left);
            }
        }
        for flag in FLAGS {
            assert!(help.contains(flag.name));
        }
        for exit in EXIT_CODES {
            assert!(help.contains(&format!("  {}  {}: ", exit.code, exit.name)));
        }
    }
}
