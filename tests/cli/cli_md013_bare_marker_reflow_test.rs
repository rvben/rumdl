//! MD013 reflow must not move text into or out of a list item written as a bare
//! marker (`-`, `1.`, `1)` with nothing after it).
//!
//! CommonMark makes such an item's content column one past the marker, so the
//! line below it belongs to the item only when indented that far; any other line
//! starts a block outside the item. A reflow that joins the line onto the marker
//! (or absorbs a sibling marker into a paragraph) moves text across that boundary.
//!
//! The oracle renders the document before and after the fix with pulldown-cmark
//! and compares the container tree together with each block's text, whitespace
//! collapsed (reflow legitimately moves soft breaks). Counting items alone would
//! pass a paragraph that left its item for the top level.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

const CONFIG: &str = "[MD013]\nreflow = true\nline-length = 40\n";

/// The container tree of a rendering with the text of its leaf blocks.
///
/// Text in a tight list item arrives without paragraph events, so text outside
/// an explicit leaf block is collected as an implicit paragraph that ends at the
/// next block event. Dropping it would hide exactly the text that moves between
/// an item and its surroundings.
fn tree(content: &str) -> String {
    fn flush(out: &mut Vec<String>, leaf: &mut Option<(&'static str, String)>) {
        if let Some((kind, buf)) = leaf.take() {
            let text = if kind == "code" {
                format!("{buf:?}")
            } else {
                buf.split_whitespace().collect::<Vec<_>>().join(" ")
            };
            out.push(format!("{kind}:{text}"));
        }
    }
    let mut out: Vec<String> = Vec::new();
    let mut leaf: Option<(&'static str, String)> = None;
    for event in Parser::new_ext(content, Options::empty()) {
        match event {
            Event::Start(Tag::List(_)) => {
                flush(&mut out, &mut leaf);
                out.push("(list".into());
            }
            Event::Start(Tag::Item) => {
                flush(&mut out, &mut leaf);
                out.push("(item".into());
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush(&mut out, &mut leaf);
                out.push("(quote".into());
            }
            Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_)) => {
                flush(&mut out, &mut leaf);
                out.push(")".into());
            }
            Event::Start(Tag::Paragraph) => {
                flush(&mut out, &mut leaf);
                leaf = Some(("p", String::new()));
            }
            Event::Start(Tag::Heading { .. }) => {
                flush(&mut out, &mut leaf);
                leaf = Some(("h", String::new()));
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut out, &mut leaf);
                leaf = Some(("code", String::new()));
            }
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock) => flush(&mut out, &mut leaf),
            Event::Text(text) | Event::Code(text) => {
                leaf.get_or_insert(("p", String::new())).1.push_str(&text);
            }
            Event::SoftBreak => {
                leaf.get_or_insert(("p", String::new())).1.push(' ');
            }
            Event::HardBreak => {
                leaf.get_or_insert(("p", String::new())).1.push_str("<br>");
            }
            _ => {}
        }
    }
    flush(&mut out, &mut leaf);
    out.join(" ")
}

/// The documents after `rumdl check --fix` with MD013 reflow at width 40, one
/// binary run for the whole batch. The exit status reports what is left unfixed,
/// which is not under test.
fn fix_all(documents: &[String], extra: &[&str]) -> Vec<String> {
    let dir = tempdir().unwrap();
    let config = dir.path().join("rumdl.toml");
    fs::write(&config, CONFIG).unwrap();
    let docs = dir.path().join("docs");
    fs::create_dir(&docs).unwrap();
    for (n, content) in documents.iter().enumerate() {
        fs::write(docs.join(format!("d{n}.md")), content).unwrap();
    }
    Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--no-cache", "--fix", "--enable", "MD013", "--config"])
        .arg(&config)
        .args(extra)
        .arg(&docs)
        .output()
        .expect("run rumdl check --fix");
    (0..documents.len())
        .map(|n| fs::read_to_string(docs.join(format!("d{n}.md"))).unwrap())
        .collect()
}

fn fix(content: &str) -> String {
    fix_all(&[content.to_string()], &[]).remove(0)
}

const LONG: &str = "This is not in the list at all. It is a long sentence that wraps around twice.";
const TAIL: &str = "Tail words of the paragraph that also run long enough.";

/// Bare markers in every container the reflow has a path for, the lines below
/// them at every indentation that matters, and the shapes of paragraph that can
/// follow (one line, two, lazy continuation, a sibling marker).
fn corpus() -> Vec<String> {
    let containers = [
        ("", ""),
        ("> ", "> "),
        (">", ">"),
        ("> > ", "> > "),
        (">> ", ">> "),
        ("- ", "  "),
        ("1. ", "   "),
        ("> - ", ">   "),
        ("- > ", "  > "),
        ("  ", "  "),
        ("- - ", "    "),
    ];
    let markers = ["-", "*", "+", "1.", "1)", "2.", "10."];
    let indents = [
        "", " ", "  ", "   ", "    ", "     ", "      ", "       ", "        ", "\t", " \t",
    ];
    let mut docs = Vec::new();
    for (first, rest) in containers {
        let lazy = rest.trim_end();
        for marker in markers {
            for indent in indents {
                docs.push(format!("{first}{marker}\n{rest}{indent}{LONG}\n"));
                docs.push(format!("{first}{marker}\n{rest}{indent}{LONG}\n{rest}{indent}{LONG}\n"));
                docs.push(format!(
                    "{first}{marker}\n{rest}{indent}{LONG}\n{lazy} {TAIL}\n{lazy} {TAIL}\n"
                ));
                docs.push(format!(
                    "{first}{marker}\n{rest}{indent}{LONG}\n{rest}{marker}\n{rest}{indent}{TAIL}\n"
                ));
                // Lines at the enclosing container's own indentation, which are
                // lazy continuations of a paragraph indented further.
                docs.push(format!(
                    "{first}{marker}\n{rest}{indent}{LONG}\n{rest}{TAIL}\n{rest}{TAIL}\n"
                ));
            }
        }
    }
    docs
}

#[test]
fn reflow_keeps_every_block_in_the_container_it_was_written_in() {
    let docs = corpus();
    let fixed = fix_all(&docs, &[]);
    let mut failures = Vec::new();
    for (before, after) in docs.iter().zip(&fixed) {
        if tree(before) != tree(after) {
            failures.push(format!(
                "{before:?} -> {after:?}\n  before: {}\n  after:  {}",
                tree(before),
                tree(after)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} documents changed structure; first ones:\n{}",
        failures.len(),
        docs.len(),
        failures.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn fixing_twice_changes_nothing_more() {
    let docs = corpus();
    let once = fix_all(&docs, &[]);
    let twice = fix_all(&once, &[]);
    let unstable: Vec<String> = once
        .iter()
        .zip(&twice)
        .filter(|(a, b)| a != b)
        .map(|(a, b)| format!("{a:?} -> {b:?}"))
        .collect();
    assert!(
        unstable.is_empty(),
        "{} unstable, first: {:?}",
        unstable.len(),
        &unstable[..unstable.len().min(5)]
    );
}

/// Regression inputs from the fix-corruption sweep.
#[test]
fn reported_inputs_keep_their_structure() {
    for content in [
        // Text indented short of the content column is outside the empty item.
        "> 1.\n>   This is not in the list at all. It is a long sentence that wraps.\n",
        // An item opened without a space after the quote marker.
        ">1.\n>       The dot in ild paragraph actually\n>       t\n",
        "> >1.\n> >  This is not in the list at all. It is a long sentence that wraps.\n",
        // A bare marker written on the marker line of another item.
        "> - 1.\n>     This is not in the list at all. It is a long sentence that wraps.\n",
        "- 1.\n  This is not in the list at all. It is a long sentence that wraps.\n",
        // Lazy lines at the outer item's indentation under text in the inner one.
        "- 1.\n     This is not in the list at all. It is a long sentence that wraps.\n  Tail words here.\n  More tail words here.\n",
        // A sibling bare marker must not be absorbed into the paragraph above.
        "-\n  This is not in the list at all. It is a long sentence.\n-\n  More text that is also long enough to wrap.\n",
        "1.\n   This is not in the list at all. It is a long sentence.\n2.\n",
    ] {
        assert_eq!(tree(content), tree(&fix(content)), "{content:?}");
    }
}

/// The lines below a bare marker keep the indentation that decides which block
/// they belong to, so the intended wrap is exact.
#[test]
fn text_under_a_bare_marker_wraps_in_place() {
    for (content, expected) in [
        // Outside the empty item: stays outside.
        (
            "> 1.\n>   This is not in the list at all. It is a long sentence that wraps.\n",
            "> 1.\n>   This is not in the list at all. It\n>   is a long sentence that wraps.\n",
        ),
        // Inside the item: stays inside.
        (
            "> 1.\n>    This is not in the list at all. It is a long sentence that wraps.\n",
            "> 1.\n>    This is not in the list at all. It\n>    is a long sentence that wraps.\n",
        ),
        (
            "-\n  This is not in the list at all. It is a long sentence that wraps.\n",
            "-\n  This is not in the list at all. It is\n  a long sentence that wraps.\n",
        ),
        // A sibling bare marker ends the paragraph above it.
        (
            "-\n  This is not in the list at all. It is a long sentence.\n-\n  Short.\n",
            "-\n  This is not in the list at all. It is\n  a long sentence.\n-\n  Short.\n",
        ),
    ] {
        assert_eq!(fix(content), expected, "{content:?}");
    }
}

/// Positive control: reflow ran in this harness, so a document left alone above is
/// a decision of the rule and not a harness that never reflows.
#[test]
fn the_harness_reflows_an_ordinary_paragraph() {
    assert_eq!(
        fix("This is a long paragraph that must wrap at forty characters.\n"),
        "This is a long paragraph that must wrap\nat forty characters.\n"
    );
    assert_eq!(
        fix("> This is a long paragraph that must wrap at forty characters.\n"),
        "> This is a long paragraph that must\n> wrap at forty characters.\n"
    );
}

/// Negative control: the oracle tells a paragraph inside an empty item's
/// successor from one outside it, and a code block from a paragraph.
#[test]
fn the_oracle_tells_inside_from_outside() {
    assert_ne!(tree("> 1.\n>   text\n"), tree("> 1. text\n"));
    assert_ne!(tree("-\n  a\n-\n  b\n"), tree("-\n  a - b\n"));
    assert_ne!(tree(">1.\n>       text\n"), tree(">       1. text\n"));
    // Text in a tight item has no paragraph events and is still compared.
    assert_ne!(tree("- 1.\n  text\n"), tree("- 1. text\n"));
    assert_ne!(tree("- 1.\n  text\n"), tree("- 1.\n  other\n"));
}
