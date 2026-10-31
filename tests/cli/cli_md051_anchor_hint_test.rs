//! The MD051 warning names the anchor that exists when a link is a near miss,
//! on the real binary. The hint is message text only: the same links warn and
//! `--fix` leaves the file alone.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn rumdl(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to execute rumdl")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

fn md051_lines(output: &Output) -> Vec<String> {
    stdout(output)
        .lines()
        .filter(|line| line.contains("[MD051]"))
        .map(str::to_string)
        .collect()
}

const REPORTED: &str = "## Purpose <!-- note -->\n\n[Purpose](#purpose)\n";

/// The document has one heading whose text ends in an HTML comment and one
/// same-document link to `#purpose`, so the warning concerns that link.
fn assert_reported_structure(content: &str) {
    let events: Vec<Event> = Parser::new(content).collect();
    let headings = events
        .iter()
        .filter(|e| matches!(e, Event::Start(Tag::Heading { .. })))
        .count();
    let links: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(headings, 1);
    assert_eq!(links, ["#purpose"]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::InlineHtml(_) | Event::Html(_)))
    );
    assert!(events.iter().any(|e| matches!(e, Event::End(TagEnd::Heading(_)))));
}

#[test]
fn the_reported_document_names_the_anchor_that_exists() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), REPORTED).unwrap();
    assert_reported_structure(REPORTED);

    for args in [
        &["check", "--isolated", "--no-cache", "a.md"][..],
        &["check", "--isolated", "--no-cache", "--enable", "MD051", "a.md"][..],
    ] {
        let out = rumdl(dir.path(), args);
        assert_eq!(
            md051_lines(&out),
            [
                "a.md:3:1: [MD051] Link anchor '#purpose' does not exist in document headings (did you mean '#purpose-'?)"
            ],
            "{}",
            stdout(&out)
        );
        assert_eq!(out.status.code(), Some(1));
    }
}

#[test]
fn the_named_anchor_resolves_and_an_unrelated_link_keeps_its_message() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("ok.md"),
        "## Purpose <!-- note -->\n\n[Purpose](#purpose-)\n",
    )
    .unwrap();
    let ok = rumdl(
        dir.path(),
        &["check", "--isolated", "--no-cache", "--enable", "MD051", "ok.md"],
    );
    assert_eq!(ok.status.code(), Some(0), "{}", stdout(&ok));
    assert!(md051_lines(&ok).is_empty());

    fs::write(
        dir.path().join("far.md"),
        "## Purpose <!-- note -->\n\n[X](#nothing-alike)\n",
    )
    .unwrap();
    let far = rumdl(
        dir.path(),
        &["check", "--isolated", "--no-cache", "--enable", "MD051", "far.md"],
    );
    assert_eq!(
        md051_lines(&far),
        ["far.md:3:1: [MD051] Link anchor '#nothing-alike' does not exist in document headings"]
    );
}

#[test]
fn the_hint_is_carried_in_json_output() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), REPORTED).unwrap();
    let out = rumdl(
        dir.path(),
        &[
            "check",
            "--isolated",
            "--no-cache",
            "--enable",
            "MD051",
            "--output-format",
            "json",
            "a.md",
        ],
    );
    let text = stdout(&out);
    assert!(
        text.contains("Link anchor '#purpose' does not exist in document headings (did you mean '#purpose-'?)"),
        "{text}"
    );
}

#[test]
fn fix_leaves_the_file_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.md");
    fs::write(&path, REPORTED).unwrap();
    let _ = rumdl(
        dir.path(),
        &[
            "check",
            "--isolated",
            "--no-cache",
            "--enable",
            "MD051",
            "--fix",
            "a.md",
        ],
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), REPORTED);
    let again = rumdl(
        dir.path(),
        &[
            "check",
            "--isolated",
            "--no-cache",
            "--enable",
            "MD051",
            "--fix",
            "a.md",
        ],
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), REPORTED);
    assert_eq!(md051_lines(&again).len(), 1);
}

#[test]
fn a_cross_file_fragment_names_the_anchor_that_exists() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("b.md"), "## Purpose <!-- note -->\n").unwrap();
    fs::write(
        dir.path().join("a.md"),
        "# A\n\n[B](b.md#purpose)\n[B](b.md#nothing-alike)\n",
    )
    .unwrap();
    let out = rumdl(
        dir.path(),
        &["check", "--no-config", "--no-cache", "--enable", "MD051", "."],
    );
    assert_eq!(
        md051_lines(&out),
        [
            "a.md:3:1: [MD051] Link fragment 'purpose' not found in 'b.md' (did you mean '#purpose-'?)",
            "a.md:4:1: [MD051] Link fragment 'nothing-alike' not found in 'b.md'",
        ],
        "{}",
        stdout(&out)
    );
}
