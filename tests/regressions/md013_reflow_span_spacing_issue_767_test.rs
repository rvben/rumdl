//! Regression tests for the inline-span half of issue #767: MD013's reflow
//! reconstructed the gap in front of an inline span from the characters the
//! accumulated line happened to end with, instead of reading it from the source.
//!
//! An inline span carries no whitespace of its own, so the gap before it lives at
//! the end of the text element preceding it, which reflow trims as it accumulates.
//! Guessing it back from the line's last character got both directions wrong. A
//! line ending in `-`, `(` or `[` lost a space the author had written, so
//! `same - $3/$15` came back as `same -$3/$15`; the content guard caught the
//! damage and returned the paragraph unchanged, which left the reflow looking
//! like it had simply declined to run. Every other ending gained a space the
//! author had not written, so `mid*word*` came back as two words.
//!
//! The oracle is the parser, as in the break-placement tests: each case renders
//! the paragraph before and after reflow and requires the two to match. That
//! alone would pass on a paragraph reflow refused to touch, so the cases that
//! must split also pin the exact output.

use pulldown_cmark::{Event, Options, Parser, Tag, html};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Both reflow modes that assemble a line out of parsed elements, and so both
/// go through the gap the source put in front of a span.
const MODES: [&str; 2] = ["sentence-per-line", "semantic-line-breaks"];

/// Run `rumdl fmt` in the given reflow mode and return the rewritten file.
fn fmt_reflow(dir: &Path, content: &str, mode: &str, line_length: usize) -> String {
    let file_path = dir.join("example.md");
    fs::write(&file_path, content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg("--no-config")
        .arg("--no-cache")
        .arg("--enable")
        .arg("MD013")
        .arg("-c")
        .arg("MD013.reflow = true")
        .arg("-c")
        .arg(format!("MD013.reflow-mode = \"{mode}\""))
        .arg("-c")
        .arg(format!("MD013.line-length = {line_length}"))
        .arg(&file_path)
        .output()
        .expect("Failed to execute rumdl");

    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl fmt should succeed, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::read_to_string(&file_path).unwrap()
}

/// Whitespace a line break is allowed to stand in for. A non-breaking space is
/// content, not layout: it exists to forbid a break there, so collapsing it
/// would hide both its loss and its conversion into an ordinary space.
fn is_breakable(c: char) -> bool {
    c.is_whitespace() && !matches!(c, '\u{a0}' | '\u{202f}' | '\u{2007}')
}

/// Collapse runs of breakable whitespace to a single space.
fn collapse(text: &str) -> String {
    text.split(is_breakable)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn parser(markdown: &str) -> Parser<'_> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    Parser::new_ext(markdown, options)
}

/// Render `markdown` to HTML with whitespace collapsed, so a soft line break and
/// the space it replaces compare equal but a change to the text does not.
fn render(markdown: &str) -> String {
    let mut out = String::new();
    html::push_html(&mut out, parser(markdown));
    collapse(&out)
}

/// The literal payload of every inline element a break must stay out of. Their
/// whitespace is content, not layout, so `render` collapsing whitespace would
/// hide exactly the damage this catches.
fn inline_atoms(markdown: &str) -> Vec<String> {
    parser(markdown)
        .filter_map(|event| match event {
            Event::Code(code) => Some(format!("code:{code}")),
            Event::Start(Tag::Link { dest_url, title, .. }) => Some(format!("link:{dest_url}|{title}")),
            Event::Start(Tag::Image { dest_url, title, .. }) => Some(format!("image:{dest_url}|{title}")),
            _ => None,
        })
        .collect()
}

/// Reflow a one-paragraph document and assert the only thing that changed is
/// where the breakable whitespace sits: same rendered text, same words in the
/// same order, no break inside an inline element, and a second pass is a no-op.
fn assert_reflow_preserves(paragraph: &str, mode: &str, line_length: usize) -> String {
    let dir = TempDir::new().unwrap();
    let content = format!("{paragraph}\n");
    let after = fmt_reflow(dir.path(), &content, mode, line_length);

    assert_eq!(
        render(&content),
        render(&after),
        "{mode} at width {line_length}: reflow changed the rendered paragraph.\nbefore:\n{content}after:\n{after}"
    );
    assert_eq!(
        collapse(&after),
        collapse(paragraph),
        "{mode} at width {line_length}: a break landed where the source had no breakable whitespace.\nafter:\n{after}"
    );
    assert_eq!(
        inline_atoms(&content),
        inline_atoms(&after),
        "{mode} at width {line_length}: a break landed inside an inline element.\nafter:\n{after}"
    );

    let again = fmt_reflow(dir.path(), &after, mode, line_length);
    assert_eq!(after, again, "{mode} at width {line_length}: reflow must be idempotent");

    after
}

/// The reporter's sentence. A span after a spaced dash used to come back with
/// the space gone, so the content guard returned the paragraph untouched and the
/// second sentence never reached its own line.
#[test]
fn issue_767_a_span_after_a_dash_reaches_its_own_line() {
    let paragraph = "It costs less. Pricing is the same - $3/$15 per million tokens.";

    for mode in MODES {
        let after = assert_reflow_preserves(paragraph, mode, 80);
        assert_eq!(
            after, "It costs less.\nPricing is the same - $3/$15 per million tokens.\n",
            "{mode}: the sentence after the boundary must reach its own line"
        );
    }
}

/// The gap belongs to the source, not to the kind of span that follows it.
#[test]
fn a_span_after_a_dash_reflows_whatever_the_span_is() {
    for span in [
        "$a$", "`abc`", "_abc_", "*abc*", "**abc**", "~~abc~~", "[a](b)", "![a](b)",
    ] {
        let paragraph = format!("One. Two - {span} x.");
        for mode in MODES {
            let after = assert_reflow_preserves(&paragraph, mode, 80);
            assert_eq!(
                after,
                format!("One.\nTwo - {span} x.\n"),
                "{mode}: the span kind must not change where the break lands"
            );
        }
    }
}

/// The same for the other two endings the old guess suppressed. A bracket or
/// paren followed by a space is an author's spacing like any other.
#[test]
fn a_span_after_a_bracket_or_paren_reflows() {
    for paragraph in [
        "One. See ( `a` ) now.",
        "One. See [ `a` ] now.",
        "One. Two -- `a` now.",
        "One. Two - `a` - `b` now.",
    ] {
        for mode in MODES {
            let after = assert_reflow_preserves(paragraph, mode, 80);
            let (first, rest) = paragraph.split_once(". ").unwrap();
            assert_eq!(
                after,
                format!("{first}.\n{rest}\n"),
                "{mode}: {paragraph:?} must break at the sentence boundary"
            );
        }
    }
}

/// A non-breaking space in front of a span is content the reader sees. Reflow
/// carries the gap through exactly as written rather than rendering it as the
/// single space a run of breakable whitespace would collapse to.
#[test]
fn a_non_breaking_space_before_a_span_survives() {
    for span in ["$a$", "`abc`", "*abc*", "[a](b)"] {
        let paragraph = format!("One. Two -\u{a0}{span} x.");
        for mode in MODES {
            let after = assert_reflow_preserves(&paragraph, mode, 80);
            assert_eq!(
                after,
                format!("One.\nTwo -\u{a0}{span} x.\n"),
                "{mode}: the non-breaking space before the span must survive verbatim"
            );
        }
    }
}

/// The other direction: where the source has no gap, reflow must not invent one.
/// The old guess inserted a space after any ending it did not recognize, which
/// turned intraword emphasis into two words.
#[test]
fn a_span_glued_to_the_word_before_it_gains_no_space() {
    for paragraph in [
        "One. Say mid*word* here.",
        "One. Say a*b*c here.",
        "One. Say mid**word** here.",
        "One. Say mid~~word~~ here.",
        "One. Say mid`code` here.",
        "One. Call f(`a`) now.",
        "One. A well-`known` case.",
        "One. Mid-_word_ and mid-`code` here.",
    ] {
        for mode in MODES {
            let after = assert_reflow_preserves(paragraph, mode, 80);
            let (first, rest) = paragraph.split_once(". ").unwrap();
            assert_eq!(
                after,
                format!("{first}.\n{rest}\n"),
                "{mode}: {paragraph:?} must keep the span attached to the word before it"
            );
        }
    }
}

/// A script that writes without interword spaces still has to break somewhere,
/// so a sentence boundary there is a break the source did not spell out. It must
/// land in front of the emphasis marker: carrying the marker back onto the
/// previous line leaves the span with no opening marker and renders the
/// asterisks as literal text.
///
/// The content of the span decides nothing. A span opening on a quotation mark
/// looks from the outside exactly like sentence punctuation followed by a
/// closing marker, so nothing about the characters around the run tells the two
/// apart. What does is that the run sits where the span the line just took on
/// begins.
#[test]
fn an_opening_emphasis_marker_is_not_carried_onto_the_previous_line() {
    for (marker, close) in [("*", "*"), ("**", "**"), ("_", "_"), ("~~", "~~")] {
        for body in ["强调文字。", "“强调文字。”"] {
            let paragraph = format!("普通文字。{marker}{body}{close} 更多文字。");
            let dir = TempDir::new().unwrap();
            let after = fmt_reflow(dir.path(), &format!("{paragraph}\n"), "sentence-per-line", 80);

            assert_eq!(
                after,
                format!("普通文字。\n{marker}{body}{close}\n更多文字。\n"),
                "the span must reach its own line whole"
            );

            // The added break is the one thing that differs, and only because a
            // script with no interword spaces has nowhere else to break.
            let spaced = paragraph.replacen("普通文字。", "普通文字。 ", 1);
            assert_eq!(
                render(&spaced),
                render(&after),
                "the emphasis span must survive the added break:\n{after}"
            );
        }
    }
}

/// The marker that does close the sentence it ends still travels with it, which
/// is what the splitter absorbs trailing markers for in the first place.
#[test]
fn a_closing_emphasis_marker_stays_with_its_sentence() {
    let cases = [
        ("One. A ~~struck.~~ Next.", "One.\nA ~~struck.~~\nNext.\n"),
        ("One. An *emphasis.* Next.", "One.\nAn *emphasis.*\nNext.\n"),
        ("One. A **bold.** Next.", "One.\nA **bold.**\nNext.\n"),
        (
            "One. He said \"Quoted.\" Then left.",
            "One.\nHe said \"Quoted.\"\nThen left.\n",
        ),
    ];

    for (paragraph, expected) in cases {
        let after = assert_reflow_preserves(paragraph, "sentence-per-line", 80);
        assert_eq!(after, expected, "{paragraph:?}: the closing marker must end the line");
    }
}

/// Only the opening marker of the span the line just took on is held back. A run
/// anywhere else closes the sentence it ends, whatever the paragraph did with
/// that marker earlier: an intraword underscore is no delimiter at all, and a
/// nested run of the same marker closes while the outer span stays open.
#[test]
fn a_closing_marker_is_absorbed_whatever_the_paragraph_did_before() {
    let cases = [
        (
            "Use snake_case here. _Emphasized text._ Next sentence.",
            "Use snake_case here.\n_Emphasized text._\nNext sentence.\n",
        ),
        (
            "A snake_case_word here. *Emphasized text.* Next sentence.",
            "A snake_case_word here.\n*Emphasized text.*\nNext sentence.\n",
        ),
        (
            "An **outer *nested.* Still outer** Then.",
            "An **outer *nested.*\nStill outer** Then.\n",
        ),
        (
            "An __outer _nested._ Still outer__ Then.",
            "An __outer _nested._\nStill outer__ Then.\n",
        ),
        (
            "An ~~struck *nested.* Still struck~~ Then.",
            "An ~~struck *nested.*\nStill struck~~ Then.\n",
        ),
    ];

    for (paragraph, expected) in cases {
        let after = assert_reflow_preserves(paragraph, "sentence-per-line", 80);
        assert_eq!(after, expected, "{paragraph:?}: the closing marker must end the line");
    }
}

/// A marker run between sentence punctuation and a word can only open, so it
/// belongs to what follows even though it looks like a closer.
#[test]
fn a_marker_run_before_a_word_is_left_where_it_is() {
    for paragraph in [
        "One. **Bold text.**More follows here.",
        "One. *Emphasis.*More follows here.",
        "One. ~~Struck.~~More follows here.",
    ] {
        for mode in MODES {
            let after = assert_reflow_preserves(paragraph, mode, 80);
            let (first, rest) = paragraph.split_once(". ").unwrap();
            assert_eq!(
                after,
                format!("{first}.\n{rest}\n"),
                "{mode}: {paragraph:?} must not be split at the marker run"
            );
        }
    }
}
