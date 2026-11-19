//! `style = "aligned"` MD077 reindents only the lines that continue a list
//! item's own paragraph. Text that follows a block which closed the item (a
//! fence at the margin, say) belongs to no item, and moving it can turn it into
//! something else: under the MkDocs flavor the required indent is four columns,
//! which makes a paragraph right after a closing fence an indented code block.
//!
//! The oracle is pulldown-cmark's block structure (see the sibling test), so a
//! paragraph that became code, or that moved into an item, is caught.

use super::cli_md077_continuation_structure_test::structure;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// One `rumdl check --fix` run with MD077 alone, `style = "aligned"`, under the
/// given flavor.
fn fix_once(content: &str, flavor: &str) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    let config = dir.path().join(".rumdl.toml");
    fs::write(&path, content).unwrap();
    fs::write(
        &config,
        format!("[global]\nflavor = \"{flavor}\"\nenable = [\"MD077\"]\n\n[MD077]\nstyle = \"aligned\"\n"),
    )
    .unwrap();
    // The exit status reports what is left unfixed, which is not under test.
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args(["check", "--no-cache", "--fix", "--config"])
        .arg(&config)
        .arg(&path)
        .output()
        .expect("run rumdl check --fix");
    fs::read_to_string(&path).unwrap()
}

const FLAVORS: &[&str] = &["standard", "mkdocs", "quarto", "mdx"];

/// Documents where the text after a block that closed the item must stay put.
const AFTER_A_CLOSING_BLOCK: &[&str] = &[
    // The reduced property-test failures.
    "- A\n```\n\n```\n![`]()\n`",
    "- \u{1d400}\n```\n\n```\n*`*\n**`**",
    // A fence at the margin ends the item; the paragraph after it is top level.
    "- A\n```\nx\n```\ntext\nmore\n",
    "- A\n~~~\nx\n~~~\ntext\nmore\n",
    "1. A\n```\nx\n```\ntext\n",
    // A fence inside the item ends with the item's paragraph, so the line
    // below it that sits left of the content column is outside the item.
    "- A\n  ```\n  x\n  ```\ntext\n",
    // Nested item: the line is left of both content columns.
    "- A\n  - B\n```\nx\n```\ntext\n",
];

#[test]
fn text_after_a_block_that_closed_the_item_keeps_its_place() {
    for flavor in FLAVORS {
        for content in AFTER_A_CLOSING_BLOCK {
            let after = fix_once(content, flavor);
            assert_eq!(
                structure(content),
                structure(&after),
                "{flavor}: {content:?} became {after:?}"
            );
        }
    }
}

#[test]
fn the_aligned_fix_reaches_its_fixed_point_in_one_application() {
    for flavor in FLAVORS {
        for content in AFTER_A_CLOSING_BLOCK {
            let once = fix_once(content, flavor);
            let twice = fix_once(&once, flavor);
            assert_eq!(once, twice, "{flavor}: {content:?}");
        }
    }
}

/// The text after a closing block is not touched at all.
#[test]
fn text_after_a_closing_fence_is_left_as_written() {
    for flavor in FLAVORS {
        assert_eq!(
            fix_once("- A\n```\nx\n```\ntext\nmore\n", flavor),
            "- A\n```\nx\n```\ntext\nmore\n",
            "{flavor}"
        );
    }
}

/// Positive control: the rule still aligns a tight continuation line, and the
/// lines that follow it in the same paragraph, so the cases above are not
/// passing because the rule stopped running.
#[test]
fn a_lazy_continuation_is_still_aligned() {
    assert_eq!(fix_once("- A\nb\nc\n", "standard"), "- A\n  b\n  c\n");
    assert_eq!(fix_once("- A\nb\nc\n", "mkdocs"), "- A\n    b\n    c\n");
    // Text before the closing block is still the item's paragraph.
    assert_eq!(
        fix_once("- A\nb\n```\nx\n```\ntext\n", "standard"),
        "- A\n  b\n```\nx\n```\ntext\n"
    );
}

/// Negative control: the oracle sees a paragraph turn into code, which is what
/// the old fix did under MkDocs.
#[test]
fn the_oracle_sees_a_paragraph_become_code() {
    let before = structure("- A\n```\nx\n```\ntext\n");
    let after = structure("- A\n```\nx\n```\n    text\n");
    assert_ne!(before, after);
    assert_eq!(before.last().unwrap().0, "p");
    assert_eq!(after.last().unwrap().0, "code");
}

/// Tight continuation lines whose first byte opens a code span sit at the
/// margin. The line is the item's own paragraph text, so it is aligned like
/// any other text; a span opening on the line's first byte is not a span the
/// line begins inside.
const CODE_SPAN_AT_THE_MARGIN: &[&str] = &[
    "- Lorem ipsum dolor sit amet.\n`Sed` do eiusmod tempor.\n  Ut enim ad minim.\n",
    "- Lorem ipsum\n``Sed`` do\n  Ut enim\n",
    "- Lorem ipsum\n`Sed` do\n `Ut` enim\n  `Duis` aute\n   `Excepteur` sint\n",
    "1. Lorem ipsum\n`Sed` do\n",
    "- a\n  - b\n`Sed` do\n",
    "- A $x$ b\n`Sed` $y$ do\n",
];

#[test]
fn a_code_span_at_the_margin_is_aligned_and_keeps_the_document_structure() {
    for flavor in FLAVORS {
        for content in CODE_SPAN_AT_THE_MARGIN {
            let once = fix_once(content, flavor);
            assert_ne!(&once, content, "{flavor}: {content:?} was left unaligned");
            assert_eq!(
                structure(content),
                structure(&once),
                "{flavor}: {content:?} became {once:?}"
            );
            assert_eq!(fix_once(&once, flavor), once, "{flavor}: {content:?} is not idempotent");
        }
    }
}

/// The aligned text of a line opening with a code span is the aligned text of
/// the same line opening with a plain word: only the first token differs. A
/// blockquoted item is one the rule leaves alone, for either spelling.
#[test]
fn a_code_span_at_the_margin_is_aligned_exactly_like_plain_text() {
    for flavor in FLAVORS {
        for content in CODE_SPAN_AT_THE_MARGIN.iter().chain(&["> - a\n> `Sed` do\n"]) {
            let plain = content.replace('`', "");
            let from_plain = fix_once(&plain, flavor);
            let from_code = fix_once(content, flavor);
            assert_eq!(from_code.replace('`', ""), from_plain, "{flavor}: {content:?}");
        }
    }
}

#[test]
fn the_issue_document_is_aligned_to_the_exact_expected_text() {
    let content = "- Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n`Sed` do eiusmod tempor incididunt ut labore.\n  Ut enim ad minim veniam.\n";
    assert_eq!(
        fix_once(content, "standard"),
        "- Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n  `Sed` do eiusmod tempor incididunt ut labore.\n  Ut enim ad minim veniam.\n"
    );
    assert_eq!(
        fix_once(content, "mkdocs"),
        "- Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n    `Sed` do eiusmod tempor incididunt ut labore.\n    Ut enim ad minim veniam.\n"
    );
}

/// A line that begins inside a span opened earlier keeps its whitespace, under
/// the reading with math and the one without it. Negative control: the same
/// text without the span's backtick is aligned.
#[test]
fn a_line_that_begins_inside_a_code_span_is_still_left_alone() {
    for flavor in FLAVORS {
        for content in ["- A $x `y$\nand z` end.\n", "- A `y\nand z` end.\n"] {
            assert_eq!(&fix_once(content, flavor), content, "{flavor}: {content:?}");
        }
        assert_ne!(
            fix_once("- A $x y$\nand z end.\n", flavor),
            "- A $x y$\nand z end.\n",
            "{flavor}"
        );
    }
}
