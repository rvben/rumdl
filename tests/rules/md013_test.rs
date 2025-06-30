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
    let content = "| This is a very long table cell that should be flagged |\n| This is another long cell |";
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
    let content =
        "# Long heading\n```\nLong code\n```\n| Long table |\nThis is a very long line that should be flagged.";
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
    let content = "[reference]: https://very-long-url-that-exceeds-length.com/path/to/resource\nNormal text.";
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
    let content =
        "https://example.com/this/is/a/very/long/url/that/should/not/be/flagged/by/md013/even/if/it/exceeds/the/limit";
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
    let content = "[reference]: https://example.com/this/is/a/very/long/url/that/should/not/be/flagged";
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
    let content =
        "> This is a very long blockquote line that should not be flagged by MD013 even if it is over the limit.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

// Additional comprehensive tests

#[test]
fn test_lines_exactly_at_limit() {
    let rule = MD013LineLength::new(50, true, true, true, false);
    let content = "This line is exactly fifty characters long here!!!";
    assert_eq!(content.chars().count(), 50);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_lines_one_char_over_limit() {
    let rule = MD013LineLength::new(50, true, true, true, false);
    let content = "This line is exactly fifty characters long here!!!!";
    assert_eq!(content.chars().count(), 51);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 51);
    assert_eq!(result[0].end_column, 52);
}

#[test]
fn test_multiple_violations_same_file() {
    let rule = MD013LineLength::new(30, true, true, true, false);
    let content = "This is the first line that is way too long for the limit\nShort line\nThis is the third line that is also way too long for the limit\nOK\nFifth line is also exceeding the thirty character limit";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 3);
    assert_eq!(result[2].line, 5);
}

#[test]
fn test_configuration_different_line_lengths() {
    let content = "This is a line that is exactly sixty characters long in total";
    assert_eq!(content.chars().count(), 61);
    
    // Test with limit of 50 - should fail
    let rule50 = MD013LineLength::new(50, true, true, true, false);
    let ctx = LintContext::new(content);
    let result50 = rule50.check(&ctx).unwrap();
    assert_eq!(result50.len(), 1);
    
    // Test with limit of 70 - should pass
    let rule70 = MD013LineLength::new(70, true, true, true, false);
    let result70 = rule70.check(&ctx).unwrap();
    assert!(result70.is_empty());
    
    // Test with limit of 100 - should pass
    let rule100 = MD013LineLength::new(100, true, true, true, false);
    let result100 = rule100.check(&ctx).unwrap();
    assert!(result100.is_empty());
}

#[test]
fn test_code_blocks_configurable() {
    let content = "```\nThis is a very long line inside a code block that exceeds the limit significantly\n```";
    let ctx = LintContext::new(content);
    
    // With code_blocks = true (check code blocks)
    let rule_check = MD013LineLength::new(50, true, true, true, false);
    let result_check = rule_check.check(&ctx).unwrap();
    assert_eq!(result_check.len(), 1);
    assert_eq!(result_check[0].line, 2);
    
    // With code_blocks = false (don't check code blocks)
    let rule_skip = MD013LineLength::new(50, false, true, true, false);
    let result_skip = rule_skip.check(&ctx).unwrap();
    assert!(result_skip.is_empty());
}

#[test]
fn test_tables_configurable() {
    let content = "| This is a very long table cell | Another very long table cell that exceeds limit |";
    let ctx = LintContext::new(content);
    
    // With tables = true (check tables) 
    let rule_check = MD013LineLength::new(50, true, true, true, false);
    let result_check = rule_check.check(&ctx).unwrap();
    assert!(result_check.is_empty()); // Tables are skipped when tables=true
    
    // With tables = false (don't skip tables)
    let rule_no_skip = MD013LineLength::new(50, true, false, true, false);
    let result_no_skip = rule_no_skip.check(&ctx).unwrap();
    assert_eq!(result_no_skip.len(), 1);
}

#[test]
fn test_headings_configurable() {
    let content = "# This is a very long heading that exceeds the maximum character limit significantly";
    let ctx = LintContext::new(content);
    
    // With headings = true (skip headings)
    let rule_skip = MD013LineLength::new(50, true, true, true, false);
    let result_skip = rule_skip.check(&ctx).unwrap();
    assert!(result_skip.is_empty());
    
    // With headings = false (check headings)
    let rule_check = MD013LineLength::new(50, true, true, false, false);
    let result_check = rule_check.check(&ctx).unwrap();
    assert_eq!(result_check.len(), 1);
    assert_eq!(result_check[0].line, 1);
}

#[test]
fn test_urls_configurable_with_strict() {
    let long_url = "https://example.com/this/is/a/very/long/url/that/exceeds/the/character/limit/significantly";
    let ctx = LintContext::new(long_url);
    
    // Non-strict mode - URL should be ignored
    let rule_lenient = MD013LineLength::new(50, true, true, true, false);
    let result_lenient = rule_lenient.check(&ctx).unwrap();
    assert!(result_lenient.is_empty());
    
    // Strict mode - URL should be flagged
    let rule_strict = MD013LineLength::new(50, true, true, true, true);
    let result_strict = rule_strict.check(&ctx).unwrap();
    assert_eq!(result_strict.len(), 1);
}

#[test]
fn test_strict_mode_vs_non_strict_mode() {
    let content = "https://example.com/very/long/url/exceeding/limit\n![This is a very long image reference][long-ref]\n[ref]: https://example.com/another/long/url\nThis is a normal line that is way too long and should always be flagged";
    let ctx = LintContext::new(content);
    
    // Non-strict mode
    let rule_lenient = MD013LineLength::new(30, true, true, true, false);
    let result_lenient = rule_lenient.check(&ctx).unwrap();
    assert_eq!(result_lenient.len(), 1); // Only the normal line
    assert_eq!(result_lenient[0].line, 4);
    
    // Strict mode  
    let rule_strict = MD013LineLength::new(30, true, true, true, true);
    let result_strict = rule_strict.check(&ctx).unwrap();
    assert_eq!(result_strict.len(), 4); // All lines flagged
}

// Fix method tests

#[test]
fn test_fix_trailing_whitespace() {
    let rule = MD013LineLength::new(57, true, true, true, false);
    let content = "This line has trailing whitespace that makes it too long     \nThis line is OK";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "This line has trailing whitespace that makes it too long\nThis line is OK");
}

#[test]
fn test_fix_multiple_lines_with_whitespace() {
    let rule = MD013LineLength::new(18, true, true, true, false);
    let content = "First line spaces     \nSecond line space    \nThird line is fine\nFourth line space     ";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "First line spaces\nSecond line space\nThird line is fine\nFourth line space");
}

#[test]
fn test_fix_preserves_intentional_trailing_spaces() {
    let rule = MD013LineLength::new(62, true, true, true, false);
    let content = "This line has two trailing spaces for hard break  \nThis line has excessive trailing spaces that should be trimmed     ";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "This line has two trailing spaces for hard break  \nThis line has excessive trailing spaces that should be trimmed");
}

#[test]
fn test_fix_empty_lines_with_spaces() {
    let rule = MD013LineLength::new(3, true, true, true, false);
    let content = "ABC\n    \nXYZ";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "ABC\n\nXYZ");
}

#[test]
fn test_fix_code_blocks_not_modified() {
    let rule = MD013LineLength::new(41, true, true, true, false);
    let content = "```\nThis is a long line in code     \n```\nThis normal line is too long with spaces    ";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```\nThis is a long line in code     \n```\nThis normal line is too long with spaces");
}

#[test]
fn test_fix_tables_not_modified() {
    let rule = MD013LineLength::new(15, true, true, true, false);
    let content = "| Long table cell with many spaces     |\n12345678901234567890     ";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    // Table line is not modified (tables are skipped when tables=true)
    // Normal line has 20 chars + 5 spaces = 25 total, after trim would be 20 chars (still over 15)
    // So no fix should happen
    assert_eq!(fixed, content);
}

#[test]
fn test_fix_no_changes_when_no_violations() {
    let rule = MD013LineLength::new(80, true, true, true, false);
    let content = "Short line\nAnother short line\nThird short line";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_fix_urls_not_modified() {
    let rule = MD013LineLength::new(32, true, true, true, false);
    let content = "https://example.com/very/long/url/that/exceeds/limit\nNormal line with trailing spaces    ";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "https://example.com/very/long/url/that/exceeds/limit\nNormal line with trailing spaces");
}

#[test]
fn test_unicode_characters_counted_correctly() {
    let rule = MD013LineLength::new(20, true, true, true, false);
    // This line has emojis which should be counted as single characters
    let content = "Hello 👋 World 🌍 Test"; // Should be exactly 20 chars
    assert_eq!(content.chars().count(), 20);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
    
    // Add one more emoji to exceed limit
    let content_long = "Hello 👋 World 🌍 Test 🚀";
    assert_eq!(content_long.chars().count(), 22);
    let ctx_long = LintContext::new(content_long);
    let result_long = rule.check(&ctx_long).unwrap();
    assert_eq!(result_long.len(), 1);
}

#[test]
fn test_setext_heading_underlines_ignored() {
    let rule = MD013LineLength::new(20, true, true, true, false);
    let content = "Short heading\n========================================\nAnother heading\n----------------------------------------";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Underlines should be ignored
}

#[test]
fn test_html_blocks_skipped() {
    let rule = MD013LineLength::new(30, true, true, true, false);
    let content = "<div class=\"very-long-class-name-that-exceeds-the-character-limit\">\n  Content\n</div>\nThis normal line is way too long for limit";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4); // Only normal line flagged
}

#[test]
fn test_inline_code_with_long_strings() {
    let rule = MD013LineLength::new(50, true, true, true, false);
    let content = "This line has `AVeryLongInlineCodeStringWithoutSpaces` in it";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Should still be flagged as the whole line is checked
}

#[test]
fn test_fix_applies_from_end_to_beginning() {
    let rule = MD013LineLength::new(23, true, true, true, false);
    let content = "First line with spaces    \nSecond line with spaces    \nThird line with spaces    ";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    // All lines should have trailing spaces removed
    assert_eq!(fixed, "First line with spaces\nSecond line with spaces\nThird line with spaces");
}

#[test]
fn test_column_positions_for_excess_characters() {
    let rule = MD013LineLength::new(10, true, true, true, false);
    let content = "1234567890ABCDEF"; // 16 chars, limit is 10
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 11); // Start of excess
    assert_eq!(result[0].end_column, 17); // End of line + 1
}

#[test]
fn test_fix_only_when_trimming_helps() {
    let rule = MD013LineLength::new(50, true, true, true, false);
    // This line is too long even after trimming
    let content1 = "This line is way too long even without any trailing whitespace at all     ";
    let ctx1 = LintContext::new(content1);
    let fixed1 = rule.fix(&ctx1).unwrap();
    assert_eq!(fixed1, content1); // No fix because trimming doesn't help
    
    // This line becomes OK after trimming  
    let content2 = "This line becomes OK after trimming the spaces     ";
    let ctx2 = LintContext::new(content2);
    let fixed2 = rule.fix(&ctx2).unwrap();
    assert_eq!(fixed2, "This line becomes OK after trimming the spaces");
}

#[test]
fn test_fix_with_exact_whitespace_removal() {
    let rule = MD013LineLength::new(40, true, true, true, false);
    // Line is 45 chars with spaces, 40 without - exactly at limit after trim
    let content = "1234567890123456789012345678901234567890     ";
    assert_eq!(content.trim_end().len(), 40);
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "1234567890123456789012345678901234567890");
}

#[test]
fn test_no_fix_for_lines_without_trailing_whitespace() {
    let rule = MD013LineLength::new(30, true, true, true, false);
    let content = "This line is too long but has no trailing whitespace";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content); // No change
    
    // Verify it was still flagged
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].fix.is_none());
}
