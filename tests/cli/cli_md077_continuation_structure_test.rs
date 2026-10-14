//! `rumdl check --fix` with MD077 must not move content out of the list item it
//! belongs to.
//!
//! The oracle renders the document before and after the fix with pulldown-cmark
//! and compares every leaf block (paragraph, heading, code block, rule) with its
//! text and the list, item and blockquote events it is nested in. Counts alone
//! would pass `-\n  a` becoming `-\n a`, which has the same blocks with the
//! paragraph in another place.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// One leaf block: its kind, its text and the containers it sits in, outermost
/// first.
type Block = (&'static str, String, Vec<&'static str>);

pub(super) fn structure(content: &str) -> Vec<Block> {
    let mut path: Vec<&'static str> = Vec::new();
    let mut blocks: Vec<Block> = Vec::new();
    let mut current: Option<Block> = None;
    // A tight item holds its text without a paragraph event around it, so text
    // arriving outside a block opens an implicit paragraph that the next
    // structural event ends.
    let mut implicit = false;
    for event in Parser::new_ext(content, Options::empty()) {
        let structural = matches!(
            event,
            Event::Start(
                Tag::List(_)
                    | Tag::Item
                    | Tag::BlockQuote(_)
                    | Tag::Paragraph
                    | Tag::Heading { .. }
                    | Tag::CodeBlock(_)
            ) | Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_))
                | Event::Rule
        );
        if structural && implicit {
            blocks.extend(current.take());
            implicit = false;
        }
        match event {
            Event::Start(Tag::List(_)) => path.push("list"),
            Event::Start(Tag::Item) => path.push("item"),
            Event::Start(Tag::BlockQuote(_)) => path.push("quote"),
            Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_)) => {
                path.pop();
            }
            Event::Start(Tag::Paragraph) => current = Some(("p", String::new(), path.clone())),
            Event::Start(Tag::Heading { .. }) => current = Some(("h", String::new(), path.clone())),
            Event::Start(Tag::CodeBlock(_)) => current = Some(("code", String::new(), path.clone())),
            Event::Rule => blocks.push(("hr", String::new(), path.clone())),
            Event::Text(text) | Event::Code(text) => {
                if current.is_none() {
                    current = Some(("p", String::new(), path.clone()));
                    implicit = true;
                }
                if let Some(block) = current.as_mut() {
                    block.1.push_str(&text);
                }
            }
            Event::SoftBreak => {
                if let Some(block) = current.as_mut() {
                    block.1.push(' ');
                }
            }
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock) => blocks.extend(current.take()),
            _ => {}
        }
    }
    blocks.extend(current.take());
    blocks
}

/// The document after `rumdl check --fix`, with the given extra arguments.
fn fix(content: &str, extra: &[&str]) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    // The exit status reports what is left unfixed, which is not under test.
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--isolated", "--no-cache", "--fix"])
        .args(extra)
        .arg(&path)
        .output()
        .expect("run rumdl check --fix");
    fs::read_to_string(&path).unwrap()
}

const MD077: &[&str] = &["--enable", "MD077"];

/// Documents whose structure MD077's fix must leave as it is: items that start
/// with a blank line, tabs, indented code first, items written on one line, and
/// the controls beside them.
const CORPUS: &[&str] = &[
    // A bare marker: the content column is one past the marker.
    "-\n  a\n",
    "+\n  a\n",
    "*\n  a\n  b\n",
    "1.\n   a\n",
    "10.\n    a\n",
    "1)\n   a\n",
    "- x\n\n  -\n    a\n",
    "-\n    a\n",
    "1.\n     a\n",
    "-\n  a\n\n  b\n",
    "-\n  foo\n-\n  ```\n  bar\n  ```\n-\n      baz\n",
    "-\n  ```\n  x\n  ```\n\n  after\n",
    "1.\n   ```\n   x\n   ```\n\n   after\n",
    // An item holds at most one blank line before its content.
    "-\n\n  a\n",
    "-\n\n a\n",
    "1.\n\n   a\n",
    "-\n a\n",
    // Tabs.
    "-\tx\n\n\ty\n",
    "-\tx\n\n\t y\n",
    "1.\tx\n\n\ty\n",
    "-\tx\n\n    y\n",
    // The first content is indented code.
    "-     t\n\n  y\n",
    "-     t\n\n      y\n",
    "1.     t\n\n   y\n",
    "-     [ ] task\n\n     y\n",
    // Items written on one line.
    "- 1. x\n\n     y\n",
    "1. -\n     f\n",
    "- 1. ```\n     ```\n     b\n",
    "- - a\n    ---\n",
    "- - a\n\n    b\n",
    "- 1. x\n\n      y\n",
    "- 1. x\n\n  y\n",
    "- - [ ] task\n        wrap\n",
    "- - [ ] task\n    wrap\n",
    "- -\n\n    a\n",
    "- -\n    a\n\n    b\n",
    // Text that is not CommonMark whitespace or a CommonMark marker.
    "- \u{0661}\u{0662}\u{0663}\u{0664}. x\n\n   y\n",
    "-  \u{a0}\n  a\n\n   y\n",
    "- -\n    \u{a0}\n\n    b\n",
    // Controls that stay as they are.
    "- a\n\n  b\n",
    "1. a\n\n   b\n",
    "- a\n  b\n",
    "- [ ] t\n\n      w\n",
    "- a\n  - b\n\n    c\n",
    "1. a\n   - b\n\n     c\n",
];

#[test]
fn md077_alone_keeps_every_block_where_it_is() {
    for content in CORPUS {
        let before = structure(content);
        let after = fix(content, MD077);
        assert_eq!(before, structure(&after), "{content:?} became {after:?}");
    }
}

#[test]
fn fixing_twice_changes_nothing_more() {
    for content in CORPUS {
        for extra in [&[][..], MD077] {
            let once = fix(content, extra);
            let twice = fix(&once, extra);
            assert_eq!(once, twice, "{content:?} with {extra:?}");
        }
    }
}

/// Exact results for the re-indents MD077 makes on purpose: content that sat
/// left of its item's content column after a blank line moves into the item,
/// and prose pushed past the column comes back to it.
#[test]
fn md077_writes_these_exact_results() {
    for (content, expected) in [
        ("- e\n\n t\n", "- e\n\n  t\n"),
        ("1. a\n\n  b\n", "1. a\n\n   b\n"),
        ("- a\n\n    b\n", "- a\n\n  b\n"),
        ("-\n  a\n\n b\n", "-\n  a\n\n  b\n"),
        ("-\n    a\n", "-\n  a\n"),
        ("1.\n     a\n", "1.\n   a\n"),
        ("-     t\n\n y\n", "-     t\n\n  y\n"),
        ("-     [ ] task\n\n     y\n", "-     [ ] task\n\n  y\n"),
        ("-\tx\n\n  y\n", "-\tx\n\n    y\n"),
        ("-\tx\n\n\t y\n", "-\tx\n\n    y\n"),
        ("- 1. x\n\n      y\n", "- 1. x\n\n     y\n"),
        ("- -\n\n    a\n", "- -\n\n  a\n"),
    ] {
        assert_eq!(fix(content, MD077), expected, "{content:?}");
    }
}

/// Positive control: the harness sees a fix when one changes the rendering, so a
/// pass above is not a harness that observes nothing.
#[test]
fn the_oracle_sees_content_that_leaves_its_item() {
    let inside = structure("-\n  a\n");
    let outside = structure("-\n a\n");
    assert_ne!(inside, outside);
    assert_eq!(inside[0].2, ["list", "item"]);
    assert!(outside[0].2.is_empty(), "{outside:?}");
    let nested = structure("- 1. x\n\n     y\n");
    let dedented = structure("- 1. x\n\n  y\n");
    assert_ne!(nested, dedented);
    let merged = structure("-     t\n\n      y\n");
    assert_eq!(merged.iter().filter(|block| block.0 == "code").count(), 1);
    assert_eq!(
        structure("-     t\n\n  y\n")
            .iter()
            .filter(|block| block.0 == "code")
            .count(),
        1
    );
}

/// Negative control: two renderings with the same structure and different
/// indentation compare equal.
#[test]
fn the_oracle_ignores_indentation_that_changes_nothing() {
    assert_eq!(structure("-\n  a\n"), structure("-\n   a\n"));
    assert_eq!(structure("- a\n\n  b\n"), structure("- a\n\n   b\n"));
}

/// The fix really runs: a document MD077 has to change is changed, so a corpus
/// that passes above is not one the rule declined to touch.
#[test]
fn md077_changes_a_document_that_needs_it() {
    assert_ne!(fix("- e\n\n t\n", MD077), "- e\n\n t\n");
}
