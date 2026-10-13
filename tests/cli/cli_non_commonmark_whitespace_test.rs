//! Fixes must not read container markers or non-CommonMark whitespace as content.
//!
//! CommonMark's whitespace is the space, the tab and the line ending. U+00A0, U+2003
//! and the other Unicode spaces are text, and the `>` of a blockquote continuation
//! line belongs to the container. A fix that trims "whitespace" with Rust's Unicode
//! `char::is_whitespace`, or slices a link out of the raw source and trims it, changes
//! the document. The oracle renders the document before and after `check --fix` with
//! pulldown-cmark and compares every event, link destinations and titles, code span
//! literals and text included.

use pulldown_cmark::{Options, Parser};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

const NBSP: char = '\u{a0}';
const EM_SPACE: char = '\u{2003}';

fn events(content: &str) -> Vec<String> {
    Parser::new_ext(content, Options::empty())
        .map(|event| format!("{event:?}"))
        .collect()
}

/// `rumdl check --fix` with the given extra arguments, returning the fixed text.
fn run_fix(content: &str, extra: &[&str]) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    // The exit status reports what is left unfixed, which is not under test.
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--isolated", "--fix"])
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl check --fix");
    fs::read_to_string(&path).unwrap()
}

fn check_output(content: &str, extra: &[&str]) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--isolated"])
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl check");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The rule configurations every document is run under: the default rule set, and
/// each rule alone.
fn configurations() -> Vec<Vec<&'static str>> {
    let mut all = vec![vec![]];
    for rule in ["MD009", "MD012", "MD021", "MD038", "MD039", "MD040", "MD062"] {
        all.push(vec!["--enable", rule]);
    }
    all
}

/// The fix must leave the document's rendered structure and text as it was, and be
/// stable on a second run.
fn assert_structure_kept(doc: &str) {
    for extra in configurations() {
        let fixed = run_fix(doc, &extra);
        assert_eq!(
            events(&fixed),
            events(doc),
            "{extra:?} changed the document\n  before: {doc:?}\n  after:  {fixed:?}"
        );
        assert_eq!(run_fix(&fixed, &extra), fixed, "{extra:?} is not idempotent on {doc:?}");
    }
}

// === Controls: the oracle must see these changes ===

#[test]
fn oracle_sees_the_blockquote_marker_become_a_destination() {
    assert_ne!(events("> [k](\n> )\n"), events("> [k](>)\n"));
}

#[test]
fn oracle_sees_nbsp_stripped_from_a_destination() {
    assert_ne!(events(&format!("[a]({NBSP}x)\n")), events("[a](x)\n"));
}

#[test]
fn oracle_sees_nbsp_stripped_from_a_code_span() {
    assert_ne!(events(&format!("`b{NBSP}`\n")), events("`b`\n"));
}

#[test]
fn oracle_sees_unicode_space_stripped_from_a_heading() {
    assert_ne!(events(&format!("# a{EM_SPACE}\n")), events("# a\n"));
}

#[test]
fn oracle_sees_a_paragraph_split() {
    assert_ne!(events(&format!("a\n{NBSP}\nb\n")), events("a\n\nb\n"));
}

// === The bugs ===

#[test]
fn blockquote_marker_is_not_a_link_destination() {
    for doc in [
        "> [k](\n> )\n",
        "> [k](\n> url)\n",
        "> [k](url\n> )\n",
        ">> [k](\n>> )\n",
        "> ![k](\n> )\n",
        "> - [k](\n>   )\n",
        "- > [k](\n  > )\n",
        "> text [k](\n> ) tail\n",
        "para\n\n> [k](\n> )\n\nafter\n",
        "> > > [k](\n> > > x\n> > > )\n",
        "- > > [k](\n  > > )\n",
        "> [k](\n> x\n> \"t\")\n",
        "> [k](x \"a\n> b\" )\n",
        "[a](\n    >url)\n",
        "![a](\n    >url)\n",
        "- [a](\n      >url)\n",
        "> [a](\n>     >url)\n",
    ] {
        assert_structure_kept(doc);
    }
}

#[test]
fn blockquote_multiline_destination_is_joined_onto_one_line() {
    // The warning is fixable: the line break and the marker inside the parentheses go,
    // and the link stays in the same quote.
    for (doc, expected) in [
        ("> [k](\n> )\n", "> [k]()\n"),
        ("> [k](\n> x\n> )\n", "> [k](x)\n"),
        ("> > [k](\n> > x\n> > )\n", "> > [k](x)\n"),
        ("- > [k](\n  > )\n", "- > [k]()\n"),
        ("> - [k](\n>   x\n>   )\n", "> - [k](x)\n"),
        ("> ![k](\n> )\n", "> ![k]()\n"),
    ] {
        for extra in [&[][..], &["--enable", "MD062"][..]] {
            let fixed = run_fix(doc, extra);
            assert_eq!(fixed, expected, "{extra:?} on {doc:?}");
            assert_eq!(events(&fixed), events(doc), "{extra:?} on {doc:?}");
        }
    }
}

#[test]
fn list_item_multiline_destination_is_still_fixed() {
    // Positive control: indentation-only containers are trimmed as before, and the
    // destination is still empty afterwards.
    assert_eq!(run_fix("- [k](\n  )\n", &["--enable", "MD062"]), "- [k]()\n");
    assert_eq!(run_fix("- [k](\n  )\n", &[]), "- [k]()\n");
    assert_structure_kept("- [k](\n  )\n");
}

#[test]
fn single_line_blockquote_destination_is_still_fixed() {
    // Positive control: the same rule still acts inside a blockquote.
    let fixed = run_fix("> [a]( x )\n", &["--enable", "MD062"]);
    assert_eq!(fixed, "> [a](x)\n");
    assert_eq!(events(&fixed), events("> [a]( x )\n"));
}

#[test]
fn nbsp_in_a_destination_is_content() {
    for ws in [NBSP, EM_SPACE, '\u{3000}'] {
        for doc in [
            format!("[a]({ws}x)\n"),
            format!("[a](x{ws})\n"),
            format!("[a]({ws}x{ws})\n"),
            format!("![a]({ws}x)\n"),
            format!("> [a]({ws}x)\n"),
            format!("- [a]({ws}x)\n"),
            format!("[a](x{ws}\"t\" )\n"),
        ] {
            assert_structure_kept(&doc);
        }
    }
    // Positive control: the ASCII space beside it is still stripped.
    assert_eq!(
        run_fix(&format!("[a]( {NBSP}x )\n"), &["--enable", "MD062"]),
        format!("[a]({NBSP}x)\n")
    );
    assert!(!check_output(&format!("[a]({NBSP}x)\n"), &["--enable", "MD062"]).contains("MD062"));
    assert!(check_output("[a]( x)\n", &["--enable", "MD062"]).contains("MD062"));
}

#[test]
fn code_span_edge_nbsp_is_content() {
    for ws in [NBSP, EM_SPACE, '\u{3000}', '\t'] {
        for doc in [
            format!("`b{ws}`\n"),
            format!("`{ws}b`\n"),
            format!("text `{ws}b{ws}` tail\n"),
            format!("- `b{ws}`\n"),
            format!("> `b{ws}`\n"),
        ] {
            assert_structure_kept(&doc);
        }
    }
    // Positive control: a real trailing space is still removed.
    assert_eq!(run_fix("`b `\n", &["--enable", "MD038"]), "`b`\n");
    assert!(check_output("`b `\n", &["--enable", "MD038"]).contains("MD038"));
}

#[test]
fn trailing_unicode_space_is_text() {
    for ws in [NBSP, EM_SPACE, '\u{2000}', '\u{3000}'] {
        for doc in [
            format!("# a{ws}\n"),
            format!("para{ws}\n"),
            format!("para{ws}\nnext\n"),
            format!("- item{ws}\n"),
            format!("> quote{ws}\n"),
            format!("a\n{ws}\nb\n"),
            format!("a\n\n\n{ws}\n\nb\n"),
            format!("- a\n\n{ws}\n- b\n"),
        ] {
            assert_structure_kept(&doc);
        }
    }
    // Positive control: ASCII trailing spaces are still removed.
    assert_eq!(run_fix("para   \n", &["--enable", "MD009"]), "para\n");
    assert_eq!(run_fix("# a   \n", &[]), "# a\n");
    // The ASCII run behind a Unicode space goes, the Unicode space stays.
    assert_eq!(
        run_fix(&format!("a{EM_SPACE}   \nb\n"), &["--enable", "MD009"]),
        format!("a{EM_SPACE}\nb\n")
    );
}

#[test]
fn other_rules_keep_unicode_spaces() {
    for ws in [NBSP, EM_SPACE] {
        for doc in [
            // MD021: a closing sequence glued to the text by a Unicode space is text.
            format!("# a{ws}#\n"),
            format!("# a {ws}#\n"),
            // MD039: link text edges.
            format!("[{ws}a{ws}](u)\n"),
            format!("![{ws}a{ws}](u)\n"),
            // MD040: an info string that is only a Unicode space.
            format!("```{ws}\ncode\n```\n"),
        ] {
            assert_structure_kept(&doc);
        }
    }
    // Positive controls.
    assert_eq!(run_fix("[ a ](u)\n", &["--enable", "MD039"]), "[a](u)\n");
    assert_eq!(run_fix("#  a  #\n", &["--enable", "MD021"]), "# a #\n");
    assert_eq!(run_fix("a\n\n\nb\n", &["--enable", "MD012"]), "a\n\nb\n");
    assert_eq!(
        run_fix("```\ncode\n```\n", &["--enable", "MD040"]),
        "```text\ncode\n```\n"
    );
}

#[test]
fn strict_hard_break_before_a_unicode_space_line_is_kept() {
    // A line of U+00A0 is a paragraph line, so the two trailing spaces before it
    // (and on it) still render a hard break. Strict mode removes only useless breaks.
    let dir = tempdir().unwrap();
    let config = dir.path().join("rumdl.toml");
    fs::write(&config, "[MD009]\nstrict = true\n").unwrap();
    let config = config.to_str().unwrap();
    for doc in [format!("a  \n{NBSP}\nb\n"), format!("{NBSP}  \nb\n")] {
        let fixed = run_fix_unisolated(&doc, &["--enable", "MD009", "--config", config]);
        assert_eq!(events(&fixed), events(&doc), "{doc:?} -> {fixed:?}");
        assert_eq!(fixed, doc);
    }
    // Positive control: with a real blank line the break is useless and removed.
    let fixed = run_fix_unisolated("a  \n\nb\n", &["--enable", "MD009", "--config", config]);
    assert_eq!(fixed, "a\n\nb\n");
}

fn run_fix_unisolated(content: &str, extra: &[&str]) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--fix"])
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl check --fix");
    fs::read_to_string(&path).unwrap()
}
