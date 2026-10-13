//! `rumdl check --fix` must not move a fenced code block out of the list item
//! holding it, and must not change a code block's text.
//!
//! The oracle renders the document before and after the fix with pulldown-cmark
//! and compares, per code block, the containers it is nested in and its literal
//! text. Counting blocks alone would pass a fence leaving its item for the top
//! level, and a block whose text gained a newline.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// What a rendering holds that a fix must not change.
#[derive(Debug, PartialEq, Eq)]
struct Structure {
    items: usize,
    quotes: usize,
    /// Per code block in document order: the list, item and blockquote events it
    /// is nested in (outermost first), and its literal text.
    code_blocks: Vec<(Vec<&'static str>, String)>,
}

fn structure(content: &str) -> Structure {
    let mut items = 0;
    let mut quotes = 0;
    let mut path: Vec<&'static str> = Vec::new();
    let mut code_blocks = Vec::new();
    let mut current: Option<(Vec<&'static str>, String)> = None;
    for event in Parser::new_ext(content, Options::empty()) {
        match event {
            Event::Start(Tag::List(_)) => path.push("list"),
            Event::Start(Tag::Item) => {
                items += 1;
                path.push("item");
            }
            Event::Start(Tag::BlockQuote(_)) => {
                quotes += 1;
                path.push("quote");
            }
            Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_)) => {
                path.pop();
            }
            Event::Start(Tag::CodeBlock(_)) => {
                current = Some((path.clone(), String::new()));
            }
            Event::Text(text) => {
                if let Some(block) = current.as_mut() {
                    block.1.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => code_blocks.extend(current.take()),
            _ => {}
        }
    }
    Structure {
        items,
        quotes,
        code_blocks,
    }
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

const STRICT: &[&str] = &["--enable", "MD031"];

/// Documents with a fence at the start of an item, inside an item, in a quote and
/// outside both, in the shapes that corrupted a document and beside them.
const CORPUS: &[&str] = &[
    "-\n  ```\n  x\n  ```\n",
    "1.\n   ```\n   x\n   ```\n",
    "1)\n   ```\n   x\n   ```\n",
    "10.\n    ```\n    x\n    ```\n",
    "+\n  ```\n  x\n  ```\n",
    "*\n  ```\n  x\n  ```\n",
    "-   \n  ```\n  x\n  ```\n",
    "-\n  ~~~\n  x\n  ~~~\n",
    "-\n  ````rust\n  x\n  ````\n",
    "-\n  ```\n  x\n  ```\n  after\n",
    "-\n  ```\n  x\n  ```\n- b\n",
    "- a\n\n  -\n    ```\n    x\n    ```\n",
    "- a\n  -\n    ```\n    x\n    ```\n",
    "-\n  -\n    ```\n    x\n    ```\n",
    "- -\n    ```\n    x\n    ```\n",
    "- -\n  ```\n  x\n  ```\n",
    "1. -\n     ```\n     x\n     ```\n",
    "> -\n>   ```\n>   x\n>   ```\n",
    "> 1.\n>    ```\n>    x\n>    ```\n",
    ">-\n>  ```\n>  x\n>  ```\n",
    "> > -\n> >   ```\n> >   x\n> >   ```\n",
    "- >\n  ```\n  x\n  ```\n",
    "-\n  > ```\n  > x\n  > ```\n",
    "1.\n   - ```\n     x\n     ```\n",
    "-\n  - ```\n    x\n    ```\n",
    "-\n  1. ```\n     x\n     ```\n",
    "- ```\n  x\n  ```\n",
    "-   ```\n    x\n    ```\n",
    "a\n- ```\n  x\n  ```\n",
    "-\n\n  ```\n  x\n  ```\n",
    "-\n```\nx\n```\n",
    "-\n ```\n x\n ```\n",
    "a\n-\n  ```\n  x\n  ```\n",
    "a\n```\nx\n```\n",
    "- a\n  ```\n  x\n  ```\n",
    "text\n-\n  ```\n  x\n  ```\n",
    "- x\n\n-\n  ```\n  x\n  ```\n",
    "> ```\nfoo\n",
    "> ```\n> ~~~\nfoo\n",
    "> ````\n> ```\nfoo\n",
    "> ```\n>     ```\nfoo\n",
    "> ```\n> x\n> ```\nfoo\n",
    "> ```\n> \n",
    "- ```\nfoo\n",
    "-\n  ```\nfoo\n",
    "-\n  ```\n  x\nfoo\n",
    "> -\n>   ```\nfoo\n",
];

/// Documents whose final line is whitespace the parser keeps out of the code
/// text. Other rules' fixes (MD047 appends the missing newline) change those
/// texts, so they are held against the fence rule alone.
const UNTERMINATED_WHITESPACE_TAIL: &[&str] = &["> ```\n> ", "```\n  ", "> ```\n> ~~~\n> "];

#[test]
fn no_default_fix_moves_or_edits_a_code_block() {
    for content in CORPUS {
        let before = structure(content);
        let after = structure(&fix(content, &[]));
        assert_eq!(before, after, "{content:?}");
    }
}

#[test]
fn the_fence_rule_alone_keeps_every_code_block_where_it_is() {
    for content in CORPUS.iter().chain(UNTERMINATED_WHITESPACE_TAIL) {
        let before = structure(content);
        let after = structure(&fix(content, STRICT));
        assert_eq!(before, after, "{content:?}");
    }
}

#[test]
fn fixing_twice_changes_nothing_more() {
    for content in CORPUS {
        for extra in [&[][..], STRICT] {
            let once = fix(content, extra);
            let twice = fix(&once, extra);
            assert_eq!(once, twice, "{content:?} with {extra:?}");
        }
    }
}

/// Exact results: a fence that begins an item is left directly under its marker,
/// and a fence that has text above it still gets its blank line.
#[test]
fn the_fence_rule_writes_these_exact_results() {
    for (content, expected) in [
        ("-\n  ```\n  x\n  ```\n", "-\n  ```\n  x\n  ```\n"),
        ("1.\n   ```\n   x\n   ```\n", "1.\n   ```\n   x\n   ```\n"),
        ("-\n  ```\n  x\n  ```\n  after\n", "-\n  ```\n  x\n  ```\n\n  after\n"),
        ("> -\n>   ```\n>   x\n>   ```\n", "> -\n>   ```\n>   x\n>   ```\n"),
        ("- -\n    ```\n    x\n    ```\n", "- -\n    ```\n    x\n    ```\n"),
        ("1.\n   - ```\n     x\n     ```\n", "1.\n   - ```\n     x\n     ```\n"),
        // The fence belongs to the outer item, so the inner empty item is not
        // emptied by the blank line.
        ("- -\n  ```\n  x\n  ```\n", "- -\n\n  ```\n  x\n  ```\n"),
        // Outside the empty item, the fence gets its blank line like any other.
        ("-\n```\nx\n```\n", "-\n\n```\nx\n```\n"),
        // A setext underline is not an item.
        ("a\n-\n  ```\n  x\n  ```\n", "a\n-\n\n  ```\n  x\n  ```\n"),
        ("- a\n  ```\n  x\n  ```\n", "- a\n\n  ```\n  x\n  ```\n"),
        ("a\n```\nx\n```\n", "a\n\n```\nx\n```\n"),
        // An unclosed fence has no closing line to follow with a blank line.
        ("> ```\nfoo\n", "> ```\nfoo\n"),
        ("> ```\n> ", "> ```\n> "),
        ("```\n  ", "```\n  "),
        ("> ```\n> ~~~\nfoo\n", "> ```\n> ~~~\nfoo\n"),
        ("> ```\n> x\n> ```\nfoo\n", "> ```\n> x\n> ```\n>\nfoo\n"),
    ] {
        assert_eq!(fix(content, STRICT), expected, "{content:?}");
    }
}

/// Positive control: the harness sees a fix when one changes the rendering, so a
/// pass above is not a harness that observes nothing.
#[test]
fn the_oracle_sees_fixes_that_change_the_document() {
    let content = "a\n```\nx\n```\nb\n";
    let fixed = fix(content, STRICT);
    assert_eq!(fixed, "a\n\n```\nx\n```\n\nb\n");
    assert_ne!(content, fixed);
}

/// Negative control: the oracle tells a fence that left its item from one that
/// stayed, and a code block whose text gained a newline from one that did not.
#[test]
fn the_oracle_detects_a_fence_leaving_its_item_and_a_changed_text() {
    let inside = structure("-\n  ```\n  x\n  ```\n");
    let outside = structure("-\n\n  ```\n  x\n  ```\n");
    assert_eq!(inside.code_blocks.len(), outside.code_blocks.len());
    assert_ne!(inside, outside);

    let plain = structure("> ```\nfoo\n");
    let edited = structure("> ```\n>\nfoo\n");
    assert_eq!(plain.code_blocks.len(), edited.code_blocks.len());
    assert_ne!(plain, edited);
}
