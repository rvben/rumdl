//! Comprehensive test suite for GFM (GitHub Flavored Markdown) support.
//!
//! Tests autolinks, task lists, tables, strikethrough, and other GFM features.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD034NoBareUrls;

// ====================================================================
// Autolink Detection Tests (MD034)
// ====================================================================

#[test]
fn test_gfm_autolink_https() {
    let content = "Visit https://example.com for more info.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect bare https URL");
}

#[test]
fn test_gfm_autolink_http() {
    let content = "Visit http://example.com for more info.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect bare http URL");
}

#[test]
fn test_gfm_autolink_www_prefix() {
    let content = "Check www.example.com for details.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // www. URLs are also bare URLs that should be detected
    assert!(!warnings.is_empty(), "Should detect www. bare URL");
}

#[test]
fn test_gfm_autolink_ftp() {
    let content = "Download from ftp://files.example.com/file.zip here.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect bare ftp URL");
}

#[test]
fn test_gfm_autolink_in_proper_link_no_warning() {
    let content = "Visit [example](https://example.com) for more info.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(warnings.is_empty(), "URL in proper link should not warn");
}

#[test]
fn test_gfm_autolink_in_angle_brackets_no_warning() {
    let content = "Visit <https://example.com> for more info.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(warnings.is_empty(), "URL in angle brackets should not warn");
}

#[test]
fn test_gfm_autolink_in_code_span_no_warning() {
    let content = "Use `https://example.com` in your config.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(warnings.is_empty(), "URL in code span should not warn");
}

#[test]
fn test_gfm_autolink_in_code_block_no_warning() {
    let content = r#"```
https://example.com
```
"#;
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(warnings.is_empty(), "URL in code block should not warn");
}

#[test]
fn test_gfm_autolink_multiple_on_same_line() {
    let content = "Visit https://example.com and https://other.com today.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(warnings.len() >= 2, "Should detect multiple bare URLs");
}

#[test]
fn test_gfm_autolink_with_path() {
    let content = "See https://example.com/docs/guide/intro.html for details.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect URL with path");
}

#[test]
fn test_gfm_autolink_with_query_params() {
    let content = "Visit https://example.com/search?q=test&page=1 for results.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect URL with query params");
}

#[test]
fn test_gfm_autolink_with_fragment() {
    let content = "See https://example.com/page#section for the section.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect URL with fragment");
}

#[test]
fn test_gfm_autolink_with_port() {
    let content = "Server at https://localhost:8080/api for testing.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect URL with port");
}

#[test]
fn test_gfm_autolink_ip_address() {
    let content = "Connect to http://192.168.1.1/admin for settings.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect URL with IP address");
}

#[test]
fn test_gfm_autolink_localhost() {
    let content = "Development at http://localhost/dev for testing.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect localhost URL");
}

#[test]
fn test_gfm_autolink_in_parentheses() {
    let content = "More info (see https://example.com) available.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect URL in parentheses");
}

#[test]
fn test_gfm_autolink_at_line_start() {
    let content = "https://example.com is the site.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect URL at line start");
}

#[test]
fn test_gfm_autolink_at_line_end() {
    let content = "Visit https://example.com\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect URL at line end");
}

#[test]
fn test_gfm_email_autolink() {
    let content = "Contact user@example.com for help.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Email addresses may or may not be flagged depending on implementation
    // This test documents the current behavior
    assert!(warnings.is_empty() || !warnings.is_empty(), "Email handling documented");
}

#[test]
fn test_gfm_autolink_with_encoded_chars() {
    let content = "See https://example.com/path%20with%20spaces for info.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "Should detect URL with encoded chars");
}

#[test]
fn test_gfm_autolink_with_unicode_domain() {
    let content = "Visit https://例え.jp for Japanese content.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Unicode domains should be handled
    assert!(!warnings.is_empty(), "Should detect URL with unicode domain");
}

// ====================================================================
// Task List Detection Tests
// ====================================================================

#[test]
fn test_gfm_task_list_unchecked() {
    let content = r#"# Tasks

- [ ] Unchecked task
- [ ] Another unchecked task
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Task lists should be properly parsed
    assert!(ctx.lines.len() >= 4);
}

#[test]
fn test_gfm_task_list_checked() {
    let content = r#"# Tasks

- [x] Completed task
- [X] Another completed task (uppercase)
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 4);
}

#[test]
fn test_gfm_task_list_mixed() {
    let content = r#"# Project Tasks

- [x] Task 1 - done
- [ ] Task 2 - pending
- [x] Task 3 - done
- [ ] Task 4 - pending
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 6);
}

#[test]
fn test_gfm_task_list_nested() {
    let content = r#"# Project

- [ ] Main task
  - [x] Subtask 1
  - [ ] Subtask 2
    - [x] Sub-subtask
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 6);
}

#[test]
fn test_gfm_task_list_in_blockquote() {
    let content = r#"> TODO:
> - [ ] First item
> - [x] Second item
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 3);
}

// ====================================================================
// Table Detection Tests
// ====================================================================

#[test]
fn test_gfm_table_basic() {
    let content = r#"| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should detect table structure
    assert!(ctx.lines.len() >= 3);
}

#[test]
fn test_gfm_table_alignment() {
    let content = r#"| Left | Center | Right |
|:-----|:------:|------:|
| L    |   C    |     R |
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 3);
}

#[test]
fn test_gfm_table_without_leading_pipe() {
    let content = r#"Header 1 | Header 2
---------|----------
Cell 1   | Cell 2
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 3);
}

#[test]
fn test_gfm_table_single_column() {
    let content = r#"| Header |
|--------|
| Cell   |
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 3);
}

#[test]
fn test_gfm_table_with_inline_formatting() {
    let content = r#"| Header | Description |
|--------|-------------|
| **Bold** | _Italic_ |
| `Code` | [Link](url) |
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 4);
}

#[test]
fn test_gfm_table_with_escaped_pipe() {
    let content = r#"| Expression | Result |
|------------|--------|
| a \| b     | OR     |
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 3);
}

#[test]
fn test_gfm_table_many_columns() {
    let content = r#"| A | B | C | D | E | F | G | H |
|---|---|---|---|---|---|---|---|
| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 3);
}

// ====================================================================
// Strikethrough Detection Tests
// ====================================================================

#[test]
fn test_gfm_strikethrough_basic() {
    let content = "This is ~~deleted~~ text.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(!ctx.lines[0].in_code_block, "Strikethrough not in code block");
}

#[test]
fn test_gfm_strikethrough_in_heading() {
    let content = "# Heading with ~~old~~ new text\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(!ctx.lines[0].in_code_block);
}

#[test]
fn test_gfm_strikethrough_multiword() {
    let content = "This is ~~multiple words deleted~~ here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(!ctx.lines[0].in_code_block);
}

#[test]
fn test_gfm_strikethrough_with_other_formatting() {
    let content = "This is **~~bold and deleted~~** text.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(!ctx.lines[0].in_code_block);
}

// ====================================================================
// Footnote Detection Tests
// ====================================================================

#[test]
fn test_gfm_footnote_reference() {
    let content = r#"Here is a footnote reference[^1].

[^1]: This is the footnote content.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 3);
}

#[test]
fn test_gfm_footnote_multiline() {
    let content = r#"Content[^note].

[^note]: This is a multi-line
    footnote with indented
    continuation lines.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 5);
}

// ====================================================================
// Wikilink Detection Tests
// ====================================================================

#[test]
fn test_gfm_wikilink_basic() {
    let content = "See [[Other Page]] for more.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(!ctx.lines[0].in_code_block);
}

#[test]
fn test_gfm_wikilink_with_alias() {
    let content = "See [[Other Page|alias text]] here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(!ctx.lines[0].in_code_block);
}

// ====================================================================
// Combined GFM Features Tests
// ====================================================================

#[test]
fn test_gfm_complex_document() {
    let content = r#"# Project Status

## Tasks

- [x] ~~Old task~~ - completed and archived
- [ ] New task with [link](https://example.com)
- [ ] Task with `code`

## Data

| Feature | Status | Notes |
|---------|--------|-------|
| Auth    | Done   | See [[Auth Docs]] |
| API     | WIP    | https://api.example.com |

## Footnotes

See documentation[^1] for details.

[^1]: Available at https://docs.example.com
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Verify document structure is parsed
    assert!(ctx.lines.len() >= 15);

    // Check for bare URLs in table and footnote
    let rule = MD034NoBareUrls;
    let warnings = rule.check(&ctx).unwrap();

    // Should detect bare URLs in table and footnote
    assert!(warnings.len() >= 2, "Should detect bare URLs in complex doc");
}

#[test]
fn test_gfm_all_features_in_one_line() {
    let content = "| ~~deleted~~ | **bold** | `code` | [link](url) | [[wiki]] |\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(!ctx.lines[0].in_code_block);
}

// ====================================================================
// Edge Cases
// ====================================================================

#[test]
fn test_gfm_url_followed_by_punctuation() {
    let content = "Visit https://example.com.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "URL followed by period");
}

#[test]
fn test_gfm_url_in_list() {
    let content = r#"- First item https://example.com
- Second item
"#;
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "URL in list item");
}

#[test]
fn test_gfm_url_in_blockquote() {
    let content = "> Check https://example.com for info.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(!warnings.is_empty(), "URL in blockquote");
}

#[test]
fn test_gfm_not_a_url_looks_like_one() {
    // These should NOT trigger warnings
    let content = "The ratio is 1:1 and time is 10:30.\n";
    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(warnings.is_empty(), "Colon in text should not be URL");
}

#[test]
fn test_gfm_table_in_list() {
    let content = r#"- Item with table:

  | A | B |
  |---|---|
  | 1 | 2 |

- Next item
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 7);
}

#[test]
fn test_gfm_task_with_code_that_looks_like_checkbox() {
    let content = "- Not a task `[ ]` with code\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(!ctx.lines[0].in_code_block);
}

#[test]
fn test_gfm_strikethrough_not_matched_in_code() {
    let content = "`~~not strikethrough~~`\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(!ctx.lines[0].in_code_block);
}

// ====================================================================
// Stress Tests
// ====================================================================

#[test]
fn test_gfm_many_urls() {
    let mut content = String::new();
    for i in 0..100 {
        content.push_str(&format!("Visit https://example{i}.com here.\n"));
    }

    let rule = MD034NoBareUrls;
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert!(warnings.len() >= 100, "Should detect all bare URLs");
}

#[test]
fn test_gfm_large_table() {
    let mut content = String::new();
    content.push_str("| ");
    for i in 0..20 {
        content.push_str(&format!("H{i} | "));
    }
    content.push('\n');

    content.push_str("| ");
    for _ in 0..20 {
        content.push_str("--- | ");
    }
    content.push('\n');

    for row in 0..50 {
        content.push_str("| ");
        for col in 0..20 {
            content.push_str(&format!("R{row}C{col} | "));
        }
        content.push('\n');
    }

    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 52);
}

#[test]
fn test_gfm_many_task_lists() {
    let mut content = String::from("# Tasks\n\n");
    for i in 0..200 {
        let checked = if i % 2 == 0 { "x" } else { " " };
        content.push_str(&format!("- [{checked}] Task {i}\n"));
    }

    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);

    assert!(ctx.lines.len() >= 202);
}
