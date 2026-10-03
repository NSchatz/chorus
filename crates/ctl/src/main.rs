//! `chorusctl`: the command line over the chorus control API.
//!
//! Everything is in the library (`chorus_ctl::run`), so the tests drive exactly
//! what this binary runs. The grammar, the `--json` shapes and the exit codes
//! are in the library's module documentation, in `chorusctl --help` and in
//! `docs/chorusctl.md`.

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = chorus_ctl::run(&args);
    // A closed pipe (`chorusctl rooms list | head -1`) is not this program's
    // failure: the exit code still says what the server answered.
    let _ = std::io::stdout().write_all(outcome.stdout.as_bytes());
    let _ = std::io::stderr().write_all(outcome.stderr.as_bytes());
    ExitCode::from(outcome.code)
}
