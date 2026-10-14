//! `rumdl check --fix` for MD065 must not change what a blank line around a
//! horizontal rule belongs to.
//!
//! The oracle parses the document before and after the fix with pulldown-cmark
//! and lists every leaf block with the containers around it, so a rule that
//! leaves its item or a heading that turns into a paragraph and a rule is seen.
//! Tight and loose lists are not told apart: the blank lines this rule adds
//! inside an item are what loosen it.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Every leaf block as `containers > kind: text`, in document order, with the
/// number of items. Paragraph tags are not kept, so looseness does not show.
fn skeleton(content: &str) -> Vec<String> {
    let mut path: Vec<&'static str> = Vec::new();
    let mut blocks = Vec::new();
    let mut text = String::new();
    // The kind of the block whose text is being collected. Text of a tight list
    // item sits in no paragraph, so it is collected under "text" until the next
    // block boundary.
    let mut open: Option<String> = None;
    for event in Parser::new_ext(content, Options::empty()) {
        // A boundary closes any text collected outside a block tag.
        let boundary = matches!(
            event,
            Event::Rule
                | Event::Start(Tag::List(_) | Tag::Item | Tag::BlockQuote(_) | Tag::Heading { .. } | Tag::CodeBlock(_))
                | Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_))
        );
        if boundary && !text.is_empty() {
            blocks.push(format!("{} > text: {}", path.join(" "), text.trim()));
            text.clear();
        }
        match event {
            Event::Start(Tag::List(_)) => path.push("list"),
            Event::Start(Tag::Item) => path.push("item"),
            Event::Start(Tag::BlockQuote(_)) => path.push("quote"),
            Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_)) => {
                path.pop();
            }
            Event::Rule => blocks.push(format!("{} > rule", path.join(" "))),
            Event::Start(Tag::Heading { level, .. }) => open = Some(format!("{} > h{}", path.join(" "), level as u8)),
            Event::Start(Tag::CodeBlock(_)) => open = Some(format!("{} > code", path.join(" "))),
            Event::Text(t) | Event::Code(t) => text.push_str(&t),
            Event::SoftBreak => text.push(' '),
            Event::End(TagEnd::Heading(_) | TagEnd::CodeBlock) => {
                if let Some(kind) = open.take() {
                    blocks.push(format!("{kind}: {}", text.trim()));
                    text.clear();
                }
            }
            // A paragraph ends its text, in a tight item as well as a loose one.
            Event::End(TagEnd::Paragraph) => {
                blocks.push(format!("{} > text: {}", path.join(" "), text.trim()));
                text.clear();
            }
            _ => {}
        }
    }
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

const ONLY_MD065: &[&str] = &["--enable", "MD065"];

/// Rules in the shapes that corrupted a document, with controls beside them.
const CORPUS: &[&str] = &[
    // A rule that opens an item whose marker is bare.
    "-\n  ***\n",
    "x\n\n-\n  ***\n",
    "1.\n   ***\n",
    "-\n  ___\n",
    "-\n  * * *\n",
    "-\n  - - -\n",
    "-\n  ***\n  text\n",
    "-\n  ***\n- b\n",
    "-\n  ***\n\n",
    "> -\n>   ***\n",
    "> -\n>   ***\n>   text\n",
    "- a\n  -\n    ***\n",
    "-\n  -\n    ***\n    text\n",
    "- -\n    ***\n",
    "1.\n   ***\n   text\n",
    // A setext heading whose text line is lazy.
    "> a\nb\n> ---\n",
    "> > a\n> b\n> > ---\n",
    "> a\nb\nc\n> ---\n",
    "> - a\nb\n>   ---\n",
    "> a\nb\n> ===\n",
    "> a\nb\n>   ---\n",
    "> a\n>b\n> ---\n",
    // Controls: real rules, which are fixed.
    "> a\nb\n> ***\n",
    "> a\nb\n> - - -\n",
    "text\n***\nmore\n",
    "- a\n  ***\n  b\n",
    "- a\n  ***\n- b\n",
    "1. a\n   ***\n2. b\n",
    "> a\n> ***\n> b\n",
    "> > a\n> > ***\n> > b\n",
    "-\n  a\n  ***\n  b\n",
    "-\n  text\n  ***\n",
    "> a\n> ---\n> b\n",
    "a\n---\nb\n",
];

#[test]
fn the_fix_keeps_every_block_in_its_container() {
    for content in CORPUS {
        let fixed = fix(content, ONLY_MD065);
        assert_eq!(skeleton(content), skeleton(&fixed), "{content:?} became {fixed:?}");
    }
}

#[test]
fn the_default_rules_keep_every_rule_and_heading_in_its_container() {
    for content in CORPUS {
        let fixed = fix(content, &[]);
        // Other rules may reword text on purpose, so compare the shape only.
        let shape = |doc: &str| -> Vec<String> {
            skeleton(doc)
                .into_iter()
                .map(|block| block.split(':').next().unwrap_or("").to_string())
                .collect()
        };
        assert_eq!(shape(content), shape(&fixed), "{content:?} became {fixed:?}");
    }
}

#[test]
fn fixing_twice_changes_nothing_more() {
    for content in CORPUS {
        let once = fix(content, ONLY_MD065);
        assert_eq!(once, fix(&once, ONLY_MD065), "{content:?}");
    }
}

/// Exact results. The rule opening a bare-marker item has nothing before it to
/// separate from, a lazy setext underline is not a rule, and a real rule still
/// gets its blank lines (with the quote marker on them).
#[test]
fn writes_these_exact_results() {
    for (content, expected) in [
        ("-\n  ***\n", "-\n  ***\n"),
        ("x\n\n-\n  ***\n", "x\n\n-\n  ***\n"),
        ("1.\n   ***\n", "1.\n   ***\n"),
        ("> -\n>   ***\n", "> -\n>   ***\n"),
        ("-\n  ***\n  text\n", "-\n  ***\n\n  text\n"),
        ("> a\nb\n> ---\n", "> a\nb\n> ---\n"),
        ("> > a\n> b\n> > ---\n", "> > a\n> b\n> > ---\n"),
        ("> a\nb\n> ***\n", "> a\nb\n>\n> ***\n"),
        ("> a\n> ***\n> b\n", "> a\n>\n> ***\n>\n> b\n"),
        ("- a\n  ***\n  b\n", "- a\n\n  ***\n\n  b\n"),
        ("text\n***\nmore\n", "text\n\n***\n\nmore\n"),
    ] {
        assert_eq!(fix(content, ONLY_MD065), expected, "{content:?}");
    }
}

/// Positive control: the harness sees a fix when one is made, and the oracle
/// tells a rule that left its item from one that stayed.
#[test]
fn the_oracle_sees_what_it_guards() {
    let content = "text\n***\nmore\n";
    assert_ne!(fix(content, ONLY_MD065), content);
    // The corruption that shipped: the rule leaving the item it opened.
    assert_ne!(skeleton("-\n  ***\n"), skeleton("-\n\n  ***\n"));
    // And the heading that became a paragraph and a rule.
    assert_ne!(skeleton("> a\nb\n> ---\n"), skeleton("> a\nb\n>\n> ---\n"));
    // Looseness alone is not a difference.
    assert_eq!(skeleton("- a\n  ***\n  b\n"), skeleton("- a\n\n  ***\n\n  b\n"));
}
