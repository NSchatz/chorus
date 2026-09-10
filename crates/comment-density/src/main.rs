// The comment-density gate: this repository, the record that declares its
// ceiling, then the demonstrations that prove the gate can go red.
//
//   cargo run -p chorus-comment-density -- gate     # or: make verify-comment-density
//
// NO NETWORK, no device, no privilege, and no require_* guard from tools/lib.sh,
// which is why tools/unrun-checks-are-visibly-unrun.sh has nothing to register.
// Exit codes are in lib.rs and in the Makefile target that invokes this.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use chorus_comment_density as density;
use density::count::Config;
use density::sweep::{tracked_rust_files, Sweep};
use density::Missing;

const DIRECTIVES: &str = "tools/comment-density-directives.txt";
const MARKERS: &str = "tools/comment-density-generated-markers.txt";
const RECORD: &str = "docs/comment-density-record.md";

fn main() {
    process::exit(run());
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut root = PathBuf::from(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("the crate sits two levels under the repository root"),
    );
    let mut command = "gate";
    let mut baseline = false;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--root" => match rest.next() {
                Some(value) => root = PathBuf::from(value),
                None => {
                    eprintln!("--root wants a directory");
                    return density::EXIT_MISSING_CAPABILITY;
                }
            },
            "--baseline" => baseline = true,
            "gate" | "report" | "record" => command = leak(arg),
            other => {
                eprintln!("unknown argument {other:?}; try gate, report or record");
                return density::EXIT_MISSING_CAPABILITY;
            }
        }
    }

    let cfg = match load_config(&root) {
        Ok(cfg) => cfg,
        Err(missing) => {
            eprint!("{}", missing.render());
            return density::EXIT_MISSING_CAPABILITY;
        }
    };
    let paths = match tracked_rust_files(&root) {
        Ok(paths) => paths,
        Err(missing) => {
            eprint!("{}", missing.render());
            return density::EXIT_MISSING_CAPABILITY;
        }
    };
    let sweep = Sweep::run(&root, paths, &cfg);

    match command {
        "record" => rewrite_record(&root, &sweep, &cfg, baseline),
        "report" => {
            print_report(&sweep, &cfg);
            sweep.verdict(&cfg).exit_code()
        }
        _ => gate(&root, &sweep, &cfg),
    }
}

fn gate(root: &Path, sweep: &Sweep, cfg: &Config) -> i32 {
    print_report(sweep, cfg);

    println!();
    println!("=== chorus: the record and the gate agree on what is enforced ================");
    println!();
    let disagreements = match fs::read_to_string(root.join(RECORD)) {
        Ok(text) => density::record::check(&density::record::parse(&text), sweep, cfg),
        Err(error) => vec![format!("{RECORD} could not be read ({error}), so nothing declares a ceiling")],
    };
    if disagreements.is_empty() {
        println!(
            "pass {RECORD} declares the {}% ceiling, the {}% warn band and the {}-line minimum \
             this gate enforces, and every row matches the tree",
            cfg.ceiling_percent, cfg.warn_percent, cfg.minimum_counted_lines
        );
    } else {
        for line in &disagreements {
            println!("DISAGREEMENT {line}");
        }
    }

    println!();
    println!("=== chorus: the committed demonstrations, each shown producing its failure ===");
    let (report, failures) = density::demos::scan(root, cfg);
    print!("{report}");
    for line in &failures {
        println!("FAIL {line}");
    }

    println!();
    let verdict = sweep.verdict(cfg).exit_code();
    if verdict != density::EXIT_OK {
        return verdict;
    }
    if !disagreements.is_empty() {
        return density::EXIT_RECORD_DISAGREES;
    }
    if !failures.is_empty() {
        return density::EXIT_DEMONSTRATION;
    }
    println!(
        "chorus: every tracked .rs file is at or under the {}% ceiling, and the gate that says so \
         was shown producing all {} of its failures",
        cfg.ceiling_percent,
        density::demos::TREES.len()
    );
    density::EXIT_OK
}

fn print_report(sweep: &Sweep, cfg: &Config) {
    println!();
    println!("=== chorus: comment density over every tracked .rs file =====================");
    println!();
    print!("{}", sweep.render(cfg));
}

fn rewrite_record(root: &Path, sweep: &Sweep, cfg: &Config, baseline: bool) -> i32 {
    let path = root.join(RECORD);
    let existing = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("{RECORD} could not be read ({error})");
            return density::EXIT_RECORD_DISAGREES;
        }
    };
    match density::record::render(&existing, sweep, cfg, baseline) {
        Ok(text) => match fs::write(&path, text) {
            Ok(()) => {
                println!("{RECORD} rewritten from the tree as it stands");
                density::EXIT_OK
            }
            Err(error) => {
                eprintln!("{RECORD} could not be written ({error})");
                density::EXIT_RECORD_DISAGREES
            }
        },
        Err(why) => {
            eprintln!("{RECORD} could not be rewritten: {why}");
            density::EXIT_RECORD_DISAGREES
        }
    }
}

// The directive prefixes and the generated-file markers are committed lists
// beside the check, not constants in it. An empty list is a legitimate state;
// an absent list is not, because a list that stopped being read and an empty
// one are the same green otherwise.
fn load_config(root: &Path) -> Result<Config, Missing> {
    let directives = read_list(root, DIRECTIVES)?;
    let markers = read_list(root, MARKERS)?;
    Ok(density::default_config(directives, markers))
}

fn read_list(root: &Path, relative: &str) -> Result<Vec<String>, Missing> {
    let text = fs::read_to_string(root.join(relative)).map_err(|error| Missing {
        criterion: "the comment-density ceiling over every tracked .rs file".to_string(),
        prerequisite: format!("the committed list at {relative}; it could not be read ({error})"),
        how: format!("commit {relative}, empty if the list is genuinely empty"),
    })?;
    Ok(text
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.to_string())
        .collect())
}

fn leak(arg: &str) -> &'static str {
    match arg {
        "report" => "report",
        "record" => "record",
        _ => "gate",
    }
}
