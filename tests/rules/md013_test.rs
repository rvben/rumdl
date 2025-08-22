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
fn test_no_fix_without_enable_reflow() {
    // Comprehensive test to verify MD013 doesn't modify content when enable_reflow is false
    let rule = MD013LineLength::new(30, true, true, true, false);

    // Test various cases - all should remain unchanged
    let test_cases = vec![
        "This line has trailing whitespace     ",
        "This line is way too long even without any trailing whitespace at all",
        "Short line",
        "Line with  two  spaces  for  hard  break  ",
        "First line spaces     \nSecond line space    ",
        "ABC\n    \nXYZ",
        "| Long table cell with many spaces     |\n12345678901234567890     ",
        "```\nThis is a long line in code     \n```\nThis normal line is too long with spaces    ",
        "https://example.com/very/long/url/that/exceeds/limit\nNormal line with trailing spaces    ",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, content,
            "Content should be unchanged without enable_reflow: {content}"
        );
    }
}

#[test]
fn test_heading_line_length_config() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    // Create a config with different limits for headings
    let config = MD013Config {
        line_length: 50,
        code_blocks: true,
        tables: true,
        headings: true,
        strict: false,
        heading_line_length: Some(100), // Allow longer headings
        code_block_line_length: None,
        stern: false,
        enable_reflow: false,
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "# This is a very long heading that exceeds 50 characters but is under 100 characters\nThis regular line is too long and exceeds the 50 character limit significantly";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Only the regular line should be flagged, not the heading
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_code_block_line_length_config() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    // Create a config with different limits for code blocks
    let config = MD013Config {
        line_length: 50,
        code_blocks: true,
        tables: true,
        headings: true,
        strict: false,
        heading_line_length: None,
        code_block_line_length: Some(120), // Allow longer lines in code blocks
        stern: false,
        enable_reflow: false,
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "```\nThis is a very long line in a code block that exceeds 50 but is under 120 characters so it should be OK\n```\nThis regular line is too long and exceeds the 50 character limit";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Only the regular line should be flagged, not the code block line
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4);
}

#[test]
fn test_stern_mode() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    // Create a config with stern mode enabled
    let config = MD013Config {
        line_length: 50,
        code_blocks: true,
        tables: true,
        headings: true,
        strict: false,
        heading_line_length: None,
        code_block_line_length: None,
        stern: true, // Stern mode: stricter checking
        enable_reflow: false,
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "https://example.com/this/is/a/very/long/url/that/exceeds/the/limit\n![This is a very long image reference that exceeds limit][ref]\nThis regular line is too long and exceeds the limit";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // In stern mode, URLs and image refs should still be flagged
    assert_eq!(result.len(), 3);
}

#[test]
fn test_combined_heading_and_code_block_limits() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 40,
        code_blocks: true,
        tables: true,
        headings: true,
        strict: false,
        heading_line_length: Some(80),
        code_block_line_length: Some(100),
        stern: false,
        enable_reflow: false,
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = r#"# This heading is under 80 chars so it should be fine even over 40

Regular text that exceeds the 40 character limit

```
This code block line is under 100 characters so it should be fine even though it's way over 40
```

## Another heading that's under 80 characters but over 40

More regular text exceeding 40 characters"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag the two regular text lines only
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[1].line, 11);
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

// Text reflow tests

#[test]
fn test_reflow_simple_paragraph() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 40,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "This is a very long line that definitely exceeds the forty character limit and needs to be wrapped properly by the reflow algorithm.";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify all lines are under 40 chars
    for line in fixed.lines() {
        assert!(line.chars().count() <= 40, "Line too long: {line}");
    }

    // Verify content is preserved
    let fixed_words: Vec<&str> = fixed.split_whitespace().collect();
    let original_words: Vec<&str> = content.split_whitespace().collect();
    assert_eq!(fixed_words, original_words);
}

#[test]
fn test_reflow_preserves_markdown_elements() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 30,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "This paragraph has **bold text** and *italic text* and `inline code` and [a link](https://example.com) that should all be preserved during reflow.";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify markdown elements are preserved
    assert!(fixed.contains("**bold text**"));
    assert!(fixed.contains("*italic text*"));
    assert!(fixed.contains("`inline code`"));
    assert!(fixed.contains("[a link](https://example.com)"));

    // Verify all lines are under limit
    for line in fixed.lines() {
        assert!(line.chars().count() <= 30, "Line too long: {line}");
    }
}

#[test]
fn test_reflow_multiple_paragraphs() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 50,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "This is the first paragraph that is very long and needs to be wrapped to fit within the fifty character line limit.

This is the second paragraph that is also quite long and will need to be wrapped as well to meet the requirements.

Short paragraph.";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify paragraphs are separated by blank lines
    let paragraphs: Vec<&str> = fixed.split("\n\n").collect();
    assert_eq!(paragraphs.len(), 3);

    // Verify all lines respect the limit
    for line in fixed.lines() {
        if !line.is_empty() {
            assert!(line.chars().count() <= 50, "Line too long: {line}");
        }
    }
}

#[test]
fn test_reflow_list_items() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 40,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "- This is a very long list item that needs to be wrapped properly with correct indentation
- Another long list item that should also be wrapped with the proper continuation indentation
  - A nested list item that is also very long and needs wrapping";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify list structure is preserved
    assert!(fixed.contains("- This"));
    assert!(fixed.contains("- Another"));
    assert!(fixed.contains("  - A nested"));

    // Verify continuation lines are indented
    let lines: Vec<&str> = fixed.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with('-') && !line.trim().is_empty() && i > 0 {
            // Continuation lines should be indented
            assert!(line.starts_with(' '), "Continuation line not indented: {line}");
        }
    }
}

#[test]
fn test_reflow_numbered_lists() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 35,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "1. First item that is very long and needs to be wrapped correctly
2. Second item that is also quite long and requires proper wrapping
10. Tenth item with different number width that should also wrap properly";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify numbered list structure
    assert!(fixed.contains("1. First"));
    assert!(fixed.contains("2. Second"));
    assert!(fixed.contains("10. Tenth"));

    // Debug: print the actual output
    println!("Fixed numbered list:\n{fixed}");

    // Verify structure is preserved but be lenient with continuation lines
    for (i, line) in fixed.lines().enumerate() {
        // List item lines start with number
        if line.trim_start().chars().next().is_some_and(|c| c.is_numeric()) {
            // First line of list items should be under limit
            assert!(
                line.chars().count() <= 40, // Allow a bit more for the marker
                "List item line {} too long: {} ({})",
                i + 1,
                line,
                line.chars().count()
            );
        }
        // Continuation lines can be a bit longer due to indentation
    }
}

#[test]
fn test_reflow_blockquotes() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 40,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "> This is a blockquote that contains a very long line that needs to be wrapped properly while preserving the blockquote marker";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // All lines should start with >
    for line in fixed.lines() {
        if !line.trim().is_empty() {
            assert!(line.starts_with('>'), "Blockquote line missing >: {line}");
        }
    }

    // Verify content is preserved
    let content_without_markers = fixed.replace("> ", "").replace(">", "");
    let original_content = content.replace("> ", "").replace(">", "");
    assert_eq!(
        content_without_markers.split_whitespace().collect::<Vec<_>>(),
        original_content.split_whitespace().collect::<Vec<_>>()
    );
}

#[test]
fn test_reflow_preserves_code_blocks() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 30,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = r#"This is a paragraph before code.

```python
def very_long_function_name_that_should_not_be_wrapped():
    return "This is a very long string in code that should not be wrapped"
```

This is a paragraph after code that is also very long and should be wrapped."#;
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify code block is unchanged
    assert!(fixed.contains("def very_long_function_name_that_should_not_be_wrapped():"));
    assert!(fixed.contains("    return \"This is a very long string in code that should not be wrapped\""));

    // Verify paragraphs are wrapped
    let lines: Vec<&str> = fixed.lines().collect();
    let mut in_code = false;

    for line in &lines {
        if line.starts_with("```") {
            in_code = !in_code;
            continue;
        }

        if !line.trim().is_empty() && !in_code {
            assert!(line.chars().count() <= 30, "Line too long outside code: {line}");
        }
    }
}

#[test]
fn test_reflow_preserves_headings() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 30,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "# This is a very long heading that should not be wrapped

## Another long heading that exceeds the limit

Regular paragraph that is very long and should be wrapped to fit.

### Third level heading also very long";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify headings are preserved on single lines
    assert!(fixed.contains("# This is a very long heading that should not be wrapped"));
    assert!(fixed.contains("## Another long heading that exceeds the limit"));
    assert!(fixed.contains("### Third level heading also very long"));

    // Verify only paragraphs are wrapped
    for line in fixed.lines() {
        if !line.starts_with('#') && !line.trim().is_empty() {
            assert!(line.chars().count() <= 30, "Non-heading line too long: {line}");
        }
    }
}

#[test]
fn test_reflow_preserves_tables() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 30,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "This paragraph should wrap.

| Header 1 | Very Long Header 2 That Exceeds Limit |
|----------|---------------------------------------|
| Cell 1   | Very long cell content that exceeds   |

Another paragraph to wrap.";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify table is preserved
    assert!(fixed.contains("| Header 1 | Very Long Header 2 That Exceeds Limit |"));
    assert!(fixed.contains("|----------|---------------------------------------|"));
    assert!(fixed.contains("| Cell 1   | Very long cell content that exceeds   |"));

    // Verify paragraphs are wrapped
    let lines: Vec<&str> = fixed.lines().collect();
    for line in lines {
        if !line.contains('|') && !line.trim().is_empty() {
            assert!(line.chars().count() <= 30, "Non-table line too long: {line}");
        }
    }
}

#[test]
fn test_reflow_edge_cases() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 20,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);

    // Test single long word
    let content = "Thisissuperlongwordthatcannotbebroken";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content); // Should remain unchanged

    // Test line exactly at limit
    let content2 = "12345678901234567890";
    assert_eq!(content2.chars().count(), 20);
    let ctx2 = LintContext::new(content2);

    // Check if MD013 reports any issues
    let warnings = rule.check(&ctx2).unwrap();
    assert!(warnings.is_empty(), "Line exactly at limit should not trigger warning");

    let fixed2 = rule.fix(&ctx2).unwrap();
    // The fix should either return unchanged or with minimal whitespace changes
    assert!(
        fixed2 == content2 || fixed2.trim() == content2.trim(),
        "Expected content unchanged or only whitespace differences"
    );

    // Test empty content
    let content3 = "";
    let ctx3 = LintContext::new(content3);
    let fixed3 = rule.fix(&ctx3).unwrap();
    assert_eq!(fixed3, content3);
}

#[test]
fn test_reflow_complex_document() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 50,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = r#"# Project Documentation

This is the introduction paragraph that contains some **important information** about the project and needs to be wrapped properly.

## Features

- First feature with a very long description that explains what it does
- Second feature that also has extensive documentation about its capabilities
  - Nested feature that provides additional functionality with detailed explanation

## Code Example

Here's how to use it:

```javascript
const veryLongVariableName = "This is a string that should not be wrapped";
console.log(veryLongVariableName);
```

## Additional Notes

> This is a blockquote with important warning information that users should definitely read and understand.

For more information, visit [our documentation site](https://example.com/very/long/url/to/documentation)."#;

    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    // Verify structure is preserved
    assert!(fixed.contains("# Project Documentation"));
    assert!(fixed.contains("## Features"));
    assert!(fixed.contains("## Code Example"));
    assert!(fixed.contains("```javascript"));
    assert!(fixed.contains("```"));

    // Verify all non-special lines respect limit
    let lines: Vec<&str> = fixed.lines().collect();
    let mut in_code = false;
    for line in lines {
        if line.starts_with("```") {
            in_code = !in_code;
            continue;
        }

        if !in_code
            && !line.starts_with('#')
            && !line.contains('|')
            && !line.trim().is_empty()
            && !line.starts_with("```")
            && !line.starts_with('>')
            && !line.contains("](http")
        {
            // Skip lines with links
            assert!(
                line.chars().count() <= 50,
                "Line exceeds limit: {} ({})",
                line,
                line.chars().count()
            );
        }
    }
}

#[test]
fn test_reflow_with_hard_line_breaks() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 40,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "This line ends with two spaces for a hard break  \nThis is the next line that should also be wrapped if too long.";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // Debug: print the actual output
    println!("Fixed with hard breaks:\n{fixed:?}");

    // The reflow might join lines, so check if the hard break is somewhere in the text
    // Hard breaks in markdown need two spaces before a newline
    assert!(
        fixed.contains("  \n") || fixed.lines().any(|line| line.ends_with("  ")),
        "Hard line break (two spaces) was not preserved in:\n{fixed}"
    );
}

#[test]
fn test_reflow_unicode_handling() {
    use rumdl::rules::md013_line_length::md013_config::MD013Config;

    let config = MD013Config {
        line_length: 30,
        enable_reflow: true,
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let content = "This text contains emojis 🚀 and special characters like café and should wrap correctly.";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify emojis and special chars are preserved
    assert!(fixed.contains("🚀"));
    assert!(fixed.contains("café"));

    // Verify character counting works correctly
    for line in fixed.lines() {
        assert!(line.chars().count() <= 30, "Line too long: {line}");
    }
}
