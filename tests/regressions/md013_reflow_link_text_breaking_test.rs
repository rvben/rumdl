//! Minimal reproducers for MD013 reflow handling of Markdown links.
//!
//! Issues reproduced:
//! 1. Reflow treats an entire `[link text](url)` construct as an atomic indivisible
//!    token and refuses to break at whitespace boundaries within `[link text]`.
//! 2. In list items (`- Prefix [link text](url)`), reflow breaks before `[` rather
//!    than inside `[link text]`, leaving `Prefix` as an awkward orphan word on the
//!    bullet line.
//! 3. Standalone links exceeding `line-length` are left on a single long line.
//! 4. Reference links `[link text][ref]` are similarly treated as indivisible.

use indoc::indoc;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Run `rumdl fmt` with the given `MD013` settings and return the rewritten file.
fn fmt(dir: &Path, content: &str, settings: &[&str]) -> String {
    let file_path = dir.join("example.md");
    fs::write(&file_path, content).unwrap();

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
    command
        .arg("fmt")
        .arg("--no-config")
        .arg("--no-cache")
        .arg("--enable")
        .arg("MD013");
    for setting in settings {
        command.arg("-c").arg(format!("MD013.{setting}"));
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

#[test]
fn test_standalone_inline_link_breaks_within_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        [A long link description with multiple words](https://example.com/some/path)
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    // Desired: Break within link text across lines so all lines fit within 40 columns.
    let expected = indoc! {"
        [A long link description with multiple
        words](https://example.com/some/path)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_list_item_link_avoids_orphan_prefix() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [Add basic syntax for variables](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    // Desired: Break within [link text] rather than breaking before `[`.
    let expected = indoc! {"
        - Prefix [Add basic syntax for
          variables](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_blockquote_list_item_link_avoids_orphan_prefix() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        > - Prefix [Add basic syntax for variables](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 45"];
    let formatted = fmt(dir.path(), content, &settings);

    // Desired: Break within [link text] inside blockquote list items.
    let expected = indoc! {"
        > - Prefix [Add basic syntax for
        >   variables](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_normal_paragraph_link_wrapping_flush_indent() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        Here is text introducing [a long link description with words](https://example.com) in a paragraph.
    "};
    let settings = ["reflow = true", "line-length = 45"];
    let formatted = fmt(dir.path(), content, &settings);

    // Desired: Break within link text with standard 0-indent paragraph continuation.
    let expected = indoc! {"
        Here is text introducing [a long link
        description with words](https://example.com)
        in a paragraph.
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_containing_code_span_breaks_at_whitespace() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        > - Prefix [Add `foo <type>` syntax for variables](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 50", "atomic-spans = false"];
    let formatted = fmt(dir.path(), content, &settings);

    // Desired: Break within [link text] around or after the code span.
    let expected = indoc! {"
        > - Prefix [Add `foo <type>` syntax for
        >   variables](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_reference_link_breaks_within_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A long link description with multiple words][ref_tag]
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    // Desired: Break within [link text] of a reference link.
    let expected = indoc! {"
        - Prefix [A long link description with
          multiple words][ref_tag]
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_collapsed_reference_link_breaks_within_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A long link description with multiple words][]
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    // Desired: Break within [link text] of a collapsed reference link.
    let expected = indoc! {"
        - Prefix [A long link description with
          multiple words][]
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_with_title_breaks_within_text_without_breaking_title() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        Prefix [A long link description with multiple words](https://example.com/some/path \"A Title That Should Not Break\")
    "};
    let settings = ["reflow = true", "line-length = 45"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        Prefix [A long link description with multiple
        words](https://example.com/some/path \"A Title That Should Not Break\")
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_reflow_link_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [Add basic syntax for variables](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted1 = fmt(dir.path(), content, &settings);
    let formatted2 = fmt(dir.path(), &formatted1, &settings);
    assert_eq!(formatted1, formatted2);
}

#[test]
fn test_list_item_link_with_trailing_punctuation() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [Add basic syntax for variables](https://example.com/pull/123).
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [Add basic syntax for
          variables](https://example.com/pull/123).
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_list_item_link_with_trailing_comma_and_continuation() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [Add basic syntax for variables](https://example.com/pull/123), and some follow-up.
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [Add basic syntax for
          variables](https://example.com/pull/123),
          and some follow-up.
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_list_item_link_breaks_when_prefix_causes_overflow_with_exemptions() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A description with some more words](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 40", "reflow-length-exemptions = true"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [A description with some more
          words](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_multiple_links_in_paragraph_break_cleanly() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        See [first long link description](https://example.com/1) and [second long link description](https://example.com/2) for info.
    "};
    let settings = ["reflow = true", "line-length = 45"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        See [first long link
        description](https://example.com/1) and
        [second long link
        description](https://example.com/2) for info.
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_containing_html_tags_with_attributes_breaks_at_whitespace_not_inside_tag() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [Link with <span class=\"highlight\">styled text</span> inside](https://example.com)
    "};
    let settings = ["reflow = true", "line-length = 45"];
    let formatted = fmt(dir.path(), content, &settings);

    // Desired: Break between words, never inside `<span class="highlight">`
    let expected = indoc! {"
        - Prefix [Link with
          <span class=\"highlight\">styled text</span>
          inside](https://example.com)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_deeply_nested_blockquote_list_item_link_wrapping() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        > > - Prefix [Add basic syntax for variables](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 45"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        > > - Prefix [Add basic syntax for
        > >   variables](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_short_link_with_long_url_remains_unbroken() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        [Short link](https://example.com/a/very/long/path/that/exceeds/the/line/length/budget/by/itself)
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    // Desired: Since link text fits within 40 columns and URL alone is overlong, do not break link text.
    let expected = indoc! {"
        [Short link](https://example.com/a/very/long/path/that/exceeds/the/line/length/budget/by/itself)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_with_task_list_marker() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - [ ] Prefix [Add basic syntax for variables](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 45"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - [ ] Prefix [Add basic syntax for
              variables](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_escaped_brackets_and_code_span_in_link_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [Add `foo [bar]` and \\[escaped\\] syntax](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 45"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [Add `foo [bar]` and \\[escaped\\]
          syntax](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_destination_with_balanced_parentheses() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A long link description with multiple words](https://en.wikipedia.org/wiki/Foo_(bar))
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [A long link description with
          multiple
          words](https://en.wikipedia.org/wiki/Foo_(bar))
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_destination_with_angle_brackets() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A long link description with multiple words](<https://example.com/some path with spaces>)
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [A long link description with
          multiple
          words](<https://example.com/some path with spaces>)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_with_bold_and_italic_in_link_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [Add **bold syntax** and *italic words* here](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [Add **bold syntax** and
          *italic words*
          here](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_inside_parentheses() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        See ([a long link description with multiple words](https://example.com/some/path)) for info.
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        See ([a long link description with
        multiple
        words](https://example.com/some/path))
        for info.
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_inside_quotes() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        See \"[a long link description with multiple words](https://example.com/some/path)\" for info.
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        See \"[a long link description with
        multiple
        words](https://example.com/some/path)\"
        for info.
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_consecutive_links_without_spaces() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        Prefix [First long link description](https://example.com/1)[Second long link description](https://example.com/2)
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    // Consecutive links without spaces remain adjacent to preserve exact content
    assert!(formatted.contains("](https://example.com/1)["));
    assert!(formatted.contains("](https://example.com/2)"));
}

#[test]
fn test_link_in_footnote_definition() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        [^1]: A footnote introducing [a long link description with multiple words](https://example.com/some/path) here.
    "};
    let settings = ["reflow = true", "line-length = 45"];
    let formatted = fmt(dir.path(), content, &settings);

    assert!(formatted.contains("](https://example.com/some/path)"));
}

#[test]
fn test_reference_link_with_spaced_label() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A long link description with multiple words][my reference label]
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [A long link description with
          multiple words][my reference label]
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_with_cjk_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - 前置 [日本語の リンク テキスト で 長い 文章 です](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    assert!(formatted.contains("](https://example.com/pull/123)"));
}

#[test]
fn test_link_with_emoji_in_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [Add 🚀 rocket and 🌟 star features to the repo](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    assert!(formatted.contains("](https://example.com/pull/123)"));
}

#[test]
fn test_numbered_list_item_with_link() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        1. Prefix [Add basic syntax for variables in the compiler](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 40"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        1. Prefix [Add basic syntax for
           variables in the
           compiler](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}
