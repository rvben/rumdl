//! Regression tests for issue #760: MD013 refused to wrap a paragraph whose
//! wrap point fell before a number that merely looked like a list marker.
//!
//! `starts_block_construct` treated any `<=9 digits><.|)>` as a list opener, so
//! a paragraph ending in `123456.` was reported as an unfixable "Line length N
//! exceeds 80 characters" instead of the fixable normalize diagnostic. Only a
//! list numbered 1 whose first item has content can interrupt a paragraph, so
//! every other shape is prose and wrapping before it is safe.
//!
//! The oracle here is the parser: each test renders the original one-line
//! paragraph and the reflowed output and requires the two to match, so a wrap
//! that changed the document's structure fails no matter which line it landed
//! on.

use pulldown_cmark::{Options, Parser, html};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// The reporter's paragraph, minus the final token. Exactly 80 columns, so the
/// wrap point falls immediately before whatever token is appended.
const EMPHASIS_80: &str = "_I cannot italicize text that is exactly 80 characters long followed by 123456_,";

/// The reporter's config: 80 columns with reflow in `normalize` mode.
fn reflow_args(cmd: &mut std::process::Command) {
    cmd.arg("--no-config")
        .arg("--no-cache")
        .arg("-c")
        .arg("MD013.line-length = 80")
        .arg("-c")
        .arg("MD013.code-blocks = false")
        .arg("-c")
        .arg("MD013.headings = false")
        .arg("-c")
        .arg("MD013.reflow = true")
        .arg("-c")
        .arg("MD013.reflow-mode = \"normalize\"");
}

/// Run `rumdl fmt` with the reporter's config and return the rewritten file.
fn fmt_normalize(dir: &Path, content: &str, line_length: usize) -> String {
    let file_path = dir.join("example.md");
    fs::write(&file_path, content).unwrap();

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    cmd.arg("fmt");
    reflow_args(&mut cmd);
    let output = cmd
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

/// The MD013 messages `rumdl check` reports for `content`.
fn md013_messages(dir: &Path, content: &str) -> Vec<String> {
    let file_path = dir.join("example.md");
    fs::write(&file_path, content).unwrap();

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    cmd.arg("check");
    reflow_args(&mut cmd);
    let output = cmd.arg(&file_path).output().expect("Failed to execute rumdl");

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("[MD013]"))
        .map(|line| line.split("[MD013] ").nth(1).unwrap_or(line).to_string())
        .collect()
}

/// Render `markdown` to HTML with whitespace collapsed, so a soft line break
/// and the space it replaces compare equal but a structural change does not.
fn render(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(markdown, options));
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn issue_760_italicized_text_followed_by_a_number_is_fixable() {
    let dir = TempDir::new().unwrap();
    let content = format!("{EMPHASIS_80} 123456.\n");

    let messages = md013_messages(dir.path(), &content);
    assert_eq!(
        messages,
        vec!["Paragraph could be normalized to use line length of 80 characters [*]".to_string()],
        "the paragraph must be reported as normalizable, not as an unfixable long line"
    );

    let after = fmt_normalize(dir.path(), &content, 80);
    assert_eq!(after, format!("{EMPHASIS_80}\n123456.\n"));
    assert_eq!(render(&content), render(&after), "wrapping changed the document");
}

#[test]
fn ordered_lookalikes_do_not_block_a_wrap() {
    // None of these can interrupt a paragraph: the number is not 1, the item
    // is empty, the marker is over nine digits, or the punctuation is not a
    // list marker at all. Each must wrap onto its own line.
    for tail in [
        "123456.",
        "12.",
        "123456)",
        "1234567890123.",
        "123456;",
        "123456!",
        "123456,",
        "x123456.",
        "123456",
        "1.",
        "1)",
        "01.",
        "001.",
        "0.",
        "7.",
        "0. item",
        "7. item",
        "42) x",
    ] {
        let dir = TempDir::new().unwrap();
        let content = format!("{EMPHASIS_80} {tail}\n");
        let after = fmt_normalize(dir.path(), &content, 80);

        assert_eq!(
            after,
            format!("{EMPHASIS_80}\n{tail}\n"),
            "tail {tail:?} should have wrapped onto its own line"
        );
        assert_eq!(
            render(&content),
            render(&after),
            "tail {tail:?}: wrapping changed the document"
        );
    }
}

/// Assert no line after the first opens an ordered list. Where the wrap lands
/// is the formatter's business; turning prose into a list is not.
fn assert_no_wrapped_list_opener(after: &str) {
    for line in after.lines().skip(1) {
        let digits = line.trim_start().chars().take_while(char::is_ascii_digit).count();
        let (number, rest) = line.trim_start().split_at(digits);
        let opens_list = number.trim_start_matches('0') == "1"
            && rest
                .strip_prefix(['.', ')'])
                .is_some_and(|after_marker| after_marker.starts_with([' ', '\t']));
        assert!(!opens_list, "a wrapped line opened an ordered list:\n{after}");
    }
}

#[test]
fn a_real_ordered_list_opener_never_starts_a_wrapped_line() {
    // A list numbered 1 with content does interrupt a paragraph, so hoisting
    // one to line start would turn prose into a list. Leading zeros still make
    // the number 1, and the marker may be followed by a tab.
    for tail in ["1. item", "01. item", "001. item", "1) item", "1.\titem"] {
        let dir = TempDir::new().unwrap();
        let content = format!("{EMPHASIS_80} {tail}\n");
        let after = fmt_normalize(dir.path(), &content, 80);

        assert_no_wrapped_list_opener(&after);
        assert_eq!(
            render(&content),
            render(&after),
            "tail {tail:?}: wrapping changed the document"
        );
    }
}

#[test]
fn wrapping_before_a_reference_definition_stays_inert() {
    // `1.` alone is inert, so it may now start a wrapped line - but a
    // following `[ref]:` merges back into it, and `1. [ref]:` is a real list
    // item. That the merge keeps folding is covered directly by
    // `merge_block_construct_continuations_merges_marker_led_lines`; this is
    // the end-to-end check that the output really is inert at every width.
    for width in [4, 6, 10, 20] {
        let dir = TempDir::new().unwrap();
        let content = "alpha beta 1. [ref]: gamma\n";
        let after = fmt_normalize(dir.path(), content, width);

        assert_no_wrapped_list_opener(&after);
        assert_eq!(
            render(content),
            render(&after),
            "width {width}: wrapping changed the document"
        );
    }
}
