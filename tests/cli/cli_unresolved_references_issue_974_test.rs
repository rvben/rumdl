//! Bracket text with no reference definition is literal text (issue #974).
//!
//! The oracle is pulldown-cmark: a line may carry an MD059 or MD045 warning
//! only when the parser renders a link or image on it. Positive controls
//! (defined references) keep the warnings, so the subset check cannot pass
//! because the rules went silent.

use pulldown_cmark::{Event, Options, Parser, Tag};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn rumdl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
}

/// 1-based lines on which pulldown-cmark renders a link or an image.
fn rendered_lines(content: &str) -> BTreeSet<usize> {
    let mut lines = BTreeSet::new();
    for (event, range) in Parser::new_ext(content, Options::empty()).into_offset_iter() {
        if matches!(event, Event::Start(Tag::Link { .. } | Tag::Image { .. })) {
            lines.insert(content[..range.start].matches('\n').count() + 1);
        }
    }
    lines
}

/// `(rule, line)` pairs reported by the real binary for the given rules.
fn findings(content: &str, rules: &str) -> Vec<(String, usize)> {
    let dir = tempdir().unwrap();
    let file = dir.path().join("doc.md");
    fs::write(&file, content).unwrap();
    let out = rumdl()
        .current_dir(dir.path())
        .args([
            "check",
            "--isolated",
            "--no-cache",
            "--output-format",
            "json",
            "--enable",
            rules,
        ])
        .arg(&file)
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let parsed: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}"));
    parsed
        .as_array()
        .unwrap()
        .iter()
        .map(|w| {
            (
                w["rule"].as_str().unwrap().to_string(),
                w["line"].as_u64().unwrap() as usize,
            )
        })
        .collect()
}

const ISSUE: &str =
    "# T\n\n- [link] x\n\n[here] alone in a paragraph.\n\n[more]\n\n[click here][]\n\n[here][missing]\n\n![][noalt]\n";

#[test]
fn issue_974_reproduction_reports_nothing() {
    assert_eq!(findings(ISSUE, "MD059,MD045"), vec![]);
}

#[test]
fn flagged_lines_are_a_subset_of_what_the_oracle_renders() {
    let corpus = [
        ISSUE,
        "# T\n\n> [here][missing]\n\n- a\n  - [click here][]\n\n1. ![][i]\n",
        "# T\n\n[here][r]\n\n![][i]\n\n[r]: /x\n[i]: /i.png\n",
        "# T\n\n[a][STRASSE]\n\n[straße]: /x\n\n[b][nope]\n",
    ];
    for content in corpus {
        let rendered = rendered_lines(content);
        for (rule, line) in findings(content, "MD059,MD045") {
            assert!(
                rendered.contains(&line),
                "{rule} flagged line {line}, not rendered as a link:\n{content}"
            );
        }
    }
}

#[test]
fn defined_references_are_still_flagged() {
    let content = "# T\n\n[here][r]\n\n![][i]\n\n[r]: /x\n[i]: /i.png\n";
    assert_eq!(
        findings(content, "MD059,MD045"),
        vec![("MD059".to_string(), 3), ("MD045".to_string(), 5)]
    );
}

#[test]
fn empty_destination_definition_is_still_a_link() {
    let content = "# T\n\n[here][r]\n\n[r]: <>\n";
    assert_eq!(findings(content, "MD059"), vec![("MD059".to_string(), 3)]);
}

#[test]
fn mkdocs_markdown_container_definitions_are_still_links() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("doc.md");
    fs::write(
        &file,
        "# T\n\n<div markdown=\"1\">\n[here][r]\n[here][missing]\n</div>\n\n[r]: /x\n",
    )
    .unwrap();
    let out = rumdl()
        .current_dir(dir.path())
        .args([
            "check",
            "--isolated",
            "--no-cache",
            "--flavor",
            "mkdocs",
            "--output-format",
            "json",
            "--enable",
            "MD059",
        ])
        .arg(&file)
        .output()
        .unwrap();
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let lines: Vec<u64> = parsed
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, vec![4]);
}

#[test]
fn fixing_is_idempotent_and_leaves_undefined_references_untouched() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("doc.md");
    fs::write(&file, ISSUE).unwrap();
    for _ in 0..2 {
        rumdl()
            .current_dir(dir.path())
            .args(["check", "--isolated", "--no-cache", "--fix", "--enable", "MD059,MD045"])
            .arg(&file)
            .output()
            .unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), ISSUE);
    }
}
