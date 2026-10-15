//! `rumdl check --fix` for MD065 must not turn text or raw HTML into a rule's
//! neighbours: a run of `*`, `-` or `_` that the parser reads as paragraph text,
//! a line of an HTML block or a line of a footnote paragraph is not a thematic
//! break, however the containers around it are written.
//!
//! The oracle parses the document before and after the fix with pulldown-cmark
//! (footnotes on) and lists every leaf block with the containers around it, HTML
//! blocks and footnote paragraphs included, so a fix that splits a paragraph, a
//! raw HTML block or a footnote body shows as a different list.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

/// Every leaf block as `containers > kind: text`, in document order. Tight and
/// loose lists are not told apart.
fn skeleton(content: &str) -> Vec<String> {
    let mut path: Vec<&'static str> = Vec::new();
    let mut blocks = Vec::new();
    let mut text = String::new();
    let mut open: Option<&'static str> = None;
    for event in Parser::new_ext(content, Options::ENABLE_FOOTNOTES) {
        let boundary = matches!(
            event,
            Event::Rule
                | Event::Start(
                    Tag::List(_)
                        | Tag::Item
                        | Tag::BlockQuote(_)
                        | Tag::FootnoteDefinition(_)
                        | Tag::Heading { .. }
                        | Tag::CodeBlock(_)
                        | Tag::HtmlBlock
                )
                | Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_) | TagEnd::FootnoteDefinition)
        );
        if boundary && !text.is_empty() {
            blocks.push(format!("{} > text: {}", path.join(" "), text.trim()));
            text.clear();
        }
        match event {
            Event::Start(Tag::List(_)) => path.push("list"),
            Event::Start(Tag::Item) => path.push("item"),
            Event::Start(Tag::BlockQuote(_)) => path.push("quote"),
            Event::Start(Tag::FootnoteDefinition(_)) => path.push("footnote"),
            Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::BlockQuote(_) | TagEnd::FootnoteDefinition) => {
                path.pop();
            }
            Event::Rule => blocks.push(format!("{} > rule", path.join(" "))),
            Event::Start(Tag::Heading { .. }) => open = Some("heading"),
            Event::Start(Tag::CodeBlock(_)) => open = Some("code"),
            Event::Start(Tag::HtmlBlock) => open = Some("html"),
            Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t) => text.push_str(&t),
            Event::SoftBreak => text.push(' '),
            Event::End(TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::HtmlBlock) => {
                if let Some(kind) = open.take() {
                    blocks.push(format!("{} > {kind}: {}", path.join(" "), text.trim()));
                    text.clear();
                }
            }
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

/// The number of MD065 warnings `rumdl check` prints for `content`.
fn md065_warnings(content: &str) -> usize {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doc.md");
    fs::write(&path, content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .args(["check", "--isolated", "--no-cache", "--enable", "MD065"])
        .arg(&path)
        .output()
        .expect("run rumdl check");
    String::from_utf8_lossy(&output.stdout).matches("[MD065]").count()
}

const ONLY_MD065: &[&str] = &["--enable", "MD065"];

/// Text the parser reads as part of a paragraph or an HTML block, where MD065 has
/// no rule to surround.
const NOT_RULES: &[&str] = &[
    // A tag alone on a line cannot interrupt a paragraph.
    "> o\n> <audio>\n>     ***\n",
    "> o\n> <video src=\"x\">\n>     ***\n",
    "> o\n> </audio>\n>     ***\n",
    "> o\n> </pre>\n>     ***\n",
    "> o\n> </script>\n>     ***\n",
    "> o\n> </style>\n>     ***\n",
    "> o\n> </textarea>\n>     ***\n",
    "- o\n  </pre>\n      ***\n",
    "> o\n> <audio/>\n>     ***\n",
    "> o\n> <span>\n>     ***\n",
    "> > o\n> > <audio>\n> >     ***\n",
    "> - o\n>   <audio>\n>       ***\n",
    "- > o\n  > <audio>\n  >     ***\n",
    "- o\n  <audio>\n      ***\n",
    // The same tag with no paragraph above it opens an HTML block.
    "> <audio>\n>     ***\n",
    "> <audio>\n> ***\n",
    "> <span>\n>     ***\n",
    "- <audio>\n  ***\n",
    // A block-level tag opens an HTML block in any container.
    "> <div>\n>     ***\n",
    "> <div>\n> ***\n",
    "> o\n> <div>\n>     ***\n",
    "> o\n> <div>\n> ***\n",
    "- <div>\n  ***\n",
    "1. <div>\n   x\n   ***\n",
    "> - <div>\n>   ***\n",
    "> > <div>\n> > ***\n",
    "> <pre>\n> ***\n> </pre>\n",
    "> <pre>\n>\n> ***\n> </pre>\n",
    "> <script>\n> ***\n",
    // An indented run under a footnote's paragraph continues it.
    "> [^a]: o\n>         ***\n\nref[^a]\n",
    "> [^a]: o\n>          ***\n\nref[^a]\n",
    "> > [^a]: o\n> >         ***\n\nref[^a]\n",
    "> [^a]: o\n>         ---\n\nref[^a]\n",
    "> [^a]: o\n>         * * *\n\nref[^a]\n",
    // A marker written that far in under a footnote's paragraph opens no item.
    "> [^a]: o\n>         - x\n>           ***\n>           y\n\nref[^a]\n",
    "> [^a]: o\n>         1. x\n>            ***\n>            y\n\nref[^a]\n",
    "> > [^a]: o\n> >         - x\n> >           ***\n> >           y\n\nref[^a]\n",
    // An item opened beside the label has its edge past the body's, not the label's.
    "> [^a]: - o\n>           ***\n>           y\n\nref[^a]\n",
    "> [^a]:   - o\n>           ***\n\nref[^a]\n",
    "> [^a]: 1. o\n>            ***\n\nref[^a]\n",
    // A line that leaves out the markers of the containers around a footnote paragraph
    // still continues it, and the gap after a label is dropped however wide it is.
    "> > [^a]: o\n>     ***\n\nref[^a]\n",
    "> [^a]:    > o\n>     >         ***\n\nref[^a]\n",
    "> [^a]:    > o\n>     >     ***\n\nref[^a]\n",
    "> [^abcd]: -\to\n>             ***\n>             tail\n\nref[^abcd]\n",
    "> [^a]: 1.\to\n>             ***\n\nref[^a]\n",
    "> [^a\\]b]: o\n>         ***\n>         tail\n\nref[^a\\]b]\n",
    "> [^a\\\\]: o\n>         ***\n\nref[^a\\\\]\n",
    // A tab in the indentation of a line without the quote marker is four columns or more.
    "> o\n \t***\n",
    "> [^a]: o\n \t***\n\nref[^a]\n",
    // Indented code in a footnote is code.
    "> [^a]: o\n>\n>         ***\n>         y\n\nref[^a]\n",
    "> > [^a]: o\n> >\n> >         ---\n> >         y\n\nref[^a]\n",
];

/// Real rules next to the same shapes, which are fixed.
const RULES: &[&str] = &[
    "> o\n> <audio>\n> ***\n",
    "> o\n> <audio>\n>   ***\n",
    "> <audio>\n>\n> ***\n> p\n",
    "> <div>\n>\n> ***\n> p\n",
    "- <div>\n\n  ***\n  p\n",
    "> [^a]: o\n>     ***\n\nref[^a]\n",
    "> > [^a]: o\n> >     ***\n\nref[^a]\n",
    "> [^a]: o\n> ***\n\nref[^a]\n",
    "> [^a]: - o\n>          ***\n>          y\n\nref[^a]\n",
    "> o\n> </pre>\n> ***\n",
    "> [^a]:    > o\n>     > ***\n\nref[^a]\n",
    "> > [^a]: o\n> > ***\n\nref[^a]\n",
    "> [^a]: o\n>     - x\n>       ***\n>       y\n\nref[^a]\n",
    "> [^a]: o\n>\n>     ***\n>     y\n\nref[^a]\n",
    "> o\n> ***\n> p\n",
];

#[test]
fn text_and_html_are_not_reported_as_rules() {
    for content in NOT_RULES {
        assert_eq!(md065_warnings(content), 0, "{content:?}");
    }
}

#[test]
fn the_oracle_sees_the_rules_it_guards() {
    // Positive control: each real rule is reported, and the oracle tells the
    // paragraph it ends from the paragraph it was part of.
    for content in RULES {
        assert!(md065_warnings(content) > 0, "{content:?}");
    }
    assert_ne!(
        skeleton("> o\n> <audio>\n>     ***\n"),
        skeleton("> o\n> <audio>\n>\n>     ***\n")
    );
    assert_ne!(skeleton("> <div>\n>     ***\n"), skeleton("> <div>\n>\n>     ***\n"));
    assert_ne!(
        skeleton("> [^a]: o\n>         ***\n\nref[^a]\n"),
        skeleton("> [^a]: o\n>\n>         ***\n\nref[^a]\n")
    );
}

#[test]
fn the_fix_leaves_text_and_html_alone() {
    for content in NOT_RULES {
        assert_eq!(fix(content, ONLY_MD065), *content);
        // MD010 replaces a tab with spaces, which moves a list item's content edge
        // whatever MD065 does, so it is left out for the inputs that hold a tab.
        let fixed = fix(
            content,
            if content.contains('\t') {
                &["--disable", "MD010"]
            } else {
                &[]
            },
        );
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
fn the_fix_keeps_the_structure_of_real_rules() {
    for content in RULES {
        let fixed = fix(content, ONLY_MD065);
        assert_ne!(&fixed, content, "{content:?}");
        assert_eq!(skeleton(content), skeleton(&fixed), "{content:?} became {fixed:?}");
        assert_eq!(fixed, fix(&fixed, ONLY_MD065), "{content:?} is not stable");
    }
}

#[test]
fn writes_these_exact_results() {
    for (content, expected) in [
        ("> o\n> <audio>\n> ***\n", "> o\n> <audio>\n>\n> ***\n"),
        ("> <div>\n>\n> ***\n> p\n", "> <div>\n>\n> ***\n>\n> p\n"),
        ("- <div>\n\n  ***\n  p\n", "- <div>\n\n  ***\n\n  p\n"),
        ("> [^a]: o\n> ***\n\nref[^a]\n", "> [^a]: o\n>\n> ***\n\nref[^a]\n"),
    ] {
        assert_eq!(fix(content, ONLY_MD065), expected, "{content:?}");
    }
}
