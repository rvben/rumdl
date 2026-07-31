//! MD087 through the CLI.
//!
//! The rule reports from a whole-run report rather than from its own `check`, so
//! the paths that assemble that report are worth pinning at the command level:
//! opt-in selection, what `fmt` counts, and what `fmt` leaves in the file.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn rumdl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rumdl")
}

const UNUSED: &str = "# Title\n\nShort line. <!-- rumdl-disable-line MD013 -->\n";

fn run(content: &str, args: &[&str]) -> (TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("test.md"), content).unwrap();

    let output = Command::new(rumdl_bin())
        .current_dir(dir.path())
        .args(args)
        .args(["--no-cache", "--no-config", "test.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (dir, stdout)
}

fn md087_lines(stdout: &str) -> Vec<&str> {
    stdout.lines().filter(|line| line.contains("[MD087]")).collect()
}

#[test]
fn md087_is_off_until_it_is_enabled() {
    let (_dir, default_run) = run(UNUSED, &["check"]);
    assert!(
        md087_lines(&default_run).is_empty(),
        "MD087 is opt-in, got:\n{default_run}"
    );

    let (_dir, enabled_run) = run(UNUSED, &["check", "--extend-enable", "MD087"]);
    assert_eq!(
        md087_lines(&enabled_run),
        ["test.md:3:13: [MD087] Unused disable-line comment: MD013"],
        "full output:\n{enabled_run}"
    );
}

#[test]
fn a_comment_that_suppresses_a_finding_is_silent() {
    let used = format!("# Title\n\n{} <!-- rumdl-disable-line MD013 -->\n", "word ".repeat(30));
    let (_dir, stdout) = run(&used, &["check", "--extend-enable", "MD087"]);
    assert!(md087_lines(&stdout).is_empty(), "full output:\n{stdout}");
}

/// `fmt` reports the warnings that survive the fix pass. It computes them by
/// re-linting the fixed content, which has to go through the same entry point as
/// the first pass or a rule reporting from the run's report is dropped: the
/// warning prints and the summary claims the file is clean.
#[test]
fn fmt_counts_the_finding_it_prints() {
    let (_dir, stdout) = run(UNUSED, &["fmt", "--extend-enable", "MD087"]);

    assert_eq!(md087_lines(&stdout).len(), 1, "full output:\n{stdout}");
    assert!(
        stdout.contains("Issues: Found 1 issues in 1 file"),
        "the printed warning must reach the summary, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("No issues found"),
        "summary contradicts the warning above it:\n{stdout}"
    );
}

#[test]
fn a_finding_takes_its_place_in_document_order() {
    // The rule reports after every other rule has run, so its findings only reach
    // the reader in the right place if they go through the same sort as the rest.
    let content = "Short line. <!-- rumdl-disable-line MD013 -->\n\n\nlast\n";
    let (_dir, stdout) = run(content, &["check", "-e", "MD012,MD013,MD087"]);
    let rules: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split(": [").nth(1))
        .filter_map(|rest| rest.split(']').next())
        .collect();

    assert_eq!(rules, ["MD087", "MD012"], "full output:\n{stdout}");
}

#[test]
fn fmt_leaves_the_comment_in_place() {
    // Removing a comment is a content decision the author owns: the line it
    // protects may be about to come back.
    let (dir, _stdout) = run(UNUSED, &["fmt", "--extend-enable", "MD087"]);
    assert_eq!(fs::read_to_string(dir.path().join("test.md")).unwrap(), UNUSED);
}
