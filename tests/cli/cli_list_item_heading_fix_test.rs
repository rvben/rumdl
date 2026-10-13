//! `rumdl check --fix` must not move a heading out of the list item holding it.
//!
//! The oracle renders the document before and after the fix with pulldown-cmark
//! and compares, per heading, the containers it is nested in. Counts alone would
//! pass `- x\n\n  ## a` becoming `- x\n\n## a`, which has the same headings, items
//! and code blocks with the heading in another place.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// What a rendering holds that a fix must not change.
#[derive(Debug, PartialEq, Eq)]
struct Structure {
    items: usize,
    code_blocks: usize,
    /// Per heading in document order: its level, its text, and the list, item and
    /// blockquote events it is nested in, outermost first.
    headings: Vec<(u8, String, Vec<&'static str>)>,
}

impl Structure {
    /// The same, without the heading levels and text that other heading rules
    /// may rewrite on purpose.
    fn shape(&self) -> (usize, usize, Vec<&[&'static str]>) {
        (
            self.items,
            self.code_blocks,
            self.headings.iter().map(|heading| heading.2.as_slice()).collect(),
        )
    }
}

fn structure(content: &str) -> Structure {
    let mut items = 0;
    let mut code_blocks = 0;
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
        headings,
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

const STRICT: &[&str] = &["--enable", "MD022,MD023"];

/// Documents with a heading inside a list item, in the shapes that corrupted it,
/// and headings outside every item beside them.
const CORPUS: &[&str] = &[
    "text\n    ---\n    - z\n===\n",
    "- x\n\n  text\n      ---\n      - z\n  ===\n",
    "-\n  ## a\n  text\n",
    "- x\n\n  ## a\n\n  text\n",
    "- x\n  ## a\n  text\n",
    "1. x\n\n   ## a\n\n   text\n",
    "- x\n  - y\n\n    ## a\n\n    text\n",
    "- x\n\n  ## a ##\n\n  text\n",
    "- x\n\n  ##  a\n\n  text\n",
    "- x\n\n    ## a\n\n  text\n",
    "- x\n\n  a\n  ===\n\n  text\n",
    "- x\n\n  a\n  ---\n\n  text\n",
    "1. x\n\n   a\n   ---\n",
    "-\n  a\n  ---\n  text\n",
    "-\n  a\n  b\n  ---\n  text\n",
    "- x\n  - y\n\n    a\n    ---\n\n    text\n",
    "10. x\n\n    ## a\n\n    text\n",
    "- x\n\n\t## a\n\n\ttext\n",
    "- x\n\n\t\t## a\n",
    "-\n  ## a\n",
    "1.\n   ## a\n   text\n",
    "-   \n  ## a\n  text\n",
    "-\n  ##  a\n  text\n",
    "-\n  -\n    ## a\n    text\n",
    "-\n\n  ## a\n\n  text\n",
    "-\n ## a\n",
    "- x\n  text\n    ## a\n",
    "- x\nlazy\n  ## a\n",
    "- x\n\n  a\nlazy\n  ---\n",
    "-\n  -\n\n    ## a\n",
    "- x\n  - y\n\n  paragraph\n\n    ## a\n",
    "-     code\n\n  ## a\n",
    "- x\n\n  > ## a\n",
    "> - x\n>\n>   ## a\n",
    "> -\n>   ## a\n>   text\n",
    "- x\n\n  a\n      ```\n  ---\n",
    "a\n    ```\n---\n",
    "- x\n\n      ## a\n",
    "-\n        ## a\n",
    "- -\n    ## a\n",
    "- -\n    a\n    ---\n",
    "1. -\n     ## a\n",
    "> x\n    ## a\n",
    "> > x\n    ## a\n",
    "- > x\n  ## a\n",
    "> - x\n    ## a\n",
    "para\n    ## a\n",
    "> x\n    a\n    ===\n",
    "- x\n\n  ## a\n\n- y\n\n  ## b\n",
    "# T\n\n- x\n\n  ### a\n\n  text\n",
    "1. x\n\n  ## a\n",
    "- x\n\nplain\n\n  ## a\n",
    "- x\n      ## A\n",
    "- x\n      - y\n        ## a\n",
    "- > x\n      ## a\n",
];

#[test]
fn no_default_fix_moves_a_heading_out_of_its_container() {
    for content in CORPUS {
        let before = structure(content);
        let after = structure(&fix(content, &[]));
        assert_eq!(before.shape(), after.shape(), "{content:?}");
    }
}

#[test]
fn the_heading_rules_alone_keep_every_heading_where_it_is() {
    for content in CORPUS {
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

/// Exact results for the shapes that corrupted a heading: the container's indent
/// stays and only the indentation past it goes.
#[test]
fn the_heading_rules_write_these_exact_results() {
    for (content, expected) in [
        ("- x\n\n    ## a\n\n  text\n", "- x\n\n  ## a\n\n  text\n"),
        ("- x\n\n    a\n    ---\n", "- x\n\n  a\n  ---\n"),
        ("-\n  ## a\n  text\n", "-\n  ## a\n\n  text\n"),
        ("- x\n  ## a\n  text\n", "- x\n\n  ## a\n\n  text\n"),
        ("-\n  a\n  ---\n  text\n", "-\n  a\n  ---\n\n  text\n"),
        ("-\n  -\n\n    ## a\n", "-\n  -\n\n  ## a\n"),
        (
            "- x\n  - y\n\n    ## a\n\n    text\n",
            "- x\n  - y\n\n    ## a\n\n    text\n",
        ),
        // Outside every item the heading still goes to the margin.
        ("-\n\n  ## a\n", "-\n\n## a\n"),
        ("- x\n\nplain\n\n  ## a\n", "- x\n\nplain\n\n## a\n"),
    ] {
        assert_eq!(fix(content, STRICT), expected, "{content:?}");
    }
}

/// Positive control: the harness sees a fix when one changes the rendering, so a
/// pass above is not a harness that observes nothing.
#[test]
fn the_oracle_sees_headings_that_fixes_change() {
    let content = "Text\n## a\n";
    let fixed = fix(content, STRICT);
    assert_eq!(fixed, "Text\n\n## a\n");
    assert_ne!(content, fixed);
    let content = "  ## a\ntext\n";
    assert_eq!(fix(content, STRICT), "## a\n\ntext\n");
}

/// Negative control: the oracle tells a heading that left its item from one that
/// stayed, although both documents hold the same headings, items and code.
#[test]
fn the_oracle_detects_a_heading_leaving_its_item() {
    let inside = structure("- x\n\n  ## a\n\n  text\n");
    let outside = structure("- x\n\n## a\n\n  text\n");
    assert_eq!(inside.headings.len(), outside.headings.len());
    assert_eq!(inside.items, outside.items);
    assert_ne!(inside.shape(), outside.shape());
    assert_ne!(inside, outside);
    // And the corruption that shipped: `text` becoming indented code.
    let corrupted = structure("- x\n  - y\n\n## a\n\n    text\n");
    assert_ne!(
        structure("- x\n  - y\n\n    ## a\n\n    text\n").shape(),
        corrupted.shape()
    );
}
