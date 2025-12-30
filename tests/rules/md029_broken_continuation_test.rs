use rumdl_lib::ListStyle;
/// Tests for MD029 respecting CommonMark list start values (Issue #247)
///
/// When CommonMark parses what the user intended as one continuous ordered list
/// into multiple separate lists (due to insufficient indentation), MD029 should:
/// 1. Respect the CommonMark-provided start value for each list
/// 2. NOT change item 11 to 1 when it's the start of a new list
/// 3. Only warn when items within a list are incorrectly numbered relative to the start
use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD029OrderedListPrefix;

#[test]
fn test_commonmark_respects_start_value() {
    // Classic issue #247 scenario: list 1-10, then item 11 breaks out
    // The second list starts at 11 per CommonMark, so 11 is the correct first item
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let content = r#"1. one
   - sub
2. two
   - sub
3. three
   - sub
4. four
   - sub
5. five
   - sub
6. six
   - sub
7. seven
   - sub
8. eight
   - sub
9. nine
   - sub
10. ten
   - sub (3 spaces - breaks the list!)
11. eleven
   - sub
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // No warnings - CommonMark says list 2 starts at 11, and item 11 is correct
    assert!(
        warnings.is_empty(),
        "Should have no warnings - item 11 is correctly numbered for its list"
    );
}

#[test]
fn test_list_starting_at_11_with_wrong_numbers() {
    // A list that CommonMark parses as starting at 11, but has wrong subsequent numbers
    // NOTE: For the list to actually break, we need insufficient indent for "10. " (4 chars)
    // 3 spaces is sufficient for "2. " (3 chars), so we need to go through 10+ items
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let content = r#"1. one
   - sub
2. two
   - sub
3. three
   - sub
4. four
   - sub
5. five
   - sub
6. six
   - sub
7. seven
   - sub
8. eight
   - sub
9. nine
   - sub
10. ten
   - sub (3 spaces - breaks the list! "10. " needs 4 spaces)
11. eleven
13. thirteen (wrong - should be 12)
15. fifteen (wrong - should be 13)
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should have warnings for 13 and 15 (expected 12 and 13)
    assert_eq!(warnings.len(), 2, "Should have 2 warnings for wrong numbers");

    // 13 should expect 12
    assert!(
        warnings[0].message.contains("expected 12"),
        "13 should expect 12: {}",
        warnings[0].message
    );

    // 15 should expect 13
    assert!(
        warnings[1].message.contains("expected 13"),
        "15 should expect 13: {}",
        warnings[1].message
    );

    // NO auto-fix for lists starting at N > 1 (user chose those numbers intentionally)
    // We warn to help users spot issues, but don't auto-fix to preserve their intent
    for w in &warnings {
        assert!(
            w.fix.is_none(),
            "Lists starting at N > 1 should NOT have auto-fix (preserves user intent)"
        );
    }
}

#[test]
fn test_separate_list_after_paragraph() {
    // A paragraph between lists creates intentional separation
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let content = r#"1. one
2. two
3. three

Some paragraph text that separates the lists.

1. new list starting at 1 (intentional)
2. two
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // No warnings - both lists are correctly numbered
    assert!(
        warnings.is_empty(),
        "Intentional restart at 1 should not produce warnings"
    );
}

#[test]
fn test_fix_preserves_list_start_values() {
    // Verify that fix() doesn't change list start values
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let content = r#"1. one
   - sub
2. two
   - sub
10. ten
   - sub (3 spaces - breaks!)
11. eleven
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    // Item 11 should NOT be changed to 1
    assert!(
        fixed.contains("11. eleven"),
        "Fix should NOT change list start values: {fixed}"
    );
}

#[test]
fn test_fix_corrects_wrong_numbers_within_list() {
    // Normal numbering errors within a list should still be fixed
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let content = r#"1. one
3. three (wrong, should be 2)
5. five (wrong, should be 3)
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // These are normal errors
    assert_eq!(warnings.len(), 2);

    for w in &warnings {
        assert!(w.fix.is_some(), "Normal errors should have auto-fix");
    }

    // Apply fix
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("2. three"), "Should fix 3 to 2");
    assert!(fixed.contains("3. five"), "Should fix 5 to 3");
}

#[test]
fn test_issue_247_exact_scenario() {
    // Exact reproduction of issue #247
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let content = r#"# Test

1. 10161017144619_aaaaaa

   - aaaaaaa

2. 10161017145045_aaaaaa

   - aaaaaaa

3. 10161103155990_aaaaaa
   - aaaaaaa

4. 10161111145131_aaaaaa
   - aaaaaaa

5. 10161111161511_aaaaaa
   - aaaaaaa

6. 10161116131648_aaaaaa
   - aaaaaaa

7. 10161108111544_aaaaaa
   - aaaaaaa

8. 10161114171054_aaaaaa
   - aaaaaaa

9. 10170619141530_aaaaaa

   - aaaaaaa

10. 10170619143735_aaaaaa

   - aaaaaaa

11. 10171104113741_aaaaaa

   - aaaaaaa
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Item 11 should NOT be flagged - it's correctly numbered for its list
    let item_11_warning = warnings.iter().find(|w| w.message.contains("11"));

    assert!(
        item_11_warning.is_none(),
        "Should NOT have warning for item 11 - it's correctly numbered"
    );

    // Verify fix doesn't change 11 to 1
    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("11. 10171104113741_aaaaaa"),
        "Fix should NOT change item 11 to 1"
    );
}

#[test]
fn test_list_starting_at_100() {
    // Test that lists starting at high numbers are respected
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);

    // Create a list that ends at 99, then 100 breaks out
    let mut content = String::new();
    for i in 1..=99 {
        content.push_str(&format!("{i}. item\n"));
    }
    content.push_str("100. item one hundred\n");
    content.push_str("   - sub (4 spaces - insufficient for 100.)\n");
    content.push_str("101. item one hundred one\n");

    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // 101 should NOT be flagged - it correctly follows 100 in its list
    let item_101_warning = warnings.iter().find(|w| w.message.contains("101"));

    assert!(
        item_101_warning.is_none(),
        "Should NOT have warning for item 101 - it's correctly numbered"
    );
}

#[test]
fn test_style_one_ignores_start_value() {
    // With style=one, all items should be 1 regardless of start value
    let rule = MD029OrderedListPrefix::new(ListStyle::OneOne);
    let content = r#"1. one
   - sub
1. two
   - sub
1. three
   - sub (breaks!)
1. four
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // With style=one, all 1s are valid
    assert!(warnings.is_empty(), "Style 'one' with all 1s should have no warnings");
}

#[test]
fn test_multiple_lists_each_correctly_numbered() {
    // Multiple lists, each starting at different values, all correctly numbered
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let content = r#"1. first list item 1
2. first list item 2

Some paragraph.

5. second list starts at 5
6. second list item 6
7. second list item 7
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // All items correctly numbered within their lists
    assert!(
        warnings.is_empty(),
        "Both lists are correctly numbered within themselves"
    );
}

#[test]
fn test_broken_list_with_correct_continuation_numbers() {
    // A list that breaks due to insufficient indentation, but continuation is correct
    let rule = MD029OrderedListPrefix::new(ListStyle::Ordered);
    let content = r#"9. nine
   - sub
10. ten
   - sub (3 spaces - insufficient for "10. " which needs 4)
11. eleven
12. twelve
13. thirteen
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // The second list starts at 11 and has 11, 12, 13 - all correct
    assert!(warnings.is_empty(), "Second list 11, 12, 13 is correctly numbered");
}
