//! MD026 reports and fixes headings inside blockquotes and list items, and a
//! fix never changes the block structure of the document.
//!
//! The oracle renders the document before and after `rumdl check --fix` with
//! pulldown-cmark and compares, per heading, its level and the containers it is
//! nested in. A heading that stops being a heading, or moves into another
//! container, changes that list.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[derive(Debug, PartialEq, Eq)]
struct Structure {
    items: usize,
    code_blocks: usize,
    rules: usize,
    /// Per heading in document order: its level, its text, and the list, item and
    /// blockquote events it is nested in, outermost first.
    headings: Vec<(u8, String, Vec<&'static str>)>,
}

fn structure(content: &str) -> Structure {
    let mut items = 0;
    let mut code_blocks = 0;
    let mut rules = 0;
    let mut path: Vec<&'static str> = Vec::new();
    let mut headings = Vec::new();
    let mut current: Option<(u8, String, Vec<&'static str>)> = None;
    for event in Parser::new_ext(content, Options::empty()) {
        match event {
            Event::Start(Tag::List(_)) => path.push("list"),
            Event::Start(Tag::Item) => {
                items += 1;
                path.push("item");
            }
            Event::Start(Tag::BlockQuote(_)) => path.push("quote"),
            Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_)) => {
                path.pop();
            }
            Event::Start(Tag::CodeBlock(_)) => code_blocks += 1,
            Event::Rule => rules += 1,
            Event::Start(Tag::Heading { level, .. }) => current = Some((level as u8, String::new(), path.clone())),
            Event::Text(text) | Event::Code(text) => {
                if let Some(heading) = current.as_mut() {
                    heading.1.push_str(&text);
                }
            }
            Event::SoftBreak => {
                if let Some(heading) = current.as_mut() {
                    heading.1.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => headings.extend(current.take()),
            _ => {}
        }
    }
    Structure {
        items,
        code_blocks,
        rules,
        headings,
    }
}

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

fn fix(content: &str) -> String {
    run(content, &["--fix", "--enable", "MD026"]).0
}

fn reported(content: &str) -> bool {
    run(content, &["--enable", "MD026"]).1.contains("MD026")
}

/// (document, result of fixing it); the result equals the document where MD026
/// reports a heading it cannot fix without changing the document's blocks.
const CASES: &[(&str, &str)] = &[
    // headings inside containers, fixed
    ("> ## a.\n", "> ## a\n"),
    ("> > ## a.\n", "> > ## a\n"),
    ("> - ## a.\n", "> - ## a\n"),
    ("- > ## a.\n", "- > ## a\n"),
    ("- x\n\n  > ## a.\n", "- x\n\n  > ## a\n"),
    ("- x\n\n    ## a.\n", "- x\n\n    ## a\n"),
    ("> ## a. ##\n", "> ## a ##\n"),
    ("> ## a. ## {#id}\n", "> ## a ## {#id}\n"),
    ("> ## a. #######\n", "> ## a #######\n"),
    ("> a.\n> ===\n", "> a\n> ===\n"),
    ("> - a.\n>   ---\n", "> - a\n>   ---\n"),
    ("- a.\n  ---\n", "- a\n  ---\n"),
    // top level, fixed
    ("# a.\n", "# a\n"),
    ("# a. #######\n", "# a #######\n"),
    ("# a.\u{2003}\n", "# a\u{2003}\n"),
    // the preserved em space keeps the remaining hash from closing the heading
    ("# a #.\u{2003}\n", "# a #\u{2003}\n"),
    ("> ## a #.\u{a0}\n", "> ## a #\u{a0}\n"),
    ("a.\n===\n", "a\n===\n"),
    ("a\nb.\n===\n", "a\nb\n===\n"),
    // reported, and the fix is declined because it would change the blocks
    ("a\n.\n===\n", "a\n.\n===\n"),
    ("> a\n> .\n> ===\n", "> a\n> .\n> ===\n"),
    ("> .\n> ===\n", "> .\n> ===\n"),
    ("> -.\n> ===\n", "> -.\n> ===\n"),
    ("> ___.\n> ===\n", "> ___.\n> ===\n"),
    ("- item\n\n    > .\n    > ===\n", "- item\n\n    > .\n    > ===\n"),
    // reported, and the fix is declined because it would expose a closing sequence
    ("# a #.\n", "# a #.\n"),
    ("> ## a #.\n", "> ## a #.\n"),
];

/// Not headings, or headings without punctuation: no warning, no change.
const CLEAN: &[&str] = &[
    "> a.\n",
    "> ##a.\n",
    "> ## a\n",
    "    ## a.\n",
    "- x\n\n      ## a.\n",
    "```\n> ## a.\n```\n",
    "text.\n",
];

#[test]
fn headings_are_fixed_or_declined_without_changing_the_blocks() {
    for (content, expected) in CASES {
        assert!(reported(content), "{content:?} must be reported");
        let fixed = fix(content);
        assert_eq!(&fixed, expected, "{content:?}");
        let (before, after) = (structure(content), structure(&fixed));
        assert_eq!(
            before.headings.len(),
            after.headings.len(),
            "{content:?} lost a heading"
        );
        for (b, a) in before.headings.iter().zip(&after.headings) {
            assert_eq!((b.0, &b.2), (a.0, &a.2), "{content:?} moved a heading");
        }
        assert_eq!(
            (before.items, before.code_blocks, before.rules),
            (after.items, after.code_blocks, after.rules),
            "{content:?} changed the blocks"
        );
        assert_eq!(fix(&fixed), fixed, "{content:?} fix is not idempotent");
    }
}

#[test]
fn a_fixed_heading_keeps_all_its_text_but_the_punctuation() {
    for (content, expected) in CASES {
        // The exact-output test covers custom IDs and closing sequences, which
        // the oracle's plain CommonMark parse reads as heading text.
        if content == expected || content.contains("{#") || content.contains(" #") {
            continue;
        }
        let (before, after) = (structure(content), structure(expected));
        for (b, a) in before.headings.iter().zip(&after.headings) {
            assert_eq!(
                b.1.trim_end()
                    .trim_end_matches(['.', ',', ';', ':', '!', '?'])
                    .trim_end(),
                a.1.trim_end(),
                "{content:?}"
            );
        }
    }
}

#[test]
fn text_that_is_not_a_punctuated_heading_is_left_alone() {
    for content in CLEAN {
        assert!(!reported(content), "{content:?} must not be reported");
        assert_eq!(&fix(content), content);
    }
}

/// Whether rumdl reads each heading of `content` as closed by a trailing hash run.
fn rumdl_closing_sequences(content: &str) -> Vec<bool> {
    use rumdl_lib::config::MarkdownFlavor;
    use rumdl_lib::lint_context::LintContext;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    ctx.headings().map(|h| h.heading.has_closing_sequence).collect()
}

/// A trailing `#` the fix leaves behind must not become the closing sequence of
/// the heading because an attribute block follows it: rumdl would then drop the
/// hash from the heading text. The fix is declined, or the heading has no closing
/// sequence it did not have before.
#[test]
fn an_attribute_block_does_not_turn_the_remaining_hash_into_a_closing_sequence() {
    for prefix in ["", "> ", "- ", "> - ", "- > ", "> > "] {
        for heading in [
            "# a #. {#id}",
            "# a #. {.class}",
            "# a #. {#id .class}",
            "# a #.! {#id}",
            "## a #?\t{#id}",
            "# a #.\u{a0}{#id}",
            "# a #.\u{2003}{#id}",
            // controls: not a closing sequence, or one that was already there
            "# a. {#id}",
            "# a #. ##",
            "# a. {.class}",
        ] {
            let content = format!("{prefix}{heading}\n");
            let before = rumdl_closing_sequences(&content);
            assert_eq!(before.len(), 1, "{content:?} must be one heading");
            let fixed = fix(&content);
            // An attribute block other than an ID ends the heading text with `}`,
            // which is not punctuation: nothing is reported or changed.
            if !reported(&content) {
                assert_eq!(fixed, content, "{content:?}");
                continue;
            }
            let after = rumdl_closing_sequences(&fixed);
            assert_eq!(after.len(), 1, "{content:?} -> {fixed:?}");
            assert!(
                !after[0] || before[0],
                "{content:?} -> {fixed:?}: the fix created a closing sequence"
            );
        }
    }
    // controls: the fix does run where nothing is exposed
    assert_eq!(fix("# a. {#id}\n"), "# a {#id}\n");
    assert_eq!(fix("> # a.\u{a0}{#id}\n"), "> # a\u{a0}{#id}\n");
}
