use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD009TrailingSpaces;

#[test]
fn test_md009_valid() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line without trailing spaces\nAnother line without trailing spaces\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md009_invalid() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line with trailing spaces  \nAnother line with trailing spaces   \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the second line should be flagged (3 spaces)
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "3 trailing spaces found");
}

#[test]
fn test_md009_empty_lines() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line without trailing spaces\n  \nAnother line without trailing spaces\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Empty line has trailing spaces");
}

#[test]
fn test_md009_code_blocks() {
    let rule = MD009TrailingSpaces::default();
    let content = "Normal line\n```\nCode with spaces    \nMore code  \n```\nNormal line  \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // Code block spaces are allowed
}

#[test]
fn test_md009_strict_mode() {
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "Line with two spaces  \nCode block```\nWith spaces  \n```\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Both lines should be flagged in strict mode
}

#[test]
fn test_md009_line_breaks() {
    let rule = MD009TrailingSpaces::default();
    let content = "This is a line  \nWith hard breaks  \nBut this has three   \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the line with 3 spaces should be flagged
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md009_custom_br_spaces() {
    let rule = MD009TrailingSpaces::new(3, false);
    let content = "Line with two spaces  \nLine with three   \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the line with 2 spaces should be flagged
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_md009_fix() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line with spaces   \nAnother line  \nNo spaces\n  \n```\nCode   \n```\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "Line with spaces  \nAnother line  \nNo spaces\n\n```\nCode   \n```\n"
    );
}

#[test]
fn test_md009_fix_strict() {
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "Line with spaces   \nAnother line  \nNo spaces\n  \n```\nCode   \n```\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Line with spaces\nAnother line\nNo spaces\n\n```\nCode\n```\n");
}

#[test]
fn test_md009_trailing_tabs() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line with trailing tab\t\nLine with tabs and spaces\t  \nMixed at end  \t\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Note: The rule only checks for trailing spaces, not tabs
    // So tabs are not detected, and "  \t" is detected as 2 trailing spaces
    assert_eq!(result.len(), 0); // The rule doesn't detect spaces followed by tabs as trailing spaces
}

#[test]
fn test_md009_multiple_trailing_spaces() {
    let rule = MD009TrailingSpaces::default();
    let content = "One space \nTwo spaces  \nThree spaces   \nFour spaces    \nFive spaces     \n";
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only lines with more than 2 spaces
    assert_eq!(result[0].line, 3);
    assert_eq!(result[0].message, "3 trailing spaces found");
}

#[test]
fn test_md009_blockquote_empty_lines() {
    let rule = MD009TrailingSpaces::default();
    let content = "> Quote\n>  \n> More quote\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // ">  " is not detected as an empty blockquote line needing fixing
}

#[test]
fn test_md009_blockquote_truly_empty() {
    let rule = MD009TrailingSpaces::default();
    let content = "> Quote\n>   \n> More quote\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // ">   " should be detected
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Empty blockquote line needs a space after >");
}

#[test]
fn test_md009_fix_preserves_line_breaks() {
    let rule = MD009TrailingSpaces::new(2, false);
    let content = "Line with one space \nLine with two  \nLine with three   \nLine with four    \n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "Line with one space  \nLine with two  \nLine with three  \nLine with four  \n"
    );
}

#[test]
fn test_md009_fix_empty_lines() {
    let rule = MD009TrailingSpaces::default();
    let content = "Text\n   \nMore text\n     \nEnd\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Text\n\nMore text\n\nEnd\n");
}

#[test]
fn test_md009_br_spaces_configuration() {
    let rule = MD009TrailingSpaces::new(4, false);
    let content = "Two spaces  \nThree spaces   \nFour spaces    \nFive spaces     \n";
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content_with_newline);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // All lines have valid 2-space line breaks

    // Test without final newline
    let content_without_newline = "Line one  \nLine two  \nLast line  ";
    let ctx = LintContext::new(content_without_newline);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Last line should be flagged
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md009_fix_last_line() {
    let rule = MD009TrailingSpaces::default();
    // Test without final newline
    let content = "Line one  \nLine two  \nLast line  ";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Line one  \nLine two  \nLast line");

    // Test with final newline
    let content = "Line one  \nLine two  \nLast line  \n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Line one  \nLine two  \nLast line  \n");
}

#[test]
fn test_md009_code_blocks_strict_mode() {
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "```python\ndef hello():  \n    print('world')   \n```\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // In strict mode, code block spaces should be flagged
}

#[test]
fn test_md009_fix_blockquote_empty_lines() {
    let rule = MD009TrailingSpaces::default();
    let content = "> Quote\n>   \n> More quote\n>\n> End\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "> Quote\n> \n> More quote\n>\n> End\n");
}

#[test]
fn test_md009_mixed_content() {
    let rule = MD009TrailingSpaces::default();
    let content = "# Heading  \n\nParagraph with line break  \nAnother line   \n\n- List item  \n- Another item    \n\n```\ncode  \n```\n\n> Quote  \n>   \n> More  \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Should flag: line 4 (3 spaces), line 7 (4 spaces), line 12 (empty blockquote)
    assert_eq!(result.len(), 3);
}

#[test]
fn test_md009_column_positions() {
    let rule = MD009TrailingSpaces::default();
    let content = "Short  \nA longer line with spaces   \n";
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "Empty line has trailing spaces");
}

#[test]
fn test_md009_heading_with_trailing_spaces() {
    let rule = MD009TrailingSpaces::default();
    let content = "# Heading  \n## Another heading   \n### Third  \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only line 2 with 3 spaces
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "3 trailing spaces found");
}

#[test]
fn test_md009_table_with_trailing_spaces() {
    let rule = MD009TrailingSpaces::default();
    let content = "| Column 1 | Column 2  |\n|----------|-----------|  \n| Data     | More data |   \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only line 3 with 3 spaces
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md009_fix_with_crlf() {
    let rule = MD009TrailingSpaces::default();
    // Note: The fix method uses .lines() which normalizes line endings to \n
    let content = "Line one  \r\nLine two   \r\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    // The fix normalizes to \n line endings
    assert_eq!(result, "Line one  \nLine two  \n");
}

#[test]
fn test_md009_indented_code_non_strict() {
    let rule = MD009TrailingSpaces::new(2, false);
    let content = "Text\n\n    indented code  \n    more code   \n\nText\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // In non-strict mode, indented code blocks should be ignored
    assert_eq!(result.len(), 0);
}

#[test]
fn test_md009_fix_complex_document() {
    let rule = MD009TrailingSpaces::default();
    let content =
        "# Title   \n\nParagraph  \n\n- List   \n  - Nested  \n\n```\ncode   \n```\n\n> Quote   \n>    \n\nEnd  ";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    // Headings should have all trailing spaces removed, regular text preserves 2 spaces for line breaks
    assert_eq!(
        result,
        "# Title\n\nParagraph  \n\n- List  \n  - Nested  \n\n```\ncode   \n```\n\n> Quote  \n> \n\nEnd"
    );
}

#[test]
fn test_md009_unicode_content() {
    let rule = MD009TrailingSpaces::default();
    let content = "Unicode text 你好  \nAnother line 世界   \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "3 trailing spaces found");
}

#[test]
fn test_md009_zero_br_spaces() {
    let rule = MD009TrailingSpaces::new(0, false);
    let content = "Line one \nLine two  \nLine three   \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3); // All lines should be flagged
}

#[test]
fn test_md009_nested_blockquotes() {
    let rule = MD009TrailingSpaces::default();
    let content = "> Level 1  \n> > Level 2   \n> > > Level 3  \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only line 2 with 3 spaces
    assert_eq!(result[0].line, 2);
}
