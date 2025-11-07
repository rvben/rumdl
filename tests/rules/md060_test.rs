use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD060TableFormat;
use unicode_width::UnicodeWidthStr;

#[test]
fn test_md060_disabled_by_default() {
    let rule = MD060TableFormat::default();
    let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 0, "Rule should be disabled by default");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "No changes when disabled");
}

#[test]
fn test_md060_align_simple_ascii_table() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("中文"), "CJK characters should be preserved");

    // Verify all rows have equal display width in aligned mode (not byte length!)
    // CJK characters take more bytes but should have same display width
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].width(), lines[1].width(), "Display widths should match");
    assert_eq!(lines[1].width(), lines[2].width(), "Display widths should match");

    let content2 = "| Name | City |\n|---|---|\n| Alice | 東京 |";
    let ctx2 = LintContext::new(content2, MarkdownFlavor::Standard);
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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
fn test_md060_inline_code_with_pipes() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Pattern | Regex |\n|---|---|\n| Time | `[0-9]|[0-9]` |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("`[0-9]|[0-9]`"),
        "Pipes in inline code should be preserved"
    );

    // Verify all rows have equal length in aligned mode
    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0].len(), lines[1].len());
    assert_eq!(lines[1].len(), lines[2].len());
}

#[test]
fn test_md060_complex_regex_in_inline_code() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content =
        "| Challenge | Solution |\n|---|---|\n| Hour:minute:second | `^([0-1]?\\d|2[0-3]):[0-5]\\d:[0-5]\\d$` |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("`^([0-1]?\\d|2[0-3]):[0-5]\\d:[0-5]\\d$`"),
        "Complex regex should be preserved"
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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.ends_with('\n'), "Trailing newline should be preserved");
}

#[test]
fn test_md060_multiple_tables() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "# First Table\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n# Second Table\n\n| X | Y | Z |\n|---|---|---|\n| a | b | c |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("Header 1"));
    assert!(fixed.contains("Header 2"));
}

#[test]
fn test_md060_none_style() {
    let rule = MD060TableFormat::new(true, "none".to_string());

    let content = "| Name | Age |\n|---|---|\n| Alice | 30 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 0, "None style should not produce warnings");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "None style should not modify content");
}

#[test]
fn test_md060_single_column_table() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let content = "| Column |\n|---|\n| Value1 |\n| Value2 |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("`a\\|b`"), "Escaped pipes should be preserved");
}

#[test]
fn test_md060_very_long_content() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    let long_text = "A".repeat(100);
    let content = format!("| Col1 | Col2 |\n|---|---|\n| {long_text} | B |");
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains(&long_text), "Long content should be preserved");
}

#[test]
fn test_md060_minimum_column_width() {
    let rule = MD060TableFormat::new(true, "aligned".to_string());

    // Test with very short column content (1-2 chars) to ensure minimum width of 3
    // This is required because GFM mandates at least 3 dashes in delimiter rows
    let content = "| ID | First Name | Last Name | Department |\n|-|-|-|-|\n| 1 | John | Doe | Engineering |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
    let ctx = LintContext::new(content, MarkdownFlavor::Standard);

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
