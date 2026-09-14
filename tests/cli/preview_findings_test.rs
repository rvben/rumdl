//! `check --diff` lists beside its diff exactly the findings the diff leaves
//! unfixed, over a file and over the same document piped under its name.
//!
//! The document the diff produces decides which those are, not whether a
//! finding carries a fix: a rule can rewrite a document without attaching a fix
//! to its finding, and a rule configured as unfixable attaches one the run never
//! applies.

use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};

const COMMON: &[&str] = &["--no-cache", "--no-config", "--color", "never"];

/// Runs `args` over `input` as the file `doc.md`, then piped as `doc.md`, and
/// returns each run's name and output.
fn both_routes(args: &[&str], input: &[u8]) -> [(&'static str, Output); 2] {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("doc.md"), input).unwrap();

    let file = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args(args)
        .args(COMMON)
        .arg("doc.md")
        .output()
        .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args(args)
        .args(COMMON)
        .args(["-", "--stdin-filename", "doc.md"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let stdin = child.wait_with_output().unwrap();

    [("file", file), ("stdin", stdin)]
}

fn describe(route: &str, output: &Output) -> String {
    format!(
        "{route}: exit {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Asserts that every route of `args` over `input` lists exactly `expected`,
/// and prints a diff exactly when `diffs`.
fn assert_lists(args: &[&str], input: &[u8], expected: &[&str], diffs: bool) {
    for (route, output) in both_routes(args, input) {
        let context = describe(route, &output);
        let stdout = String::from_utf8(output.stdout).unwrap();
        let listed: Vec<&str> = stdout.lines().filter(|line| line.starts_with("doc.md:")).collect();
        assert_eq!(listed, expected, "{context}");
        assert_eq!(stdout.contains("\n@@ "), diffs, "{context}");
        assert_eq!(output.status.code(), Some(1), "{context}");
    }
}

#[test]
fn a_finding_the_diff_resolves_is_not_listed_though_it_carries_no_fix() {
    // MD046 rewrites the indented block from the rule's document fix and
    // reports the finding with no fix attached.
    assert_lists(
        &["check", "--diff"],
        b"# T\n\n```text\ncode\n```\n\n    indented\n",
        &[],
        true,
    );
}

#[test]
fn a_finding_whose_fix_the_run_does_not_apply_is_listed() {
    assert_lists(
        &["check", "--diff", "--unfixable", "MD022"],
        b"# Title\ntext\n",
        &["doc.md:1:1: [MD022] Expected 1 blank line below heading"],
        false,
    );
}

#[test]
fn a_finding_is_listed_where_it_sits_in_the_document_the_diff_applies_to() {
    // The inserted blank line moves the unresolved reference to line 5.
    assert_lists(
        &["check", "--diff"],
        b"# Title\ntext\n\n[a][missing]\n",
        &["doc.md:4:1: [MD052] Reference 'missing' not found"],
        true,
    );
}
