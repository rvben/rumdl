use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD009TrailingSpaces;

#[test]
fn test_md009_valid() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line without trailing spaces\nAnother line without trailing spaces\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md009_invalid() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line with trailing spaces  \nAnother line with trailing spaces   \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the second line should be flagged (3 spaces)
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "3 trailing spaces found");
}

#[test]
fn test_md009_empty_lines() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line without trailing spaces\n  \nAnother line without trailing spaces\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Empty line has trailing spaces");
}

#[test]
fn test_md009_code_blocks() {
    let rule = MD009TrailingSpaces::default();
    let content = "Normal line\n```\nCode with spaces    \nMore code  \n```\nNormal line  \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // Code block spaces are allowed
}

#[test]
fn test_md009_strict_mode() {
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "Line with two spaces  \nCode block```\nWith spaces  \n```\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Both lines should be flagged in strict mode
}

#[test]
fn test_md009_line_breaks() {
    let rule = MD009TrailingSpaces::default();
    let content = "This is a line  \nWith hard breaks  \nBut this has three   \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the line with 3 spaces should be flagged
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md009_custom_br_spaces() {
    let rule = MD009TrailingSpaces::new(3, false);
    let content = "Line with two spaces  \nLine with three   \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the line with 2 spaces should be flagged
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_md009_fix() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line with spaces   \nAnother line  \nNo spaces\n  \n```\nCode   \n```\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // Default br_spaces=2, so only lines with exactly 2 spaces are preserved
    // Line 1: 3 spaces -> removed
    // Line 2: 2 spaces -> preserved
    // Line 4: 2 spaces (empty line) -> removed
    assert_eq!(
        result,
        "Line with spaces\nAnother line  \nNo spaces\n\n```\nCode   \n```\n"
    );
}

#[test]
fn test_md009_fix_strict() {
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "Line with spaces   \nAnother line  \nNo spaces\n  \n```\nCode   \n```\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Line with spaces\nAnother line\nNo spaces\n\n```\nCode\n```\n");
}

#[test]
fn test_md009_trailing_tabs() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line with trailing tab\t\nLine with tabs and spaces\t  \nMixed at end  \t\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Note: The rule only checks for trailing spaces, not tabs
    // So tabs are not detected, and "  \t" is detected as 2 trailing spaces
    assert_eq!(result.len(), 0); // The rule doesn't detect spaces followed by tabs as trailing spaces
}

#[test]
fn test_md009_multiple_trailing_spaces() {
    let rule = MD009TrailingSpaces::default();
    let content = "One space \nTwo spaces  \nThree spaces   \nFour spaces    \nFive spaces     \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4); // Lines with 1, 3, 4, and 5 spaces should be flagged (2 spaces allowed for line breaks)
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].message, "Trailing space found");
    assert_eq!(result[1].line, 3);
    assert_eq!(result[1].message, "3 trailing spaces found");
    assert_eq!(result[2].line, 4);
    assert_eq!(result[2].message, "4 trailing spaces found");
    assert_eq!(result[3].line, 5);
    assert_eq!(result[3].message, "5 trailing spaces found");
}

#[test]
fn test_md009_lists_with_trailing_spaces() {
    let rule = MD009TrailingSpaces::default();
    let content = "- List item without spaces\n- List item with spaces  \n  - Nested with spaces   \n  - Nested without\n* Another list  \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only lines with more than 2 spaces
    assert_eq!(result[0].line, 3);
    assert_eq!(result[0].message, "3 trailing spaces found");
}

#[test]
fn test_md009_blockquote_empty_lines() {
    let rule = MD009TrailingSpaces::default();
    let content = "> Quote\n>  \n> More quote\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // ">  " is not detected as an empty blockquote line needing fixing
}

#[test]
fn test_md009_blockquote_truly_empty() {
    let rule = MD009TrailingSpaces::default();
    let content = "> Quote\n>   \n> More quote\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // ">   " should be detected
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "3 trailing spaces found");
}

#[test]
fn test_md009_fix_preserves_line_breaks() {
    let rule = MD009TrailingSpaces::new(2, false);
    let content = "Line with one space \nLine with two  \nLine with three   \nLine with four    \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // Only lines with exactly 2 spaces (br_spaces) are preserved
    // Lines with 1, 3, or 4 spaces have them removed
    assert_eq!(
        result,
        "Line with one space\nLine with two  \nLine with three\nLine with four\n"
    );
}

#[test]
fn test_md009_fix_empty_lines() {
    let rule = MD009TrailingSpaces::default();
    let content = "Text\n   \nMore text\n     \nEnd\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Text\n\nMore text\n\nEnd\n");
}

#[test]
fn test_md009_br_spaces_configuration() {
    let rule = MD009TrailingSpaces::new(4, false);
    let content = "Two spaces  \nThree spaces   \nFour spaces    \nFive spaces     \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3); // All except line with 4 spaces should be flagged
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 4);
}

#[test]
fn test_md009_last_line_handling() {
    let rule = MD009TrailingSpaces::default();
    // Test with final newline
    let content_with_newline = "Line one  \nLine two  \nLast line  \n";
    let ctx = LintContext::new(content_with_newline, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // All lines have valid 2-space line breaks

    // Test without final newline
    let content_without_newline = "Line one  \nLine two  \nLast line  ";
    let ctx = LintContext::new(
        content_without_newline,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Last line should be flagged
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md009_fix_last_line() {
    let rule = MD009TrailingSpaces::default();
    // Test without final newline
    let content = "Line one  \nLine two  \nLast line  ";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Line one  \nLine two  \nLast line");

    // Test with final newline
    let content = "Line one  \nLine two  \nLast line  \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Line one  \nLine two  \nLast line  \n");
}

#[test]
fn test_md009_code_blocks_strict_mode() {
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "```python\ndef hello():  \n    print('world')   \n```\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // In strict mode, code block spaces should be flagged
}

#[test]
fn test_md009_fix_blockquote_empty_lines() {
    let rule = MD009TrailingSpaces::default();
    let content = "> Quote\n>   \n> More quote\n>\n> End\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "> Quote\n>\n> More quote\n>\n> End\n"); // Trailing spaces removed
}

#[test]
fn test_md009_mixed_content() {
    let rule = MD009TrailingSpaces::default();
    let content = "# Heading  \n\nParagraph with line break  \nAnother line   \n\n- List item  \n- Another item    \n\n```\ncode  \n```\n\n> Quote  \n>   \n> More  \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should flag: line 4 (3 spaces), line 7 (4 spaces), line 12 (empty blockquote)
    assert_eq!(result.len(), 3);
}

#[test]
fn test_md009_column_positions() {
    let rule = MD009TrailingSpaces::default();
    let content = "Short  \nA longer line with spaces   \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].column, 26); // Position of first trailing space
    assert_eq!(result[0].end_column, 29); // Position after last trailing space (3 spaces)
}

#[test]
fn test_md009_only_spaces_line() {
    let rule = MD009TrailingSpaces::default();
    let content = "Text\n    \nMore text\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Empty line has trailing spaces");
}

#[test]
fn test_md009_heading_with_trailing_spaces() {
    let rule = MD009TrailingSpaces::default();
    let content = "# Heading  \n## Another heading   \n### Third  \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only line 2 with 3 spaces
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "3 trailing spaces found");
}

#[test]
fn test_md009_table_with_trailing_spaces() {
    let rule = MD009TrailingSpaces::default();
    let content = "| Column 1 | Column 2  |\n|----------|-----------|  \n| Data     | More data |   \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only line 3 with 3 spaces
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md009_fix_with_crlf() {
    let rule = MD009TrailingSpaces::default();
    // Note: The fix method uses .lines() which normalizes line endings to \n
    let content = "Line one  \r\nLine two   \r\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // The fix normalizes to \n line endings
    // Line 1: 2 spaces -> preserved (matches br_spaces=2)
    // Line 2: 3 spaces -> removed (doesn't match br_spaces=2)
    assert_eq!(result, "Line one  \nLine two\n");
}

#[test]
fn test_md009_indented_code_non_strict() {
    let rule = MD009TrailingSpaces::new(2, false);
    let content = "Text\n\n    indented code  \n    more code   \n\nText\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // In non-strict mode, indented code blocks should be ignored
    assert_eq!(result.len(), 0);
}

#[test]
fn test_md009_fix_complex_document() {
    let rule = MD009TrailingSpaces::default();
    let content =
        "# Title   \n\nParagraph  \n\n- List   \n  - Nested  \n\n```\ncode   \n```\n\n> Quote   \n>    \n\nEnd  ";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    // Headings should have all trailing spaces removed
    // Lines with exactly 2 spaces are preserved
    // Lines with 3 spaces are removed
    assert_eq!(
        result,
        "# Title\n\nParagraph  \n\n- List\n  - Nested  \n\n```\ncode   \n```\n\n> Quote\n>\n\nEnd" // Empty blockquote line trailing spaces removed
    );
}

#[test]
fn test_md009_unicode_content() {
    let rule = MD009TrailingSpaces::default();
    let content = "Unicode text 你好  \nAnother line 世界   \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "3 trailing spaces found");
}

#[test]
fn test_md009_nested_blockquotes() {
    let rule = MD009TrailingSpaces::default();
    let content = "> Level 1  \n> > Level 2   \n> > > Level 3  \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only line 2 with 3 spaces
    assert_eq!(result[0].line, 2);
}

// ============================================================================
// Issue #248: Multi-byte character fix range tests
// These tests verify that fix ranges are correctly calculated for lines
// containing multi-byte UTF-8 characters (like €, 你, 🎉)
// ============================================================================

#[test]
fn test_md009_euro_sign_fix_range() {
    // Issue #248: Euro sign (€) is 3 bytes in UTF-8
    // The fix range must use character positions, not byte positions
    let rule = MD009TrailingSpaces::new(2, true); // strict mode to catch all trailing spaces
    let content = "- 1€ expenses \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 1);

    // Verify the fix range produces correct output when applied
    let fix = warnings[0].fix.as_ref().expect("Should have fix");

    // "- 1€ expenses " = 14 characters, 16 bytes (€ = 3 bytes)
    // Trailing space is at character position 14 (byte position 16)
    // Fix range should be 15..16 (byte range for the trailing space)
    assert_eq!(fix.range.start, 15);
    assert_eq!(fix.range.end, 16);

    // Verify the actual fix works correctly
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "- 1€ expenses\n");
}

#[test]
fn test_md009_multiple_euro_signs_fix_range() {
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "€100 + €50 = €150   \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 1);
    assert_eq!(warnings[0].message, "3 trailing spaces found");

    let fix = warnings[0].fix.as_ref().expect("Should have fix");
    // "€100 + €50 = €150" = 17 characters, 23 bytes (3 € signs × 3 bytes each = 9 extra bytes)
    // 3 trailing spaces at byte positions 23, 24, 25
    assert_eq!(fix.range.start, 23);
    assert_eq!(fix.range.end, 26);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "€100 + €50 = €150\n");
}

#[test]
fn test_md009_cjk_characters_fix_range() {
    let rule = MD009TrailingSpaces::new(2, true);
    // Chinese: 你好世界 = 4 characters, 12 bytes (3 bytes each)
    let content = "Hello 你好世界   \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);

    let fix = warnings[0].fix.as_ref().expect("Should have fix");
    // "Hello 你好世界" = 10 characters, 18 bytes
    // 3 trailing spaces at byte positions 18, 19, 20
    assert_eq!(fix.range.start, 18);
    assert_eq!(fix.range.end, 21);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Hello 你好世界\n");
}

#[test]
fn test_md009_emoji_fix_range() {
    let rule = MD009TrailingSpaces::new(2, true);
    // Emoji 🎉 is 4 bytes in UTF-8
    let content = "Party 🎉🎉🎉   \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);

    let fix = warnings[0].fix.as_ref().expect("Should have fix");
    // "Party 🎉🎉🎉" = 9 characters, 18 bytes (3 emoji × 4 bytes = 12, + 6 ASCII)
    // 3 trailing spaces at byte positions 18, 19, 20
    assert_eq!(fix.range.start, 18);
    assert_eq!(fix.range.end, 21);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Party 🎉🎉🎉\n");
}

#[test]
fn test_md009_mixed_multibyte_fix_range() {
    let rule = MD009TrailingSpaces::new(2, true);
    // Mix of ASCII, Euro, CJK, and emoji
    let content = "Price: €100 你好 🎉  \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].message, "2 trailing spaces found");

    // Verify the fix works correctly
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Price: €100 你好 🎉\n");
}

#[test]
fn test_md009_issue_248_exact_repro() {
    // Exact reproduction of issue #248
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "- foobar \n- 1€ expenses \n\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    // Line 1: 1 trailing space -> flagged
    // Line 2: 1 trailing space after multi-byte char -> flagged
    assert_eq!(warnings.len(), 2);

    // Verify both lines have valid fix ranges
    for warning in &warnings {
        let fix = warning.fix.as_ref().expect("Should have fix");
        assert!(fix.range.start < fix.range.end, "Fix range should not be empty");
    }

    // Verify the fix works correctly
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "- foobar\n- 1€ expenses\n\n");
}

#[test]
fn test_md009_warning_based_fix_multibyte() {
    // Test that warning-based fixes (used by LSP) work correctly with multi-byte chars
    use rumdl_lib::utils::fix_utils::apply_warning_fixes;

    let rule = MD009TrailingSpaces::new(2, true);
    let content = "- 1€ expenses \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);

    // Apply warning-based fixes (simulates LSP formatting)
    let fixed = apply_warning_fixes(content, &warnings).expect("Should apply fixes");
    assert_eq!(fixed, "- 1€ expenses\n");
}

#[test]
fn test_md009_multiple_lines_multibyte_fix() {
    // Test multiple lines with multi-byte characters
    use rumdl_lib::utils::fix_utils::apply_warning_fixes;

    let rule = MD009TrailingSpaces::new(2, true);
    let content = "€ price  \n¥ yen   \n£ pound \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 3);

    // Apply warning-based fixes
    let fixed = apply_warning_fixes(content, &warnings).expect("Should apply fixes");
    assert_eq!(fixed, "€ price\n¥ yen\n£ pound\n");
}

#[test]
fn test_md009_column_positions_multibyte() {
    // Verify column positions are character-based, not byte-based
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "€€€   \n"; // 3 euro signs (9 bytes) + 3 spaces
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);

    // Column should be character position 4 (after 3 euro signs), not byte position 10
    assert_eq!(warnings[0].column, 4);
    // End column should be character position 7 (after 3 trailing spaces)
    assert_eq!(warnings[0].end_column, 7);
}

#[test]
fn test_md009_korean_fix_range() {
    // Korean characters (3 bytes each in UTF-8)
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "안녕하세요   \n"; // "Hello" in Korean + 3 trailing spaces
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);

    let fix = warnings[0].fix.as_ref().expect("Should have fix");
    // "안녕하세요" = 5 characters, 15 bytes
    // 3 trailing spaces at byte positions 15, 16, 17
    assert_eq!(fix.range.start, 15);
    assert_eq!(fix.range.end, 18);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "안녕하세요\n");
}

#[test]
fn test_md009_combining_characters_fix() {
    // Test with combining characters (e.g., é as e + combining acute)
    let rule = MD009TrailingSpaces::new(2, true);
    // Using precomposed é (U+00E9, 2 bytes) vs decomposed e + ́ (1 + 2 bytes)
    let content = "café   \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "café\n");
}

#[test]
fn test_md009_list_with_multibyte_marker_content() {
    // Test list items where the content after marker contains multi-byte chars
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "- 价格: €50   \n- 價格: ¥100  \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 2);

    // Verify fixes work
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "- 价格: €50\n- 價格: ¥100\n");
}
