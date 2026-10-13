//! `rumdl check --fix` must not change the text of an inline code span when it
//! replaces tabs.
//!
//! A tab between a code span's backticks is part of the span's text (CommonMark
//! 6.1), so replacing it with spaces changes what the document renders. The
//! oracle parses the document before and after the fix with pulldown-cmark and
//! compares the text of every code span in order. The controls assert exact
//! output, because a rule that skipped every line holding a span would pass the
//! oracle too.

use pulldown_cmark::{Event, Options, Parser};
use std::fs;
use std::process::{Command, Output};
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

const CODE_BLOCKS_ON: &str = "MD010.code-blocks = true";

fn run(subcommand_args: &[&str], content: &str, extra: &[&str]) -> (Output, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .args(["--no-cache", "--isolated"])
        .args(subcommand_args)
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl check");
    (output, fs::read_to_string(&path).unwrap())
}

/// The file after `check --fix`. The exit status reports what is left unfixed,
/// which is not under test.
fn run_fix(content: &str, extra: &[&str]) -> String {
    run(&["--fix"], content, extra).1
}

/// Whether `check` reports MD010 for the document.
fn reports_md010(content: &str, extra: &[&str]) -> bool {
    let (output, _) = run(&[], content, extra);
    String::from_utf8_lossy(&output.stdout).contains("MD010")
}

/// Documents whose code span holds a tab. MD010 must leave every byte alone.
const SPANS: &[&str] = &[
    "`a\tb`\n",
    "`a\t\tb`\n",
    "text `a\tb` more\n",
    "`a\t\nb`\n",
    "`a\t\t\nb`\n",
    "`a \t\nb`\n",
    "`a\t \nb`\n",
    "``a\t\nb``\n",
    "``a`\t\nb``\n",
    "`a\t\nmid\t\nb`\n",
    "`\u{e9}\t\nb`\n",
    "- `a\t\n  b`\n",
    "1. `a\t\n   b`\n",
    "- x\n  - `a\t\n    b`\n",
    "> `a\t\n> b`\n",
    "> > `a\t\n> > b`\n",
    "- > `a\t\n  > b`\n",
    "> - `a\t\n>   b`\n",
    "> `a\t\nb`\n",
    "`a\t\r\nb`\r\n",
    "`a\t\nb` and `c\t\nd`\n",
    "*`a\t\nb`*\n",
    "[`a\t\nb`](u)\n",
    "<span>`a\t\nb`</span>\n",
    "$x$ `a\t\nb`\n",
    "A $x `y$\t\nz` end\n",
    "`a\tb`",
    // Leading whitespace of a continuation line: indentation when the line
    // matches all its containers, span text when it is lazy.
    "`a\n\tb`\n",
    "- `a\n\tb`\n",
    "> `a\n>\tb`\n",
    "> `a\n> \tb`\n",
    "- > `a\n  >\tb`\n",
    "> `a\n\tb`\n",
    "> para\n`a\n\tb`\n",
    "> `a\n    >\tb`\n",
    "- `a\n  2.\tb`\n",
    "`a\n2.\tb`\n",
    "> `a\n> 2.\tb`\n",
];

/// Documents MD010 leaves alone but the default rules are not asked to keep.
const OTHER_RULES_REINDENT: &[&str] = &[
    // MD077 re-indents the continuation line of a list item. Here the tab is
    // stripped indentation (cmark reads `a b`) but pulldown-cmark keeps it, so
    // this oracle cannot judge a change that is correct for the spec.
    "- `a\n  \tb`\n",
    // A lazy continuation line in a list item in a quote keeps its whitespace
    // as span text (cmark reads `a<space><tab>b`), and MD077 drops the tab.
    "- > para\n  `a\n  \tb`\n",
];

#[test]
fn the_oracle_can_see_a_changed_code_span() {
    // Positive control: a tab replaced by spaces is a different code text.
    assert_ne!(code_texts("`a\tb`\n"), code_texts("`a    b`\n"));
    assert_ne!(code_texts("`a\t\nb`\n"), code_texts("`a\nb`\n"));
    assert_ne!(code_texts("> `a\n\tb`\n"), code_texts("> `a\n    b`\n"));
}

#[test]
fn md010_leaves_the_text_of_every_code_span_alone_by_default() {
    for source in SPANS.iter().chain(OTHER_RULES_REINDENT) {
        assert!(
            !code_texts(source).is_empty(),
            "the corpus entry must contain a code span for the oracle: {source:?}"
        );
        let fixed = run_fix(source, &["--enable", "MD010"]);
        assert_eq!(fixed, *source, "MD010 rewrote a code span: {source:?}");
        assert!(
            !reports_md010(source, &["--enable", "MD010"]),
            "a tab in a code span is not reported by default: {source:?}"
        );
    }
}

#[test]
fn md010_reports_but_does_not_fix_a_code_span_tab_when_code_blocks_is_on() {
    for source in SPANS.iter().chain(OTHER_RULES_REINDENT) {
        assert!(
            reports_md010(source, &["--enable", "MD010", "--config", CODE_BLOCKS_ON]),
            "code-blocks = true reports the tab (and proves the override took effect): {source:?}"
        );
        let fixed = run_fix(source, &["--enable", "MD010", "--config", CODE_BLOCKS_ON]);
        assert_eq!(fixed, *source, "code-blocks = true rewrote a code span: {source:?}");
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
fn tabs_outside_a_span_are_fixed_while_the_tab_inside_it_is_kept() {
    for (source, expected) in [
        ("a\t`b\tc`\n", "a    `b\tc`\n"),
        ("-\t`a\tb`\n", "-   `a\tb`\n"),
        ("`a\t\nb` tail\t\nnext\n", "`a\t\nb` tail\nnext\n"),
    ] {
        let fixed = run_fix(source, &["--enable", "MD010"]);
        assert_eq!(fixed, expected, "{source:?}");
        assert_eq!(code_texts(&fixed), code_texts(source), "{source:?}");
    }
}

#[test]
fn tabs_around_a_span_are_still_fixed_and_the_code_text_survives() {
    for (source, expected) in [
        ("`a` x\ty\n", "`a` x    y\n"),
        ("text\t`a` and\t`b`\n", "text    `a` and    `b`\n"),
        ("`a\nb`\tx\n", "`a\nb`    x\n"),
        ("`a`\t\nnext\n", "`a`\nnext\n"),
        // Not code spans: the tab is ordinary text.
        ("`a\t\nb\n", "`a\nb\n"),
        ("`a\t\n\nb`\n", "`a\n\nb`\n"),
        ("<code>a\t\nb</code>\n", "<code>a\nb</code>\n"),
    ] {
        let fixed = run_fix(source, &["--enable", "MD010"]);
        assert_eq!(fixed, expected, "{source:?}");
        assert_eq!(code_texts(&fixed), code_texts(source), "{source:?}");
        assert_eq!(
            run_fix(&fixed, &["--enable", "MD010"]),
            fixed,
            "not idempotent: {source:?}"
        );
    }
}
