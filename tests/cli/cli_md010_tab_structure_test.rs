//! `rumdl check --fix` must not change what a document means when it replaces tabs.
//!
//! A tab in leading whitespace advances to the next multiple of four columns
//! (CommonMark 2.2), so its width depends on where it starts: after `10.` it is
//! one column, after `-` three. A fixed replacement width moves a list item's
//! content column, a blockquote's content, and the boundary of indented code. A
//! tab at the end of a line is a soft break, while the same width in spaces is a
//! hard break.
//!
//! The oracle renders the document before and after the fix with pulldown-cmark
//! and compares the block structure: every start and end of a list, item,
//! blockquote, heading, paragraph and code block in order, the heading text, the
//! soft and hard breaks, and the code text where the rule leaves code alone.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// What the fix is allowed to leave unchanged: the whole block tree.
fn structure(content: &str, compare_code_text: bool) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut code_lines: Option<usize> = None;
    let mut code_text = String::new();
    for (event, range) in Parser::new_ext(content, Options::empty()).into_offset_iter() {
        match event {
            Event::Start(Tag::List(first)) => tokens.push(format!("list:{first:?}")),
            Event::Start(Tag::Item) => tokens.push("item".into()),
            Event::Start(Tag::BlockQuote(_)) => tokens.push("quote".into()),
            Event::Start(Tag::Paragraph) => tokens.push("para".into()),
            Event::Start(Tag::Heading { level, .. }) => tokens.push(format!("heading:{}", level as u8)),
            Event::Start(Tag::CodeBlock(kind)) => {
                let fenced = matches!(kind, CodeBlockKind::Fenced(_));
                tokens.push(format!("code:fenced={fenced}"));
                code_lines = Some(0);
                code_text.clear();
            }
            Event::End(
                TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_) | TagEnd::Paragraph | TagEnd::Heading(_),
            ) => tokens.push("end".into()),
            Event::End(TagEnd::CodeBlock) => {
                let lines = code_lines.take().unwrap_or(0);
                tokens.push(if compare_code_text {
                    format!("code-end:{code_text:?}")
                } else {
                    format!("code-end:lines={lines}")
                });
            }
            Event::Text(text) => {
                if let Some(lines) = code_lines.as_mut() {
                    *lines += text.matches('\n').count();
                    code_text.push_str(&text);
                }
            }
            Event::SoftBreak => tokens.push("soft".into()),
            Event::HardBreak => {
                // CommonMark makes a hard break of a backslash or of two or more
                // spaces before the line ending. pulldown-cmark also reports a
                // tab and a space there as one, so the source decides.
                let written = &content[range];
                let before_newline = written.trim_end_matches(['\r', '\n']);
                let hard = before_newline.ends_with('\\') || before_newline.ends_with("  ");
                tokens.push(if hard { "hard" } else { "soft" }.into());
            }
            _ => {}
        }
    }
    tokens
}

/// Heading text is part of the structure the other rules may touch, so it is only
/// compared through the token for the heading itself.
fn run_fix(content: &str, extra: &[&str], config: Option<&str>) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command.args(["check", "--no-cache", "--fix"]);
    match config {
        Some(config) => {
            let config_path = dir.path().join("rumdl.toml");
            fs::write(&config_path, config).unwrap();
            command.arg("--config").arg(config_path);
        }
        None => {
            command.arg("--isolated");
        }
    }
    // The exit status reports what is left unfixed, which is not under test.
    command.args(extra).arg(&path).output().expect("run rumdl check --fix");
    fs::read_to_string(&path).unwrap()
}

/// A document and what is known about it.
struct Row {
    source: &'static str,
    /// The tab after a code line is converted at `spaces-per-tab` on purpose, so
    /// code text is not compared when `code-blocks` is true.
    code_text_converted: bool,
}

const fn row(source: &'static str) -> Row {
    Row {
        source,
        code_text_converted: false,
    }
}

/// One row per shape of the category, in prose, lists, quotes and code.
fn corpus() -> Vec<Row> {
    let mut rows: Vec<Row> = [
        // Tabs after list markers: the marker width moves the tab stop.
        "-\tx\n\n    ## a\n",
        "*\tx\n\n    ## a\n",
        "+\tx\n\n    ## a\n",
        "- \tx\n\n      ## a\n",
        "1.\tx\n\n    ## a\n",
        "1)\tx\n\n    ## a\n",
        "10.\tx\n\n    ## a\n",
        "100.\tx\n\n        ## a\n",
        "-\t\tx\n\n    ## a\n",
        "-\t-\tx\n\n      ## a\n",
        "1.\t-\tx\n\n        ## a\n",
        "-\tx\n\n\ty\n\n\t## a\n",
        // Leading indentation of continuation lines.
        "- x\n\n\tcont\n\n\t## a\n",
        "- x\n\t- y\n\n\t  ## a\n",
        "- x\n\t- y\n\t\t- z\n",
        "1. x\n\t1. y\n\t\t1. z\n",
        "10. x\n\t1. y\n\n\t   ## a\n",
        // Space-tab mixes that all reach column four.
        "- x\n\n \tcont\n\n \t## a\n",
        "- x\n\n  \tcont\n",
        "- x\n\n   \tcont\n",
        "- x\n\n\t cont\n",
        "- x\n  - y\n\n   \t## a\n",
        // Blockquotes.
        ">\tfoo\n",
        ">\t\tfoo\n",
        "> \tfoo\n",
        ">\t- a\n>\t  b\n",
        ">\t> foo\n",
        "-\t> foo\n\n    ## a\n",
        "- > a\n  >\tb\n",
        "> - x\n>\n>\t## a\n",
        "> -\tx\n>\n>     ## a\n",
        "> \t- x\n>\n>      ## a\n",
        // Indented code and what looks like it.
        "para\n\n\tcode\n\t\tmore\n",
        "para\n\n \tcode\n",
        "para\n\n  \tcode\n",
        "- x\n\n\t\tcode\n",
        "1. x\n\n\t\t\tcode\n",
        "para\n\n\t# not heading\n",
        "foo\n\t---\n",
        // Fenced code.
        "```\n\tcode\n```\n",
        "```\n\t```\nfoo\n",
        "- ```\n\tcode\n  ```\n",
        "> ```\n>\tcode\n> ```\n",
        "1. x\n\n   ```\n\tcode\n   ```\n",
        "```\n\t\n```\n",
        // A tab at the end of a line must not become a hard break.
        "foo\t\nbar\n",
        "foo \t\nbar\n",
        "foo  \t\nbar\n",
        "foo\t\t\nbar\n",
        "foo\t \nbar\n",
        "foo\t  \nbar\n",
        "foo\\\t\nbar\n",
        "foo\n+\t\nbar\n",
        "foo\n1.\t\nbar\n",
        "- foo\t\n  bar\n",
        "> foo\t\n> bar\n",
        "-\t\n  x\n",
        "1.\t\n   x\n",
        ">\t\n> x\n",
        "# h\t\n\ntext\n",
        // Controls: tabs that carry no structure.
        "a\tb\n",
        "- a\tb\n",
        "|a\t|b|\n|-|-|\n|1\t|2|\n",
        "#\tfoo\n",
        "-\t-\t-\n",
        "[^1]:\tfoo\n\n    bar\n",
        "foo\n\tbar\n",
        "foo\n2.\tbar\n",
    ]
    .into_iter()
    .map(row)
    .collect();
    rows.push(Row {
        source: "para\n\n    a\tb\n",
        code_text_converted: true,
    });
    rows.push(Row {
        source: "```\nfoo\tbar\nbaz\t\n```\n",
        code_text_converted: true,
    });
    rows
}

/// Rows where a different default rule rewrites the structure itself, checked
/// with MD010 alone instead: MD069 rewrites the nested marker pair `- -` of a
/// line that came from `-<tab>-<tab>x`, and MD009 deletes the six trailing spaces
/// (a hard break) of `foo<tab>  `, as it does for any document that has them,
/// and MD077 dedents the continuation line of a bare list marker.
const OTHER_RULE_ROWS: &[&str] = &[
    "-\t-\tx\n\n      ## a\n",
    "foo\t  \nbar\n",
    "-\t\n  x\n",
    "1.\t\n   x\n",
];

fn check_rows(label: &str, extra: &[&str], config: Option<&str>, compare_code_text: bool) {
    let mut failures = Vec::new();
    for Row {
        source,
        code_text_converted,
    } in corpus()
    {
        if extra.is_empty() && OTHER_RULE_ROWS.contains(&source) {
            continue;
        }
        let compare = compare_code_text && !code_text_converted;
        let fixed = run_fix(source, extra, config);
        if structure(source, compare) != structure(&fixed, compare) {
            failures.push(format!("{source:?} -> {fixed:?}"));
        }
        let again = run_fix(&fixed, extra, config);
        if again != fixed {
            failures.push(format!("not idempotent: {source:?} -> {fixed:?} -> {again:?}"));
        }
    }
    assert!(failures.is_empty(), "{label}:\n{}", failures.join("\n"));
}

fn config(spaces_per_tab: usize, code_blocks: bool) -> String {
    format!("[MD010]\nspaces-per-tab = {spaces_per_tab}\ncode-blocks = {code_blocks}\n")
}

#[test]
fn md010_alone_keeps_the_structure_of_every_row() {
    check_rows("--enable MD010", &["--enable", "MD010"], None, true);
}

#[test]
fn md010_keeps_the_structure_for_any_spaces_per_tab() {
    for spaces_per_tab in [1, 2, 3, 8] {
        check_rows(
            &format!("spaces-per-tab = {spaces_per_tab}"),
            &["--enable", "MD010"],
            Some(&config(spaces_per_tab, false)),
            true,
        );
    }
}

#[test]
fn md010_keeps_the_structure_when_it_also_fixes_code_blocks() {
    for spaces_per_tab in [2, 4, 8] {
        check_rows(
            &format!("code-blocks = true, spaces-per-tab = {spaces_per_tab}"),
            &["--enable", "MD010"],
            Some(&config(spaces_per_tab, true)),
            false,
        );
    }
}

#[test]
fn the_default_rules_keep_the_structure_of_every_row() {
    check_rows("default rules", &[], None, true);
}

/// Exact results, so a pass is not a fix that happens to do nothing.
#[test]
fn md010_writes_these_exact_results() {
    for (content, expected) in [
        ("-\tx\n\n    ## a\n", "-   x\n\n    ## a\n"),
        ("1.\tx\n\n    ## a\n", "1.  x\n\n    ## a\n"),
        ("10.\tx\n\n    ## a\n", "10. x\n\n    ## a\n"),
        ("- x\n\n  \tcont\n", "- x\n\n    cont\n"),
        ("> \tfoo\n", ">   foo\n"),
        ("foo\t\nbar\n", "foo\nbar\n"),
        ("foo  \t\nbar\n", "foo\nbar\n"),
        ("foo\\\t\nbar\n", "foo\\\t\nbar\n"),
        // Controls: a tab inside text still becomes spaces-per-tab spaces.
        ("a\tb\n", "a    b\n"),
        ("- a\tb\n", "- a    b\n"),
    ] {
        assert_eq!(run_fix(content, &["--enable", "MD010"], None), expected, "{content:?}");
    }
    assert_eq!(
        run_fix("a\tb\n", &["--enable", "MD010"], Some(&config(2, false))),
        "a  b\n"
    );
}

/// Negative control: the oracle tells apart the documents the old fix produced.
#[test]
fn the_oracle_detects_the_corruption_the_fixed_width_caused() {
    for (source, broken) in [
        ("-\tx\n\n    ## a\n", "-    x\n\n    ## a\n"),
        ("10.\tx\n\n    ## a\n", "10.    x\n\n    ## a\n"),
        ("- x\n\n  \tcont\n", "- x\n\n      cont\n"),
        ("> \tfoo\n", ">     foo\n"),
        ("foo\t\nbar\n", "foo    \nbar\n"),
        ("foo\t \nbar\n", "foo     \nbar\n"),
    ] {
        assert_ne!(structure(source, true), structure(broken, true), "{source:?}");
    }
}
