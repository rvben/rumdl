//! `rumdl check --fix` must not change the text of an inline code span.
//!
//! A code span is literal: the whitespace before a line ending inside it is part
//! of its text (CommonMark 6.1), so removing it as "trailing spaces" changes what
//! the document renders. The oracle parses the document before and after the fix
//! with pulldown-cmark and compares the text of every code span in order. A
//! hard break next to a span, or trailing spaces outside it, still change, so
//! the controls assert the exact output and not only the code text.

use pulldown_cmark::{Event, Options, Parser};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// The text of every inline code span, in document order.
fn code_texts(content: &str) -> Vec<String> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES | Options::ENABLE_STRIKETHROUGH;
    Parser::new_ext(content, options)
        .filter_map(|event| match event {
            Event::Code(text) => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

fn run_fix(content: &str, extra: &[&str]) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    // The exit status reports what is left unfixed, which is not under test.
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--fix", "--isolated"])
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl check --fix");
    fs::read_to_string(&path).unwrap()
}

/// Documents whose code span holds whitespace before a line ending. MD009 must
/// leave every byte alone.
const SPANS: &[&str] = &[
    "`a   \nb`\n",
    "`a \nb`\n",
    "`a    \nb`\n",
    "`a  \nb`\n",
    "` \n`\n",
    "``a   \nb``\n",
    "``a`  \nb``\n",
    "`a   \nmid  \nb`\n",
    "`a   \nmid    \nb`\n",
    "`\u{e9}   \nb`\n",
    "`a\u{2000}\nb`\n",
    "- `a   \n  b`\n",
    "1. `a   \n   b`\n",
    "- x\n  - `a   \n    b`\n",
    "> `a   \n> b`\n",
    "> > `a   \n> > b`\n",
    "- > `a   \n  > b`\n",
    "> - `a   \n>   b`\n",
    "> `a   \nb`\n",
    "`a   \r\nb`\r\n",
    "text `a   \nb` more\n",
    "`a   \nb` and `c   \nd`\n",
    "*`a   \nb`*\n",
    "[`a   \nb`](u)\n",
    "<span>`a   \nb`</span>\n",
    "$x$ `a   \nb`\n",
    "A $x `y$   \nz` end\n",
    "[^1]: `a   \n    b`\n\nSee[^1].\n",
];

/// Spans whose whitespace includes a tab. MD010 owns tabs, so only MD009's own
/// behavior is compared for these.
const TAB_SPANS: &[&str] = &["`a \t \nb`\n", "`a\t\nb`\n"];

#[test]
fn md009_leaves_the_text_of_every_code_span_alone() {
    for source in SPANS.iter().chain(TAB_SPANS) {
        assert!(
            !code_texts(source).is_empty(),
            "the corpus entry must contain a code span for the oracle: {source:?}"
        );
        let fixed = run_fix(source, &["--enable", "MD009"]);
        assert_eq!(fixed, *source, "MD009 rewrote a code span: {source:?}");
        assert_eq!(code_texts(&fixed), code_texts(source), "{source:?}");
    }
}

#[test]
fn the_default_rules_keep_the_text_of_every_code_span() {
    for source in SPANS {
        let fixed = run_fix(source, &[]);
        assert_eq!(
            code_texts(&fixed),
            code_texts(source),
            "default fix changed code text: {source:?} -> {fixed:?}"
        );
    }
}

#[test]
fn trailing_spaces_around_a_span_are_still_fixed_and_a_hard_break_is_kept() {
    for (source, expected) in [
        ("`a`   \nnext\n", "`a`\nnext\n"),
        ("text   \nnext\n", "text\nnext\n"),
        ("`a   \nb` tail   \nnext\n", "`a   \nb` tail\nnext\n"),
        ("`a   \nb\n", "`a\nb\n"),
        ("`a   \n\nb`\n", "`a\n\nb`\n"),
        ("<code>a   \nb</code>\n", "<code>a\nb</code>\n"),
        ("`a`  \nnext\n", "`a`  \nnext\n"),
        ("text  \n`a   \nb`\n", "text  \n`a   \nb`\n"),
    ] {
        let fixed = run_fix(source, &["--enable", "MD009"]);
        assert_eq!(fixed, expected, "{source:?}");
        assert_eq!(code_texts(&fixed), code_texts(source), "{source:?}");
        // A second pass has nothing left to do.
        assert_eq!(
            run_fix(&fixed, &["--enable", "MD009"]),
            fixed,
            "not idempotent: {source:?}"
        );
    }
}
