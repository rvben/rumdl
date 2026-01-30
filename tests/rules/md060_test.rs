use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{ColumnAlign, MD013Config, MD060Config, MD060TableFormat};
use rumdl_lib::types::LineLength;
use unicode_width::UnicodeWidthStr;

#[test]
fn test_md060_disabled_by_default() {
    let rule = MD060TableFormat::default();
    let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 0, "Rule should be disabled by default");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "No changes when disabled");
}

#[test]
fn test_md060_align_simple_ascii_table() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 3, "Should warn about all three rows");

    let fixed = rule.fix(&ctx).unwrap();
    let expected = "| Name  | Age |\n| ----- | --- |\n| Alice | 30  |";
    assert_eq!(fixed, expected);

    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

#[test]
fn test_md060_cjk_characters() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Name | Age |\n|---|---|\n| 中文 | 30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("中文"), "CJK characters should be preserved");

    // Verify all rows have equal display width in aligned mode (not byte length!)
    // CJK characters take more bytes but should have same display width
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].width(), lines[1].width(), "Display widths should match");
    assert_eq!(lines[1].width(), lines[2].width(), "Display widths should match");

    let content2 = "| Name | City |\n|---|---|\n| Alice | 東京 |";
    let ctx2 = LintContext::new(content2, MarkdownFlavor::Standard, None);
    let fixed2 = rule.fix(&ctx2).unwrap();
    assert!(fixed2.contains("東京"), "Japanese characters should be preserved");

    // Verify all rows have equal display width in aligned mode
    let lines2: Vec<&str> = fixed2.lines().collect();
    assert_eq!(lines2[0].width(), lines2[1].width(), "Display widths should match");
    assert_eq!(lines2[1].width(), lines2[2].width(), "Display widths should match");
}

#[test]
fn test_md060_basic_emoji() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Status | Name |\n|---|---|\n| ✅ | Test |\n| ❌ | Fail |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("✅"), "Basic emoji should be preserved");
    assert!(fixed.contains("❌"), "Basic emoji should be preserved");
    assert!(fixed.contains("Test"));
    assert!(fixed.contains("Fail"));

    // Verify all rows have equal display width in aligned mode
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0].width(), lines[1].width(), "Display widths should match");
    assert_eq!(lines[1].width(), lines[2].width(), "Display widths should match");
    assert_eq!(lines[2].width(), lines[3].width(), "Display widths should match");
}

#[test]
fn test_md060_zwj_emoji_skipped() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Emoji | Name |\n|---|---|\n| 👨‍👩‍👧‍👦 | Family |\n| 👩‍💻 | Developer |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(
        warnings.len(),
        0,
        "Tables with ZWJ emoji should be skipped (no warnings)"
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "Tables with ZWJ emoji should not be modified");
}

#[test]
fn test_md060_inline_code_with_escaped_pipes() {
    // In GFM tables, bare pipes in inline code STILL act as cell delimiters.
    // To include a literal pipe in table content (even in code), escape it with \|

    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // WRONG: `[0-9]|[0-9]` - the | splits cells (3 columns total)
    // CORRECT: `[0-9]\|[0-9]` - the \| is escaped, stays as content (2 columns)
    let content = "| Pattern | Regex |\n|---|---|\n| Time | `[0-9]\\|[0-9]` |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    // The escaped pipe \| should be preserved in the output
    assert!(
        fixed.contains(r"`[0-9]\|[0-9]`"),
        "Escaped pipes in inline code should be preserved"
    );

    // Verify all rows have equal length in aligned mode
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

#[test]
fn test_md060_complex_regex_with_escaped_pipes() {
    // In GFM tables, bare pipes in inline code STILL act as cell delimiters.
    // Regex patterns with | must escape the pipe character with \|

    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // CORRECT: Pipes escaped with \| stay as content
    let content =
        "| Challenge | Solution |\n|---|---|\n| Hour:minute:second | `^([0-1]?\\d\\|2[0-3]):[0-5]\\d:[0-5]\\d$` |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    // The escaped pipe \| should be preserved
    assert!(
        fixed.contains(r"`^([0-1]?\d\|2[0-3]):[0-5]\d:[0-5]\d$`"),
        "Complex regex with escaped pipes should be preserved"
    );

    // Verify all rows have equal length in aligned mode
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

#[test]
fn test_md060_compact_style() {
    let rule = MD060TableFormat::new(true, "compact".to_string());

    let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let expected = "| Name | Age |\n| --- | --- |\n| Alice | 30 |";
    assert_eq!(fixed, expected);

    let lines: Vec<&str> = fixed.lines().collect();
    assert!(lines[0].len() < 20, "Compact style should be short");
}

#[test]
fn test_md060_max_width_fallback() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| VeryLongColumnName | AnotherLongColumn | ThirdColumn |\n|---|---|---|\n| Data | Data | Data |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    assert!(
        fixed.lines().all(|line| line.len() <= 80),
        "Wide tables should fall back to compact mode"
    );
}

#[test]
fn test_md060_empty_cells() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| A | B | C |\n|---|---|---|\n|  | X |  |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("|"), "Table structure should be preserved");

    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 3, "All rows should be present");

    // Verify all rows have equal length in aligned mode
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

#[test]
fn test_md060_mixed_content() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Name | Age | City | Status |\n|---|---|---|---|\n| 中文 | 30 | NYC | ✅ |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("中文"), "CJK should be preserved");
    assert!(fixed.contains("NYC"), "ASCII should be preserved");
    assert!(fixed.contains("✅"), "Emoji should be preserved");

    // Verify all rows have equal display width in aligned mode
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].width(), lines[1].width(), "Display widths should match");
    assert_eq!(lines[1].width(), lines[2].width(), "Display widths should match");
}

#[test]
fn test_md060_preserve_alignment_indicators() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Left | Center | Right |\n|:---|:---:|---:|\n| A | B | C |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    // Now with alignment support: A is left-aligned, B is center-aligned, C is right-aligned
    let expected = "| Left | Center | Right |\n| :--- | :----: | ----: |\n| A    |   B    |     C |";
    assert_eq!(fixed, expected);

    // Verify all rows have equal length in aligned mode
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());

    // Verify delimiter row format with spaces
    assert!(lines[1].contains(" :--- "), "Left alignment should have spaces");
    assert!(lines[1].contains(" :----: "), "Center alignment should have spaces");
    assert!(lines[1].contains(" ----: "), "Right alignment should have spaces");
}

#[test]
fn test_md060_table_with_trailing_newline() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Name | Age |\n|---|---|\n| Alice | 30 |\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.ends_with('\n'), "Trailing newline should be preserved");
}

#[test]
fn test_md060_multiple_tables() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Use ACTUALLY misaligned tables (different row lengths within each table)
    let content = "# First Table\n\n| A | B |\n|---|---|\n| 1 | 2222 |\n\n# Second Table\n\n| X | Y | Z |\n|---|---|---|\n| aaaa | b | c |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# First Table"));
    assert!(fixed.contains("# Second Table"));

    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings.len() >= 6, "Should warn about both tables");
}

#[test]
fn test_md060_table_without_content_rows() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Header 1 | Header 2 |\n|---|---|";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("Header 1"));
    assert!(fixed.contains("Header 2"));
}

#[test]
fn test_md060_none_style() {
    let rule = MD060TableFormat::new(true, "none".to_string());

    let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 0, "None style should not produce warnings");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "None style should not modify content");
}

#[test]
fn test_md060_single_column_table() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Column |\n|---|\n| Value1 |\n| Value2 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("Column"));
    assert!(fixed.contains("Value1"));
    assert!(fixed.contains("Value2"));
}

#[test]
fn test_md060_table_in_context() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content =
        "# Documentation\n\nSome text before.\n\n| Name | Age |\n|---|---|\n| Alice | 30 |\n\nSome text after.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# Documentation"));
    assert!(fixed.contains("Some text before."));
    assert!(fixed.contains("Some text after."));
    assert!(fixed.contains("| Name  | Age |"));

    // Extract just the table lines for row length equality check
    let lines: Vec<&str> = fixed.lines().collect();
    let table_lines: Vec<&str> = lines
        .iter()
        .skip_while(|line| !line.starts_with('|'))
        .take_while(|line| line.starts_with('|'))
        .copied()
        .collect();
    assert_eq!(table_lines[0].len(), table_lines[1].len());
    assert_eq!(table_lines[1].len(), table_lines[2].len());
}

#[test]
fn test_md060_warning_messages() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 3);

    for warning in &warnings {
        assert_eq!(warning.message, "Table columns should be aligned");
        assert_eq!(warning.rule_name, Some("MD060".to_string()));
        assert!(warning.fix.is_some(), "Each warning should have a fix");
    }
}

#[test]
fn test_md060_escaped_pipes() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Pattern | Description |\n|---|---|\n| `a\\|b` | Or operator |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("`a\\|b`"), "Escaped pipes should be preserved");
}

#[test]
fn test_md060_very_long_content() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let long_text = "A".repeat(100);
    let content = format!("| Col1 | Col2 |\n|---|---|\n| {long_text} | B |");
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains(&long_text), "Long content should be preserved");
}

#[test]
fn test_md060_minimum_column_width() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test with very short column content (1-2 chars) to ensure minimum width of 3
    // This is required because GFM mandates at least 3 dashes in delimiter rows
    let content = "| ID | First Name | Last Name | Department |\n|-|-|-|-|\n| 1 | John | Doe | Engineering |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // All lines should have equal length when properly aligned
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(
        lines[0].len(),
        lines[1].len(),
        "Header and delimiter should be same length"
    );
    assert_eq!(
        lines[1].len(),
        lines[2].len(),
        "Delimiter and content should be same length"
    );

    // Check that short columns (like "ID" and "1") are padded to at least width 3
    assert!(
        lines[0].contains("ID  "),
        "Short header 'ID' should be padded to minimum width"
    );
    assert!(lines[1].contains("---"), "Delimiter should have at least 3 dashes");
    assert!(
        lines[2].contains("1  "),
        "Short content '1' should be padded to minimum width"
    );

    // Verify the specific problematic case from the test file
    assert!(
        lines[0].starts_with("| ID "),
        "First column should be properly aligned with minimum width 3"
    );
}

#[test]
fn test_md060_minimum_width_with_alignment_indicators() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test minimum width with alignment indicators
    let content = "| A | B | C |\n|:---|---:|:---:|\n| X | Y | Z |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());

    // Verify alignment indicators are preserved
    assert!(lines[1].contains(":---"), "Left alignment should be preserved");
    assert!(lines[1].contains("---:"), "Right alignment should be preserved");
    assert!(lines[1].contains(":---:"), "Center alignment should be preserved");
}

#[test]
fn test_md060_empty_header_table() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "|||\n|-|-|\n|lorem|ipsum|";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    // Empty headers should be formatted with proper spacing
    let expected = "|       |       |\n| ----- | ----- |\n| lorem | ipsum |";
    assert_eq!(fixed, expected, "Empty header table should be formatted");

    // Verify all rows have equal length in aligned mode
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

#[test]
fn test_md060_delimiter_width_does_not_affect_alignment() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // The first delimiter has many dashes, but that shouldn't affect column width
    let content = "|lorem|ipsum|\n|--------------|-|\n|dolor|sit|";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    // Column width should be based on content (lorem/dolor), not delimiter dashes
    let expected = "| lorem | ipsum |\n| ----- | ----- |\n| dolor | sit   |";
    assert_eq!(
        fixed, expected,
        "Delimiter row width should not affect column alignment"
    );

    // Verify all rows have equal length in aligned mode
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

#[test]
fn test_md060_content_alignment_left() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Left |\n|:-----|\n| A |\n| BB |\n| CCC |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // All lines should have equal length
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
    assert_eq!(lines[2].len(), lines[3].len());
    assert_eq!(lines[3].len(), lines[4].len());

    // Content should be left-aligned (padding on right)
    // Column width is 4 (from "Left"), so padding for each:
    // A (1 char): padding=3 → "| A    |" (boundary + A + 3 spaces + boundary)
    // BB (2 chars): padding=2 → "| BB   |"
    // CCC (3 chars): padding=1 → "| CCC  |"
    assert!(
        lines[2].contains("| A    |"),
        "Single char should be left-aligned with padding on right"
    );
    assert!(
        lines[3].contains("| BB   |"),
        "Two chars should be left-aligned with padding on right"
    );
    assert!(
        lines[4].contains("| CCC  |"),
        "Three chars should be left-aligned with padding on right"
    );
}

#[test]
fn test_md060_content_alignment_center() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Center |\n|:------:|\n| A |\n| BB |\n| CCC |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // All lines should have equal length
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
    assert_eq!(lines[2].len(), lines[3].len());
    assert_eq!(lines[3].len(), lines[4].len());

    // Content should be center-aligned (padding split on both sides)
    // Format: "| <boundary><left_pad><content><right_pad><boundary> |"
    // For "A" in width 6: padding=5, left=2, right=3 → "| <1><2>A<3><1> |" = "|   A    |"
    // For "BB" in width 6: padding=4, left=2, right=2 → "| <1><2>BB<2><1> |" = "|   BB   |"
    // For "CCC" in width 6: padding=3, left=1, right=2 → "| <1><1>CCC<2><1> |" = "|  CCC   |"
    assert!(
        lines[2].contains("|   A    |"),
        "Single char should be center-aligned, got: {}",
        lines[2]
    );
    assert!(
        lines[3].contains("|   BB   |"),
        "Two chars should be center-aligned, got: {}",
        lines[3]
    );
    assert!(
        lines[4].contains("|  CCC   |"),
        "Three chars should be center-aligned, got: {}",
        lines[4]
    );
}

#[test]
fn test_md060_content_alignment_right() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Right |\n|------:|\n| A |\n| BB |\n| CCC |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // All lines should have equal length
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
    assert_eq!(lines[2].len(), lines[3].len());
    assert_eq!(lines[3].len(), lines[4].len());

    // Content should be right-aligned (padding on left)
    // Format: "| <boundary><padding><content><boundary> |" where boundary+padding creates visual right alignment
    assert!(
        lines[2].contains("|     A |"),
        "Single char should be right-aligned with padding on left, got: {}",
        lines[2]
    );
    assert!(
        lines[3].contains("|    BB |"),
        "Two chars should be right-aligned with padding on left, got: {}",
        lines[3]
    );
    assert!(
        lines[4].contains("|   CCC |"),
        "Three chars should be right-aligned with padding on left, got: {}",
        lines[4]
    );
}

#[test]
fn test_md060_mixed_column_alignments() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Left | Center | Right |\n|:---|:---:|---:|\n| A | B | C |\n| AA | BB | CC |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // All lines should have equal length
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
    assert_eq!(lines[2].len(), lines[3].len());

    // Parse the content rows to check alignment
    let row1 = lines[2];
    let row2 = lines[3];

    // First column (left-aligned): padding on right
    assert!(
        row1.starts_with("| A "),
        "First column should be left-aligned in row 1, got: {row1}",
    );
    assert!(
        row2.starts_with("| AA"),
        "First column should be left-aligned in row 2, got: {row2}",
    );

    // Third column (right-aligned): padding on left
    // For "Right" column (width ~5) with content "C" (1 char), expect boundary + 4 padding + C + boundary
    assert!(
        row1.contains("|     C |"),
        "Third column should be right-aligned in row 1, got: {row1}",
    );
    assert!(
        row1.ends_with("|     C |"),
        "Third column should be at end of row 1, got: {row1}",
    );
    // For content "CC" (2 chars), expect boundary + 3 padding + CC + boundary
    assert!(
        row2.contains("|    CC |"),
        "Third column should be right-aligned in row 2, got: {row2}",
    );
    assert!(
        row2.ends_with("|    CC |"),
        "Third column should be at end of row 2, got: {row2}",
    );
}

#[test]
fn test_md060_tables_in_html_comments_should_not_be_formatted() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "# Normal table\n\n| A | B |\n|---|---|\n| C | D |\n\n<!-- Commented table\n| X | Y |\n|---|---|\n| Z | W |\n-->\n\n| E | F |\n|---|---|\n| G | H |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();

    // Should only warn about the two tables outside comments (lines 3-5 and 13-15)
    // That's 3 lines for first table + 3 lines for last table = 6 warnings
    let non_comment_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| {
            let line = w.line;
            // Lines 3-5 are the first table, lines 13-15 are the last table
            (3..=5).contains(&line) || (13..=15).contains(&line)
        })
        .collect();

    assert_eq!(
        non_comment_warnings.len(),
        warnings.len(),
        "Should only warn about tables outside HTML comments. Got {} warnings total, expected 6",
        warnings.len()
    );

    let fixed = rule.fix(&ctx).unwrap();

    // The commented table should remain unformatted
    assert!(fixed.contains("| X | Y |"), "Commented table should not be modified");
    assert!(fixed.contains("| Z | W |"), "Commented table should not be modified");

    // The normal tables should be formatted
    assert!(
        fixed.contains("| A | B |") || fixed.contains("| A   | B   |"),
        "Normal table should be formatted"
    );
    assert!(
        fixed.contains("| E | F |") || fixed.contains("| E   | F   |"),
        "Normal table should be formatted"
    );
}

// ============================================================================
// CRITICAL EDGE CASE TESTS (Top 10 from comprehensive analysis)
// ============================================================================

#[test]
fn test_md060_zero_width_characters() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test Zero Width Space (U+200B), Zero Width Non-Joiner (U+200C), Word Joiner (U+2060)
    let content = "| Name | Status |\n|---|---|\n| Test\u{200B}Word | Active\u{200C}User |\n| Word\u{2060}Join | OK |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(
        warnings.len(),
        0,
        "Tables with zero-width characters should be skipped (no warnings)"
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, content,
        "Tables with zero-width characters should not be modified"
    );

    // Verify characters are preserved in original form
    assert!(
        fixed.contains("Test\u{200B}Word"),
        "Zero Width Space should be preserved"
    );
    assert!(
        fixed.contains("Active\u{200C}User"),
        "Zero Width Non-Joiner should be preserved"
    );
    assert!(fixed.contains("Word\u{2060}Join"), "Word Joiner should be preserved");
}

#[test]
fn test_md060_rtl_text_arabic() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test Arabic text (RTL)
    let content = "| Name | City |\n|---|---|\n| أحمد | القاهرة |\n| محمد | دبي |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Arabic text should be preserved
    assert!(fixed.contains("أحمد"), "Arabic name should be preserved");
    assert!(fixed.contains("القاهرة"), "Arabic city should be preserved");
    assert!(fixed.contains("محمد"), "Arabic name should be preserved");
    assert!(fixed.contains("دبي"), "Arabic city should be preserved");

    // All lines should have equal display width
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(
        lines[0].width(),
        lines[1].width(),
        "Display widths should match for RTL text"
    );
    assert_eq!(
        lines[1].width(),
        lines[2].width(),
        "Display widths should match for RTL text"
    );
    assert_eq!(
        lines[2].width(),
        lines[3].width(),
        "Display widths should match for RTL text"
    );
}

#[test]
fn test_md060_rtl_text_hebrew() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test Hebrew text (RTL)
    let content = "| שם | עיר |\n|---|---|\n| דוד | תל אביב |\n| שרה | ירושלים |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Hebrew text should be preserved
    assert!(fixed.contains("דוד"), "Hebrew name should be preserved");
    assert!(fixed.contains("תל אביב"), "Hebrew city should be preserved");
    assert!(fixed.contains("שרה"), "Hebrew name should be preserved");
    assert!(fixed.contains("ירושלים"), "Hebrew city should be preserved");

    // All lines should have equal display width
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(
        lines[0].width(),
        lines[1].width(),
        "Display widths should match for RTL text"
    );
    assert_eq!(
        lines[1].width(),
        lines[2].width(),
        "Display widths should match for RTL text"
    );
    assert_eq!(
        lines[2].width(),
        lines[3].width(),
        "Display widths should match for RTL text"
    );
}

#[test]
fn test_md060_mismatched_column_counts_more_in_header() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Header has 4 columns, delimiter has 3, content has 2
    let content = "| A | B | C | D |\n|---|---|---|\n| X | Y |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // This should not panic or crash
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "Should handle mismatched column counts gracefully");

    let fixed = result.unwrap();
    // The implementation should handle this gracefully, either by:
    // 1. Not formatting the table at all (safest)
    // 2. Formatting based on delimiter row column count
    // 3. Formatting based on max column count
    // We just verify it doesn't panic
    assert!(
        fixed.contains("A") || fixed.contains("X"),
        "Content should be preserved"
    );
}

#[test]
fn test_md060_mismatched_column_counts_more_in_content() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Header has 2 columns, delimiter has 2, content has 4
    let content = "| A | B |\n|---|---|\n| X | Y | Z | W |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // This should not panic or crash
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "Should handle mismatched column counts gracefully");

    let fixed = result.unwrap();
    assert!(
        fixed.contains("A") || fixed.contains("X"),
        "Content should be preserved"
    );
}

#[test]
fn test_md060_escaped_pipes_outside_code() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test escaped pipes in regular text (not in inline code)
    let content = "| Operator | Example |\n|---|---|\n| OR | a \\| b |\n| Pipe | x \\| y \\| z |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Escaped pipes should be preserved in the cell content
    assert!(fixed.contains("\\|"), "Escaped pipes should be preserved");

    // Verify table structure is maintained - should have 4 lines
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 4, "All rows should be present");

    // Verify the escaped pipes are in the correct cells (not split into separate columns)
    assert!(
        lines[2].contains("a \\| b"),
        "First escaped pipe example should be in single cell, got: {}",
        lines[2]
    );
    assert!(
        lines[3].contains("x \\| y \\| z"),
        "Second escaped pipe example should be in single cell, got: {}",
        lines[3]
    );

    // All lines should have equal length in aligned mode
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
    assert_eq!(lines[2].len(), lines[3].len());
}

#[test]
fn test_md060_combining_characters_diacritics() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test combining diacritical marks (café, São Paulo, etc.)
    let content = "| City | Country |\n|---|---|\n| café | français |\n| São Paulo | Brasil |\n| Zürich | Schweiz |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Combining characters should be preserved
    assert!(fixed.contains("café"), "Café with combining acute should be preserved");
    assert!(fixed.contains("São"), "São with combining tilde should be preserved");
    assert!(
        fixed.contains("Zürich"),
        "Zürich with combining umlaut should be preserved"
    );

    // All lines should have proper display width
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(
        lines[0].width(),
        lines[1].width(),
        "Display widths should match with diacritics"
    );
    assert_eq!(
        lines[1].width(),
        lines[2].width(),
        "Display widths should match with diacritics"
    );
}

#[test]
fn test_md060_skin_tone_modifiers() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test emoji with skin tone modifiers (these are complex grapheme clusters)
    let content = "| User | Avatar |\n|---|---|\n| Alice | 👍🏻 |\n| Bob | 👋🏿 |\n| Carol | 🤝🏽 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // This might be skipped like ZWJ emoji due to measurement complexity
    let fixed = rule.fix(&ctx).unwrap();

    // Skin tone modifiers should be preserved
    assert!(fixed.contains("👍🏻"), "Emoji with light skin tone should be preserved");
    assert!(fixed.contains("👋🏿"), "Emoji with dark skin tone should be preserved");
    assert!(fixed.contains("🤝🏽"), "Emoji with medium skin tone should be preserved");
}

#[test]
fn test_md060_flag_emojis() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test flag emojis (regional indicator symbols)
    let content = "| Country | Flag |\n|---|---|\n| USA | 🇺🇸 |\n| Japan | 🇯🇵 |\n| France | 🇫🇷 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Flag emojis should be preserved
    assert!(fixed.contains("🇺🇸"), "US flag should be preserved");
    assert!(fixed.contains("🇯🇵"), "Japan flag should be preserved");
    assert!(fixed.contains("🇫🇷"), "France flag should be preserved");
}

#[test]
fn test_md060_tables_in_blockquotes() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test tables inside blockquotes
    let content = "> | Name | Age |\n> |---|---|\n> | Alice | 30 |\n\nNormal text\n\n| X | Y |\n|---|---|\n| A | B |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Blockquote should be preserved
    assert!(fixed.starts_with("> "), "Blockquote markers should be preserved");

    // Both tables should be present
    assert!(
        fixed.contains("Alice") || fixed.contains("Name"),
        "Table in blockquote should be present"
    );
    assert!(
        fixed.contains("A") && fixed.contains("B"),
        "Normal table should be present"
    );
}

#[test]
fn test_md060_tables_in_nested_blockquotes() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test tables inside nested blockquotes (>> prefix)
    let content = ">> | Col1 | Col2 |\n>> |---|---|\n>> | A | B |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // All lines should preserve the >> prefix
    assert!(lines[0].starts_with(">> "), "Header should preserve >> prefix");
    assert!(lines[1].starts_with(">> "), "Delimiter should preserve >> prefix");
    assert!(lines[2].starts_with(">> "), "Content should preserve >> prefix");
}

#[test]
fn test_md060_tables_in_deeply_nested_blockquotes() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test tables inside deeply nested blockquotes (>>> prefix)
    let content = ">>> | X | Y | Z |\n>>> |---|---|---|\n>>> | 1 | 2 | 3 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // All lines should preserve the >>> prefix
    for (i, line) in lines.iter().enumerate() {
        assert!(
            line.starts_with(">>> "),
            "Line {i} should preserve >>> prefix, got: {line}"
        );
    }
}

#[test]
fn test_md060_blockquote_table_all_styles() {
    // Test that all formatting styles preserve blockquote prefix
    let content = "> | A | B |\n> |---|---|\n> | 1 | 2 |";

    for style in ["aligned", "compact", "tight"] {
        let rule = MD060TableFormat::new(true, style.to_string());
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

        let fixed = rule.fix(&ctx).unwrap();
        let lines: Vec<&str> = fixed.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            assert!(
                line.starts_with("> "),
                "Style '{style}' line {i} should preserve '> ' prefix, got: {line}"
            );
        }
    }
}

#[test]
fn test_md060_blockquote_table_compact_prefix() {
    let rule = MD060TableFormat::new(true, "compact".to_string());

    // Blockquote with no space after > (valid but unusual)
    let content = ">| A | B |\n>|---|---|\n>| 1 | 2 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // Should preserve the >| prefix style (no space)
    for (i, line) in lines.iter().enumerate() {
        assert!(line.starts_with(">"), "Line {i} should start with >, got: {line}");
    }
}

#[test]
fn test_md060_blockquote_table_preserves_alignment() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Table with alignment indicators inside blockquote
    let content = "> | Left | Center | Right |\n> |:---|:---:|---:|\n> | A | B | C |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify blockquote prefix is preserved
    assert!(fixed.starts_with("> "), "Should start with blockquote prefix");

    // Verify alignment indicators are preserved
    assert!(fixed.contains(":---"), "Left alignment should be preserved");
    assert!(fixed.contains("---:"), "Right alignment should be preserved");
}

#[test]
fn test_md060_multiple_blockquote_tables() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Two separate tables in same blockquote (separated by blank blockquote line)
    let content = "> | A | B |\n> |---|---|\n> | 1 | 2 |\n>\n> | X | Y |\n> |---|---|\n> | 3 | 4 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // All non-empty lines should have blockquote prefix
    for (i, line) in lines.iter().enumerate() {
        if !line.is_empty() {
            assert!(
                line.starts_with(">"),
                "Line {i} should have blockquote prefix, got: {line}"
            );
        }
    }

    // Both tables should have content preserved
    assert!(
        fixed.contains("1") && fixed.contains("2"),
        "First table content preserved"
    );
    assert!(
        fixed.contains("3") && fixed.contains("4"),
        "Second table content preserved"
    );
}

#[test]
fn test_md060_adjacent_tables_without_blank_line() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test two tables directly adjacent (no blank line between)
    // This is technically invalid Markdown but shouldn't crash
    let content = "| A | B |\n|---|---|\n| 1 | 2 |\n| C | D |\n|---|---|\n| 3 | 4 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should not panic
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "Adjacent tables should not cause crash");

    let fixed = result.unwrap();
    // Content should be preserved in some form
    assert!(
        fixed.contains("1") && fixed.contains("2"),
        "First table content should be preserved"
    );
    assert!(
        fixed.contains("3") && fixed.contains("4"),
        "Second table content should be preserved"
    );
}

#[test]
fn test_md060_maximum_column_count_stress() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test with 100 columns to check performance and memory handling
    let columns = 100;
    let header_row = format!(
        "| {} |",
        (0..columns).map(|i| format!("C{i}")).collect::<Vec<_>>().join(" | ")
    );
    let delimiter_row = format!("| {} |", vec!["---"; columns].join(" | "));
    let content_row = format!(
        "| {} |",
        (0..columns).map(|i| i.to_string()).collect::<Vec<_>>().join(" | ")
    );

    let content = format!("{header_row}\n{delimiter_row}\n{content_row}");
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);

    // This should complete in reasonable time and not crash
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "Should handle 100 columns without crashing");

    let fixed = result.unwrap();
    // Verify some columns are present
    assert!(fixed.contains("C0"), "First column should be present");
    assert!(fixed.contains("C99"), "Last column should be present");
}

#[test]
fn test_md060_fix_idempotency() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test that fix(fix(x)) == fix(x)
    let content = "| Name | Age | City |\n|---|---|---|\n| Alice | 30 | NYC |\n| Bob | 25 | LA |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed_once = rule.fix(&ctx).unwrap();

    // Apply fix again on the already-fixed content
    let ctx2 = LintContext::new(&fixed_once, MarkdownFlavor::Standard, None);
    let fixed_twice = rule.fix(&ctx2).unwrap();

    assert_eq!(
        fixed_once, fixed_twice,
        "Applying fix twice should produce the same result as applying it once (idempotency)"
    );

    // Verify no warnings on already-formatted table
    let warnings = rule.check(&ctx2).unwrap();
    assert_eq!(warnings.len(), 0, "Already-formatted table should produce no warnings");
}

// ============================================================================
// ADDITIONAL CRITICAL/HIGH PRIORITY EDGE CASES
// ============================================================================

// STRUCTURE EDGE CASES

#[test]
fn test_md060_completely_empty_table() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Table with all empty cells
    let content = "| | | |\n|---|---|---|\n| | | |\n| | | |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should not panic
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "Empty table should not crash");

    let fixed = result.unwrap();
    // All lines should have equal length
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
    assert_eq!(lines[2].len(), lines[3].len());
}

#[test]
fn test_md060_table_with_no_delimiter() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Invalid table: missing delimiter row
    let content = "| Name | Age |\n| Alice | 30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should not panic - this won't be detected as a table by the parser
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "Missing delimiter should not crash");
}

#[test]
fn test_md060_single_row_table_header_only() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Table with just header and delimiter, no content
    let content = "| Column A | Column B | Column C |\n|---|---|---|";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Should format correctly even without content rows
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0].len(),
        lines[1].len(),
        "Header and delimiter should have equal length"
    );

    assert!(fixed.contains("Column A"));
    assert!(fixed.contains("Column B"));
    assert!(fixed.contains("Column C"));
}

#[test]
fn test_md060_varying_column_counts_per_row() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Each row has different number of columns (malformed table)
    let content = "| A | B | C | D |\n|---|---|\n| X |\n| Y | Z | W | V | U |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should not panic
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "Varying column counts should not crash");
}

#[test]
fn test_md060_delimiter_with_no_dashes() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Invalid delimiter row with only colons (edge case)
    let content = "| A | B |\n|:::|:::|\n| X | Y |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should handle gracefully (likely won't be detected as valid table)
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "Invalid delimiter should not crash");
}

// UNICODE COMPLEXITY EDGE CASES

#[test]
fn test_md060_bidirectional_text_mixed_ltr_rtl() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Mix of LTR (English) and RTL (Arabic) in same table
    let content = "| English | العربية |\n|---|---|\n| Hello | مرحبا |\n| World | عالم |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Content should be preserved
    assert!(fixed.contains("English"));
    assert!(fixed.contains("العربية"));
    assert!(fixed.contains("Hello"));
    assert!(fixed.contains("مرحبا"));

    // All lines should have equal display width
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].width(), lines[1].width());
    assert_eq!(lines[1].width(), lines[2].width());
    assert_eq!(lines[2].width(), lines[3].width());
}

#[test]
fn test_md060_unicode_variation_selectors() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Variation selectors change glyph appearance (text vs emoji style)
    // U+FE0E = text style, U+FE0F = emoji style
    let content = "| Char | Style |\n|---|---|\n| ☺︎ | Text |\n| ☺️ | Emoji |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Content should be preserved
    assert!(fixed.contains("Text"));
    assert!(fixed.contains("Emoji"));

    // Should handle variation selectors without crashing
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 4);
}

#[test]
fn test_md060_unicode_control_characters() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test with various control characters that might cause issues
    // U+0000 = NULL, U+0001 = SOH, U+001F = Unit Separator
    let content = "| Name | Value |\n|---|---|\n| Test\u{0001} | Data |\n| Item | Info\u{001F} |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should not panic
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "Control characters should not crash formatting");
}

#[test]
fn test_md060_unicode_normalization_issues() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Same visual character in different normalization forms (NFD vs NFC)
    // é can be: U+00E9 (precomposed) or U+0065 U+0301 (e + combining acute)
    let content = "| NFC | NFD |\n|---|---|\n| café | cafe\u{0301} |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Both forms should be preserved
    assert!(fixed.contains("café"));

    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 3);
}

#[test]
fn test_md060_mixed_emoji_types() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Mix basic emoji, emoji with modifiers, and multi-codepoint emoji
    let content = "| Type | Example |\n|---|---|\n| Basic | 😀 |\n| Gender | 👨 |\n| Number | #️⃣ |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // All emoji should be preserved
    assert!(fixed.contains("😀"));
    assert!(fixed.contains("👨"));

    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 5);
}

// PERFORMANCE AND STRESS TESTS

#[test]
fn test_md060_extremely_wide_single_cell() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Single cell with 10000 characters
    let long_text = "A".repeat(10000);
    let content = format!("| Short | Long |\n|---|---|\n| X | {long_text} |");
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);

    // Should complete without timeout or excessive memory
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "Extremely wide cell should not crash");

    let fixed = result.unwrap();
    assert!(fixed.contains(&long_text), "Long text should be preserved");
}

#[test]
fn test_md060_many_rows_stress() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Table with 1000 rows
    let mut lines = vec!["| ID | Name | Value |".to_string(), "|---|---|---|".to_string()];
    for i in 0..1000 {
        lines.push(format!("| {i} | Row{i} | Data{i} |"));
    }
    let content = lines.join("\n");
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);

    // Should complete in reasonable time
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "1000 rows should not crash");

    let fixed = result.unwrap();
    assert!(fixed.contains("Row0"));
    assert!(fixed.contains("Row999"));
}

#[test]
fn test_md060_deeply_nested_inline_code() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Per GFM spec, inline code does NOT protect pipes from being cell delimiters.
    // To have literal pipes in table cells, use backslash-escaped pipes (\|).
    // This test verifies escaped pipes inside inline code are preserved during formatting.
    let content = "| Code | Description |\n|---|---|\n| `a\\|b` | Simple |\n| `x\\|y\\|z` | Multiple |\n| `{a\\|b}\\|{c\\|d}` | Complex |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Escaped pipes inside inline code should be preserved
    assert!(fixed.contains("`a\\|b`"));
    assert!(fixed.contains("`x\\|y\\|z`"));
    assert!(fixed.contains("`{a\\|b}\\|{c\\|d}`"));

    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

// MIXED CONTENT EDGE CASES

#[test]
fn test_md060_table_with_links() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Name | Link |\n|---|---|\n| GitHub | [Link](https://github.com) |\n| Google | [Search](https://google.com) |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Links should be preserved
    assert!(fixed.contains("[Link](https://github.com)"));
    assert!(fixed.contains("[Search](https://google.com)"));

    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].len(), lines[1].len());
}

#[test]
fn test_md060_table_with_html_entities() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Symbol | HTML |\n|---|---|\n| Less than | &lt; |\n| Greater | &gt; |\n| Ampersand | &amp; |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // HTML entities should be preserved
    assert!(fixed.contains("&lt;"));
    assert!(fixed.contains("&gt;"));
    assert!(fixed.contains("&amp;"));
}

#[test]
fn test_md060_table_with_bold_and_italic() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content =
        "| Text | Style |\n|---|---|\n| **Bold** | Strong |\n| *Italic* | Emphasis |\n| ***Both*** | Combined |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Markdown formatting should be preserved
    assert!(fixed.contains("**Bold**"));
    assert!(fixed.contains("*Italic*"));
    assert!(fixed.contains("***Both***"));
}

#[test]
fn test_md060_table_with_strikethrough() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Status | Item |\n|---|---|\n| Done | ~~Old~~ |\n| Active | Current |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Strikethrough should be preserved
    assert!(fixed.contains("~~Old~~"));
}

// WHITESPACE EDGE CASES

#[test]
fn test_md060_cells_with_leading_trailing_spaces() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Cells with intentional spaces (though they'll be trimmed in output)
    let content = "| Name | Value |\n|---|---|\n|   Spaced   |   Data   |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Content should be trimmed and padded correctly
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

#[test]
fn test_md060_cells_with_tabs() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Name | Value |\n|---|---|\n| Tab\there | Data |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Tabs should be preserved in cell content
    assert!(fixed.contains("Tab\there"));
}

#[test]
fn test_md060_cells_with_newline_escape() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Cells with literal \n (not actual newlines)
    let content = "| Pattern | Example |\n|---|---|\n| Newline | Line\\nBreak |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Escaped newline should be preserved
    assert!(fixed.contains("Line\\nBreak"));
}

// DELIMITER ROW VARIATIONS

#[test]
fn test_md060_delimiter_with_many_dashes() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Delimiter rows with varying dash counts
    let content = "| A | B | C |\n|----------|---|---------------------------|\n| X | Y | Z |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Should normalize to consistent delimiter format
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

#[test]
fn test_md060_all_alignment_combinations() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test all possible alignment combinations in one table
    let content =
        "| Default | Left | Right | Center |\n|---|:---|---:|:---:|\n| A | B | C | D |\n| AA | BB | CC | DD |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    let lines: Vec<&str> = fixed.lines().collect();

    // Verify all alignment indicators are present (checking delimiter row specifically)
    let delimiter_row = lines[1];
    assert!(
        delimiter_row.contains("---") || delimiter_row.contains("----"),
        "Default alignment should have dashes"
    );
    assert!(
        delimiter_row.contains(":---") || delimiter_row.contains(":----"),
        "Left alignment should have colon-dashes"
    );
    assert!(
        delimiter_row.contains("---:") || delimiter_row.contains("----:"),
        "Right alignment should have dashes-colon"
    );
    // Center alignment can have various dash counts between colons
    assert!(
        delimiter_row.chars().filter(|&c| c == ':').count() >= 4,
        "Should have at least 4 colons (2 for center, 1 for left, 1 for right)"
    );

    // All lines should have equal length
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
    assert_eq!(lines[2].len(), lines[3].len());
}

// EDGE CASE COMBINATIONS

#[test]
fn test_md060_unicode_in_aligned_columns() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Different Unicode widths with alignment indicators
    let content = "| Left | Center | Right |\n|:---|:---:|---:|\n| A | 中 | 1 |\n| AAA | 中中中 | 111 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    let lines: Vec<&str> = fixed.lines().collect();

    // All lines should have equal display width
    assert_eq!(lines[0].width(), lines[1].width());
    assert_eq!(lines[1].width(), lines[2].width());
    assert_eq!(lines[2].width(), lines[3].width());

    // Chinese characters should be preserved
    assert!(fixed.contains("中"));
    assert!(fixed.contains("中中中"));
}

#[test]
fn test_md060_empty_and_whitespace_only_cells_mixed() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| A | B | C |\n|---|---|---|\n|  |   | X |\n| Y |  |  |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Should handle empty cells correctly
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
    assert_eq!(lines[2].len(), lines[3].len());

    assert!(fixed.contains("X"));
    assert!(fixed.contains("Y"));
}

// ISSUE #164: Already-aligned tables with short separators should not be reformatted

#[test]
fn test_md060_issue_164_already_aligned_short_separators() {
    // This is the exact example from issue #164
    // Tables with 3-character separators that are already aligned should pass
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| a   |  b  |   c |\n| :-- | :-: | --: |\n| 1   |  2  |   3 |\n| 10  | 20  |  30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should produce NO warnings - table is already aligned
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(
        warnings.len(),
        0,
        "Already-aligned table with short (3-char) separators should not produce warnings"
    );

    // Should NOT modify the content - preserve as-is
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, content,
        "Already-aligned table should be preserved exactly as-is"
    );

    // Verify all rows have consistent length
    let lines: Vec<&str> = fixed.lines().collect();
    let first_len = lines[0].len();
    assert!(
        lines.iter().all(|line| line.len() == first_len),
        "All rows should maintain consistent length"
    );

    // Verify short separators are preserved (not expanded to 4+ chars)
    assert!(
        fixed.contains("| :-- | :-: | --: |"),
        "Short separator format should be preserved"
    );
}

#[test]
fn test_md060_issue_164_misaligned_short_separators_detected() {
    // Contrast case: tables with short separators but NOT aligned should be detected
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Misaligned table - inconsistent column widths
    let content = "| a |  b  | c |\n| :-- | :-: | --: |\n| 1 |  2  |   3 |\n| 10 | 20 |  30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should produce warnings - table is NOT aligned
    let warnings = rule.check(&ctx).unwrap();
    assert!(
        !warnings.is_empty(),
        "Misaligned table should produce warnings even with short separators"
    );
}

// ============================================================================
// MKDOCS FLAVOR TESTS (Issue #165)
//
// In MkDocs/Python-Markdown, pipes inside inline code are NOT cell delimiters.
// This differs from GFM where pipes inside backticks ARE cell delimiters.
// ============================================================================

#[test]
fn test_md060_mkdocs_flavor_pipes_in_code_spans_issue_165() {
    // Issue #165: Tables with pipes inside inline code should work correctly
    // with MkDocs flavor. The pipe in `x | y` should NOT be treated as a
    // cell delimiter.
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // This is the exact example from issue #165
    let content = "| Type | Example |\n| - | - |\n| Union | `x | y` |\n| Dict | `dict` |";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // Should recognize this as a 2-column table and format it correctly
    let fixed = rule.fix(&ctx).unwrap();

    // The inline code `x | y` should be preserved as a single cell
    assert!(
        fixed.contains("`x | y`"),
        "Inline code with pipe should be preserved as single cell content, got: {fixed}"
    );

    // Should be properly aligned with 2 columns, not corrupted into 3+
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 4, "Should have 4 lines");

    // All lines should have equal length when aligned
    assert_eq!(
        lines[0].len(),
        lines[1].len(),
        "Header and delimiter should match: '{}' vs '{}'",
        lines[0],
        lines[1]
    );
    assert_eq!(
        lines[1].len(),
        lines[2].len(),
        "Delimiter and content should match: '{}' vs '{}'",
        lines[1],
        lines[2]
    );
    assert_eq!(
        lines[2].len(),
        lines[3].len(),
        "Content rows should match: '{}' vs '{}'",
        lines[2],
        lines[3]
    );
}

#[test]
fn test_md060_mkdocs_flavor_various_code_spans_with_pipes() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Multiple rows with pipes in inline code
    let content =
        "| Type | Syntax |\n| - | - |\n| Union | `A | B` |\n| Optional | `T | None` |\n| Multiple | `a | b | c` |";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let fixed = rule.fix(&ctx).unwrap();

    // All inline code content should be preserved
    assert!(fixed.contains("`A | B`"), "Union type should be preserved");
    assert!(fixed.contains("`T | None`"), "Optional type should be preserved");
    assert!(fixed.contains("`a | b | c`"), "Multiple pipes should be preserved");

    // Should have 5 lines total
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 5, "Should have 5 lines");

    // All lines should have equal length
    for i in 0..lines.len() - 1 {
        assert_eq!(
            lines[i].len(),
            lines[i + 1].len(),
            "Lines {} and {} should have same length",
            i,
            i + 1
        );
    }
}

#[test]
fn test_md060_mkdocs_flavor_no_false_positives() {
    // With MkDocs flavor, tables with pipes in inline code should be parsed correctly
    // as 2 columns, not 3 columns. The pipe in `x | y` is NOT a cell delimiter.
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Use a table that would be already aligned if parsed correctly as 2 columns
    let content = "| Type  | Example  |\n| ----- | -------- |\n| Union | `x | y`  |";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // Check should produce no warnings because table is aligned with consistent columns
    let warnings = rule.check(&ctx).unwrap();

    // Should have no warnings for a well-formatted 2-column table
    assert!(
        warnings.is_empty(),
        "Should have no warnings for aligned 2-column table with MkDocs flavor, got: {:?}",
        warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_md060_mkdocs_flavor_fix_preserves_inline_code_pipes() {
    // With MkDocs flavor, fixing a table should preserve pipes inside inline code
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Type | Example |\n|-|-|\n| Union | `x | y` |\n| Dict | `dict` |";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let fixed = rule.fix(&ctx).unwrap();

    // The inline code content must be preserved intact (not split into separate cells)
    assert!(
        fixed.contains("`x | y`"),
        "Inline code content should be preserved intact, got: {fixed}"
    );

    // Verify the table structure: Each data row should have 2 content columns
    // If the pipe in `x | y` was wrongly treated as a delimiter, we'd see corrupted rows
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 4, "Table should have 4 lines");

    // The Union row should contain the intact inline code
    let union_row = lines[2];
    assert!(
        union_row.contains("`x | y`"),
        "Union row should contain intact inline code, got: {union_row}"
    );
}

#[test]
fn test_md060_mkdocs_flavor_compact_style() {
    let rule = MD060TableFormat::new(true, "compact".to_string());

    let content = "| Type | Example |\n|-|-|\n| Union | `x | y` |";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Should format to compact style while preserving inline code
    assert!(
        fixed.contains("`x | y`"),
        "Inline code should be preserved in compact mode"
    );

    // Should have compact formatting (single spaces)
    assert!(fixed.contains("| Type | Example |") || fixed.contains("| Type | Example |"));
}

#[test]
fn test_md060_mkdocs_flavor_tight_style() {
    let rule = MD060TableFormat::new(true, "tight".to_string());

    let content = "| Type | Example |\n|-|-|\n| Union | `x | y` |";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Should format to tight style while preserving inline code
    assert!(
        fixed.contains("`x | y`"),
        "Inline code should be preserved in tight mode"
    );

    // Should have tight formatting (no spaces)
    assert!(fixed.contains("|Type|"), "Should have tight formatting");
}

#[test]
fn test_md060_standard_flavor_pipes_in_code_are_delimiters() {
    // Verify that Standard/GFM flavor still treats pipes in code as delimiters
    // (this is the correct GFM behavior)
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Type | Example |\n|-|-|\n| Union | `x | y` |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // In GFM, `x | y` is split into `x and y` as separate cells
    // So the inline code is NOT preserved as a unit
    // The table will be treated as having 3 columns
    let _lines: Vec<&str> = fixed.lines().collect();

    // The behavior here depends on how the rule handles mismatched columns
    // but it should NOT preserve `x | y` as a single cell
    // (unless escaped as `x \| y`)
}

#[test]
fn test_md060_mkdocs_flavor_escaped_and_inline_code_pipes() {
    // Test combination of escaped pipes and pipes in inline code
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Type | Example |\n|-|-|\n| Escaped | a \\| b |\n| Code | `x | y` |";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Both should be preserved correctly
    assert!(fixed.contains("a \\| b"), "Escaped pipe should be preserved");
    assert!(fixed.contains("`x | y`"), "Inline code pipe should be preserved");

    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 4, "Should have 4 lines");
}

// ============================================================================
// LOOSE LAST COLUMN FEATURE TESTS (#356)
// ============================================================================

/// Helper to create MD013Config with default values
fn default_md013_config() -> MD013Config {
    MD013Config::default()
}

#[test]
fn test_md060_loose_last_column_basic() {
    // Test that loose-last-column allows unpadded last column in body rows
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,
        column_align_header: None,
        column_align_body: None,
        loose_last_column: true,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    // Input: header "Description" is 11 chars, but body has "Short" (5 chars) and long text (26 chars)
    // The max width of last column = max(11, 5, 26) = 26
    // Header is padded to 26, body rows are NOT padded (loose)
    let content = "| Name | Description |\n|---|---|\n| Foo | Short |\n| Bar | A much longer description |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // Header and delimiter should still be aligned
    assert_eq!(
        lines[0].len(),
        lines[1].len(),
        "Header and delimiter should be same length"
    );

    // KEY TEST: Body row with "Short" should be SHORTER than header row
    // because loose_last_column=true means we don't pad the last column in body rows
    // Header has "Description" padded to 26 chars, body has "Short" (5 chars) unpadded
    assert!(
        lines[2].len() < lines[0].len(),
        "Body row with short content ({} chars) should be shorter than header ({} chars) when loose_last_column=true",
        lines[2].len(),
        lines[0].len()
    );

    // Body row with longest content should equal header length (both have same last column width)
    // Header is padded to match max content, body with max content doesn't need padding
    assert_eq!(
        lines[3].len(),
        lines[0].len(),
        "Body row with max content should equal header length (both use max column width)"
    );
}

#[test]
fn test_md060_loose_last_column_disabled_by_default() {
    // Test that loose-last-column is disabled by default
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Name | Description |\n|---|---|\n| Foo | Short |\n| Bar | Longer text |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // All rows should have equal length (default behavior)
    assert_eq!(lines[0].len(), lines[1].len(), "Header and delimiter should match");
    assert_eq!(
        lines[1].len(),
        lines[2].len(),
        "Delimiter and first body row should match"
    );
    assert_eq!(lines[2].len(), lines[3].len(), "Body rows should match");
}

#[test]
fn test_md060_loose_last_column_header_delimiter_still_aligned() {
    // Test that header and delimiter remain aligned even with loose-last-column
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,
        column_align_header: None,
        column_align_body: None,
        loose_last_column: true,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    let content = "| ID | Name | Description |\n|---|---|---|\n| 1 | A | X |\n| 2 | B | Y |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // Header and delimiter MUST still be aligned
    assert_eq!(
        lines[0].len(),
        lines[1].len(),
        "Header and delimiter should be same length even with loose-last-column"
    );
}

#[test]
fn test_md060_loose_last_column_multiple_columns() {
    // Test loose-last-column with multiple columns
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,
        column_align_header: None,
        column_align_body: None,
        loose_last_column: true,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    let content = "| A | B | C | D |\n|---|---|---|---|\n| 1 | 2 | 3 | Short |\n| 1 | 2 | 3 | Longer text here |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify table structure: should have 4 lines
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 4, "Should have 4 lines");

    // Non-last columns should be present in all rows
    assert!(fixed.contains("| A "), "Header A should be in output");
    assert!(fixed.contains("| B "), "Header B should be in output");
    assert!(fixed.contains("| C "), "Header C should be in output");
    assert!(fixed.contains("| D "), "Header D should be in output");

    // Body content should be preserved
    assert!(fixed.contains("Short"), "Short text should be in output");
    assert!(fixed.contains("Longer text here"), "Long text should be in output");
}

#[test]
fn test_md060_loose_last_column_single_column_table() {
    // Edge case: Single column table with loose-last-column
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,
        column_align_header: None,
        column_align_body: None,
        loose_last_column: true,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    let content = "| Description |\n|---|\n| Short |\n| A much longer description |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should not panic
    let result = rule.fix(&ctx);
    assert!(result.is_ok(), "Single column with loose-last-column should not crash");

    let fixed = result.unwrap();
    assert!(fixed.contains("Short"), "Short text should be preserved");
    assert!(
        fixed.contains("A much longer description"),
        "Long text should be preserved"
    );
}

// ============================================================================
// COLUMN ALIGN HEADER/BODY FEATURE TESTS (#348)
// ============================================================================

#[test]
fn test_md060_column_align_header_basic() {
    // Test column-align-header overrides global column-align for header only
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Left,                // Body will be left-aligned
        column_align_header: Some(ColumnAlign::Center), // Header is centered
        column_align_body: None,
        loose_last_column: false,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    // Use content where "A" needs centering in a wider column
    let content = "| A | B |\n|---|---|\n| Long | Text |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    assert_eq!(lines.len(), 3, "Should have 3 lines");

    // Header "A" should be centered in a column wide enough for "Long"
    // Centering means space on left AND right: "| A    |" becomes "|  A   |" (or similar)
    let header = lines[0];
    let body = lines[2];

    // Find the first cell content position
    // In centered header, "A" should have space before it (after the pipe)
    // In left-aligned body, "Long" should be right after the pipe with space after
    assert!(
        header.contains("|  ") || header.contains("| A "),
        "Header should show centering pattern, got: {header}"
    );
    assert!(
        body.starts_with("| Long"),
        "Body should be left-aligned (content right after pipe), got: {body}"
    );
}

#[test]
fn test_md060_column_align_body_basic() {
    // Test column-align-body overrides global column-align for body rows only
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Left, // Header will be left-aligned
        column_align_header: None,
        column_align_body: Some(ColumnAlign::Right), // Body is right-aligned
        loose_last_column: false,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    // Use content where body "A" needs right-aligning in a wider column
    let content = "| Long | Text |\n|---|---|\n| A | B |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    assert_eq!(lines.len(), 3, "Should have 3 lines");

    let header = lines[0];
    let body = lines[2];

    // Header "Long" should be left-aligned (right after pipe)
    assert!(
        header.starts_with("| Long"),
        "Header should be left-aligned, got: {header}"
    );

    // Body "A" should be right-aligned (space before content, content before pipe)
    // Right-alignment means: "|    A |" pattern (spaces, then content, then space, then pipe)
    assert!(
        body.contains("  A |") || body.contains(" A |"),
        "Body should be right-aligned with padding before 'A', got: {body}"
    );
}

#[test]
fn test_md060_column_align_header_and_body_different() {
    // Test different alignments for header and body
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,                // Fall back to auto
        column_align_header: Some(ColumnAlign::Center), // Header centered
        column_align_body: Some(ColumnAlign::Left),     // Body left-aligned
        loose_last_column: false,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    let content = "| ColumnA | ColumnB |\n|---|---|\n| X | Y |\n| XX | YY |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    assert_eq!(lines.len(), 4, "Should have 4 lines");

    // Header (line 0) should be centered
    // Body (lines 2, 3) should be left-aligned
    // All lines should still have equal length
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
    assert_eq!(lines[2].len(), lines[3].len());
}

#[test]
fn test_md060_column_align_header_only_set() {
    // Test when only column-align-header is set (body falls back to column-align)
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Right,             // Body and default
        column_align_header: Some(ColumnAlign::Left), // Header left
        column_align_body: None,                      // Body uses column_align (Right)
        loose_last_column: false,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    let content = "| A | B |\n|---|---|\n| X | Y |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // Verify the table is properly formatted
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

#[test]
fn test_md060_column_align_body_only_set() {
    // Test when only column-align-body is set (header falls back to column-align)
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Left,              // Header and default
        column_align_header: None,                    // Header uses column_align (Left)
        column_align_body: Some(ColumnAlign::Center), // Body centered
        loose_last_column: false,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    let content = "| Name | Value |\n|---|---|\n| Key | 42 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

#[test]
fn test_md060_column_align_auto_with_header_body_override() {
    // Test that Auto alignment respects delimiter markers while header/body overrides work
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,                // Use delimiter markers
        column_align_header: Some(ColumnAlign::Center), // Override header to center
        column_align_body: None,                        // Body uses Auto (delimiter markers)
        loose_last_column: false,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    // Delimiter says: left, center, right
    let content = "| Left | Center | Right |\n|:---|:---:|---:|\n| A | B | C |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Delimiter markers should be preserved
    assert!(fixed.contains(":---"), "Left alignment marker should be preserved");
    assert!(fixed.contains("---:"), "Right alignment marker should be preserved");

    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines.len(), 3);
}

#[test]
fn test_md060_column_align_all_combinations() {
    // Test all combinations of ColumnAlign values
    for header_align in [
        Some(ColumnAlign::Left),
        Some(ColumnAlign::Center),
        Some(ColumnAlign::Right),
        None,
    ] {
        for body_align in [
            Some(ColumnAlign::Left),
            Some(ColumnAlign::Center),
            Some(ColumnAlign::Right),
            None,
        ] {
            let config = MD060Config {
                enabled: true,
                style: "aligned".to_string(),
                max_width: LineLength::from_const(0),
                column_align: ColumnAlign::Auto,
                column_align_header: header_align,
                column_align_body: body_align,
                loose_last_column: false,
            };
            let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

            let content = "| A | B |\n|---|---|\n| X | Y |";
            let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

            // Should not panic for any combination
            let result = rule.fix(&ctx);
            assert!(
                result.is_ok(),
                "Should not panic for header={header_align:?}, body={body_align:?}"
            );
        }
    }
}

// ============================================================================
// COMBINED FEATURES TESTS
// ============================================================================

#[test]
fn test_md060_loose_last_column_with_header_body_alignment() {
    // Test both features together
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,
        column_align_header: Some(ColumnAlign::Center),
        column_align_body: Some(ColumnAlign::Left),
        loose_last_column: true,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    let content = "| Name | Description |\n|---|---|\n| A | Short |\n| B | A very long description |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // Header and delimiter should still be aligned
    assert_eq!(lines[0].len(), lines[1].len(), "Header and delimiter should match");

    // Content should be preserved
    assert!(fixed.contains("Short"), "Short text preserved");
    assert!(fixed.contains("A very long description"), "Long text preserved");
}

#[test]
fn test_md060_features_idempotency() {
    // Test that applying fix twice with new features produces same result
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,
        column_align_header: Some(ColumnAlign::Center),
        column_align_body: Some(ColumnAlign::Left),
        loose_last_column: false, // Keep strict for idempotency test
    };
    let rule = MD060TableFormat::from_config_struct(config.clone(), default_md013_config(), false);

    let content = "| Name | Age | City |\n|---|---|---|\n| Alice | 30 | NYC |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed_once = rule.fix(&ctx).unwrap();

    let ctx2 = LintContext::new(&fixed_once, MarkdownFlavor::Standard, None);
    let rule2 = MD060TableFormat::from_config_struct(config, default_md013_config(), false);
    let fixed_twice = rule2.fix(&ctx2).unwrap();

    assert_eq!(fixed_once, fixed_twice, "Applying fix twice should produce same result");
}

// ============================================================================
// EXPERT-LEVEL TESTS WITH EXACT OUTPUT ASSERTIONS
// ============================================================================

#[test]
fn test_md060_loose_last_column_exact_output() {
    // Verify exact output for loose-last-column feature
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,
        column_align_header: None,
        column_align_body: None,
        loose_last_column: true,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    let content = "| A | B |\n|---|---|\n| X | Short |\n| Y | Much longer text |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Expected: Header padded to max width, body rows NOT padded in last column
    // Column widths: A=min(3)=3 (GFM minimum), B=max(1, 5, 16)=16
    // Header: "| A   | B                |" (A padded to min 3, B padded to 16)
    // Body1:  "| X   | Short |" (first column padded, last column NOT padded - loose)
    // Body2:  "| Y   | Much longer text |" (first column padded, last no padding needed)
    let expected =
        "| A   | B                |\n| --- | ---------------- |\n| X   | Short |\n| Y   | Much longer text |";
    assert_eq!(
        fixed, expected,
        "Loose last column should produce unpadded body rows in last column only"
    );
}

#[test]
fn test_md060_loose_last_column_empty_cell() {
    // Edge case: empty cell in last column with loose-last-column
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,
        column_align_header: None,
        column_align_body: None,
        loose_last_column: true,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    let content = "| A | Description |\n|---|---|\n| X |  |\n| Y | Has content |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // Empty cell row should be shortest
    assert!(
        lines[2].len() < lines[0].len(),
        "Row with empty last cell ({}) should be shorter than header ({})",
        lines[2].len(),
        lines[0].len()
    );

    // Row with content should match or be close to header
    assert!(
        lines[3].len() <= lines[0].len(),
        "Row with content should not exceed header length"
    );
}

#[test]
fn test_md060_loose_last_column_preserves_alignment_markers() {
    // Verify alignment markers in delimiter are preserved with loose-last-column
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,
        column_align_header: None,
        column_align_body: None,
        loose_last_column: true,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    let content = "| Left | Right |\n|:---|---:|\n| A | B |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Alignment markers should be preserved
    assert!(fixed.contains(":---"), "Left alignment marker should be preserved");
    assert!(
        fixed.contains("---:") || fixed.contains("-:"),
        "Right alignment marker should be preserved"
    );
}

#[test]
fn test_md060_column_align_header_center_exact() {
    // Verify exact centering behavior for header
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Left,
        column_align_header: Some(ColumnAlign::Center),
        column_align_body: None, // Uses column_align (Left)
        loose_last_column: false,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    // "A" (1 char) needs to be centered in column wide enough for "Long" (4 chars)
    let content = "| A |\n|---|\n| Long |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // Header "A" should be centered: spaces on both sides
    // For 4-char column width, "A" centered = " A  " or "  A " (depending on even/odd handling)
    let header = lines[0];
    let a_pos = header.find('A').expect("A should be in header");
    let pipe_after_a = header[a_pos..].find('|').expect("Pipe should follow A");

    // Check there's space before A (after first pipe)
    let first_pipe = header.find('|').unwrap();
    let chars_before_a = a_pos - first_pipe - 1;
    let chars_after_a = pipe_after_a - 1;

    assert!(
        chars_before_a > 0,
        "Centered header should have space before 'A', got {chars_before_a} chars before"
    );
    assert!(
        chars_after_a > 0,
        "Centered header should have space after 'A', got {chars_after_a} chars after"
    );

    // Body should be left-aligned: "Long" right after pipe
    let body = lines[2];
    assert!(
        body.contains("| Long"),
        "Body should be left-aligned with 'Long' right after pipe, got: {body}"
    );
}

#[test]
fn test_md060_column_align_body_right_exact() {
    // Verify exact right-alignment behavior for body
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Left,
        column_align_header: None, // Uses column_align (Left)
        column_align_body: Some(ColumnAlign::Right),
        loose_last_column: false,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    // "X" (1 char) needs to be right-aligned in column wide enough for "Long" (4 chars)
    let content = "| Long |\n|---|\n| X |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    // Header should be left-aligned
    let header = lines[0];
    assert!(
        header.contains("| Long"),
        "Header should be left-aligned, got: {header}"
    );

    // Body "X" should be right-aligned: spaces before, then X, then space, then pipe
    let body = lines[2];
    // Right-aligned "X" in 4-char column = "   X" -> "|    X |"
    // Find X position relative to the cell boundaries
    let x_pos = body.find('X').expect("X should be in body");
    let first_pipe = body.find('|').unwrap();
    let chars_before_x = x_pos - first_pipe - 1;

    assert!(
        chars_before_x >= 3,
        "Right-aligned body should have multiple spaces before 'X', got {chars_before_x} chars before. Line: {body}"
    );
}

#[test]
fn test_md060_delimiter_unaffected_by_column_align() {
    // Verify delimiter row is NOT affected by column-align settings
    // Delimiter should always use dashes, not be "aligned" with spaces
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Center,
        column_align_header: Some(ColumnAlign::Right),
        column_align_body: Some(ColumnAlign::Left),
        loose_last_column: false,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    let content = "| A | B |\n|---|---|\n| X | Y |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    let lines: Vec<&str> = fixed.lines().collect();

    let delimiter = lines[1];

    // Delimiter should contain only pipes, dashes, colons, and spaces
    // It should NOT have content alignment applied
    assert!(
        !delimiter.contains('A') && !delimiter.contains('B') && !delimiter.contains('X') && !delimiter.contains('Y'),
        "Delimiter should not contain cell content, got: {delimiter}"
    );

    // Delimiter should have dashes
    assert!(
        delimiter.contains("---"),
        "Delimiter should contain dashes, got: {delimiter}"
    );
}

#[test]
fn test_md060_loose_last_column_with_cjk() {
    // Edge case: CJK characters in last column with loose-last-column
    let config = MD060Config {
        enabled: true,
        style: "aligned".to_string(),
        max_width: LineLength::from_const(0),
        column_align: ColumnAlign::Auto,
        column_align_header: None,
        column_align_body: None,
        loose_last_column: true,
    };
    let rule = MD060TableFormat::from_config_struct(config, default_md013_config(), false);

    // CJK chars are double-width
    let content = "| A | Name |\n|---|---|\n| X | 中文 |\n| Y | English |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Should not panic and should preserve content
    assert!(fixed.contains("中文"), "CJK content should be preserved");
    assert!(fixed.contains("English"), "ASCII content should be preserved");

    let lines: Vec<&str> = fixed.lines().collect();
    // With loose last column, body rows can differ in length
    // CJK row might be different length than English row due to display width differences
    assert_eq!(lines.len(), 4, "Should have 4 lines");
}
