//! Regression tests for issue #825: MD013's reflow absorbed a setext heading's
//! underline into the paragraph above it and joined the two, so `Setup\n=====`
//! became `Setup =====` and the heading was silently demoted to prose.
//!
//! The defect was one missing boundary in the paragraph-collection loop, so it
//! spanned far more than the reported case:
//!
//! - Every reflow mode. `default` looked immune only because it reflows solely
//!   on an over-length line, which a short heading never trips; a heading whose
//!   text exceeds `line-length` was corrupted in `default` too.
//! - Both heading levels. A `---` underline escaped by coincidence, being also
//!   a thematic break (3+ dashes), which already ended a paragraph. `=`, `-`
//!   and `--` have no such overlap and were all absorbed.
//! - Top level and inside a blockquote.
//!
//! A setext heading is a heading, so reflow now skips its text line and its
//! underline together, the way it already skips an ATX heading. Every test here
//! pairs the no-op assertions with positive controls that reflow still runs, so
//! a rule that merely stopped firing fails instead of passing.

use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Run `rumdl fmt` with MD013 reflow in `mode` and return the rewritten file.
/// An empty `mode` leaves `reflow-mode` unset, exercising the default.
fn fmt_reflow(dir: &Path, content: &str, mode: &str, line_length: usize) -> String {
    let file_path = dir.join("example.md");
    fs::write(&file_path, content).unwrap();

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command
        .arg("fmt")
        .arg("--no-config")
        .arg("--no-cache")
        .arg("--enable")
        .arg("MD013")
        .arg("-c")
        .arg("MD013.reflow = true")
        .arg("-c")
        .arg(format!("MD013.line-length = {line_length}"));
    if !mode.is_empty() {
        command.arg("-c").arg(format!("MD013.reflow-mode = \"{mode}\""));
    }

    let output = command.arg(&file_path).output().expect("Failed to execute rumdl");

    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl fmt should succeed, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::read_to_string(&file_path).unwrap()
}

/// Every reflow mode, including the unset default.
const MODES: [&str; 5] = ["", "default", "normalize", "sentence-per-line", "semantic-line-breaks"];

/// Render `markdown` to HTML with CommonMark defaults.
fn render_html(markdown: &str) -> String {
    let parser = pulldown_cmark::Parser::new_ext(markdown, pulldown_cmark::Options::empty());
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

/// Render `markdown` and collapse whitespace runs, so two documents that differ
/// only in where their soft line breaks fall compare equal. Anything a reflow is
/// allowed to change is whitespace; anything else is a structural change.
fn render_html_normalized(markdown: &str) -> String {
    render_html(markdown).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `fmt` must leave `content` byte-identical in every reflow mode.
fn assert_unchanged_in_every_mode(content: &str, what: &str) {
    for mode in MODES {
        let dir = TempDir::new().unwrap();
        let result = fmt_reflow(dir.path(), content, mode, 80);
        assert_eq!(
            result,
            content,
            "{what} was rewritten in reflow-mode {:?}",
            if mode.is_empty() { "<unset>" } else { mode }
        );
    }
}

#[test]
fn setext_h1_is_never_joined_to_its_underline() {
    // The exact document from issue #825.
    assert_unchanged_in_every_mode("Setup\n=====\n\nSome text.\n", "setext h1");
}

#[test]
fn setext_underline_length_does_not_matter() {
    assert_unchanged_in_every_mode("Setup\n=\n\nSome text.\n", "setext h1 with a one-char underline");
    assert_unchanged_in_every_mode(
        "Setup\n==========================\n\nSome text.\n",
        "setext h1 with a long underline",
    );
}

#[test]
fn setext_h2_is_protected_at_every_underline_width() {
    // `---` and `----` were already safe, but only because 3+ dashes are also a
    // thematic break. `-` and `--` are setext underlines with no such overlap
    // and were corrupted exactly like `=`.
    for underline in ["-", "--", "---", "----"] {
        let content = format!("Subhead\n{underline}\n\nSome text.\n");
        assert_unchanged_in_every_mode(&content, &format!("setext h2 underlined with {underline:?}"));
    }
}

#[test]
fn setext_h1_longer_than_the_line_length_survives_default_mode() {
    // The case that proves this was never mode-specific: `default` reflows on an
    // over-length line, so a heading whose text exceeds `line-length` was
    // corrupted with `reflow-mode` unset. Before the fix this produced
    // `...Alike\nToday ====...`, destroying the heading.
    let heading = "Setting Up The Development Environment For Contributors And Maintainers Alike Today";
    let underline = "=".repeat(heading.len());
    let content = format!("{heading}\n{underline}\n\nSome text.\n");
    assert!(heading.len() > 80, "the heading must exceed the line length under test");
    assert_unchanged_in_every_mode(&content, "a setext h1 longer than line-length");
}

#[test]
fn setext_headings_are_protected_anywhere_in_the_document() {
    assert_unchanged_in_every_mode(
        "# Title\n\nIntro.\n\nSetup\n=====\n\nText.\n",
        "a setext h1 following other content",
    );
    assert_unchanged_in_every_mode("One\n===\n\nTwo\n===\n\nText.\n", "consecutive setext h1 headings");
    assert_unchanged_in_every_mode(
        "Setup\n =====\n\nSome text.\n",
        "a setext h1 with an indented underline",
    );
    assert_unchanged_in_every_mode(
        "Setup\n=====   \n\nSome text.\n",
        "a setext underline with trailing spaces",
    );
}

#[test]
fn setext_headings_inside_a_blockquote_are_protected() {
    // The blockquote paragraph path has its own boundary check, and the parser
    // records no heading for a line starting with `>`, so this needed its own
    // predicate rather than the parser lookup used at top level.
    assert_unchanged_in_every_mode("> Setup\n> =====\n>\n> Some text.\n", "a blockquoted setext h1");
    assert_unchanged_in_every_mode("> Subhead\n> -\n>\n> Some text.\n", "a blockquoted setext h2");
}

#[test]
fn the_heading_survives_as_a_heading_not_just_as_bytes() {
    // Byte equality could in principle be satisfied by reflow declining to run
    // at all. Confirm the construct still parses as a level-1 heading, which is
    // what the issue reported losing (`check` on the mangled output started
    // reporting MD041, "first line should be a level 1 heading").
    let dir = TempDir::new().unwrap();
    let result = fmt_reflow(dir.path(), "Setup\n=====\n\nSome text.\n", "normalize", 80);

    let html = render_html(&result);
    assert!(
        html.contains("<h1>Setup</h1>"),
        "the setext heading should still render as an h1, got: {html}"
    );

    // And the pre-fix output genuinely does not, so the assertion above has teeth.
    let broken_html = render_html("Setup =====\n\nSome text.\n");
    assert!(
        !broken_html.contains("<h1>"),
        "control: the joined form must not contain a heading, got: {broken_html}"
    );
}

// ---------------------------------------------------------------------------
// Multi-line heading text. A setext heading's text may span several lines, and
// the parser records the heading only on the LAST of them, so the lines above it
// are reached as an ordinary paragraph whose continuation runs into the heading.
// ---------------------------------------------------------------------------

#[test]
fn a_multiline_setext_heading_is_never_joined_into_prose() {
    // Reflow reaches this heading from the paragraph line above it rather than at
    // its own start, so it exercises the paragraph-continuation boundary instead
    // of the paragraph-start skip. Without that boundary the whole construct is
    // collected as one paragraph and joined into `alpha Setup =====`, exactly the
    // reported corruption.
    assert_unchanged_in_every_mode("alpha\nSetup\n=====\n\nSome text.\n", "a multi-line setext h1");
    assert_unchanged_in_every_mode("alpha\nSubhead\n-\n\nSome text.\n", "a multi-line setext h2");
}

#[test]
fn rewrapping_multiline_heading_text_keeps_the_document_intact() {
    // The accepted boundary of the fix. Reflow skips a setext heading once it
    // reaches one, but the leading lines of a multi-line heading's text are
    // indistinguishable from a paragraph until the heading is reached, so an
    // over-long one is still re-wrapped. That only moves soft line breaks, which
    // a setext heading collapses to spaces: the document renders identically.
    let heading = "Setting Up The Development Environment For Contributors And Maintainers Alike Today";
    for content in [
        format!("{heading}\nSetup\n=====\n\nSome text.\n"),
        format!("> {heading}\n> =\n>\n> Some text.\n"),
    ] {
        for mode in MODES {
            let dir = TempDir::new().unwrap();
            let result = fmt_reflow(dir.path(), &content, mode, 80);
            assert_eq!(
                render_html_normalized(&result),
                render_html_normalized(&content),
                "reflow-mode {:?} changed the rendered document.\nbefore:\n{content}\nafter:\n{result}",
                if mode.is_empty() { "<unset>" } else { mode }
            );
        }
    }

    // Control: the pre-fix output for the same input does NOT survive this
    // assertion, so it is not satisfied by any output whatsoever.
    assert_ne!(
        render_html_normalized(&format!("{heading} Setup =====\n\nSome text.\n")),
        render_html_normalized(&format!("{heading}\nSetup\n=====\n\nSome text.\n")),
        "control: the joined form must render differently"
    );
}

#[test]
fn wrapping_never_starts_a_line_with_a_setext_underline() {
    // A wrapped line that began with a bare `=====` or `---` would close the
    // heading text early and split one heading into two. Reflow keeps the token
    // attached to the word before it, leaving the line over-length instead.
    for marker in ["=====", "---"] {
        let dir = TempDir::new().unwrap();
        let content = format!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa {marker}\nSetup\n=====\n");
        let result = fmt_reflow(dir.path(), &content, "", 40);
        assert_eq!(
            render_html_normalized(&result),
            render_html_normalized(&content),
            "wrapping split the heading for marker {marker:?}; got:\n{result}"
        );
        assert_eq!(
            render_html(&result).matches("<h1>").count(),
            1,
            "the document must still hold exactly one h1; got:\n{result}"
        );
    }
}

// ---------------------------------------------------------------------------
// Positive controls: reflow must still do its job.
// ---------------------------------------------------------------------------

#[test]
fn an_over_long_paragraph_still_wraps() {
    let dir = TempDir::new().unwrap();
    let content = "aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk lll mmm nnn ooo ppp qqq rrr sss ttt uuu vvv www.\n";
    let result = fmt_reflow(dir.path(), content, "", 80);
    assert_eq!(
        result, "aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk lll mmm nnn ooo ppp qqq rrr sss ttt\nuuu vvv www.\n",
        "default-mode reflow must still wrap an over-long paragraph"
    );
}

#[test]
fn a_multiline_paragraph_still_normalizes() {
    let dir = TempDir::new().unwrap();
    let result = fmt_reflow(dir.path(), "alpha\nbravo\ncharlie\n", "normalize", 80);
    assert_eq!(
        result, "alpha bravo charlie\n",
        "normalize mode must still join a multi-line paragraph"
    );
}

#[test]
fn paragraphs_adjacent_to_a_setext_heading_still_reflow() {
    // The heading is skipped; the prose around it is not. This is what
    // distinguishes the fix from "reflow stopped working near headings".
    let dir = TempDir::new().unwrap();
    let after = fmt_reflow(dir.path(), "Setup\n=====\n\nalpha\nbravo\n", "normalize", 80);
    assert_eq!(
        after, "Setup\n=====\n\nalpha bravo\n",
        "a paragraph after a setext heading must still be joined"
    );

    let dir = TempDir::new().unwrap();
    let before = fmt_reflow(dir.path(), "alpha\nbravo\n\nSetup\n=====\n", "normalize", 80);
    assert_eq!(
        before, "alpha bravo\n\nSetup\n=====\n",
        "a paragraph before a setext heading must still be joined"
    );
}

#[test]
fn blockquote_paragraphs_still_reflow() {
    let dir = TempDir::new().unwrap();
    let result = fmt_reflow(dir.path(), "> alpha\n> bravo\n", "normalize", 80);
    assert_eq!(
        result, "> alpha bravo\n",
        "a blockquoted paragraph must still be joined"
    );
}

#[test]
fn a_thematic_break_still_separates_paragraphs() {
    // `---` between blank lines is a thematic break, not a setext underline.
    // It was already a boundary; confirm the new checks did not change that.
    let dir = TempDir::new().unwrap();
    let result = fmt_reflow(dir.path(), "alpha\nbravo\n\n---\n\ncharlie\ndelta\n", "normalize", 80);
    assert_eq!(
        result, "alpha bravo\n\n---\n\ncharlie delta\n",
        "paragraphs either side of a thematic break must reflow independently"
    );
}
