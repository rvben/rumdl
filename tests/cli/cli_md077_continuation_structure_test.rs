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

const MD077_ALIGNED: &[&str] = &["--enable", "MD077", "--config", "MD077.style=\"aligned\""];

/// The number of list items holding a block.
fn items(content: &str) -> usize {
    structure(content)
        .iter()
        .filter(|block| block.2.last() == Some(&"item"))
        .count()
}

/// Text that looks like a list marker is paragraph text while its paragraph
/// stands outside the item (`2. a` cannot interrupt a paragraph, and neither can
/// a marker with nothing after it). Pulling the paragraph into the item would
/// release that line to open a list, so the fix is withheld and no text becomes
/// a list item. A marker ends at a space, a tab or the end of the line.
#[test]
fn a_paragraph_joining_its_item_never_turns_its_text_into_a_list() {
    for content in [
        "- X\n\n a\n2. a\n``",
        "- X\n\n a\n2. a\nzz\n",
        "- X\n\n a\nb\n 7) c\n",
        "1. X\n\n  a\n3. b\n",
        "- X\n\n a\n2.\tb\n",
        "- X\n\n a\n2.\n",
        "- X\n\n a\n+\n",
        "- X\n\n a\n*\n",
        "- X\n\n a\n2.\r\n",
        "- X\n\n a\n123456789. b\n",
        "- X\n\n a\n b\n2. c\n d\n",
    ] {
        // The oracle sees the list the withheld fix would open.
        let moved = content
            .replacen("\n a\n", "\n  a\n", 1)
            .replacen("\n  a\n", "\n   a\n", 1);
        let moved = if content.starts_with("1.") {
            moved
        } else {
            content.replacen("\n a\n", "\n  a\n", 1)
        };
        assert_ne!(moved, *content);
        assert!(items(&moved) > items(content), "{content:?} against {moved:?}");
        for extra in [MD077, MD077_ALIGNED] {
            let after = fix(content, extra);
            assert_eq!(items(content), items(&after), "{content:?} became {after:?}");
            assert_eq!(after, *content, "{content:?} with {extra:?}");
        }
    }
    // Control: without the marker line the paragraph is pulled in.
    assert_eq!(fix("- X\n\n a\nb\n", MD077), "- X\n\n  a\nb\n");
}

/// The fix is withheld only for a line that would open a list. Text that merely
/// resembles a marker, a marker in a later paragraph, and a marker the item
/// still holds as paragraph text all leave the fix in place.
#[test]
fn a_paragraph_joins_its_item_when_no_text_would_become_a_list() {
    const MD077_INDENT_4: &[&str] = &["--enable", "MD077", "--config", "MD077.indent=4"];
    for (content, extra, expected) in [
        // Ten digits are not an ordered marker.
        ("- X\n\n a\n1234567890. b\n", MD077, "- X\n\n  a\n1234567890. b\n"),
        // A marker needs a space, a tab or the end of the line after it.
        ("- X\n\n a\n2.b\n", MD077, "- X\n\n  a\n2.b\n"),
        ("- X\n\n a\n+b\n", MD077, "- X\n\n  a\n+b\n"),
        // The heading ends the paragraph; the marker belongs to the next one.
        (
            "- X\n\n a\n# Heading\ntext\n2. b\n",
            MD077,
            "- X\n\n  a\n# Heading\ntext\n2. b\n",
        ),
        // A blank line ends the paragraph.
        ("- X\n\n a\n\ntext\n2. b\n", MD077, "- X\n\n  a\n\ntext\n2. b\n"),
        // The marker sits at the item's content column, short of the configured
        // indent: the item holds it as paragraph text before and after.
        ("- X\n\n a\nb\n  2. c\n", MD077_INDENT_4, "- X\n\n    a\nb\n  2. c\n"),
        // Four columns past the item's marker is too far in to be a marker.
        (
            "123456. X\n\n a\nb\n     2. c\n",
            MD077,
            "123456. X\n\n        a\nb\n     2. c\n",
        ),
    ] {
        let after = fix(content, extra);
        assert_eq!(after, expected, "{content:?} with {extra:?}");
        // The paragraph is the one block that joins the item, and no block is
        // split or added.
        assert_eq!(items(content) + 1, items(&after), "{content:?} became {after:?}");
        assert_eq!(
            structure(content).len(),
            structure(&after).len(),
            "{content:?} became {after:?}"
        );
    }
}

/// The marker search follows the paragraph as CommonMark reads it and measures
/// a marker's indent from the block holding the item. Each withheld document
/// is paired with the hand-moved one the fix would have produced, which the
/// oracle reads as holding one more list.
#[test]
fn the_marker_search_follows_the_paragraph_and_the_block_holding_the_item() {
    const MD077_INDENT_7: &[&str] = &["--enable", "MD077", "--config", "MD077.indent=7"];
    for (content, extra, moved) in [
        // A comment opened partway through a line is paragraph text, and so
        // are the lines it runs over: the marker below still belongs to it.
        ("- X\n\n a <!--\nx -->\n2. b\n", MD077, "- X\n\n  a <!--\nx -->\n2. b\n"),
        // A marker written inside such a comment opens a list all the same.
        ("- X\n\n a <!--\n2. b -->\n", MD077, "- X\n\n  a <!--\n2. b -->\n"),
        // Three columns past the outer item's content column is a marker.
        (
            "- P\n   -    X\n\n     a\nb\n     2. c\n",
            MD077,
            "- P\n   -    X\n\n        a\nb\n     2. c\n",
        ),
        // An item its marker falls short of is a sibling, not the block
        // holding it: three columns from the document is a marker.
        (
            "- A\n -    X\n\n   a\nb\n   2. c\n",
            MD077,
            "- A\n -    X\n\n      a\nb\n   2. c\n",
        ),
        // A configured indent that reaches the content column joins the item.
        ("12345. X\n\n a\n2. b\n", MD077_INDENT_7, "12345. X\n\n       a\n2. b\n"),
    ] {
        assert!(items(moved) > items(content), "{content:?} against {moved:?}");
        let after = fix(content, extra);
        assert_eq!(after, content, "{content:?} with {extra:?}");
    }

    const MD077_INDENT_2: &[&str] = &["--enable", "MD077", "--config", "MD077.indent=2"];
    // `joins` is whether the paragraph was outside every item before the fix.
    for (content, extra, expected, joins) in [
        // A comment opened at the start of a line is a block of its own: it
        // ends the paragraph, and what it holds is never a marker.
        (
            "- X\n\n a\n<!-- c\n2. b\n-->\n",
            MD077,
            "- X\n\n  a\n<!-- c\n2. b\n-->\n",
            true,
        ),
        // Indented, the comment is still the first thing on its line.
        (
            "- X\n\n a\n <!-- c\n2. b\n-->\n",
            MD077,
            "- X\n\n  a\n <!-- c\n2. b\n-->\n",
            true,
        ),
        // Four columns from the document, past a sibling item's content column.
        (
            "- A\n -    X\n\n   a\nb\n    2. c\n",
            MD077,
            "- A\n -    X\n\n      a\nb\n    2. c\n",
            true,
        ),
        // Four columns past the block holding the item is too far in to be a
        // marker, however far in the item's own marker sits.
        (
            "  100. X\n\n   a\nb\n    2. c\n",
            MD077,
            "  100. X\n\n       a\nb\n    2. c\n",
            true,
        ),
        (
            "- P\n   -    X\n\n     a\nb\n      2. c\n",
            MD077,
            "- P\n   -    X\n\n        a\nb\n      2. c\n",
            // The paragraph moves from the outer item to the inner one.
            false,
        ),
        // Only spaces and tabs indent a marker: after a no-break space the
        // digits are text.
        ("- X\n\n a\n\u{a0}2. b\n", MD077, "- X\n\n  a\n\u{a0}2. b\n", true),
        // Short of the content column the paragraph stays outside the item, so
        // the line below it stays its text.
        (
            "12345. X\n\n a\n2. b\n",
            MD077_INDENT_2,
            "12345. X\n\n  a\n2. b\n",
            false,
        ),
    ] {
        let after = fix(content, extra);
        assert_eq!(after, expected, "{content:?} with {extra:?}");
        assert_eq!(
            items(content) + usize::from(joins),
            items(&after),
            "{content:?} became {after:?}"
        );
        assert_eq!(
            structure(content).len(),
            structure(&after).len(),
            "{content:?} became {after:?}"
        );
    }
}

/// The blocks a document holds, as (kind, text), wherever they sit.
fn shape(content: &str) -> Vec<(String, String)> {
    structure(content)
        .into_iter()
        .map(|block| (block.0.to_string(), block.1.to_string()))
        .collect()
}

/// A line four or more columns in is text of the paragraph above it whatever
/// it begins with. Inside the item it is read from the content column, where
/// it begins the block its text spells, so the paragraph stays where it is and
/// the line is not treated as over-indented item content. Each document is
/// paired with the hand-moved one, which the oracle reads as other blocks.
#[test]
fn a_paragraph_stays_outside_its_item_when_its_indented_text_would_become_a_block() {
    for content in [
        "- X\n\n a\n    > b\n",
        "- X\n\n a\n    # b\n",
        "- X\n\n a\n    ---\n",
        "- X\n\n a\n    ==\n",
        "- X\n\n a\n    * * *\n",
        "- X\n\n a\n    ```\n",
        "- X\n\n a\n    <div>\n",
        "- X\n\n a\n    <?b\n",
        "- X\n\n a\n    - b\n",
        "- X\n\n a\n    1. b\n",
        "- X\n\n a\nb\n     > c\n",
        // The comment becomes an HTML block, which ends the paragraph and
        // leaves the marker below it to open a list.
        "- X\n\n a\n    <!-- c -->\n2. b\n",
    ] {
        let moved = content.replacen("\n a\n", "\n  a\n", 1);
        assert_ne!(moved, *content);
        assert_ne!(shape(&moved), shape(content), "{content:?} against {moved:?}");
        let after = fix(content, MD077);
        assert_eq!(after, *content, "{content:?}");
    }
    for (content, expected) in [
        // Four columns past the content column is still text inside the item.
        ("- X\n\n a\n      > b\n", "- X\n\n  a\n      > b\n"),
        // Short of the content column the line leaves the item, and four
        // columns from the document it is text there too.
        (
            "123456. X\n\n a\nb\n     > c\n",
            "123456. X\n\n        a\nb\n     > c\n",
        ),
        // Text that begins no block joins the item and is then its content.
        ("- X\n\n a\n    b\n", "- X\n\n  a\n  b\n"),
        ("- X\n\n a\n    #b\n", "- X\n\n  a\n  #b\n"),
        // An autolink, an inline tag and a backtick run with a backtick in
        // what follows it are text wherever they stand.
        (
            "- X\n\n a\n    <https://example.org>\n",
            "- X\n\n  a\n  <https://example.org>\n",
        ),
        ("- X\n\n a\n    <span>b</span>\n", "- X\n\n  a\n  <span>b</span>\n"),
        ("- X\n\n a\n    ```b`c\n", "- X\n\n  a\n  ```b`c\n"),
        // An ordered marker other than 1 cannot interrupt the item's paragraph.
        ("- X\n\n a\n    2. b\n", "- X\n\n  a\n    2. b\n"),
        // The marker line is under-indented too, and moves with the paragraph.
        ("- X\n\n a\n 2. b\n", "- X\n\n  a\n  2. b\n"),
        ("- X\n\n a\n 2. b\n 3. c\n d\n", "- X\n\n  a\n  2. b\n  3. c\n  d\n"),
        // An item closed by a sibling holds nothing below it: four columns
        // from the document is too far in to be a marker.
        (
            "- A\n - B\n  -    C\n\n   a\nb\n    2. c\n",
            "- A\n - B\n  -    C\n\n       a\nb\n    2. c\n",
        ),
    ] {
        let after = fix(content, MD077);
        assert_eq!(after, expected, "{content:?}");
        assert_eq!(shape(content), shape(&after), "{content:?} became {after:?}");
    }
}

/// The kinds of block rumdl's own parser reads, extensions included.
fn rumdl_blocks(content: &str) -> Vec<String> {
    Parser::new_ext(content, rumdl_lib::utils::rumdl_parser_options_for(content))
        .filter_map(|event| match event {
            Event::Start(tag) => {
                let name = format!("{tag:?}");
                Some(name.split(|c: char| !c.is_alphanumeric()).next().unwrap().to_string())
            }
            _ => None,
        })
        .collect()
}

/// rumdl reads blocks CommonMark has none of, and indented text that spells
/// one becomes it the same way once its paragraph joins the item.
#[test]
fn a_paragraph_stays_outside_its_item_when_its_indented_text_would_become_an_extension_block() {
    for (content, block) in [
        ("- X\n\n a\n    [^n]: f\n", "FootnoteDefinition"),
        ("- X\n\n a\n    [^n\\]o]: f\n", "FootnoteDefinition"),
        ("- X\n\n a\n    : d\n", "DefinitionList"),
        ("- X\n\n a | b\n    --- | ---\n", "Table"),
        ("- X\n\n a | b\n    | - | - |\n", "Table"),
        ("- X\n\n a | b\n    | - - | -- |\n", "Table"),
        // An even run of backslashes ends the header without a hard break.
        ("- X\n\n a | b\\\\\n    --- | ---\n", "Table"),
        ("- X\n\n a | b\\ \n    --- | ---\n", "Table"),
    ] {
        let moved = content.replacen("\n a", "\n  a", 1);
        assert!(!rumdl_blocks(content).iter().any(|kind| kind == block), "{content:?}");
        assert!(rumdl_blocks(&moved).iter().any(|kind| kind == block), "{moved:?}");
        assert_eq!(fix(content, MD077), *content, "{content:?}");
    }
    for (content, expected) in [
        // A link reference definition and a tag CommonMark does not name
        // give way to the paragraph they stand in.
        ("- X\n\n a\n    [n]: f\n", "- X\n\n  a\n    [n]: f\n"),
        ("- X\n\n a\n    <video>b\n", "- X\n\n  a\n    <video>b\n"),
        ("- X\n\n a\n    | b |\n", "- X\n\n  a\n  | b |\n"),
        // A delimiter row under a line without a pipe heads no table.
        ("- X\n\n a\n    |---|---|\n", "- X\n\n  a\n  |---|---|\n"),
    ] {
        let after = fix(content, MD077);
        assert_eq!(after, expected, "{content:?}");
        let moved = content.replacen("\n a", "\n  a", 1);
        assert_eq!(rumdl_blocks(&moved), rumdl_blocks(&after), "{content:?}");
    }
}

/// Every marker line of a paragraph moves with the line opening it, in one
/// pass, however many of them there are.
#[test]
fn a_long_run_of_marker_lines_is_fixed_in_full() {
    let markers: String = (2..=150).map(|n| format!(" {n}. b\n")).collect();
    let content = format!("- X\n\n a\n{markers}");
    let after = fix(&content, MD077);
    assert_eq!(after, content.replace("\n ", "\n  "));
    assert_eq!(shape(&content), shape(&after));
}

/// A line short of the content column moves with its paragraph, and is read
/// where it lands. A paragraph also opens where the block above it ends,
/// without a blank line between them.
#[test]
fn the_fix_reads_lines_where_they_land_and_paragraphs_where_they_open() {
    for (content, moved) in [
        ("1234. X\n\n a\n    # h\n", "1234. X\n\n      a\n      # h\n"),
        ("- X\n\n  <!-- c -->\n a\n2. b\n", "- X\n\n  <!-- c -->\n  a\n2. b\n"),
        (
            "- X\n\n  ```\n  c\n  ```\n a\n2. b\n",
            "- X\n\n  ```\n  c\n  ```\n  a\n2. b\n",
        ),
        // A quote that ends in an empty line or a heading leaves no
        // paragraph open either.
        ("- X\n\n  >\n a\n2. b\n", "- X\n\n  >\n  a\n2. b\n"),
        ("- X\n\n  > # h\n a\n2. b\n", "- X\n\n  > # h\n  a\n2. b\n"),
        (
            "- X\n\n  > h\n  > ===\n a\n2. b\n",
            "- X\n\n  > h\n  > ===\n  a\n2. b\n",
        ),
    ] {
        assert_ne!(shape(content), shape(moved), "{content:?} against {moved:?}");
        assert_eq!(fix(content, MD077), content, "{content:?}");
    }
    // Nor does the last row of a table, where rumdl reads one.
    let content = "- X\n\n  | h |\n  |---|\n a\n2. b\n";
    let moved = "- X\n\n  | h |\n  |---|\n  a\n2. b\n";
    assert_ne!(rumdl_blocks(content), rumdl_blocks(moved));
    assert_eq!(fix(content, MD077), content);
    // A line that is not reported keeps its place and its reading: four
    // columns in, the quote marker is text of the paragraph on either side.
    let content = "1234. X\n\n a\n    > q\n";
    let after = fix(content, MD077);
    assert_eq!(after, "1234. X\n\n      a\n    > q\n");
    assert_eq!(shape(content), shape(&after));
}

/// The parser decides what a moved paragraph would become, so the lines
/// around it count as much as its own: a delimiter row already at the
/// content column, a block the quote above has closed, and whitespace the
/// parser accepts after a heading marker.
#[test]
fn the_fix_is_decided_by_the_blocks_it_would_produce() {
    for (content, moved) in [
        (
            "- X\n\n a\n    | h | h |\n  |-|-|\n",
            "- X\n\n  a\n  | h | h |\n  |-|-|\n",
        ),
        (
            "- X\n\n  > <div>x</div>\n a\n2. b\n",
            "- X\n\n  > <div>x</div>\n  a\n2. b\n",
        ),
        ("- X\n\n a\n    #\x0cTitle\n", "- X\n\n  a\n  #\x0cTitle\n"),
    ] {
        assert_ne!(
            rumdl_blocks(content),
            rumdl_blocks(moved),
            "{content:?} against {moved:?}"
        );
        assert_eq!(fix(content, MD077), content, "{content:?}");
    }
    // A quote that opens with `===` holds a paragraph, which the line below
    // it continues wherever it is indented.
    let content = "- X\n\n  > ===\n a\n    > b\n";
    let after = fix(content, MD077);
    assert_eq!(after, "- X\n\n  > ===\n  a\n  > b\n");
    assert_eq!(shape(content), shape(&after));
}

/// A list is tight until a paragraph joins one of its items, and the parser
/// writes the text of a tight item differently from a paragraph: without an
/// element around it, and with a task marker in another place. Neither is a
/// change to a block, so the paragraph below such an item is moved.
#[test]
fn a_paragraph_joins_an_item_whose_text_the_parser_writes_bare() {
    for (content, moved) in [
        ("- X\n  `b\n  c`\n\n a\n", "- X\n  `b\n  c`\n\n  a\n"),
        ("- [ ]\n  task\n\n a\n", "- [ ]\n  task\n\n  a\n"),
        ("- [x] task\n\n a\n", "- [x] task\n\n  a\n"),
        ("- X\n  <b>y\n  z</b>\n\n a\n", "- X\n  <b>y\n  z</b>\n\n  a\n"),
    ] {
        assert_eq!(fix(content, MD077), moved, "{content:?}");
    }
}

/// Joining a paragraph to the item above it can make one list of two, and
/// the items of the second are then numbered on from the first. Where every
/// item is written `1.` that is the list the author meant.
#[test]
fn a_paragraph_joining_its_item_can_join_two_ordered_lists() {
    let content = "1. X\n\n a\n1. Y\n";
    assert_eq!(fix(content, MD077), "1. X\n\n   a\n1. Y\n");
}

/// However many paragraphs stand outside one item, they are moved in one
/// run and none is left reported.
#[test]
fn many_paragraphs_outside_one_item_are_all_moved() {
    let content = format!("- X\n{}", "\n a\n".repeat(6500));
    let moved = format!("- X\n{}", "\n  a\n".repeat(6500));
    assert_eq!(fix(&content, MD077), moved);
}

/// A table row short of a cell above the list does not keep the fix from
/// being compared: text that would become an item still holds it back.
#[test]
fn a_short_table_row_above_does_not_release_a_marker_line() {
    let table = "| a | b |\n| - | - |\n| c |\n\n";
    let content = format!("{table}- X\n\n a\n2. b\n");
    assert_eq!(fix(&content, MD077), content);
    let content = format!("{table}- X\n\n a\n b\n");
    assert_eq!(fix(&content, MD077), format!("{table}- X\n\n  a\n  b\n"));
}

/// A footnote definition changes the line that refers to it, wherever that
/// line is. A paragraph whose move would make its last line a definition
/// stays where it is, and the paragraph holding the reference, which changes
/// nothing by itself, is still moved.
#[test]
fn a_paragraph_is_moved_though_another_would_define_its_footnote() {
    let defining = "- X\n\n a\n    [^n]: f\n";
    assert_eq!(fix(defining, MD077), defining);
    assert_eq!(
        fix(&format!("- S\n\n good [^n]\n\n# Heading\n\n{defining}"), MD077),
        format!("- S\n\n  good [^n]\n\n# Heading\n\n{defining}")
    );
    assert_eq!(
        fix(&format!("{defining}\n# Heading\n\n- S\n\n good [^n]\n"), MD077),
        format!("{defining}\n# Heading\n\n- S\n\n  good [^n]\n")
    );
    // Many paragraphs refer to it, and each of them is moved.
    let referring = |indent: &str| {
        (0..300)
            .map(|_| format!("- S\n\n{indent}good [^n]\n\n# Heading\n\n"))
            .collect::<String>()
    };
    assert_eq!(
        fix(&format!("{}{defining}", referring(" ")), MD077),
        format!("{}{defining}", referring("  "))
    );
}

/// A paragraph that stays outside its item has ended the list, so nothing
/// of the item below it reaches the item by moving. A fence moved to a deep
/// content column would be indented code, and a paragraph given the item's
/// indent would still stand outside it.
#[test]
fn nothing_below_a_paragraph_that_stays_outside_is_moved() {
    for content in [
        "100. X\n\n a\n    # h\n\n ~~~rust\n y\n ~~~\n",
        "- X\n\n a\n    # h\n\n ~~~rust\n y\n ~~~\n",
        "- X\n\n a\n    > b\n\n c\n",
        "- X\n\n a\n    > b\n\n c\n\n d\n",
        "- X\n\n a\n    # h\n\n   c\n",
    ] {
        assert_eq!(fix(content, MD077), content, "{content:?}");
    }
    // The item above and the item below are fixed as before.
    assert_eq!(
        fix("- S\n\n s\n\n- X\n\n a\n    > b\n\n c\n\n- Y\n\n y\n", MD077),
        "- S\n\n  s\n\n- X\n\n a\n    > b\n\n c\n\n- Y\n\n  y\n"
    );
    // A paragraph that does move takes the fence below it along.
    assert_eq!(
        fix("100. X\n\n a\n\n ~~~rust\n y\n ~~~\n", MD077),
        "100. X\n\n     a\n\n     ~~~rust\n     y\n     ~~~\n"
    );
}

/// A fence left of the content column ends the list as a paragraph does.
/// What stands below it reaches the item only with it, in the same run, and
/// not at all while the fence stays where it is.
/// A Kramdown extension block is hidden from the document, so a fence marker
/// inside one opens no code block, and the fixes below it are compared as
/// those of any other document.
#[test]
fn text_hidden_in_a_kramdown_extension_block_does_not_decide_the_fix() {
    let kramdown: &[&str] = &["--enable", "MD077", "--flavor", "kramdown"];
    for hidden in ["~~~", "plain", "```rust"] {
        let above = format!("{{::comment}}\n{hidden}\n{{:/comment}}\n\n");
        assert_eq!(
            fix(&format!("{above}- X\n\n a\n"), kramdown),
            format!("{above}- X\n\n  a\n"),
            "{hidden:?}"
        );
        // What the fix would turn into a block still stays where it is.
        for stays in ["- X\n\n a\n    # h\n", "- X\n\n a\n2. b\n"] {
            let content = format!("{above}{stays}");
            assert_eq!(fix(&content, kramdown), content, "{hidden:?} {stays:?}");
        }
    }
    // Outside Kramdown the braces are text and the fence is one.
    let content = "{::comment}\n~~~\n{:/comment}\n\n- X\n\n a\n";
    assert_eq!(fix(content, MD077), content);
}

/// A paragraph joins an item that holds a definition list, whose end the
/// parser places by what stands below it.
#[test]
fn a_paragraph_joins_an_item_that_holds_a_definition_list() {
    assert_eq!(fix("- X\n  : def\n\n a\n", MD077), "- X\n  : def\n\n  a\n");
}

/// A fence moves into its item with its text, however the text is indented
/// and whichever line ending the document uses.
#[test]
fn a_fence_joins_its_item_whatever_the_indentation_of_its_text() {
    for (content, fixed) in [
        ("- X\n\n ~~~\nx\ny\n ~~~\n", "- X\n\n  ~~~\n  x\n  y\n  ~~~\n"),
        (
            "- X\n\n ~~~\n x\ny\n\n  z\n ~~~\n",
            "- X\n\n  ~~~\n  x\n  y\n\n   z\n  ~~~\n",
        ),
        (
            "- X\n\n ~~~rust\nx\ny\n ~~~\n\n a\n",
            "- X\n\n  ~~~rust\n  x\n  y\n  ~~~\n\n  a\n",
        ),
    ] {
        assert_eq!(fix(content, MD077), fixed, "{content:?}");
        let (content, fixed) = (content.replace('\n', "\r\n"), fixed.replace('\n', "\r\n"));
        assert_eq!(fix(&content, MD077), fixed, "{content:?}");
    }
}

#[test]
fn what_stands_below_a_fence_outside_its_item_moves_only_with_it() {
    assert_eq!(
        fix("100. X\n\n ~~~rust\n y\n ~~~\n\n ~~~py\n z\n ~~~\n\n b\n", MD077),
        "100. X\n\n     ~~~rust\n     y\n     ~~~\n\n     ~~~py\n     z\n     ~~~\n\n     b\n"
    );
    // A paragraph is compared with the fence above it moved, as it will be:
    // the marker line is text behind the fence and a list inside the item.
    assert_eq!(
        fix("- X\n\n ~~~\n y\n ~~~\n\n a\n2. b\n", MD077),
        "- X\n\n  ~~~\n  y\n  ~~~\n\n a\n2. b\n"
    );
    // A lazy line of a paragraph the item holds ends nothing, so a block
    // below it does not wait for it.
    assert_eq!(
        fix("- X\n\n  a\n b <!-- rumdl-disable-line MD077 -->\n\n c\n", MD077),
        "- X\n\n  a\n b <!-- rumdl-disable-line MD077 -->\n\n  c\n"
    );
    let held = "100. X\n\n     <!-- rumdl-disable-next-line MD077 -->\n ~~~\n y\n ~~~\n\n ~~~\n z\n ~~~\n";
    assert_eq!(fix(held, MD077), held);
}

/// An indented line of a paragraph outside its item is left alone while the
/// paragraph stands there, and is brought to the content column once the
/// paragraph has moved, in the same run.
#[test]
fn an_indented_line_follows_its_paragraph_into_the_item() {
    assert_eq!(fix("- X\n\n a\n    b\n", MD077), "- X\n\n  a\n  b\n");
}

/// A list written below the paragraph belongs to whatever the paragraph
/// belongs to. Once the paragraph is the item's, an item indented to the
/// item's content is its child and an ordered item is numbered on from it:
/// the paragraph had cut the list in two, and the move undoes the cut.
#[test]
fn a_paragraph_joining_its_item_takes_the_list_below_it_along() {
    assert_eq!(fix("- X\n\n a\n\n  - Y\n", MD077), "- X\n\n  a\n\n  - Y\n");
    assert_eq!(fix("1. X\n\n a\n\n42. Y\n", MD077), "1. X\n\n   a\n\n42. Y\n");
}

/// Text that only resembles a block start is the same paragraph text on
/// either side of the move, so its paragraph is moved into the item.
#[test]
fn a_paragraph_joins_its_item_when_its_indented_text_stays_text() {
    for content in [
        "- X\n\n a | b\n    |---|---|---|\n",
        "- X\n\n a \\| b\n    |---|---|\n",
        "- X\n\n a\n    [^]: f\n",
        "- X\n\n a\n    [^a[b]: f\n",
        "- X\n\n a\n    </script>\n",
        "- X\n\n a\n    </textarea>\n",
        "- X\n\n a\n    <pre/>\n",
        "- X\n\n a | b\\\n    --- | ---\n",
        "- X\n\n a\n b | c\n    --- | ---\n",
        "- X\n\n  > q\n a\n2. b\n",
    ] {
        let fixed = fix(content, MD077);
        assert_ne!(fixed, content, "{content:?}");
        let kinds = |text: &str| {
            let mut kinds = rumdl_blocks(text);
            kinds.retain(|kind| kind != "Paragraph");
            kinds
        };
        assert_eq!(kinds(&fixed), kinds(content), "{content:?} became {fixed:?}");
    }
}
