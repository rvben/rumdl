//! MD013 `reflow-break-link-text`: wrapping long lines within link text.
//!
//! By default every link and image is one atomic token during reflow: issues
//! #412, #704 and #252 all asked for links to stay whole, so an overlong link
//! moves to its own (exempt) line rather than being split. With
//! `reflow-break-link-text = true`, the bracketed text of a link or image may
//! wrap at whitespace when the construct alone can never fit a line. The
//! `](...)` tail is never split, and the option follows two consistency
//! rules, exercised throughout this file:
//!
//! - `fmt` must never produce a line `check` reports but cannot fix. A split
//!   link earns no URL exemption, so a link stays whole when its tail could
//!   not end a clean line (an oversized destination, a pushed-out title).
//!   Every test here re-runs `check` on the formatted output.
//! - The inline, full, collapsed and shortcut reference forms, and image
//!   forms, all wrap under the same rules.

use indoc::indoc;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Run `rumdl fmt` with the given `MD013` settings, then assert `rumdl check`
/// with the same settings is clean, and return the rewritten file.
fn fmt(dir: &Path, content: &str, settings: &[&str]) -> String {
    let file_path = dir.join("example.md");
    fs::write(&file_path, content).unwrap();

    let run = |subcommand: &str| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"));
        command
            .arg(subcommand)
            .arg("--no-config")
            .arg("--no-cache")
            .arg("--enable")
            .arg("MD013");
        for setting in settings {
            command.arg("-c").arg(format!("MD013.{setting}"));
        }
        command.arg(&file_path).output().expect("Failed to execute rumdl")
    };

    let output = run("fmt");
    let status = output.status.code();
    assert!(
        status == Some(0) || status == Some(1),
        "rumdl fmt should succeed, got status {status:?}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let formatted = fs::read_to_string(&file_path).unwrap();

    let output = run("check");
    assert_eq!(
        output.status.code(),
        Some(0),
        "fmt output must be clean under the same check settings; violations:\n{}\nformatted:\n{formatted}",
        String::from_utf8_lossy(&output.stdout)
    );

    formatted
}

#[test]
fn test_default_config_keeps_link_text_atomic() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [Add basic syntax for variables](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 45"];
    let formatted = fmt(dir.path(), content, &settings);

    // Without the option the link is one atomic token, so it moves whole to
    // its own exempt line (#412, #704, #252).
    let expected = indoc! {"
        - Prefix
          [Add basic syntax for variables](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_standalone_inline_link_breaks_within_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        [A long link description with multiple words](https://example.com/some/path)
    "};
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

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
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    // Break within [link text] rather than orphaning `Prefix` on the bullet
    // line. The tail line is a single token, which check forgives.
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
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

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
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

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
    let settings = [
        "reflow = true",
        "line-length = 50",
        "reflow-break-link-text = true",
        "atomic-spans = false",
    ];
    let formatted = fmt(dir.path(), content, &settings);

    // Break within [link text] around the code span, never inside it.
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

        [ref_tag]: https://example.com
    "};
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [A long link description with
          multiple words][ref_tag]

        [ref_tag]: https://example.com
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_collapsed_reference_link_breaks_within_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A long link description with multiple words][]

        [A long link description with multiple words]: https://example.com
    "};
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [A long link description with
          multiple words][]

        [A long link description with multiple words]: https://example.com
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_shortcut_reference_link_breaks_within_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A long link description with multiple words]

        [A long link description with multiple words]: https://example.com
    "};
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    // Reference labels normalize internal whitespace, so the label still
    // resolves after the line break.
    let expected = indoc! {"
        - Prefix [A long link description with
          multiple words]

        [A long link description with multiple words]: https://example.com
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_reference_image_breaks_within_alt_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix ![A long image alt description with multiple words][ref_tag]

        [ref_tag]: https://example.com/img.png
    "};
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix ![A long image alt description
          with multiple words][ref_tag]

        [ref_tag]: https://example.com/img.png
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_with_overlong_title_tail_stays_atomic() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        Prefix [A long link description with multiple words](https://example.com/some/path \"A Title That Should Not Break\")
    "};
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    // The `](url "title")` tail exceeds the limit and holds whitespace, so a
    // split would leave a tail line check reports (its overflow is not
    // confined to the final token) and fmt cannot fix. The link stays whole
    // on its own line, which check exempts as a standalone link.
    let expected = indoc! {"
        Prefix
        [A long link description with multiple words](https://example.com/some/path \"A Title That Should Not Break\")
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_with_short_title_breaks_and_keeps_title_intact() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        Prefix [A long link description with multiple words](https://e.com \"T\")
    "};
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        Prefix [A long link description with multiple
        words](https://e.com \"T\")
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_reflow_link_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [Add basic syntax for variables](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
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
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
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
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [Add basic syntax for
          variables](https://example.com/pull/123),
          and some follow-up.
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_length_exemptions_keep_url_exempt_link_whole() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A description with some more words](https://example.com/pull/123)
    "};
    let settings = [
        "reflow = true",
        "line-length = 40",
        "reflow-break-link-text = true",
        "reflow-length-exemptions = true",
    ];
    let formatted = fmt(dir.path(), content, &settings);

    // With the exemptions mirrored, reflow measures the link the way check
    // does: minus its URL it fits the limit, so the link is not split, it
    // moves whole to a line check forgives.
    let expected = indoc! {"
        - Prefix
          [A description with some more words](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_multiple_links_in_paragraph_break_cleanly() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        See [first long link description](https://example.com/1) and [second long link description](https://example.com/2) for info.
    "};
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
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
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    // Break between words, never inside `<span class="highlight">`.
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
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
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
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    // The destination alone exceeds the budget while the bracketed text fits:
    // every split would still leave a line at least as wide as the URL, so
    // splitting only trades one exempt standalone-link line for fragments.
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
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
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
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
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
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    // The tail line exceeds the limit but is a single token, the one overflow
    // check forgives, so breaking the text is still an improvement.
    let expected = indoc! {"
        - Prefix [A long link description with
          multiple
          words](https://en.wikipedia.org/wiki/Foo_(bar))
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_destination_with_angle_brackets_stays_atomic_when_tail_overflows() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A long link description with multiple words](<https://example.com/some path with spaces>)
    "};
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    // A spaced destination means the overflowing tail line would not be a
    // single token, so check would report it: the link stays whole.
    let expected = indoc! {"
        - Prefix
          [A long link description with multiple words](<https://example.com/some path with spaces>)
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_destination_with_angle_brackets_breaks_when_tail_fits() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A long link description with multiple words](<https://example.com/some path with spaces>)
    "};
    let settings = ["reflow = true", "line-length = 55", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [A long link description with multiple
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
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
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
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
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
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
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
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
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
    let settings = ["reflow = true", "line-length = 45", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    assert!(formatted.contains("](https://example.com/some/path)"));
}

#[test]
fn test_reference_link_with_spaced_label() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [A long link description with multiple words][my reference label]

        [my reference label]: https://example.com
    "};
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        - Prefix [A long link description with
          multiple words][my reference label]

        [my reference label]: https://example.com
    "};
    assert_eq!(formatted, expected);
}

#[test]
fn test_link_with_cjk_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - 前置 [日本語の リンク テキスト で 長い 文章 です](https://example.com/pull/123)
    "};
    let settings = [
        "reflow = true",
        "line-length = 40",
        "reflow-break-link-text = true",
        "ignore-link-urls = false",
    ];
    let formatted = fmt(dir.path(), content, &settings);

    assert!(formatted.contains("](https://example.com/pull/123)"));
    assert!(formatted.lines().count() > 1, "CJK link text should wrap: {formatted}");
}

#[test]
fn test_url_exempt_cjk_link_measured_by_display_width() {
    let dir = TempDir::new().unwrap();
    // The non-URL portion is 33 display columns (under the limit) but 49
    // bytes (over it). The check forgives the line via the URL exemption
    // measured in display columns, so reflow must leave it alone too.
    let content = indoc! {"
        [日本語の リンク テキスト です](https://example.com/some/longer/path)
    "};
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    assert_eq!(formatted, content);
}

#[test]
fn test_link_with_emoji_in_text() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        - Prefix [Add 🚀 rocket and 🌟 star features to the repo](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    assert!(formatted.contains("](https://example.com/pull/123)"));
}

#[test]
fn test_numbered_list_item_with_link() {
    let dir = TempDir::new().unwrap();
    let content = indoc! {"
        1. Prefix [Add basic syntax for variables in the compiler](https://example.com/pull/123)
    "};
    let settings = ["reflow = true", "line-length = 40", "reflow-break-link-text = true"];
    let formatted = fmt(dir.path(), content, &settings);

    let expected = indoc! {"
        1. Prefix [Add basic syntax for
           variables in the
           compiler](https://example.com/pull/123)
    "};
    assert_eq!(formatted, expected);
}
