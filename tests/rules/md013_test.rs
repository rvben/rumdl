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
    let rule = MD013LineLength::new(20, true, true, true, false);
    let content = "This is a very long line that exceeds the maximum length limit.\nThis is another very long line that also exceeds the limit.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_code_blocks() {
    let rule = MD013LineLength::new(20, false, true, true, false);
    let content = "```\nThis is a very long line in a code block that should be ignored.\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_tables() {
    let rule = MD013LineLength::new(20, true, false, true, false);
    let content = "| Column 1 | Column 2 | Column 3 | Column 4 | Column 5 |\n|-----------|-----------|-----------|-----------|-----------|";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_headings() {
    let rule = MD013LineLength::new(20, true, true, false, false);
    let content =
        "# This is a very long heading that exceeds the line length limit\nThis is a normal line.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only normal line should be flagged
    assert_eq!(result[0].line, 2);
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
    let content = "```\nThisIsAVeryLongStringWithoutSpacesThatShouldBeIgnored\nThis is a normal code line that is too long and should be flagged\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the normal code line should be flagged
}

#[test]
fn test_setext_headings() {
    let rule = MD013LineLength::new(20, true, true, false, false);
    let content =
        "This is a very long setext heading\n==========================\nThis is a normal line.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only normal line should be flagged
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_table_alignment() {
    let rule = MD013LineLength::new(20, true, false, true, false);
    let content = "| Left | Center | Right |\n|:-----|:------:|------:|\n| Long cell content | More content | Content |";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Table should be ignored
}
