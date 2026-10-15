//! Under the MkDocs flavor MD077 must place a nested item's continuation where
//! Python-Markdown reads it as that item's content: four columns per nesting
//! level, not the CommonMark content column.
//!
//! pulldown-cmark is CommonMark and cannot judge MkDocs structure, so the
//! expectations are exact outputs. The documents were rendered with
//! Python-Markdown 3.11 (`markdown.markdown(text, extensions=["tables",
//! "def_list", "admonition", "fenced_code"])`); the rendering that fixes each
//! expectation is quoted beside it. The standard-flavor controls do use the
//! pulldown-cmark structure oracle.

use super::cli_md077_continuation_structure_test::structure;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// One `rumdl check --fix` run with MD077 alone under `flavor`.
fn fix_once(content: &str, flavor: &str) -> String {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    // The exit status reports what is left unfixed, which is not under test.
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(dir.path())
        .args([
            "check",
            "--isolated",
            "--no-cache",
            "--fix",
            "--flavor",
            flavor,
            "--enable",
            "MD077",
        ])
        .arg(&path)
        .output()
        .expect("run rumdl check --fix");
    fs::read_to_string(&path).unwrap()
}

/// Layouts Python-Markdown reads as content of the innermost item.
const CORRECT: &[&str] = &[
    // <ul><li>a<ul><li><p>b</p><p>text</p></li></ul></li></ul>
    "- a\n    - b\n\n        text\n",
    // <li>a<ul><li>b<ul><li><p>c</p><p>text</p></li></ul></li></ul></li>
    "- a\n    - b\n        - c\n\n            text\n",
    // <ol><li>a<ol><li><p>b</p><p>text</p></li></ol></li></ol>
    "1. a\n    1. b\n\n        text\n",
    "1. a\n    - b\n\n        text\n",
    "- a\n    1. b\n\n        text\n",
    // <ul><li><p>a</p><ul><li><p>b</p></li><li><p>c</p><p>text</p></li></ul></li></ul>
    "- a\n    - b\n\n    - c\n\n        text\n",
    // <ul><li><ul><li><p>a</p><p>text</p></li></ul></li></ul>
    "- - a\n\n        text\n",
    // <ul><li><ul><li>a</li><li><p>b</p><p>text</p></li></ul></li></ul>
    "- a\n    - b\n\n        ```\n        code\n        ```\n",
    "- - a\n    - b\n\n        text\n",
    // <ol><li>a</li><li>b<pre><code>text</code></pre></li></ol>: code stays code
    "1. a\n   - b\n\n        text\n",
];

#[test]
fn mkdocs_continuation_at_the_nesting_column_is_left_alone() {
    for content in CORRECT {
        assert_eq!(&fix_once(content, "mkdocs"), content, "{content:?}");
    }
}

/// Text at the CommonMark column of a nested item is a paragraph of its parent
/// in Python-Markdown (`<li><p>a</p><ul><li>b</li></ul><p>text</p></li>`); the
/// fix moves it to the item's own column, where it joins the item.
#[test]
fn mkdocs_continuation_at_the_commonmark_column_moves_to_the_nesting_column() {
    for (content, fixed) in [
        ("- a\n    - b\n\n      text\n", "- a\n    - b\n\n        text\n"),
        (
            "- a\n    - b\n        - c\n\n          text\n",
            "- a\n    - b\n        - c\n\n            text\n",
        ),
        ("1. a\n    1. b\n\n       text\n", "1. a\n    1. b\n\n        text\n"),
        ("- a\n    - b\n\n         text\n", "- a\n    - b\n\n        text\n"),
    ] {
        assert_eq!(fix_once(content, "mkdocs"), fixed, "{content:?}");
    }
}

#[test]
fn mkdocs_nesting_fix_reaches_its_fixed_point_in_one_application() {
    for content in [
        "- a\n    - b\n\n      text\n",
        "- a\n    - b\n        - c\n\n          text\n",
        "1. a\n    1. b\n\n       text\n",
        "- - a\n\n      text\n",
        "- a\n    - b\n\n         text\n",
    ] {
        let once = fix_once(content, "mkdocs");
        assert_eq!(once, fix_once(&once, "mkdocs"), "{content:?}");
    }
}

/// Control: the standard flavor keeps the CommonMark content column and its fix
/// preserves the structure.
#[test]
fn standard_flavor_nested_continuation_keeps_the_content_column() {
    let content = "- a\n    - b\n\n        text\n";
    let after = fix_once(content, "standard");
    assert_eq!(after, "- a\n    - b\n\n      text\n");
    assert_eq!(structure(content), structure(&after));
}

/// Only the first line after a blank can open Python-Markdown's indented code
/// block; a deeper line inside a paragraph continues that paragraph
/// (`<p>para text</p>`) and is snapped back like any over-indented continuation.
#[test]
fn mkdocs_over_indented_paragraph_continuation_is_not_taken_for_code() {
    assert_eq!(
        fix_once("- a\n\n    para\n        text\n", "mkdocs"),
        "- a\n\n    para\n    text\n"
    );
    assert_eq!(
        fix_once("- a\n    - b\n\n        para\n            text\n", "mkdocs"),
        "- a\n    - b\n\n        para\n        text\n"
    );
}

/// A marker two levels below its parent is text of the parent's paragraph for
/// Python-Markdown, and what follows it is literal code that must keep its
/// indentation: `<ol><li>a - b<pre><code>  text</code></pre></li></ol>`.
#[test]
fn mkdocs_marker_that_python_markdown_does_not_nest_is_left_alone() {
    for content in [
        "123456. a\n        - b\n\n          text\n",
        "-    a\n        - b\n\n            text\n",
    ] {
        assert_eq!(fix_once(content, "mkdocs"), content, "{content:?}");
    }
}

/// A marker with nothing after it is paragraph text for Python-Markdown
/// (`<p>-    - b</p><pre><code>  text</code></pre>`), so the line below is
/// literal code that keeps its indentation. With a space after the marker it is
/// an item and the line is its content.
#[test]
fn mkdocs_bare_marker_is_not_an_item() {
    for content in [
        "-\n    - b\n\n      text\n",
        "1.\n    - b\n\n      text\n",
        "- a\n    -\n\n      text\n",
        "-\n    -\n\n        text\n",
    ] {
        assert_eq!(fix_once(content, "mkdocs"), content, "{content:?}");
    }
    assert_eq!(
        fix_once("- \n    - b\n\n      text\n", "mkdocs"),
        "- \n    - b\n\n        text\n"
    );
}

/// The bare marker above a heading is not the parent of the list below it.
#[test]
fn mkdocs_bare_marker_does_not_unscope_a_later_list() {
    assert_eq!(
        fix_once("-\n\n# Heading\n\n  - b\n\n     text\n", "mkdocs"),
        "-\n\n# Heading\n\n  - b\n\n    text\n"
    );
}

/// Python-Markdown does not know `1)`: `b` is text of `a`, so text at column 7
/// is a paragraph of `a` and text at column 8 is indented code.
#[test]
fn mkdocs_paren_marker_is_not_an_item() {
    let paragraph = "- a\n    1) b\n\n       text\n";
    assert_eq!(fix_once(paragraph, "mkdocs"), paragraph);
    let code = "- a\n    1) b\n\n        text\n";
    assert_eq!(fix_once(code, "mkdocs"), code);
}

/// Python-Markdown reads `- b` as text of `- a` here, so the text at column 10
/// is a paragraph; at column 12 it would be code.
#[test]
fn mkdocs_item_below_a_same_line_item_is_text() {
    let content = "1. - a\n        - b\n\n          text\n";
    assert_eq!(fix_once(content, "mkdocs"), content);
}
