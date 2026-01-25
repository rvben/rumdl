use super::*;
use crate::config::MarkdownFlavor;
use crate::lint_context::LintContext;

#[test]
fn test_default_config() {
    let rule = MD013LineLength::default();
    assert_eq!(rule.config.line_length.get(), 80);
    assert!(rule.config.code_blocks); // Default is true
    assert!(!rule.config.tables); // Default is false (changed to prevent conflicts with MD060)
    assert!(rule.config.headings); // Default is true
    assert!(!rule.config.strict);
}

#[test]
fn test_custom_config() {
    let rule = MD013LineLength::new(100, true, true, false, true);
    assert_eq!(rule.config.line_length.get(), 100);
    assert!(rule.config.code_blocks);
    assert!(rule.config.tables);
    assert!(!rule.config.headings);
    assert!(rule.config.strict);
}

#[test]
fn test_basic_line_length_violation() {
    let rule = MD013LineLength::new(50, false, false, false, false);
    let content = "This is a line that is definitely longer than fifty characters and should trigger a warning.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1);
    assert!(result[0].message.contains("Line length"));
    assert!(result[0].message.contains("exceeds 50 characters"));
}

#[test]
fn test_no_violation_under_limit() {
    let rule = MD013LineLength::new(100, false, false, false, false);
    let content = "Short line.\nAnother short line.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 0);
}

#[test]
fn test_multiple_violations() {
    let rule = MD013LineLength::new(30, false, false, false, false);
    let content =
        "This line is definitely longer than thirty chars.\nThis is also a line that exceeds the limit.\nShort line.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_no_lint_front_matter() {
    let rule = MD013LineLength::new(80, false, false, false, false);

    // YAML front matter with long lines should NOT be flagged
    let content = "---\ntitle: This is a very long title that exceeds eighty characters and should not trigger MD013\nauthor: Another very long line in YAML front matter that exceeds the eighty character limit\n---\n\n# Heading\n\nThis is a very long line in actual content that exceeds eighty characters and SHOULD trigger MD013.\n";

    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the content line, not front matter lines
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 8); // The actual content line

    // Also test with TOML front matter
    let content_toml = "+++\ntitle = \"This is a very long title in TOML that exceeds eighty characters and should not trigger MD013\"\nauthor = \"Another very long line in TOML front matter that exceeds the eighty character limit\"\n+++\n\n# Heading\n\nThis is a very long line in actual content that exceeds eighty characters and SHOULD trigger MD013.\n";

    let ctx_toml = LintContext::new(content_toml, crate::config::MarkdownFlavor::Standard, None);
    let result_toml = rule.check(&ctx_toml).unwrap();

    // Should only flag the content line, not TOML front matter lines
    assert_eq!(result_toml.len(), 1);
    assert_eq!(result_toml[0].line, 8); // The actual content line
}

#[test]
fn test_code_blocks_exemption() {
    // With code_blocks = false, code blocks should be skipped
    let rule = MD013LineLength::new(30, false, false, false, false);
    let content = "```\nThis is a very long line inside a code block that should be ignored.\n```";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 0);
}

#[test]
fn test_code_blocks_not_exempt_when_configured() {
    // With code_blocks = true, code blocks should be checked
    let rule = MD013LineLength::new(30, true, false, false, false);
    let content = "```\nThis is a very long line inside a code block that should NOT be ignored.\n```";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(!result.is_empty());
}

#[test]
fn test_heading_checked_when_enabled() {
    let rule = MD013LineLength::new(30, false, false, true, false);
    let content = "# This is a very long heading that would normally exceed the limit";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1);
}

#[test]
fn test_heading_exempt_when_disabled() {
    let rule = MD013LineLength::new(30, false, false, false, false);
    let content = "# This is a very long heading that should trigger a warning";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 0);
}

#[test]
fn test_table_checked_when_enabled() {
    let rule = MD013LineLength::new(30, false, true, false, false);
    let content = "| This is a very long table header | Another long column header |\n|-----------------------------------|-------------------------------|";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 2); // Both table lines exceed limit
}

#[test]
fn test_issue_78_tables_after_fenced_code_blocks() {
    // Test for GitHub issue #78 - tables with tables=false after fenced code blocks
    let rule = MD013LineLength::new(20, false, false, false, false); // tables=false
    let content = r#"# heading

```plain
some code block longer than 20 chars length
```

this is a very long line

| column A | column B |
| -------- | -------- |
| `var` | `val` |
| value 1 | value 2 |

correct length line"#;
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag line 7 ("this is a very long line"), not the table lines
    assert_eq!(result.len(), 1, "Should only flag 1 line (the non-table long line)");
    assert_eq!(result[0].line, 7, "Should flag line 7");
    assert!(result[0].message.contains("24 exceeds 20"));
}

#[test]
fn test_issue_78_tables_with_inline_code() {
    // Test that tables with inline code (backticks) are properly detected as tables
    let rule = MD013LineLength::new(20, false, false, false, false); // tables=false
    let content = r#"| column A | column B |
| -------- | -------- |
| `var with very long name` | `val exceeding limit` |
| value 1 | value 2 |

This line exceeds limit"#;
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the last line, not the table lines
    assert_eq!(result.len(), 1, "Should only flag the non-table line");
    assert_eq!(result[0].line, 6, "Should flag line 6");
}

#[test]
fn test_issue_78_indented_code_blocks() {
    // Test with indented code blocks instead of fenced
    // Indented code blocks require 4 spaces of indentation (CommonMark spec)
    let rule = MD013LineLength::new(20, false, false, false, false); // tables=false, code_blocks=false
    // Use raw string with actual 4 spaces for indented code block on line 3
    let content = "# heading

    some code block longer than 20 chars length

this is a very long line

| column A | column B |
| -------- | -------- |
| value 1 | value 2 |

correct length line";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag line 5 ("this is a very long line"), not the table lines
    // Line 3 is an indented code block (4 spaces) so it's skipped when code_blocks=false
    assert_eq!(result.len(), 1, "Should only flag 1 line (the non-table long line)");
    assert_eq!(result[0].line, 5, "Should flag line 5");
}

#[test]
fn test_url_exemption() {
    let rule = MD013LineLength::new(30, false, false, false, false);
    let content = "https://example.com/this/is/a/very/long/url/that/exceeds/the/limit";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 0);
}

#[test]
fn test_image_reference_exemption() {
    let rule = MD013LineLength::new(30, false, false, false, false);
    let content = "![This is a very long image alt text that exceeds limit][reference]";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 0);
}

#[test]
fn test_link_reference_exemption() {
    let rule = MD013LineLength::new(30, false, false, false, false);
    let content = "[reference]: https://example.com/very/long/url/that/exceeds/limit";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 0);
}

#[test]
fn test_strict_mode() {
    let rule = MD013LineLength::new(30, false, false, false, true);
    let content = "https://example.com/this/is/a/very/long/url/that/exceeds/the/limit";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // In strict mode, even URLs trigger warnings
    assert_eq!(result.len(), 1);
}

#[test]
fn test_blockquote_exemption() {
    let rule = MD013LineLength::new(30, false, false, false, false);
    let content = "> This is a very long line inside a blockquote that should be ignored.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 0);
}

#[test]
fn test_setext_heading_underline_exemption() {
    let rule = MD013LineLength::new(30, false, false, false, false);
    let content = "Heading\n========================================";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // The underline should be exempt
    assert_eq!(result.len(), 0);
}

#[test]
fn test_no_fix_without_reflow() {
    let rule = MD013LineLength::new(60, false, false, false, false);
    let content = "This line has trailing whitespace that makes it too long      ";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1);
    // Without reflow, no fix is provided
    assert!(result[0].fix.is_none());

    // Fix method returns content unchanged
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_character_vs_byte_counting() {
    let rule = MD013LineLength::new(10, false, false, false, false);
    // Unicode characters should count as 1 character each
    let content = "你好世界这是测试文字超过限制"; // 14 characters
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_empty_content() {
    let rule = MD013LineLength::default();
    let ctx = LintContext::new("", crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 0);
}

#[test]
fn test_excess_range_calculation() {
    let rule = MD013LineLength::new(10, false, false, false, false);
    let content = "12345678901234567890"; // 20 chars, limit is 10
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1);
    // The warning should highlight from character 11 onwards
    assert_eq!(result[0].column, 11);
    assert_eq!(result[0].end_column, 21);
}

#[test]
fn test_html_block_exemption() {
    let rule = MD013LineLength::new(30, false, false, false, false);
    let content = "<div>\nThis is a very long line inside an HTML block that should be ignored.\n</div>";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // HTML blocks should be exempt
    assert_eq!(result.len(), 0);
}

#[test]
fn test_mixed_content() {
    // code_blocks=false, tables=false, headings=false (all skipped/exempt)
    let rule = MD013LineLength::new(30, false, false, false, false);
    let content = r#"# This heading is very long but should be exempt

This regular paragraph line is too long and should trigger.

```
Code block line that is very long but exempt.
```

| Table | With very long content |
|-------|------------------------|

Another long line that should trigger a warning."#;

    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have warnings for the two regular paragraph lines only
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[1].line, 12);
}

#[test]
fn test_fix_without_reflow_preserves_content() {
    let rule = MD013LineLength::new(50, false, false, false, false);
    let content = "Line 1\nThis line has trailing spaces and is too long      \nLine 3";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    // Without reflow, content is unchanged
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_content_detection() {
    let rule = MD013LineLength::default();

    // Use a line longer than default line_length (80) to ensure it's not skipped
    let long_line = "a".repeat(100);
    let ctx = LintContext::new(&long_line, crate::config::MarkdownFlavor::Standard, None);
    assert!(!rule.should_skip(&ctx)); // Should not skip processing when there's long content

    let empty_ctx = LintContext::new("", crate::config::MarkdownFlavor::Standard, None);
    assert!(rule.should_skip(&empty_ctx)); // Should skip processing when content is empty
}

#[test]
fn test_rule_metadata() {
    let rule = MD013LineLength::default();
    assert_eq!(rule.name(), "MD013");
    assert_eq!(rule.description(), "Line length should not be excessive");
    assert_eq!(rule.category(), RuleCategory::Whitespace);
}

#[test]
fn test_url_embedded_in_text() {
    let rule = MD013LineLength::new(50, false, false, false, false);

    // This line would be 85 chars, but only ~45 without the URL
    let content = "Check the docs at https://example.com/very/long/url/that/exceeds/limit for info";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not flag because effective length (with URL placeholder) is under 50
    assert_eq!(result.len(), 0);
}

#[test]
fn test_multiple_urls_in_line() {
    let rule = MD013LineLength::new(50, false, false, false, false);

    // Line with multiple URLs
    let content = "See https://first-url.com/long and https://second-url.com/also/very/long here";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();

    // Should not flag because effective length is reasonable
    assert_eq!(result.len(), 0);
}

#[test]
fn test_markdown_link_with_long_url() {
    let rule = MD013LineLength::new(50, false, false, false, false);

    // Markdown link with very long URL
    let content = "Check the [documentation](https://example.com/very/long/path/to/documentation/page) for details";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not flag because effective length counts link as short
    assert_eq!(result.len(), 0);
}

#[test]
fn test_line_too_long_even_without_urls() {
    let rule = MD013LineLength::new(50, false, false, false, false);

    // Line that's too long even after URL exclusion
    let content = "This is a very long line with lots of text and https://url.com that still exceeds the limit";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag because even with URL placeholder, line is too long
    assert_eq!(result.len(), 1);
}

#[test]
fn test_strict_mode_counts_urls() {
    let rule = MD013LineLength::new(50, false, false, false, true); // strict=true

    // Same line that passes in non-strict mode
    let content = "Check the docs at https://example.com/very/long/url/that/exceeds/limit for info";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // In strict mode, should flag because full URL is counted
    assert_eq!(result.len(), 1);
}

#[test]
fn test_documentation_example_from_md051() {
    let rule = MD013LineLength::new(80, false, false, false, false);

    // This is the actual line from md051.md that was causing issues
    let content = r#"For more information, see the [CommonMark specification](https://spec.commonmark.org/0.30/#link-reference-definitions)."#;
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not flag because the URL is in a markdown link
    assert_eq!(result.len(), 0);
}

#[test]
fn test_text_reflow_simple() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(30),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This is a very long line that definitely exceeds thirty characters and needs to be wrapped.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify all lines are under 30 chars
    for line in fixed.lines() {
        assert!(
            line.chars().count() <= 30,
            "Line too long: {} (len={})",
            line,
            line.chars().count()
        );
    }

    // Verify content is preserved
    let fixed_words: Vec<&str> = fixed.split_whitespace().collect();
    let original_words: Vec<&str> = content.split_whitespace().collect();
    assert_eq!(fixed_words, original_words);
}

#[test]
fn test_text_reflow_preserves_markdown_elements() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(40),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This paragraph has **bold text** and *italic text* and [a link](https://example.com) that should be preserved.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify markdown elements are preserved
    assert!(fixed.contains("**bold text**"), "Bold text not preserved in: {fixed}");
    assert!(fixed.contains("*italic text*"), "Italic text not preserved in: {fixed}");
    assert!(
        fixed.contains("[a link](https://example.com)"),
        "Link not preserved in: {fixed}"
    );

    // Verify all lines are under 40 chars
    for line in fixed.lines() {
        assert!(line.len() <= 40, "Line too long: {line}");
    }
}

#[test]
fn test_text_reflow_preserves_code_blocks() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(30),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = r#"Here is some text.

```python
def very_long_function_name_that_exceeds_limit():
return "This should not be wrapped"
```

More text after code block."#;
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify code block is preserved
    assert!(fixed.contains("def very_long_function_name_that_exceeds_limit():"));
    assert!(fixed.contains("```python"));
    assert!(fixed.contains("```"));
}

#[test]
fn test_text_reflow_preserves_lists() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(30),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = r#"Here is a list:

1. First item with a very long line that needs wrapping
2. Second item is short
3. Third item also has a long line that exceeds the limit

And a bullet list:

- Bullet item with very long content that needs wrapping
- Short bullet"#;
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify list structure is preserved
    assert!(fixed.contains("1. "));
    assert!(fixed.contains("2. "));
    assert!(fixed.contains("3. "));
    assert!(fixed.contains("- "));

    // Verify proper indentation for wrapped lines
    let lines: Vec<&str> = fixed.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim().starts_with("1.") || line.trim().starts_with("2.") || line.trim().starts_with("3.") {
            // Check if next line is a continuation (should be indented with 3 spaces for numbered lists)
            if i + 1 < lines.len()
                && !lines[i + 1].trim().is_empty()
                && !lines[i + 1].trim().starts_with(char::is_numeric)
                && !lines[i + 1].trim().starts_with("-")
            {
                // Numbered list continuation lines should have 3 spaces
                assert!(lines[i + 1].starts_with("   ") || lines[i + 1].trim().is_empty());
            }
        } else if line.trim().starts_with("-") {
            // Check if next line is a continuation (should be indented with 2 spaces for dash lists)
            if i + 1 < lines.len()
                && !lines[i + 1].trim().is_empty()
                && !lines[i + 1].trim().starts_with(char::is_numeric)
                && !lines[i + 1].trim().starts_with("-")
            {
                // Dash list continuation lines should have 2 spaces
                assert!(lines[i + 1].starts_with("  ") || lines[i + 1].trim().is_empty());
            }
        }
    }
}

#[test]
fn test_issue_83_numbered_list_with_backticks() {
    // Test for issue #83: enable_reflow was incorrectly handling numbered lists
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // The exact case from issue #83
    let content = "1. List `manifest` to find the manifest with the largest ID. Say it's `00000000000000000002.manifest` in this example.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // The expected output: properly wrapped at 100 chars with correct list formatting
    // After the fix, it correctly accounts for "1. " (3 chars) leaving 97 for content
    let expected = "1. List `manifest` to find the manifest with the largest ID. Say it's\n   `00000000000000000002.manifest` in this example.";

    assert_eq!(
        fixed, expected,
        "List should be properly reflowed with correct marker and indentation.\nExpected:\n{expected}\nGot:\n{fixed}"
    );
}

#[test]
fn test_text_reflow_disabled_by_default() {
    let rule = MD013LineLength::new(30, false, false, false, false);

    let content = "This is a very long line that definitely exceeds thirty characters.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Without reflow enabled, it should only trim whitespace (if any)
    // Since there's no trailing whitespace, content should be unchanged
    assert_eq!(fixed, content);
}

#[test]
fn test_reflow_with_hard_line_breaks() {
    // Test that lines with exactly 2 trailing spaces are preserved as hard breaks
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(40),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // Test with exactly 2 spaces (hard line break)
    let content = "This line has a hard break at the end  \nAnd this continues on the next line that is also quite long and needs wrapping";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Should preserve the hard line break (2 spaces)
    assert!(
        fixed.contains("  \n"),
        "Hard line break with exactly 2 spaces should be preserved"
    );
}

#[test]
fn test_reflow_preserves_reference_links() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(40),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content =
        "This is a very long line with a [reference link][ref] that should not be broken apart when reflowing the text.

[ref]: https://example.com";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Reference link should remain intact
    assert!(fixed.contains("[reference link][ref]"));
    assert!(!fixed.contains("[ reference link]"));
    assert!(!fixed.contains("[ref ]"));
}

#[test]
fn test_reflow_with_nested_markdown_elements() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(35),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This text has **bold with `code` inside** and should handle it properly when wrapping";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Nested elements should be preserved
    assert!(fixed.contains("**bold with `code` inside**"));
}

#[test]
fn test_reflow_with_unbalanced_markdown() {
    // Test edge case with unbalanced markdown
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(30),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This has **unbalanced bold that goes on for a very long time without closing";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Should handle gracefully without panic
    // The text reflow handles unbalanced markdown by treating it as a bold element
    // Check that the content is properly reflowed without panic
    assert!(!fixed.is_empty());
    // Verify the content is wrapped to 30 chars
    for line in fixed.lines() {
        assert!(line.len() <= 30 || line.starts_with("**"), "Line exceeds limit: {line}");
    }
}

#[test]
fn test_reflow_fix_indicator() {
    // Test that reflow provides fix indicators
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(30),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This is a very long line that definitely exceeds the thirty character limit";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should have a fix indicator when reflow is true
    assert!(!warnings.is_empty());
    assert!(
        warnings[0].fix.is_some(),
        "Should provide fix indicator when reflow is true"
    );
}

#[test]
fn test_no_fix_indicator_without_reflow() {
    // Test that without reflow, no fix is provided
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(30),
        reflow: false,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This is a very long line that definitely exceeds the thirty character limit";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should NOT have a fix indicator when reflow is false
    assert!(!warnings.is_empty());
    assert!(warnings[0].fix.is_none(), "Should not provide fix when reflow is false");
}

#[test]
fn test_reflow_preserves_all_reference_link_types() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(40),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "Test [full reference][ref] and [collapsed][] and [shortcut] reference links in a very long line.

[ref]: https://example.com
[collapsed]: https://example.com
[shortcut]: https://example.com";

    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // All reference link types should be preserved
    assert!(fixed.contains("[full reference][ref]"));
    assert!(fixed.contains("[collapsed][]"));
    assert!(fixed.contains("[shortcut]"));
}

#[test]
fn test_reflow_handles_images_correctly() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(40),
        reflow: true,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content =
        "This line has an ![image alt text](https://example.com/image.png) that should not be broken when reflowing.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Image should remain intact
    assert!(fixed.contains("![image alt text](https://example.com/image.png)"));
}

#[test]
fn test_normalize_mode_flags_short_lines() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // Content with short lines that could be combined
    let content = "This is a short line.\nAnother short line.\nA third short line that could be combined.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should flag the paragraph as needing normalization
    assert!(!warnings.is_empty(), "Should flag paragraph for normalization");
    assert!(warnings[0].message.contains("normalized"));
}

#[test]
fn test_normalize_mode_combines_short_lines() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // Content with short lines that should be combined
    let content =
        "This is a line with\nmanual line breaks at\n80 characters that should\nbe combined into longer lines.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Should combine into a single line since it's under 100 chars total
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 1, "Should combine into single line");
    assert!(lines[0].len() > 80, "Should use more of the 100 char limit");
}

#[test]
fn test_normalize_mode_preserves_paragraph_breaks() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "First paragraph with\nshort lines.\n\nSecond paragraph with\nshort lines too.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Should preserve paragraph breaks (empty lines)
    assert!(fixed.contains("\n\n"), "Should preserve paragraph breaks");

    let paragraphs: Vec<&str> = fixed.split("\n\n").collect();
    assert_eq!(paragraphs.len(), 2, "Should have two paragraphs");
}

#[test]
fn test_default_mode_only_fixes_violations() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Default, // Default mode
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // Content with short lines that are NOT violations
    let content = "This is a short line.\nAnother short line.\nA third short line.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should NOT flag anything in default mode
    assert!(warnings.is_empty(), "Should not flag short lines in default mode");

    // Fix should preserve the short lines
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed.lines().count(), 3, "Should preserve line breaks in default mode");
}

#[test]
fn test_normalize_mode_with_lists() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(80),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = r#"A paragraph with
short lines.

1. List item with
   short lines
2. Another item"#;
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Should normalize the paragraph but preserve list structure
    let lines: Vec<&str> = fixed.lines().collect();
    assert!(lines[0].len() > 20, "First paragraph should be normalized");
    assert!(fixed.contains("1. "), "Should preserve list markers");
    assert!(fixed.contains("2. "), "Should preserve list markers");
}

#[test]
fn test_normalize_mode_with_code_blocks() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = r#"A paragraph with
short lines.

```
code block should not be normalized
even with short lines
```

Another paragraph with
short lines."#;
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Code block should be preserved as-is
    assert!(fixed.contains("code block should not be normalized\neven with short lines"));
    // But paragraphs should be normalized
    let lines: Vec<&str> = fixed.lines().collect();
    assert!(lines[0].len() > 20, "First paragraph should be normalized");
}

#[test]
fn test_issue_76_use_case() {
    // This tests the exact use case from issue #76
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(999999), // Set absurdly high
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // Content with manual line breaks at 80 characters (typical markdown)
    let content = "We've decided to eliminate line-breaks in paragraphs. The obvious solution is\nto disable MD013, and call it good. However, that doesn't deal with the\nexisting content's line-breaks. My initial thought was to set line_length to\n999999 and enable_reflow, but realised after doing so, that it never triggers\nthe error, so nothing happens.";

    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    // Should flag for normalization even though no lines exceed limit
    let warnings = rule.check(&ctx).unwrap();
    assert!(!warnings.is_empty(), "Should flag paragraph for normalization");

    // Should combine into a single line
    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 1, "Should combine into single line with high limit");
    assert!(!fixed.contains("\n"), "Should remove all line breaks within paragraph");
}

#[test]
fn test_normalize_mode_single_line_unchanged() {
    // Single lines should not be flagged or changed
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This is a single line that should not be changed.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings.is_empty(), "Single line should not be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "Single line should remain unchanged");
}

#[test]
fn test_normalize_mode_with_inline_code() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(80),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content =
        "This paragraph has `inline code` and\nshould still be normalized properly\nwithout breaking the code.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(!warnings.is_empty(), "Multi-line paragraph should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("`inline code`"), "Inline code should be preserved");
    assert!(fixed.lines().count() < 3, "Lines should be combined");
}

#[test]
fn test_normalize_mode_with_emphasis() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This has **bold** and\n*italic* text that\nshould be preserved.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("**bold**"), "Bold should be preserved");
    assert!(fixed.contains("*italic*"), "Italic should be preserved");
    assert_eq!(fixed.lines().count(), 1, "Should be combined into one line");
}

#[test]
fn test_normalize_mode_respects_hard_breaks() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // Two spaces at end of line = hard break
    let content = "First line with hard break  \nSecond line after break\nThird line";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    // Hard break should be preserved
    assert!(fixed.contains("  \n"), "Hard break should be preserved");
    // But lines without hard break should be combined
    assert!(
        fixed.contains("Second line after break Third line"),
        "Lines without hard break should combine"
    );
}

#[test]
fn test_normalize_mode_with_links() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This has a [link](https://example.com) that\nshould be preserved when\nnormalizing the paragraph.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("[link](https://example.com)"),
        "Link should be preserved"
    );
    assert_eq!(fixed.lines().count(), 1, "Should be combined into one line");
}

#[test]
fn test_normalize_mode_empty_lines_between_paragraphs() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "First paragraph\nwith multiple lines.\n\n\nSecond paragraph\nwith multiple lines.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    // Multiple empty lines should be preserved
    assert!(fixed.contains("\n\n\n"), "Multiple empty lines should be preserved");
    // Each paragraph should be normalized
    let parts: Vec<&str> = fixed.split("\n\n\n").collect();
    assert_eq!(parts.len(), 2, "Should have two parts");
    assert_eq!(parts[0].lines().count(), 1, "First paragraph should be one line");
    assert_eq!(parts[1].lines().count(), 1, "Second paragraph should be one line");
}

#[test]
fn test_normalize_mode_mixed_list_types() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(80),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = r#"Paragraph before list
with multiple lines.

- Bullet item
* Another bullet
+ Plus bullet

1. Numbered item
2. Another number

Paragraph after list
with multiple lines."#;

    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Lists should be preserved
    assert!(fixed.contains("- Bullet item"), "Dash list should be preserved");
    assert!(fixed.contains("* Another bullet"), "Star list should be preserved");
    assert!(fixed.contains("+ Plus bullet"), "Plus list should be preserved");
    assert!(fixed.contains("1. Numbered item"), "Numbered list should be preserved");

    // But paragraphs should be normalized
    assert!(
        fixed.starts_with("Paragraph before list with multiple lines."),
        "First paragraph should be normalized"
    );
    assert!(
        fixed.ends_with("Paragraph after list with multiple lines."),
        "Last paragraph should be normalized"
    );
}

#[test]
fn test_normalize_mode_with_horizontal_rules() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "Paragraph before\nhorizontal rule.\n\n---\n\nParagraph after\nhorizontal rule.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("---"), "Horizontal rule should be preserved");
    assert!(
        fixed.contains("Paragraph before horizontal rule."),
        "First paragraph normalized"
    );
    assert!(
        fixed.contains("Paragraph after horizontal rule."),
        "Second paragraph normalized"
    );
}

#[test]
fn test_normalize_mode_with_indented_code() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "Paragraph before\nindented code.\n\n    This is indented code\n    Should not be normalized\n\nParagraph after\nindented code.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("    This is indented code\n    Should not be normalized"),
        "Indented code preserved"
    );
    assert!(
        fixed.contains("Paragraph before indented code."),
        "First paragraph normalized"
    );
    assert!(
        fixed.contains("Paragraph after indented code."),
        "Second paragraph normalized"
    );
}

#[test]
fn test_normalize_mode_disabled_without_reflow() {
    // Normalize mode should have no effect if reflow is disabled
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: false, // Disabled
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This is a line\nwith breaks that\nshould not be changed.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings.is_empty(), "Should not flag when reflow is disabled");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "Content should be unchanged when reflow is disabled");
}

#[test]
fn test_default_mode_with_long_lines() {
    // Default mode should fix paragraphs that contain lines exceeding limit
    // The paragraph-based approach treats consecutive lines as a unit
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(50),
        reflow: true,
        reflow_mode: ReflowMode::Default,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "Short line.\nThis is a very long line that definitely exceeds the fifty character limit and needs wrapping.\nAnother short line.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1, "Should flag the paragraph with long line");
    // The warning reports the line that violates in default mode
    assert_eq!(warnings[0].line, 2, "Should flag line 2 that exceeds limit");

    let fixed = rule.fix(&ctx).unwrap();
    // The paragraph gets reflowed as a unit
    assert!(
        fixed.contains("Short line. This is"),
        "Should combine and reflow the paragraph"
    );
    assert!(
        fixed.contains("wrapping. Another short"),
        "Should include all paragraph content"
    );
}

#[test]
fn test_normalize_vs_default_mode_same_content() {
    let content = "This is a paragraph\nwith multiple lines\nthat could be combined.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    // Test default mode
    let default_config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Default,
        ..Default::default()
    };
    let default_rule = MD013LineLength::from_config_struct(default_config);
    let default_warnings = default_rule.check(&ctx).unwrap();
    let default_fixed = default_rule.fix(&ctx).unwrap();

    // Test normalize mode
    let normalize_config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let normalize_rule = MD013LineLength::from_config_struct(normalize_config);
    let normalize_warnings = normalize_rule.check(&ctx).unwrap();
    let normalize_fixed = normalize_rule.fix(&ctx).unwrap();

    // Verify different behavior
    assert!(default_warnings.is_empty(), "Default mode should not flag short lines");
    assert!(
        !normalize_warnings.is_empty(),
        "Normalize mode should flag multi-line paragraphs"
    );

    assert_eq!(
        default_fixed, content,
        "Default mode should not change content without violations"
    );
    assert_ne!(
        normalize_fixed, content,
        "Normalize mode should change multi-line paragraphs"
    );
    assert_eq!(
        normalize_fixed.lines().count(),
        1,
        "Normalize should combine into single line"
    );
}

#[test]
fn test_normalize_mode_with_reference_definitions() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This paragraph uses\na reference [link][ref]\nacross multiple lines.\n\n[ref]: https://example.com";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("[link][ref]"), "Reference link should be preserved");
    assert!(
        fixed.contains("[ref]: https://example.com"),
        "Reference definition should be preserved"
    );
    assert!(
        fixed.starts_with("This paragraph uses a reference [link][ref] across multiple lines."),
        "Paragraph should be normalized"
    );
}

#[test]
fn test_normalize_mode_with_html_comments() {
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "Paragraph before\nHTML comment.\n\n<!-- This is a comment -->\n\nParagraph after\nHTML comment.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("<!-- This is a comment -->"),
        "HTML comment should be preserved"
    );
    assert!(
        fixed.contains("Paragraph before HTML comment."),
        "First paragraph normalized"
    );
    assert!(
        fixed.contains("Paragraph after HTML comment."),
        "Second paragraph normalized"
    );
}

#[test]
fn test_normalize_mode_line_starting_with_number() {
    // Regression test for the bug we fixed where "80 characters" was treated as a list
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This line mentions\n80 characters which\nshould not break the paragraph.";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed.lines().count(), 1, "Should be combined into single line");
    assert!(
        fixed.contains("80 characters"),
        "Number at start of line should be preserved"
    );
}

#[test]
fn test_default_mode_preserves_list_structure() {
    // In default mode, list continuation lines should be preserved
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(80),
        reflow: true,
        reflow_mode: ReflowMode::Default,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = r#"- This is a bullet point that has
  some text on multiple lines
  that should stay separate

1. Numbered list item with
   multiple lines that should
   also stay separate"#;

    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // In default mode, the structure should be preserved
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(
        lines[0], "- This is a bullet point that has",
        "First line should be unchanged"
    );
    assert_eq!(
        lines[1], "  some text on multiple lines",
        "Continuation should be preserved"
    );
    assert_eq!(
        lines[2], "  that should stay separate",
        "Second continuation should be preserved"
    );
}

#[test]
fn test_normalize_mode_multi_line_list_items_no_extra_spaces() {
    // Test that multi-line list items don't get extra spaces when normalized
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(80),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = r#"- This is a bullet point that has
  some text on multiple lines
  that should be combined

1. Numbered list item with
   multiple lines that need
   to be properly combined
2. Second item"#;

    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Check that there are no extra spaces in the combined list items
    assert!(
        !fixed.contains("lines  that"),
        "Should not have double spaces in bullet list"
    );
    assert!(
        !fixed.contains("need  to"),
        "Should not have double spaces in numbered list"
    );

    // Check that the list items are properly combined
    assert!(
        fixed.contains("- This is a bullet point that has some text on multiple lines that should be"),
        "Bullet list should be properly combined"
    );
    assert!(
        fixed.contains("1. Numbered list item with multiple lines that need to be properly combined"),
        "Numbered list should be properly combined"
    );
}

#[test]
fn test_normalize_mode_actual_numbered_list() {
    // Ensure actual numbered lists are still detected correctly
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(100),
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "Paragraph before list\nwith multiple lines.\n\n1. First item\n2. Second item\n10. Tenth item";
    let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("1. First item"), "Numbered list 1 should be preserved");
    assert!(fixed.contains("2. Second item"), "Numbered list 2 should be preserved");
    assert!(fixed.contains("10. Tenth item"), "Numbered list 10 should be preserved");
    assert!(
        fixed.starts_with("Paragraph before list with multiple lines."),
        "Paragraph should be normalized"
    );
}

#[test]
fn test_sentence_per_line_detection() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config.clone());

    // Test detection of multiple sentences
    let content = "This is sentence one. This is sentence two. And sentence three!";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

    // Debug: check if should_skip returns false
    assert!(!rule.should_skip(&ctx), "Should not skip for sentence-per-line mode");

    let result = rule.check(&ctx).unwrap();

    assert!(!result.is_empty(), "Should detect multiple sentences on one line");
    assert_eq!(
        result[0].message,
        "Line contains 3 sentences (one sentence per line required)"
    );
}

#[test]
fn test_sentence_per_line_fix() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "First sentence. Second sentence.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(!result.is_empty(), "Should detect violation");
    assert!(result[0].fix.is_some(), "Should provide a fix");

    let fix = result[0].fix.as_ref().unwrap();
    assert_eq!(fix.replacement.trim(), "First sentence.\nSecond sentence.");
}

#[test]
fn test_sentence_per_line_abbreviations() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // Should NOT trigger on abbreviations
    let content = "Mr. Smith met Dr. Jones at 3:00 PM.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Should not detect abbreviations as sentence boundaries"
    );
}

#[test]
fn test_sentence_per_line_with_markdown() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "# Heading\n\nSentence with **bold**. Another with [link](url).";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(!result.is_empty(), "Should detect multiple sentences with markdown");
    assert_eq!(result[0].line, 3); // Third line has the violation
}

#[test]
fn test_sentence_per_line_questions_exclamations() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "Is this a question? Yes it is! And a statement.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(!result.is_empty(), "Should detect sentences with ? and !");

    let fix = result[0].fix.as_ref().unwrap();
    let lines: Vec<&str> = fix.replacement.trim().lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "Is this a question?");
    assert_eq!(lines[1], "Yes it is!");
    assert_eq!(lines[2], "And a statement.");
}

#[test]
fn test_sentence_per_line_in_lists() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "- List item one. With two sentences.\n- Another item.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(!result.is_empty(), "Should detect sentences in list items");
    // The fix should preserve list formatting
    let fix = result[0].fix.as_ref().unwrap();
    assert!(fix.replacement.starts_with("- "), "Should preserve list marker");
}

#[test]
fn test_multi_paragraph_list_item_with_3_space_indent() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: crate::types::LineLength::from_const(999999),
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "1. First paragraph\n   continuation line.\n\n   Second paragraph\n   more content.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(!result.is_empty(), "Should detect multi-line paragraphs in list item");
    let fix = result[0].fix.as_ref().unwrap();

    // Should preserve paragraph structure, not collapse everything
    assert!(
        fix.replacement.contains("\n\n"),
        "Should preserve blank line between paragraphs"
    );
    assert!(fix.replacement.starts_with("1. "), "Should preserve list marker");
}

#[test]
fn test_multi_paragraph_list_item_with_4_space_indent() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: crate::types::LineLength::from_const(999999),
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // User's example from issue #76 - uses 4 spaces for continuation
    let content = "1. It **generated an application template**. There's a lot of files and\n    configurations required to build a native installer, above and\n    beyond the code of your actual application.\n\n    If you're not happy with the template provided by Briefcase, you can\n    provide your own.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        !result.is_empty(),
        "Should detect multi-line paragraphs in list item with 4-space indent"
    );
    let fix = result[0].fix.as_ref().unwrap();

    // Should preserve paragraph structure
    assert!(
        fix.replacement.contains("\n\n"),
        "Should preserve blank line between paragraphs"
    );
    assert!(fix.replacement.starts_with("1. "), "Should preserve list marker");

    // Both paragraphs should be reflowed but kept separate
    let lines: Vec<&str> = fix.replacement.split('\n').collect();
    let blank_line_idx = lines.iter().position(|l| l.trim().is_empty());
    assert!(blank_line_idx.is_some(), "Should have blank line separating paragraphs");
}

#[test]
fn test_multi_paragraph_bullet_list_item() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: crate::types::LineLength::from_const(999999),
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "- First paragraph\n  continuation.\n\n  Second paragraph\n  more text.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(!result.is_empty(), "Should detect multi-line paragraphs in bullet list");
    let fix = result[0].fix.as_ref().unwrap();

    assert!(
        fix.replacement.contains("\n\n"),
        "Should preserve blank line between paragraphs"
    );
    assert!(fix.replacement.starts_with("- "), "Should preserve bullet marker");
}

#[test]
fn test_code_block_in_list_item_five_spaces() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: crate::types::LineLength::from_const(80),
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // 5 spaces = code block indentation (marker_len=3 + 4 = 7, but we have 5 which is marker_len+2, still valid continuation but >= marker_len+4 would be code)
    // For "1. " marker (3 chars), 3+4=7 spaces would be code block
    let content = "1. First paragraph with some text that should be reflowed.\n\n       code_block()\n       more_code()\n\n   Second paragraph.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    if !result.is_empty() {
        let fix = result[0].fix.as_ref().unwrap();
        // Code block lines should NOT be reflowed - they should be preserved with original indentation
        assert!(
            fix.replacement.contains("       code_block()"),
            "Code block should be preserved: {}",
            fix.replacement
        );
        assert!(
            fix.replacement.contains("       more_code()"),
            "Code block should be preserved: {}",
            fix.replacement
        );
    }
}

#[test]
fn test_fenced_code_block_in_list_item() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: crate::types::LineLength::from_const(80),
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "1. First paragraph with some text.\n\n   ```rust\n   fn foo() {}\n   let x = 1;\n   ```\n\n   Second paragraph.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    if !result.is_empty() {
        let fix = result[0].fix.as_ref().unwrap();
        // Fenced code block should be preserved
        assert!(
            fix.replacement.contains("```rust"),
            "Should preserve fence: {}",
            fix.replacement
        );
        assert!(
            fix.replacement.contains("fn foo() {}"),
            "Should preserve code: {}",
            fix.replacement
        );
        assert!(
            fix.replacement.contains("```"),
            "Should preserve closing fence: {}",
            fix.replacement
        );
    }
}

#[test]
fn test_mixed_indentation_3_and_4_spaces() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: crate::types::LineLength::from_const(999999),
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // First continuation has 3 spaces, second has 4 - both should be accepted
    let content = "1. Text\n   3 space continuation\n    4 space continuation";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(!result.is_empty(), "Should detect multi-line list item");
    let fix = result[0].fix.as_ref().unwrap();
    // Should reflow all content together
    assert!(
        fix.replacement.contains("3 space continuation"),
        "Should include 3-space line: {}",
        fix.replacement
    );
    assert!(
        fix.replacement.contains("4 space continuation"),
        "Should include 4-space line: {}",
        fix.replacement
    );
}

#[test]
fn test_nested_list_in_multi_paragraph_item() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: crate::types::LineLength::from_const(999999),
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "1. First paragraph.\n\n   - Nested item\n     continuation\n\n   Second paragraph.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Nested lists at continuation indent should be INCLUDED in parent item
    assert!(!result.is_empty(), "Should detect and reflow parent item");
    if let Some(fix) = result[0].fix.as_ref() {
        // The nested list should be preserved in the output
        assert!(
            fix.replacement.contains("- Nested"),
            "Should preserve nested list: {}",
            fix.replacement
        );
        assert!(
            fix.replacement.contains("Second paragraph"),
            "Should include content after nested list: {}",
            fix.replacement
        );
    }
}

#[test]
fn test_nested_fence_markers_different_types() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: crate::types::LineLength::from_const(80),
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // Nested fences with different markers (backticks inside tildes)
    let content = "1. Example with nested fences:\n\n   ~~~markdown\n   This shows ```python\n   code = True\n   ```\n   ~~~\n\n   Text after.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    if !result.is_empty() {
        let fix = result[0].fix.as_ref().unwrap();
        // Inner fence should NOT close outer fence (different markers)
        assert!(
            fix.replacement.contains("```python"),
            "Should preserve inner fence: {}",
            fix.replacement
        );
        assert!(
            fix.replacement.contains("~~~"),
            "Should preserve outer fence: {}",
            fix.replacement
        );
        // All lines should remain as code
        assert!(
            fix.replacement.contains("code = True"),
            "Should preserve code: {}",
            fix.replacement
        );
    }
}

#[test]
fn test_nested_fence_markers_same_type() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: crate::types::LineLength::from_const(80),
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // Nested backticks - inner must have different length or won't work
    let content =
        "1. Example:\n\n   ````markdown\n   Shows ```python in code\n   ```\n   text here\n   ````\n\n   After.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    if !result.is_empty() {
        let fix = result[0].fix.as_ref().unwrap();
        // 4 backticks opened, 3 backticks shouldn't close it
        assert!(
            fix.replacement.contains("```python"),
            "Should preserve inner fence: {}",
            fix.replacement
        );
        assert!(
            fix.replacement.contains("````"),
            "Should preserve outer fence: {}",
            fix.replacement
        );
        assert!(
            fix.replacement.contains("text here"),
            "Should keep text as code: {}",
            fix.replacement
        );
    }
}

#[test]
fn test_sibling_list_item_breaks_parent() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: crate::types::LineLength::from_const(999999),
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // Sibling list item (at indent 0, before parent marker at 3)
    let content = "1. First item\n   continuation.\n2. Second item";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should process first item only, second item breaks it
    if !result.is_empty() {
        let fix = result[0].fix.as_ref().unwrap();
        // Should only include first item
        assert!(fix.replacement.starts_with("1. "), "Should start with first marker");
        assert!(fix.replacement.contains("continuation"), "Should include continuation");
        // Should NOT include second item (it's outside the byte range)
    }
}

#[test]
fn test_nested_list_at_continuation_indent_preserved() {
    let config = MD013Config {
        reflow: true,
        reflow_mode: ReflowMode::Normalize,
        line_length: crate::types::LineLength::from_const(999999),
        ..Default::default()
    };
    let rule = MD013LineLength::from_config_struct(config);

    // Nested list at exactly continuation indent (3 spaces for "1. ")
    let content = "1. Parent paragraph\n   with continuation.\n\n   - Nested at 3 spaces\n   - Another nested\n\n   After nested.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    if !result.is_empty() {
        let fix = result[0].fix.as_ref().unwrap();
        // All nested content should be preserved
        assert!(
            fix.replacement.contains("- Nested"),
            "Should include first nested item: {}",
            fix.replacement
        );
        assert!(
            fix.replacement.contains("- Another"),
            "Should include second nested item: {}",
            fix.replacement
        );
        assert!(
            fix.replacement.contains("After nested"),
            "Should include content after nested list: {}",
            fix.replacement
        );
    }
}

#[test]
fn test_paragraphs_false_skips_regular_text() {
    // Test that paragraphs=false skips checking regular text
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(50),
        paragraphs: false, // Don't check paragraphs
        code_blocks: true,
        tables: true,
        headings: true,
        strict: false,
        reflow: false,
        reflow_mode: ReflowMode::default(),
        length_mode: LengthMode::default(),
        abbreviations: Vec::new(),
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content =
        "This is a very long line of regular text that exceeds fifty characters and should not trigger a warning.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not report any warnings when paragraphs=false
    assert_eq!(
        result.len(),
        0,
        "Should not warn about long paragraph text when paragraphs=false"
    );
}

#[test]
fn test_paragraphs_false_still_checks_code_blocks() {
    // Test that paragraphs=false still checks code blocks
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(50),
        paragraphs: false, // Don't check paragraphs
        code_blocks: true, // But DO check code blocks
        tables: true,
        headings: true,
        strict: false,
        reflow: false,
        reflow_mode: ReflowMode::default(),
        length_mode: LengthMode::default(),
        abbreviations: Vec::new(),
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = r#"```
This is a very long line in a code block that exceeds fifty characters.
```"#;
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // SHOULD report warnings for code blocks even when paragraphs=false
    assert_eq!(
        result.len(),
        1,
        "Should warn about long lines in code blocks even when paragraphs=false"
    );
}

#[test]
fn test_paragraphs_false_still_checks_headings() {
    // Test that paragraphs=false still checks headings
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(50),
        paragraphs: false, // Don't check paragraphs
        code_blocks: true,
        tables: true,
        headings: true, // But DO check headings
        strict: false,
        reflow: false,
        reflow_mode: ReflowMode::default(),
        length_mode: LengthMode::default(),
        abbreviations: Vec::new(),
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "# This is a very long heading that exceeds fifty characters and should trigger a warning";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // SHOULD report warnings for headings even when paragraphs=false
    assert_eq!(
        result.len(),
        1,
        "Should warn about long headings even when paragraphs=false"
    );
}

#[test]
fn test_paragraphs_false_with_reflow_sentence_per_line() {
    // Test issue #121 use case: paragraphs=false with sentence-per-line reflow
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(80),
        paragraphs: false,
        code_blocks: true,
        tables: true,
        headings: false,
        strict: false,
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        length_mode: LengthMode::default(),
        abbreviations: Vec::new(),
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This is a very long sentence that exceeds eighty characters and contains important information that should not be flagged.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT warn when paragraphs=false
    assert_eq!(
        result.len(),
        0,
        "Should not warn about long sentences when paragraphs=false"
    );
}

#[test]
fn test_paragraphs_true_checks_regular_text() {
    // Test that paragraphs=true (default) checks regular text
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(50),
        paragraphs: true, // Default: DO check paragraphs
        code_blocks: true,
        tables: true,
        headings: true,
        strict: false,
        reflow: false,
        reflow_mode: ReflowMode::default(),
        length_mode: LengthMode::default(),
        abbreviations: Vec::new(),
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This is a very long line of regular text that exceeds fifty characters.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // SHOULD report warnings when paragraphs=true
    assert_eq!(
        result.len(),
        1,
        "Should warn about long paragraph text when paragraphs=true"
    );
}

#[test]
fn test_line_length_zero_disables_all_checks() {
    // Test that line_length = 0 disables all line length checks
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(0), // 0 = no limit
        paragraphs: true,
        code_blocks: true,
        tables: true,
        headings: true,
        strict: false,
        reflow: false,
        reflow_mode: ReflowMode::default(),
        length_mode: LengthMode::default(),
        abbreviations: Vec::new(),
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This is a very very very very very very very very very very very very very very very very very very very very very very very very long line that would normally trigger MD013.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT warn when line_length = 0
    assert_eq!(
        result.len(),
        0,
        "Should not warn about any line length when line_length = 0"
    );
}

#[test]
fn test_line_length_zero_with_headings() {
    // Test that line_length = 0 disables checks even for headings
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(0), // 0 = no limit
        paragraphs: true,
        code_blocks: true,
        tables: true,
        headings: true, // Even with headings enabled
        strict: false,
        reflow: false,
        reflow_mode: ReflowMode::default(),
        length_mode: LengthMode::default(),
        abbreviations: Vec::new(),
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "# This is a very very very very very very very very very very very very very very very very very very very very very long heading";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT warn when line_length = 0
    assert_eq!(
        result.len(),
        0,
        "Should not warn about heading line length when line_length = 0"
    );
}

#[test]
fn test_line_length_zero_with_code_blocks() {
    // Test that line_length = 0 disables checks even for code blocks
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(0), // 0 = no limit
        paragraphs: true,
        code_blocks: true, // Even with code_blocks enabled
        tables: true,
        headings: true,
        strict: false,
        reflow: false,
        reflow_mode: ReflowMode::default(),
        length_mode: LengthMode::default(),
        abbreviations: Vec::new(),
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "```\nThis is a very very very very very very very very very very very very very very very very very very very very very long code line\n```";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT warn when line_length = 0
    assert_eq!(
        result.len(),
        0,
        "Should not warn about code block line length when line_length = 0"
    );
}

#[test]
fn test_line_length_zero_with_sentence_per_line_reflow() {
    // Test issue #121 use case: line_length = 0 with sentence-per-line reflow
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(0), // 0 = no limit
        paragraphs: true,
        code_blocks: true,
        tables: true,
        headings: true,
        strict: false,
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        length_mode: LengthMode::default(),
        abbreviations: Vec::new(),
    };
    let rule = MD013LineLength::from_config_struct(config);

    let content = "This is sentence one. This is sentence two. This is sentence three.";
    let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have warnings with fixes (reflow enabled)
    assert_eq!(result.len(), 1, "Should provide reflow fix for multiple sentences");
    assert!(result[0].fix.is_some(), "Should have a fix available");
}

#[test]
fn test_line_length_zero_config_parsing() {
    // Test that line_length = 0 can be parsed from TOML config
    let toml_str = r#"
        line-length = 0
        paragraphs = true
        reflow = true
        reflow-mode = "sentence-per-line"
    "#;
    let config: MD013Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.line_length.get(), 0, "Should parse line_length = 0");
    assert!(config.line_length.is_unlimited(), "Should be unlimited");
    assert!(config.paragraphs);
    assert!(config.reflow);
    assert_eq!(config.reflow_mode, ReflowMode::SentencePerLine);
}

#[test]
fn test_template_directives_as_paragraph_boundaries() {
    // mdBook template tags should act as paragraph boundaries
    let content = r#"Some regular text here.

{{#tabs }}
{{#tab name="Tab 1" }}

More text in the tab.

{{#endtab }}
{{#tabs }}

Final paragraph.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let config = MD013Config {
        line_length: crate::types::LineLength::from_const(80),
        code_blocks: true,
        tables: true,
        headings: true,
        paragraphs: true,
        strict: false,
        reflow: true,
        reflow_mode: ReflowMode::SentencePerLine,
        length_mode: LengthMode::default(),
        abbreviations: Vec::new(),
    };
    let rule = MD013LineLength::from_config_struct(config);
    let result = rule.check(&ctx).unwrap();

    // Template directives should not be flagged as "multiple sentences"
    // because they act as paragraph boundaries
    for warning in &result {
        assert!(
            !warning.message.contains("multiple sentences"),
            "Template directives should not trigger 'multiple sentences' warning. Got: {}",
            warning.message
        );
    }
}

#[test]
fn test_template_directive_detection() {
    // Handlebars/mdBook/Mustache syntax
    assert!(is_template_directive_only("{{#tabs }}"));
    assert!(is_template_directive_only("{{#endtab }}"));
    assert!(is_template_directive_only("{{variable}}"));
    assert!(is_template_directive_only("  {{#tabs }}  "));

    // Jinja2/Liquid syntax
    assert!(is_template_directive_only("{% for item in items %}"));
    assert!(is_template_directive_only("{%endfor%}"));
    assert!(is_template_directive_only("  {% if condition %}  "));

    // Not template directives
    assert!(!is_template_directive_only("This is {{variable}} in text"));
    assert!(!is_template_directive_only("{{incomplete"));
    assert!(!is_template_directive_only("incomplete}}"));
    assert!(!is_template_directive_only(""));
    assert!(!is_template_directive_only("   "));
    assert!(!is_template_directive_only("Regular text"));
}

#[test]
fn test_mixed_content_with_templates() {
    // Lines with mixed content should NOT be treated as template directives
    let content = "This has {{variable}} in the middle.";
    assert!(!is_template_directive_only(content));

    let content2 = "Start {{#something}} end";
    assert!(!is_template_directive_only(content2));
}
