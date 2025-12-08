/// Test for correct fix counting when some warnings can't be fixed
/// This test specifically targets the bug where MD013 with reflow enabled
/// counts table rows as "fixed" even though they can't be reflowed
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD009TrailingSpaces, MD013LineLength};
use rumdl_lib::types::LineLength;

#[test]
fn test_md013_fix_counting_with_tables() {
    // Create content with both fixable and unfixable long lines
    let content = r#"# Test Document

This is a very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very long line that can be reflowed.

| Column 1 | Column 2 | Column 3 |
|----------|----------|----------|
| Short | Short | Short |
| This is a very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very long table cell that cannot be reflowed | B | C |

Another very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very very long line that can be reflowed.
"#;

    // Set up MD013 with reflow enabled
    // MD013LineLength::new(line_length, code_blocks, tables, headings, strict)
    // Note: We create with tables=false to ensure table warnings are detected
    // Then we'll create one with reflow for testing the fix
    use rumdl_lib::rules::md013_line_length::md013_config::MD013Config;
    use rumdl_lib::types::LineLength;

    let config = MD013Config {
        line_length: LineLength::from_const(80),
        code_blocks: true,
        tables: true, // Check tables for line length
        headings: true,
        paragraphs: true,
        strict: false,
        reflow: true, // Enable reflow
        ..Default::default()
    };

    let rule = MD013LineLength::from_config_struct(config);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Check for warnings
    let warnings = rule.check(&ctx).unwrap();

    // Should have 3 warnings: 2 regular long lines and 1 table row
    assert_eq!(warnings.len(), 3, "Should detect 3 long lines");

    // Apply the fix
    let fixed_content = rule.fix(&ctx).unwrap();

    // Check the fixed content for remaining warnings
    let ctx_fixed = LintContext::new(&fixed_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let remaining_warnings = rule.check(&ctx_fixed).unwrap();

    // The table row should still have a warning because it can't be reflowed
    assert_eq!(
        remaining_warnings.len(),
        1,
        "Table row should still exceed line length after fix"
    );

    // Verify that the table row warning is the one that remains (line 10 after reflow)
    assert!(
        remaining_warnings[0].line == 10,
        "Remaining warning should be for the table row"
    );

    // The actual number of fixed warnings should be 2, not 3
    let actual_fixed_count = warnings.len() - remaining_warnings.len();
    assert_eq!(actual_fixed_count, 2, "Only 2 of 3 warnings should be actually fixed");
}

#[test]
fn test_mixed_rules_fix_counting() {
    // Test with multiple rules where some fixes work and some don't
    // Note: Using concat to ensure trailing spaces are preserved
    // MD009 by default allows 2 trailing spaces (for line breaks), so we use 3 and 4
    let content = concat!(
        "# Test\n",
        "\n",
        "This line has trailing spaces   \n", // 3 trailing spaces
        "\n",
        "| Column | Very very very very very very very very very very very very very very very very very very very very very very very very long header |\n",
        "|--------|-----------|\n",
        "| A | B |\n",
        "\n",
        "More trailing spaces    \n" // 4 trailing spaces
    );

    // Set up rules
    use rumdl_lib::rules::md013_line_length::md013_config::MD013Config;

    let md013_config = MD013Config {
        line_length: LineLength::from_const(80),
        code_blocks: true,
        tables: true, // Check tables for line length
        headings: true,
        paragraphs: true,
        strict: false,
        reflow: true,
        ..Default::default()
    };

    let md013 = MD013LineLength::from_config_struct(md013_config);
    let md009 = MD009TrailingSpaces::default();

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Check MD013 warnings
    let md013_warnings = md013.check(&ctx).unwrap();
    assert_eq!(md013_warnings.len(), 1, "Should have 1 long line (table header)");

    // Check MD009 warnings
    let md009_warnings = md009.check(&ctx).unwrap();
    assert_eq!(md009_warnings.len(), 2, "Should have 2 trailing space warnings");

    // Fix MD009 (trailing spaces)
    let fixed_by_md009 = md009.fix(&ctx).unwrap();
    let ctx_after_md009 = LintContext::new(&fixed_by_md009, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let md009_remaining = md009.check(&ctx_after_md009).unwrap();
    assert_eq!(md009_remaining.len(), 0, "All trailing spaces should be fixed");

    // Fix MD013 (line length)
    let fixed_by_md013 = md013.fix(&ctx_after_md009).unwrap();
    let ctx_after_md013 = LintContext::new(&fixed_by_md013, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let md013_remaining = md013.check(&ctx_after_md013).unwrap();
    assert_eq!(md013_remaining.len(), 1, "Table header should still be too long");

    // Summary: MD009 fixed 2/2, MD013 fixed 0/1 (table can't be reflowed)
    // Total should be 2/3 fixed, not 3/3
}
