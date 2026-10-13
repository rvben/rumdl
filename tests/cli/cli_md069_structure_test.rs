//! `rumdl check --fix` for MD069 must change only the list nesting it reports.
//!
//! Collapsing `- - a` removes one list nesting level on purpose. Everything else
//! in the document has to keep its meaning: a thematic break must stay a break,
//! a paragraph must stay a paragraph, a list must stay one list, and a later line
//! must stay in the block it was in. The oracle renders the document before and
//! after the fix with pulldown-cmark and compares the block tree. For a line the
//! rule collapsed, the extra (list, item) pairs that start on that line are
//! removed from the tree before the document is compared.
//!
//! Runs use `--enable MD069` so that only this rule edits the document.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Block tree as (token, source line of the start) pairs; a token that closes a
/// block carries line 0. Text is part of the tree so that lost or moved text is seen.
fn tree(content: &str) -> Vec<(String, usize)> {
    let line_of = |offset: usize| content[..offset].matches('\n').count() + 1;
    let mut out = Vec::new();
    for (event, range) in Parser::new_ext(content, Options::empty()).into_offset_iter() {
        let line = line_of(range.start);
        match event {
            Event::Start(Tag::List(first)) => out.push((format!("list:{first:?}"), line)),
            Event::Start(Tag::Item) => out.push(("item".into(), line)),
            Event::Start(Tag::BlockQuote(_)) => out.push(("quote".into(), line)),
            Event::Start(Tag::Paragraph) => out.push(("para".into(), line)),
            Event::Start(Tag::Heading { level, .. }) => out.push((format!("heading:{}", level as u8), line)),
            Event::Start(Tag::CodeBlock(_)) => out.push(("code".into(), line)),
            Event::End(
                TagEnd::List(_)
                | TagEnd::Item
                | TagEnd::BlockQuote(_)
                | TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::CodeBlock,
            ) => out.push(("end".into(), 0)),
            Event::Start(Tag::HtmlBlock) => out.push(("html".into(), line)),
            Event::End(TagEnd::HtmlBlock) => out.push(("end".into(), 0)),
            Event::Html(text) | Event::InlineHtml(text) | Event::Code(text) => out.push((format!("raw:{text}"), 0)),
            Event::Rule => out.push(("hr".into(), line)),
            Event::Text(text) => out.push((format!("text:{text}"), 0)),
            Event::SoftBreak => out.push(("soft".into(), 0)),
            _ => {}
        }
    }
    out
}

fn tokens(tree: &[(String, usize)]) -> Vec<String> {
    tree.iter().map(|(token, _)| token.clone()).collect()
}

/// The tree of `content` with the extra (list, item) pairs that start on `line`
/// removed: the nesting levels the collapse is meant to take away.
fn tree_without_extra_nesting(content: &str, line: usize) -> Vec<String> {
    let tree = tree(content);
    let mut remove = vec![false; tree.len()];
    // The first item that starts on `line` is the outer item and stays; every
    // later item on that line sits in a list opened just before it.
    let mut items_on_line = 0;
    for (i, (token, start_line)) in tree.iter().enumerate() {
        if token == "item" && *start_line == line {
            items_on_line += 1;
            if items_on_line > 1 {
                assert!(tree[i - 1].0.starts_with("list:"), "an inner item opens its own list");
                remove[i] = true;
                remove[i - 1] = true;
            }
        }
    }
    // A closing token is removed exactly when the token that opened it is.
    let mut stack: Vec<usize> = Vec::new();
    for (i, (token, _)) in tree.iter().enumerate() {
        if token == "end" {
            let opened = stack.pop().expect("balanced tree");
            remove[i] = remove[opened];
        } else if !token.starts_with("text:") && !token.starts_with("raw:") && token != "soft" && token != "hr" {
            stack.push(i);
        }
    }
    tree.iter()
        .enumerate()
        .filter(|(i, _)| !remove[*i])
        .map(|(_, (token, _))| token.clone())
        .collect()
}

fn run_fix(content: &str) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    // The exit status reports what is left unfixed, which is not under test.
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--isolated", "--fix", "--enable", "MD069"])
        .arg(&path)
        .output()
        .expect("run rumdl check --fix");
    fs::read_to_string(&path).unwrap()
}

fn warns(content: &str) -> bool {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--isolated", "--enable", "MD069"])
        .arg(&path)
        .output()
        .expect("run rumdl check");
    String::from_utf8_lossy(&out.stdout).contains("MD069")
}

// === Controls: the oracle must see structure changes and must not invent them ===

#[test]
fn oracle_sees_a_thematic_break_become_a_list() {
    assert_ne!(tokens(&tree("1.\n    - - -\n")), tokens(&tree("1.\n    - -\n")));
}

#[test]
fn oracle_sees_a_paragraph_become_a_heading() {
    assert_ne!(tokens(&tree("o\n- - \n")), tokens(&tree("o\n- \n")));
}

#[test]
fn oracle_sees_a_list_split() {
    assert_ne!(tokens(&tree("- a\n- * b\n- c\n")), tokens(&tree("- a\n* b\n- c\n")));
}

#[test]
fn oracle_sees_a_paragraph_become_code() {
    assert_ne!(tokens(&tree("- - a\n\n      b\n")), tokens(&tree("- a\n\n      b\n")));
}

#[test]
fn oracle_sees_an_item_change_parent() {
    // After the extra nesting of line 1 is taken away, `b` is still a sibling of
    // `a`; collapsing by hand moves it under `a`, and the collapse oracle sees it.
    assert_ne!(
        tree_without_extra_nesting("- - a\n  - b\n", 1),
        tokens(&tree("- a\n  - b\n"))
    );
}

#[test]
fn oracle_sees_a_lost_html_block() {
    assert_ne!(tokens(&tree("- - <div>x</div>\n")), tokens(&tree("- \n")));
}

#[test]
fn a_document_without_duplicates_keeps_its_tree() {
    // The fixer leaves it alone and the oracle, shown above to see changes, agrees.
    let doc = "- a\n- b\n\n    code\n";
    let fixed = run_fix(doc);
    assert!(!tree(doc).is_empty());
    assert_eq!(tokens(&tree(&fixed)), tokens(&tree(doc)));
}

#[test]
fn oracle_tree_of_a_plain_document_is_what_it_should_be() {
    let doc = "- a\n- b\n\n    code\n";
    let expected = [
        "list:None",
        "item",
        "para",
        "text:a",
        "end",
        "end",
        "item",
        "para",
        "text:b",
        "end",
        "para",
        "text:code",
        "end",
        "end",
        "end",
    ];
    assert_eq!(tokens(&tree(doc)), expected);
    assert_eq!(run_fix(doc), doc);
}

#[test]
fn oracle_removes_exactly_the_collapsed_nesting() {
    // Positive control for the collapse oracle itself.
    assert_eq!(
        tree_without_extra_nesting("- - a\n- b\n", 1),
        tokens(&tree("- a\n- b\n"))
    );
    assert_eq!(tree_without_extra_nesting("- - - a\n", 1), tokens(&tree("- a\n")));
    // Not the same when a different line is named.
    assert_ne!(
        tree_without_extra_nesting("- - a\n- b\n", 2),
        tokens(&tree("- a\n- b\n"))
    );
}

// === Collapsed lines: exact output and the removed nesting is the only change ===

fn assert_collapse(input: &str, line: usize, expected: &str) {
    let fixed = run_fix(input);
    assert_eq!(fixed, expected, "output for {input:?}");
    assert_eq!(
        tokens(&tree(&fixed)),
        tree_without_extra_nesting(input, line),
        "only the extra nesting of line {line} may change for {input:?}"
    );
    assert_eq!(run_fix(&fixed), fixed, "second pass changed {input:?}");
    assert!(!warns(&fixed), "still reported after the fix: {input:?}");
}

#[test]
fn collapse_cases() {
    assert_collapse("- - a\n", 1, "- a\n");
    assert_collapse("- - a\n- b\n", 1, "- a\n- b\n");
    assert_collapse("- - - a\n", 1, "- a\n");
    assert_collapse("- a\n- * b\n- c\n", 2, "- a\n- b\n- c\n");
    assert_collapse("* - a\n", 1, "* a\n");
    assert_collapse("+ + a\n", 1, "+ a\n");
    assert_collapse("-\t-\tx\n", 1, "- x\n");
    assert_collapse("1. x\n   - - a\n", 2, "1. x\n   - a\n");
    assert_collapse("- p\n  - - a\n", 2, "- p\n  - a\n");
    assert_collapse("- Foo\n- - * * *\n", 2, "- Foo\n- * * *\n");
    assert_collapse("o\n- - x\n", 2, "o\n- x\n");
    assert_collapse("- - a\nb\n", 1, "- a\nb\n");
    assert_collapse("- - a\n\nb\n", 1, "- a\n\nb\n");
    assert_collapse("- - a\n# h\n\n      c\n", 1, "- a\n# h\n\n      c\n");
}

// === Declined: the document is left byte for byte as it was ===

fn assert_untouched(input: &str, reported: bool) {
    assert_eq!(run_fix(input), input, "document changed: {input:?}");
    assert_eq!(warns(input), reported, "reported state of {input:?}");
}

#[test]
fn thematic_breaks_are_left_alone() {
    for c in [
        "* * *\n",
        "- - -\n",
        "_ _ _\n",
        "- - - -\n",
        "1.\n    - - -\n",
        "- Foo\n- * * *\n",
        "- Foo\n- * * *\n- Bar\n",
    ] {
        assert_untouched(c, false);
    }
}

#[test]
fn text_that_is_not_a_list_item_is_left_alone() {
    assert_untouched("a\n    - - b\n", false);
    assert_untouched("> - - a\n", false);
    assert_untouched("```\n- - a\n```\n", false);
}

#[test]
fn empty_and_code_remainders_are_left_alone() {
    assert_untouched("o\n- - \n", false);
    assert_untouched("- -     x\n", false);
}

#[test]
fn a_collapse_that_would_be_a_thematic_break_is_left_alone() {
    assert_untouched("- * - - -\n", true);
}

#[test]
fn lines_that_depend_on_the_content_column_are_left_alone() {
    for c in [
        "- - a\n\n      b\n",
        "- - a\n    - b\n",
        "- - a\n    ---\n",
        "- - a\n  b\n",
        "- - a\nb\n\n      c\n",
        "- - a\nb\n  - c\n",
        "> - - a\n> \n>       b\n",
    ] {
        let reported = !c.starts_with('>');
        assert_untouched(c, reported);
    }
}
