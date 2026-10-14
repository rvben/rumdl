//! CommonMark 4.2 strips only spaces and tabs from the content of an ATX heading.
//! Any other whitespace (an em space, a no-break space) is text, so a rule that
//! re-emits a heading must keep it, and a `#` run set off by it is not a closing
//! sequence. Each rewrite is also checked against pulldown-cmark: the heading's
//! text must be the same before and after the fix.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Texts of every heading in `content`, per pulldown-cmark.
fn heading_texts(content: &str) -> Vec<String> {
    let mut texts = Vec::new();
    let mut current: Option<String> = None;
    for event in Parser::new(content) {
        match event {
            Event::Start(Tag::Heading { .. }) => current = Some(String::new()),
            Event::Text(t) | Event::Code(t) => {
                if let Some(c) = current.as_mut() {
                    c.push_str(&t);
                }
            }
            Event::SoftBreak => {
                if let Some(c) = current.as_mut() {
                    c.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => texts.extend(current.take()),
            _ => {}
        }
    }
    texts
}

/// Run `rumdl check` on `content` with `args`; returns (file after, stdout).
fn run(content: &str, args: &[&str]) -> (String, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--isolated", "--no-cache"])
        .args(args)
        .arg(&path)
        .output()
        .expect("run rumdl check");
    (
        fs::read_to_string(&path).unwrap(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

fn fix(content: &str, args: &[&str]) -> String {
    let mut full = vec!["--fix"];
    full.extend_from_slice(args);
    run(content, &full).0
}

#[test]
fn md003_setext_keeps_the_whitespace_and_hash_of_the_text() {
    let args = ["--enable", "MD003", "--config", "MD003.style=\"setext\""];
    for (content, expected) in [
        ("# a\u{2003}#\n", "a\u{2003}#\n===\n"),
        ("# a\u{2003}\n", "a\u{2003}\n==\n"),
        ("# \u{2003}a\n", "\u{2003}a\n==\n"),
        ("# a #\u{2003}\n", "a #\u{2003}\n====\n"),
        // controls: spaces and tabs are stripped, a real closing sequence is dropped
        ("# a \t\n", "a\n=\n"),
        ("# a #\n", "a\n=\n"),
        ("# a\t#\n", "a\n=\n"),
    ] {
        let fixed = fix(content, &args);
        assert_eq!(fixed, expected, "{content:?}");
        // pulldown-cmark keeps a tab beside the content as text, against the
        // CommonMark spec; the exact output above is the expectation for those rows.
        if !content.contains('\t') {
            assert_eq!(
                heading_texts(&fixed),
                heading_texts(content),
                "{content:?}: heading text"
            );
        }
        assert_eq!(fix(&fixed, &args), fixed, "{content:?}: idempotent");
    }
}

/// A heading whose only text is an em space has text; it is not rewritten as an
/// empty heading, and its setext form keeps that text.
#[test]
fn md003_does_not_treat_an_em_space_heading_as_empty() {
    let setext = ["--enable", "MD003", "--config", "MD003.style=\"setext\""];
    let as_setext = fix("# \u{2003}\n", &setext);
    assert_eq!(as_setext, "\u{2003}\n=\n");
    assert_eq!(
        heading_texts(&as_setext),
        heading_texts("# \u{2003}\n"),
        "{as_setext:?}"
    );
    assert_eq!(fix(&as_setext, &setext), as_setext, "idempotent");
    let closed = ["--enable", "MD003", "--config", "MD003.style=\"atx-closed\""];
    let fixed = fix("# \u{2003}\n", &closed);
    assert_eq!(heading_texts(&fixed), heading_texts("# \u{2003}\n"), "{fixed:?}");
    assert!(fixed.contains('\u{2003}'), "{fixed:?}");
}

#[test]
fn md003_atx_closed_keeps_the_whitespace_of_the_text() {
    let args = ["--enable", "MD003", "--config", "MD003.style=\"atx-closed\""];
    for content in ["# a\u{2003}\n", "# \u{2003}a\n", "# a\u{2003}#\n", "# a #\u{2003}\n"] {
        let fixed = fix(content, &args);
        assert_eq!(heading_texts(&fixed), heading_texts(content), "{content:?}: {fixed:?}");
        assert_eq!(fix(&fixed, &args), fixed, "{content:?}: idempotent");
    }
}

#[test]
fn md025_demotion_keeps_the_text_exactly() {
    let args = ["--enable", "MD025"];
    let content = "# a\n\n# b\u{2003}\n";
    let fixed = fix(content, &args);
    assert_eq!(fixed, "# a\n\n## b\u{2003}\n");
    assert_eq!(fix(&fixed, &args), fixed);
}

#[test]
fn md041_level_fix_keeps_a_leading_em_space() {
    let args = ["--enable", "MD041", "--config", "MD041.fix=true"];
    for (content, expected) in [
        ("## \u{2003}a\n", "# \u{2003}a\n"),
        ("##\ta\n", "# a\n"),
        ("## a\n", "# a\n"),
    ] {
        let fixed = fix(content, &args);
        assert_eq!(fixed, expected, "{content:?}");
    }
}

/// MD026 reports punctuation that ends the visible text, and the fix deletes only
/// the punctuation.
#[test]
fn md026_fixes_punctuation_before_an_em_space_exactly() {
    let args = ["--enable", "MD026"];
    let content = "# a.\u{2003}\n";
    assert!(run(content, &args).1.contains("MD026"));
    assert_eq!(fix(content, &args), "# a\u{2003}\n");
}

/// The display text is what readers and anchors compare, so an invisible trailing
/// em space does not make a heading unique.
#[test]
fn md024_still_treats_headings_differing_by_an_em_space_as_duplicates() {
    let (_, out) = run("# a\u{2003}\n\n# a\n", &["--enable", "MD024"]);
    assert!(out.contains("MD024"), "{out}");
}

/// The anchor a heading generates ignores the invisible whitespace at the end of
/// its text, whatever `raw_text` keeps.
#[test]
fn md051_anchor_verdicts_are_stable_for_end_whitespace() {
    for heading in ["# a\u{2003}", "# \u{2003}a", "# a\u{a0}b"] {
        let content = format!("[x](#a)\n\n{heading}\n");
        let (_, out) = run(&content, &["--enable", "MD051"]);
        let reported = out.contains("MD051");
        // `#a` resolves to the heading `a` only where the whitespace is at the
        // end of the text; inside the text it is part of the anchor.
        assert_eq!(reported, heading == "# a\u{a0}b", "{heading:?}: {out}");
    }
}
