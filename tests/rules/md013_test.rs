use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD013LineLength;

#[test]
fn test_valid_line_length() {
    let rule = MD013LineLength::default();
    let content = "This is a short line.\nThis is another short line.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_line_length() {
    let rule = MD013LineLength::new(20, false, true, true, false);
    let content = "This is a very long line that exceeds the maximum length limit.\nThis is another very long line that also exceeds the limit.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_code_blocks() {
    let rule = MD013LineLength::new(60, true, true, true, false);
    let content = "```
This is a code block line that is very very very very very very very long and should be flagged.
```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_tables() {
    let rule = MD013LineLength::new(50, false, false, true, false);
    let content =
        "| This is a very long table cell that should be flagged |\n| This is another long cell |";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_headings() {
    let rule = MD013LineLength::new(20, false, false, false, false);
    let content = "# This is a very long heading\nThis is a short heading.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_mixed_content() {
    let rule = MD013LineLength::new(20, false, false, false, false);
    let content = "# Long heading\n```\nLong code\n```\n| Long table |\nThis is a very long line that should be flagged.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the last line should be flagged
    assert_eq!(result[0].line, 6);
}

#[test]
fn test_strict_mode() {
    let rule = MD013LineLength::new(20, true, true, true, true);
    let content = "https://very-long-url-that-exceeds-length.com\n![Long image ref][ref]\n[ref]: https://long-url.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3); // All lines should be flagged in strict mode
}

#[test]
fn test_url_exceptions() {
    let rule = MD013LineLength::new(20, true, true, true, false);
    let content = "https://very-long-url-that-exceeds-length.com\nNormal text that is too long";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only normal text should be flagged
}

#[test]
fn test_image_ref_exceptions() {
    let rule = MD013LineLength::new(20, true, true, true, false);
    let content = "![This is a very long image reference with a long description][reference-id]\nNormal text.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // Image reference should be ignored
}

#[test]
fn test_link_ref_exceptions() {
    let rule = MD013LineLength::new(20, true, true, true, false);
    let content =
        "[reference]: https://very-long-url-that-exceeds-length.com/path/to/resource\nNormal text.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // Link reference should be ignored
}

#[test]
fn test_code_block_string_exceptions() {
    let rule = MD013LineLength::new(20, true, true, true, false);
    let content = "```
ThisIsAVeryLongStringWithoutSpacesThatShouldBeIgnored
This is a normal code line that is too long and should be flagged
```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the line with spaces should be flagged (line without spaces is ignored)
}

#[test]
fn test_setext_headings() {
    let rule = MD013LineLength::new(20, false, true, false, false);
    let content = "This is a very long setext heading\n==========================\nThis is another long heading.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 3);
}

#[test]
fn test_table_alignment() {
    let rule = MD013LineLength::new(20, false, false, true, false);
    let content = "| This is a long cell |\n| Another long cell |\n| Yet another long cell |";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);
}

#[test]
fn test_parity_wrapped_paragraph_only_last_line_checked() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "This is a very long line that exceeds the eighty character limit but
is continued here and should not be flagged because only the last line
of the paragraph is checked for length and this line is also very long and should be flagged by MD013.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_parity_list_items_are_checked() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "- This is a very long list item that exceeds the eighty character limit and should be flagged by MD013.\n- Short item.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_parity_only_url_line_skipped() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "https://example.com/this/is/a/very/long/url/that/should/not/be/flagged/by/md013/even/if/it/exceeds/the/limit";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_parity_line_containing_url_checked() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "This line contains a URL https://example.com/this/is/a/very/long/url but is not only a URL and should be checked for length if it exceeds the limit.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_parity_code_blocks_checked() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "```
// This is a very long line inside a code block that should now be flagged by MD013 to match markdownlint behavior.
```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Code block line should be flagged (matches markdownlint)
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_parity_headings_skipped() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "# This is a very long heading that should not be flagged by MD013 even if it is over the limit";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_parity_soft_wrapped_paragraph_only_last_line_checked() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "This is a long paragraph that
is soft-wrapped and
only the last line should be checked for length if it is too long and the previous lines should not be flagged.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_parity_hard_line_breaks_each_line_checked() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "This is a long line that exceeds the limit and is intentionally made much longer than eighty characters to trigger the warning.  \nThis is another long line that exceeds the limit and is also intentionally made much longer than eighty characters to trigger the warning.";
    // Debug: print the raw bytes of the first line
    let first_line = content.lines().next().unwrap();
    println!("First line bytes: {:?}", first_line.as_bytes());
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_parity_image_reference_line_skipped() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "![alt text][reference]";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_parity_link_reference_definition_skipped() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content =
        "[reference]: https://example.com/this/is/a/very/long/url/that/should/not/be/flagged";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_parity_table_rows_skipped() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "| This is a very long table cell that should not be flagged by MD013 even if it is over the limit |
| --- |";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_parity_blockquotes_skipped() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "> This is a very long blockquote line that should not be flagged by MD013 even if it is over the limit.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}
