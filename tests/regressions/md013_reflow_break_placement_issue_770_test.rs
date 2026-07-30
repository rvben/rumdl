//! Regression tests for issue #770 and the em dash case in #767: MD013's
//! semantic reflow placed a line break where the source had no whitespace.
//!
//! A newline inside a paragraph renders as a space, so a break is only ever a
//! stand-in for whitespace the author already wrote. Breaking elsewhere splits
//! one word into two: `Lorem (ipsum sit).` became `(ipsum sit)` and a line
//! holding nothing but `.`, and `cost—benefit` became `cost—` / `benefit`.
//!
//! The oracle is the parser. Each test renders the paragraph before and after
//! reflow and requires the two to match, which fails on any break that changes
//! the rendered text no matter which strategy proposed it.

use pulldown_cmark::{Event, Options, Parser, Tag, html};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Run `rumdl fmt` in semantic-line-breaks mode and return the rewritten file.
fn fmt_semantic(dir: &Path, content: &str, line_length: usize) -> String {
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
        .arg("MD013.reflow-mode = \"semantic-line-breaks\"")
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

/// Render `markdown` to HTML with whitespace collapsed, so a soft line break
/// and the space it replaces compare equal but a change to the text does not.
fn render(markdown: &str) -> String {
    let mut out = String::new();
    html::push_html(&mut out, parser(markdown));
    collapse(&out)
}

/// The literal payload of every inline element a break must stay out of. Their
/// whitespace is content, not layout: a code span keeps its spaces verbatim and
/// a link destination or title has no room for a break at all, so `render`
/// collapsing whitespace would hide exactly the damage this catches.
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

/// Reflow a one-paragraph document and assert every break stands in for
/// whitespace: the rendered text is unchanged, joining the lines with a single
/// space reproduces the source, and a second pass is a no-op.
fn assert_breaks_replace_whitespace(paragraph: &str, line_length: usize) -> String {
    let dir = TempDir::new().unwrap();
    let content = format!("{paragraph}\n");
    let after = fmt_semantic(dir.path(), &content, line_length);

    assert_eq!(
        render(&content),
        render(&after),
        "width {line_length}: reflow changed the rendered paragraph.\nbefore:\n{content}after:\n{after}"
    );
    assert_eq!(
        collapse(&after),
        collapse(paragraph),
        "width {line_length}: a break landed where the source had no breakable whitespace.\nafter:\n{after}"
    );
    assert_eq!(
        inline_atoms(&content),
        inline_atoms(&after),
        "width {line_length}: a break landed inside an inline element.\nafter:\n{after}"
    );

    let again = fmt_semantic(dir.path(), &after, line_length);
    assert_eq!(after, again, "width {line_length}: reflow must be idempotent");

    after
}

/// The reporter's case: a sentence-ending period after a parenthetical that
/// fits the width on its own but not with the period attached.
#[test]
fn issue_770_a_period_after_a_parenthetical_is_not_orphaned() {
    let after = assert_breaks_replace_whitespace("Lorem (ipsum sit). Dolor amet.", 11);
    assert_eq!(after, "Lorem\n(ipsum\nsit).\nDolor amet.\n");
}

/// The quoted form of the same sentence, which never orphaned its period. The
/// parenthetical must reflow the same way.
#[test]
fn issue_770_a_parenthetical_wraps_like_the_quoted_form() {
    let quoted = assert_breaks_replace_whitespace("Lorem \"ipsum sit\". Dolor amet.", 11);
    let parens = assert_breaks_replace_whitespace("Lorem (ipsum sit). Dolor amet.", 11);

    assert_eq!(
        quoted.replace(['"'], ""),
        parens.replace(['(', ')'], ""),
        "a parenthetical and a quotation must break at the same places"
    );
}

/// Punctuation of any kind stays with the parenthetical it follows, and a
/// parenthetical that still fits with its tail keeps its own line.
#[test]
fn punctuation_after_a_parenthetical_stays_attached() {
    for tail in [".", "!", "?", ",", ";", ":", "\"", "'", ").", "...", ".\""] {
        for width in [11, 12, 20, 30] {
            let after = assert_breaks_replace_whitespace(
                &format!("Lorem (ipsum sit){tail} Dolor amet consectetur adipiscing elit."),
                width,
            );
            assert!(
                !after.lines().any(|line| line.starts_with(tail)),
                "tail {tail:?} at width {width} was orphaned on its own line:\n{after}"
            );
        }
    }
}

/// #767: an em dash with no space around it joins two words, so a break after
/// it inserts a space that was not there.
#[test]
fn issue_767_an_unspaced_em_dash_is_not_a_break_point() {
    let after = assert_breaks_replace_whitespace(
        "The cost of scaling—adding more nodes to the cluster—grows faster than the throughput it buys you in practice.",
        80,
    );

    assert!(
        !after.lines().any(|line| line.ends_with('\u{2014}')),
        "a break landed after an em dash that had no following space:\n{after}"
    );
}

/// A spaced em dash is still a clause boundary: the fix removes an exemption,
/// it does not stop the em dash from being a break point.
#[test]
fn a_spaced_em_dash_is_still_a_clause_break() {
    let after = assert_breaks_replace_whitespace(
        "The cost of scaling in this cluster — adding many more nodes to it — grows faster than the throughput.",
        80,
    );

    assert!(
        after.lines().any(|line| line.ends_with('\u{2014}')),
        "a spaced em dash must remain a clause break:\n{after}"
    );
}

/// A clause mark is a break point only when breakable whitespace follows it. A
/// non-breaking space is there to forbid the break, so the search has to fall
/// back to the earlier comma rather than split at the colon and consume the
/// space that was holding the two words together.
#[test]
fn a_clause_mark_followed_by_a_non_breaking_space_is_not_a_break_point() {
    let after = assert_breaks_replace_whitespace(
        "Alpha beta gamma delta epsilon, zeta eta theta iota kappa:\u{a0}lambda mu nu xi omicron pi rho sigma tau upsilon phi.",
        70,
    );

    assert!(
        after.contains("kappa:\u{a0}lambda"),
        "the non-breaking space after the colon must survive the reflow:\n{after}"
    );
    assert!(
        after.lines().any(|line| line.ends_with(',')),
        "the earlier clause boundary should be used instead:\n{after}"
    );
}

/// A `(` that follows a character rather than a space belongs to the token
/// before it, so the parenthetical cannot be hoisted onto its own line.
#[test]
fn a_parenthetical_attached_to_a_word_is_not_hoisted() {
    let after = assert_breaks_replace_whitespace(
        "Lorem ipsum dolor sit amet consectetur adipiscing elit func(alpha beta) trailing words here.",
        80,
    );

    assert!(
        !after.lines().any(|line| line.starts_with('(')),
        "a parenthetical was split away from the word it is attached to:\n{after}"
    );
}

/// An inline element attached to a parenthetical extends it: the scan for the
/// end of the group runs past the element rather than stopping at whitespace
/// that belongs to the element's own content.
#[test]
fn a_parenthetical_keeps_an_inline_element_attached_to_it() {
    let after = assert_breaks_replace_whitespace("(foo bar)`a  b` trailing words to force wrapping here.", 15);
    assert!(
        after.starts_with("(foo bar)`a  b`\n"),
        "the code span must stay whole and attached to the parenthetical:\n{after}"
    );

    let after = assert_breaks_replace_whitespace(
        "(foo bar)[baz qux](https://example.com) trailing words to wrap here.",
        40,
    );
    assert!(
        after.starts_with("(foo bar)[baz qux](https://example.com)\n"),
        "the link must stay whole and attached to the parenthetical:\n{after}"
    );
}

/// No inline element is ever broken into, whichever strategy takes the line.
#[test]
fn no_strategy_breaks_into_an_inline_element() {
    let paragraphs = [
        "(foo bar)`a  b` trailing words to force wrapping here.",
        "Lorem ipsum `code  with  spaces` dolor sit amet consectetur adipiscing elit sed do eiusmod.",
        "Lorem ipsum [link text here](https://example.com \"a title\") dolor sit amet consectetur elit.",
        "Lorem ipsum ![alt text](https://example.com/a.png \"a title\") dolor sit amet consectetur elit.",
        "A sentence, with `a  b` and [c d](https://example.com), that is long enough to need several lines.",
    ];

    for paragraph in paragraphs {
        for width in [15, 20, 40, 60, 80] {
            assert_breaks_replace_whitespace(paragraph, width);
        }
    }
}

/// The same invariant across the shapes the cascade's other strategies pick:
/// clause punctuation inside a token, break-words, and mid-word parentheses.
#[test]
fn no_strategy_breaks_a_token_apart() {
    let paragraphs = [
        "Aspect ratios like 16:9 and 4:3 and key:value pairs must survive a reflow that is long enough to need several lines of output.",
        "The MyST role {cite:p}`smith2020` and the path a/b:c must not be split even when the paragraph is long enough to wrap repeatedly.",
        "One two three four five six seven eight nine—ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty.",
        "Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron(pi rho) sigma tau upsilon phi chi psi omega.",
        "Numbers 1,000 and 2,500 and ranges 10:30–11:45 appear in a paragraph that is far longer than any configured line length here.",
    ];

    for paragraph in paragraphs {
        for width in [20, 40, 60, 80] {
            assert_breaks_replace_whitespace(paragraph, width);
        }
    }
}
