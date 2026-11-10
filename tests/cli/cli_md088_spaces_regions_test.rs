//! MD088 `normalize-spaces` through the real binary: the diagnostics name the
//! code point, `check` and `fmt` agree on which spaces are rewritten, and a
//! rewrite never changes the document's Markdown structure.

use pulldown_cmark::{Event, Options, Parser};
use std::fs;
use std::process::{Command, Output};
use tempfile::tempdir;

const NBSP: char = '\u{00A0}';

/// Block structure of `content` per pulldown-cmark, with text stripped.
fn block_shape(content: &str) -> Vec<String> {
    Parser::new_ext(content, Options::all())
        .filter_map(|event| match event {
            Event::Start(tag) => Some(format!("start {tag:?}")),
            Event::End(tag) => Some(format!("end {tag:?}")),
            Event::HardBreak => Some("hardbreak".to_string()),
            Event::Rule => Some("rule".to_string()),
            _ => None,
        })
        .collect()
}

/// Run `rumdl <subcommand>` over `content`; returns (file afterwards, output).
fn run(subcommand: &str, content: &str, extra: &[&str]) -> (String, Output) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg(subcommand)
        .args(["--no-cache", "--isolated", "--enable", "MD088"])
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl");
    (fs::read_to_string(&path).unwrap(), output)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

const SPACES_ON: [&str; 2] = ["--config", "MD088.normalize-spaces=true"];

#[test]
fn check_names_the_code_point_and_never_prints_the_glyph() {
    let (_, output) = run("check", &format!("a{NBSP}b\n"), &SPACES_ON);
    let out = stdout(&output);
    assert!(out.contains("[MD088]"), "{out}");
    assert!(out.contains("U+00A0"), "{out}");
    assert!(out.contains("No-Break Space"), "{out}");
    assert!(
        !out.contains(NBSP),
        "the invisible glyph leaked into the output: {out:?}"
    );
    assert_eq!(output.status.code(), Some(1), "{out}");
}

#[test]
fn default_config_reports_and_rewrites_nothing() {
    let content = format!("a{NBSP}b c\u{202F}d\n");
    let (_, output) = run("check", &content, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(!stdout(&output).contains("MD088"), "{}", stdout(&output));

    let (after, _) = run("fmt", &content, &[]);
    assert_eq!(after, content);

    // Control: the option on does find them.
    let (_, output) = run("check", &content, &SPACES_ON);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn fmt_rewrites_only_spaces_that_cannot_change_the_structure() {
    let content = format!(
        "100{NBSP}km at 5{NBSP}pm\n\n#{NBSP}not a heading\n\n1.{NBSP}not a list\n\ntrailing{NBSP}\n\n{NBSP}lead\n\n- item{NBSP}x\n"
    );
    let expected = format!(
        "100 km at 5 pm\n\n#{NBSP}not a heading\n\n1.{NBSP}not a list\n\ntrailing{NBSP}\n\n{NBSP}lead\n\n- item x\n"
    );

    let (_, output) = run("check", &content, &SPACES_ON);
    let out = stdout(&output);
    // 3 safe + 4 report-only findings, and check says which are fixable.
    assert!(out.contains("Found 7 issues"), "{out}");
    assert!(out.contains("fix 3 of the 7"), "{out}");

    let (after, _) = run("fmt", &content, &SPACES_ON);
    assert_eq!(after, expected);
    assert_eq!(
        block_shape(&after),
        block_shape(&content),
        "fmt changed the block structure"
    );

    // Idempotent: a second pass changes nothing.
    let (again, _) = run("fmt", &after, &SPACES_ON);
    assert_eq!(again, after);
}

#[test]
fn fmt_keeps_crlf_line_endings_and_allowed_spaces() {
    let content = format!("a{NBSP}b\r\nQuoi\u{202F}?\r\nc{NBSP}\r\n");
    let (after, _) = run(
        "fmt",
        &content,
        &[
            "--config",
            "MD088.normalize-spaces=true",
            "--config",
            "MD088.allow=[\"U+202F\"]",
        ],
    );
    assert_eq!(after, format!("a b\r\nQuoi\u{202F}?\r\nc{NBSP}\r\n"));
}
