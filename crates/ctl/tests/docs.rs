//! `docs/chorusctl.md` and the program say the same thing: the page carries
//! `chorusctl --help` verbatim, a row for every verb and every exit code.

mod support;

use chorus_ctl::grammar::{global_help, EXIT_CODES, NOUNS};

#[test]
fn the_page_carries_the_help_every_verb_and_every_exit_code() {
    let path = support::repository_root().join("docs/chorusctl.md");
    let page = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is committed and readable: {}", path.display(), e));
    assert!(
        page.contains(&format!("```text\n{}```\n", global_help())),
        "docs/chorusctl.md does not carry `chorusctl --help` as the program prints it"
    );
    for noun in NOUNS {
        assert!(
            page.contains(&format!("\n### {}\n", noun.name)),
            "{}",
            noun.name
        );
        for verb in noun.verbs {
            assert!(
                page.contains(&format!("\n| `{} {}", noun.name, verb.name)),
                "docs/chorusctl.md has no row for '{} {}'",
                noun.name,
                verb.name
            );
            for sends in verb.sends {
                assert!(page.contains(&format!("`{}`", sends)), "{}", sends);
            }
        }
    }
    for exit in EXIT_CODES {
        assert!(
            page.contains(&format!(
                "\n| {} | {} | {} |\n",
                exit.code, exit.name, exit.about
            )),
            "docs/chorusctl.md has no row for exit code {}",
            exit.code
        );
        assert!(
            page.contains(&format!(
                "{{\"error\":\"{}\",\"exit\":{},",
                exit.name, exit.code
            )) || exit.code == 0
        );
    }
}
