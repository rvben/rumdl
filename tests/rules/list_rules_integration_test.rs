use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{MD004UnorderedListStyle, MD005ListIndent, MD007ULIndent, MD029OrderedListPrefix};

/// Cross-rule integration tests for list rules (MD004, MD005, MD007, MD029)
///
/// These tests ensure that all list rules work harmoniously together
/// and that fixes from one rule don't break others.

#[test]
fn test_mixed_unordered_list_style_and_indentation() {
    // Test MD004 (unordered list style) + MD005 (list indentation) + MD007 (unordered list indentation)
    let md004 = MD004UnorderedListStyle::default(); // Consistent marker style
    let md005 = MD005ListIndent::default(); // Consistent indentation
    let md007 = MD007ULIndent::default(); // Proper nested indentation

    let content = "\
* First item with asterisk
+ Second item with plus (MD004 violation)
 * Wrong indent 1 space (MD005, MD007 violations)
* Third item back to asterisk";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // All rules should detect their respective issues
    let md004_result = md004.check(&ctx).unwrap();
    let md005_result = md005.check(&ctx).unwrap();
    let md007_result = md007.check(&ctx).unwrap();

    assert!(!md004_result.is_empty(), "MD004 should detect mixed markers");
    assert!(!md005_result.is_empty(), "MD005 should detect wrong indentation");
    assert!(!md007_result.is_empty(), "MD007 should detect wrong nested indentation");

    // Test that fixing MD004 doesn't break others
    let md004_fixed = md004.fix(&ctx).unwrap();
    let ctx_after_md004 = LintContext::new(&md004_fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD004 issues should be resolved
    let md004_recheck = md004.check(&ctx_after_md004).unwrap();
    assert!(md004_recheck.is_empty(), "MD004 issues should be fixed");

    // But MD005 and MD007 should still detect their issues
    let md005_after_md004 = md005.check(&ctx_after_md004).unwrap();
    let md007_after_md004 = md007.check(&ctx_after_md004).unwrap();
    assert!(
        !md005_after_md004.is_empty(),
        "MD005 issues should remain after MD004 fix"
    );
    assert!(
        !md007_after_md004.is_empty(),
        "MD007 issues should remain after MD004 fix"
    );
}

#[test]
fn test_ordered_list_style_and_indentation() {
    // Test MD029 (ordered list prefix) + MD005 (list indentation)
    let md029 = MD029OrderedListPrefix::default(); // Sequential numbering
    let md005 = MD005ListIndent::default(); // Consistent indentation

    let content = "\
1. First item
3. Wrong number (MD029 violation)
 2. Wrong indent and number (MD005, MD029 violations)
4. Fourth item";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Both rules should detect their respective issues
    let md029_result = md029.check(&ctx).unwrap();
    let md005_result = md005.check(&ctx).unwrap();

    assert!(!md029_result.is_empty(), "MD029 should detect wrong numbering");
    assert!(!md005_result.is_empty(), "MD005 should detect wrong indentation");

    // Test that fixing MD029 doesn't break MD005
    let md029_fixed = md029.fix(&ctx).unwrap();
    let ctx_after_md029 = LintContext::new(&md029_fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD029 issues should be resolved
    let md029_recheck = md029.check(&ctx_after_md029).unwrap();
    assert!(md029_recheck.is_empty(), "MD029 issues should be fixed");

    // MD005 should still detect indentation issues
    let md005_after_md029 = md005.check(&ctx_after_md029).unwrap();
    assert!(
        !md005_after_md029.is_empty(),
        "MD005 issues should remain after MD029 fix"
    );
}

#[test]
fn test_complex_nested_mixed_lists() {
    // Test all list rules together with complex nesting
    let md004 = MD004UnorderedListStyle::default();
    let md005 = MD005ListIndent::default();
    let md007 = MD007ULIndent::default();
    let md029 = MD029OrderedListPrefix::default();

    // Note: MD005 groups items by (parent_content_column, is_ordered) to prevent
    // oscillation with MD007. To trigger MD005, we need inconsistent same-type siblings.
    let content = "\
* Unordered list item
  1. Ordered nested item
  2. Another ordered nested item
  4. Wrong number in same list (MD029 violation)
+ Mixed marker style (MD004 violation)
  * Bullet at 2 spaces
   * Wrong indent 3 spaces (MD005 - inconsistent with sibling, MD007 violation)
* Back to proper unordered";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // All rules should work independently
    let md004_result = md004.check(&ctx).unwrap();
    let md005_result = md005.check(&ctx).unwrap();
    let md007_result = md007.check(&ctx).unwrap();
    let md029_result = md029.check(&ctx).unwrap();

    assert!(!md004_result.is_empty(), "MD004 should detect mixed markers");
    assert!(
        !md005_result.is_empty(),
        "MD005 should detect inconsistent bullet indentation"
    );
    assert!(!md007_result.is_empty(), "MD007 should detect wrong nested indentation");
    assert!(!md029_result.is_empty(), "MD029 should detect wrong numbering");

    // Apply fixes sequentially and ensure they don't conflict
    let step1 = md004.fix(&ctx).unwrap();
    let ctx1 = LintContext::new(&step1, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let step2 = md005.fix(&ctx1).unwrap();
    let ctx2 = LintContext::new(&step2, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let step3 = md007.fix(&ctx2).unwrap();
    let ctx3 = LintContext::new(&step3, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let step4 = md029.fix(&ctx3).unwrap();
    let ctx_final = LintContext::new(&step4, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // After all fixes, all rules should be satisfied
    assert!(
        md004.check(&ctx_final).unwrap().is_empty(),
        "MD004 should pass after all fixes"
    );
    assert!(
        md005.check(&ctx_final).unwrap().is_empty(),
        "MD005 should pass after all fixes"
    );
    assert!(
        md007.check(&ctx_final).unwrap().is_empty(),
        "MD007 should pass after all fixes"
    );
    assert!(
        md029.check(&ctx_final).unwrap().is_empty(),
        "MD029 should pass after all fixes"
    );
}

#[test]
fn test_deep_nesting_with_multiple_list_types() {
    // Test how rules handle deeply nested lists with multiple types
    let md005 = MD005ListIndent::default();
    let md007 = MD007ULIndent::default();
    let md029 = MD029OrderedListPrefix::default();

    let content = "\
1. Top level ordered
   * Second level unordered
     1. Third level ordered
        * Fourth level unordered
          1. Fifth level ordered (independent numbering)
   * Back to second level
2. Second top level";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // With proper indentation, all rules should pass
    let md005_result = md005.check(&ctx).unwrap();
    let md007_result = md007.check(&ctx).unwrap();
    let md029_result = md029.check(&ctx).unwrap();

    assert!(md005_result.is_empty(), "MD005 should pass with proper indentation");
    assert!(md007_result.is_empty(), "MD007 should pass with proper nesting");
    // MD029 should pass - the sequence "1. ... 2." is correct with sequential numbering
    // The nested content doesn't interrupt the top-level ordered list
    assert!(
        md029_result.is_empty(),
        "MD029 should pass with correct sequential numbering"
    );
}

#[test]
fn test_list_rules_with_code_blocks() {
    // Test that list rules handle code blocks correctly without interference
    let md005 = MD005ListIndent::default();
    let md029 = MD029OrderedListPrefix::default();

    let content = "\
1. First item with code:
   ```rust
   fn example() {
       // This indentation should not affect list rules
       println!(\"Hello\");
   }
   ```
2. Second item continues sequence
   ```python
   def another():
   pass  # Wrong Python indentation, but shouldn't affect MD005
   ```
3. Third item";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Code blocks should not interfere with list rule detection
    let md005_result = md005.check(&ctx).unwrap();
    let md029_result = md029.check(&ctx).unwrap();

    assert!(md005_result.is_empty(), "MD005 should ignore code block indentation");
    assert!(
        md029_result.is_empty(),
        "MD029 should maintain sequence across code blocks"
    );
}

#[test]
fn test_list_rules_with_blockquotes() {
    // Test that list rules work correctly inside blockquotes
    let md005 = MD005ListIndent::default();
    let md029 = MD029OrderedListPrefix::default();

    let content = "\
> This is a blockquote with lists:
> 1. First quoted item
>   1. Nested quoted item
>   2. Another nested item
> 2. Second quoted item
>
> * Unordered list in quote
>   * Nested unordered in quote";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Rules should work correctly within blockquotes
    let md005_result = md005.check(&ctx).unwrap();
    let md029_result = md029.check(&ctx).unwrap();

    assert!(
        md005_result.is_empty(),
        "MD005 should handle blockquote lists correctly"
    );
    assert!(
        md029_result.is_empty(),
        "MD029 should handle blockquote lists correctly"
    );
}

#[test]
fn test_list_continuation_across_rules() {
    // Test that list continuation works across different rule fixes
    let md004 = MD004UnorderedListStyle::default();
    let md005 = MD005ListIndent::default();

    let content = "\
* First item
  Some continuation text
+ Second item with wrong marker
  More continuation text
  * Nested item
    Nested continuation";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Both rules should handle continuation text correctly
    let md004_result = md004.check(&ctx).unwrap();
    let md005_result = md005.check(&ctx).unwrap();

    assert!(!md004_result.is_empty(), "MD004 should detect marker inconsistency");
    assert!(
        md005_result.is_empty(),
        "MD005 should accept proper continuation indentation"
    );

    // Fix MD004 and ensure continuation text is preserved
    let md004_fixed = md004.fix(&ctx).unwrap();
    let ctx_fixed = LintContext::new(&md004_fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Continuation text should still be properly indented
    let md005_after_fix = md005.check(&ctx_fixed).unwrap();
    assert!(md005_after_fix.is_empty(), "MD005 should still pass after MD004 fix");

    // The fixed content should preserve continuation text
    assert!(
        md004_fixed.contains("Some continuation text"),
        "Continuation text should be preserved"
    );
    assert!(
        md004_fixed.contains("More continuation text"),
        "All continuation text should be preserved"
    );
    assert!(
        md004_fixed.contains("Nested continuation"),
        "Nested continuation should be preserved"
    );
}

#[test]
fn test_empty_lines_between_list_items() {
    // Test that empty lines between list items don't interfere with rules
    let md004 = MD004UnorderedListStyle::default();
    let md005 = MD005ListIndent::default();
    let md029 = MD029OrderedListPrefix::default();

    let content = "\
1. First item

2. Second item with empty line above

3. Third item

   * Nested unordered with empty line above

   * Another nested item

4. Fourth item";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Empty lines should not break rule detection
    let md004_result = md004.check(&ctx).unwrap();
    let md005_result = md005.check(&ctx).unwrap();
    let md029_result = md029.check(&ctx).unwrap();

    assert!(md004_result.is_empty(), "MD004 should handle empty lines correctly");
    assert!(md005_result.is_empty(), "MD005 should handle empty lines correctly");
    // MD029 should correctly recognize that the nested unordered list is part of item 3's content
    // Therefore item "4." is correctly the 4th item in the sequence
    assert!(
        md029_result.is_empty(),
        "MD029 should pass - nested list is part of item 3's content"
    );
}

#[test]
#[ignore] // Skip in normal test runs - performance test
fn test_performance_with_large_mixed_lists() {
    // Test performance and correctness with larger list structures
    let md004 = MD004UnorderedListStyle::default();
    let md005 = MD005ListIndent::default();
    let md007 = MD007ULIndent::default();
    let md029 = MD029OrderedListPrefix::default();

    // Generate a large list structure that passes all rules
    // Use consistent ordered list without interruptions
    let mut content = String::new();
    for i in 1..=50 {
        content.push_str(&format!("{i}. Ordered item {i}\n"));
        if i % 3 == 0 {
            // Add indented paragraph content (not new list items)
            // For double-digit items, we need 4+ spaces of indentation
            let indent = if i < 10 { "   " } else { "    " };
            content.push_str(&format!(
                "{indent}This item has extended content with proper indentation.\n"
            ));
            content.push_str(&format!("{indent}It includes multiple lines to test performance.\n"));
        }
    }

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // All rules should handle large structures efficiently
    let start = std::time::Instant::now();

    let md004_result = md004.check(&ctx).unwrap();
    let md005_result = md005.check(&ctx).unwrap();
    let md007_result = md007.check(&ctx).unwrap();
    let md029_result = md029.check(&ctx).unwrap();

    let duration = start.elapsed();

    // Should complete quickly (within reasonable time)
    assert!(
        duration.as_millis() < 1000,
        "List rules should be fast on large structures"
    );

    // Should not find issues in the correctly formatted large list
    assert!(md004_result.is_empty(), "MD004 should pass on large correct list");
    assert!(md005_result.is_empty(), "MD005 should pass on large correct list");
    assert!(md007_result.is_empty(), "MD007 should pass on large correct list");
    assert!(md029_result.is_empty(), "MD029 should pass on large correct list");
}
