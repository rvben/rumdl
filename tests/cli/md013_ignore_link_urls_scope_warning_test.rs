//! `ignore-link-urls` forgives a line-length *violation*; it does not change how
//! reflow measures a line. Setting it alongside `reflow` therefore does not stop
//! reflow from rewriting a paragraph whose only over-length cause is a link URL,
//! which reads as the setting being ignored (see issue #749). These tests pin the
//! warning that surfaces the boundary, and pin that it stays quiet otherwise.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

const LONG_LINK_LINE: &str = "This is a piece of [text](https://example.com/lol/alskjdhflkajshdfljkahsdfljkhasdkjfhasdkjfhasdjklfhlkajsdhfkjlashdflkjashdflkjahdskfjlhasdjfha) that should be on a single line\n";

fn run_check(config: &str) -> (String, String) {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".rumdl.toml"), config).unwrap();
    fs::write(dir.path().join("test.md"), LONG_LINK_LINE).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn warns_when_ignore_link_urls_is_set_alongside_reflow() {
    let (_, stderr) = run_check("[MD013]\nreflow = true\nignore-link-urls = true\n");
    assert!(
        stderr.contains("[config warning]") && stderr.contains("MD013"),
        "expected an MD013 config warning on stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("ignore-link-urls") && stderr.contains("reflow"),
        "warning should name both settings, got:\n{stderr}"
    );
}

#[test]
fn warns_for_the_snake_case_spelling_too() {
    let (_, stderr) = run_check("[MD013]\nreflow = true\nignore_link_urls = true\n");
    assert!(
        stderr.contains("[config warning]"),
        "snake_case spelling must warn as well, got:\n{stderr}"
    );
}

#[test]
fn warns_for_the_deprecated_alias_too() {
    let (_, stderr) = run_check("[MD013]\nreflow = true\nsemantic-link-understanding = true\n");
    // Assert the scope warning specifically: the alias also (wrongly) produced an
    // "Unknown option" warning, which would make a bare `[config warning]` check
    // pass without the scope warning existing at all.
    assert!(
        stderr.contains("does not affect 'reflow'"),
        "deprecated alias must produce the scope warning, got:\n{stderr}"
    );
}

#[test]
fn deprecated_alias_is_not_reported_as_an_unknown_option() {
    // `semantic-link-understanding` is documented (docs/md013.md) and honored by
    // serde, but the key validator did not know it and called it unknown.
    let (_, stderr) = run_check("[MD013]\nsemantic-link-understanding = true\n");
    assert!(
        !stderr.contains("Unknown option"),
        "documented, working alias must not be reported as unknown, got:\n{stderr}"
    );
}

#[test]
fn a_genuinely_unknown_option_is_still_reported() {
    // Control for the test above: the validator must still catch real typos.
    let (_, stderr) = run_check("[MD013]\nignore-link-urlz = true\n");
    assert!(
        stderr.contains("Unknown option"),
        "a misspelled key must still be reported, got:\n{stderr}"
    );
}

#[test]
fn silent_when_reflow_is_off() {
    // The setting does exactly what it says without reflow, so there is nothing
    // to warn about.
    let (_, stderr) = run_check("[MD013]\nignore-link-urls = true\n");
    assert!(
        !stderr.contains("[config warning]"),
        "must not warn when reflow is off, got:\n{stderr}"
    );
}

#[test]
fn silent_when_the_setting_is_not_explicit() {
    // `ignore-link-urls` defaults to true, so warning on the default would fire
    // for every user who enables reflow.
    let (_, stderr) = run_check("[MD013]\nreflow = true\n");
    assert!(
        !stderr.contains("[config warning]"),
        "must not warn when the user never set the option, got:\n{stderr}"
    );
}

#[test]
fn warning_does_not_change_behavior() {
    // The warning is advisory: what rumdl reports is unchanged. Compare only the
    // finding lines - the summary carries a duration that differs between runs.
    let findings = |config: &str| -> Vec<String> {
        run_check(config)
            .0
            .lines()
            .filter(|line| line.contains("[MD013]"))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        findings("[MD013]\nreflow = true\nignore-link-urls = true\n"),
        findings("[MD013]\nreflow = true\nignore-link-urls = false\n"),
        "the setting must not change what reflow reports (that is the point of the warning)"
    );
}
