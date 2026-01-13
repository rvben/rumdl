use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD005ListIndent;

#[test]
fn test_valid_unordered_list() {
    let rule = MD005ListIndent::default();
    let content = "\
* Item 1
* Item 2
  * Nested 1
  * Nested 2
* Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_valid_ordered_list() {
    let rule = MD005ListIndent::default();
    let content = "\
1. Item 1
2. Item 2
   1. Nested 1
   2. Nested 2
3. Item 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // With dynamic alignment, nested items should align with parent's text content
    // Ordered items starting with "1. " have text at column 3, so nested items need 3 spaces
    assert!(result.is_empty());
}

#[test]
fn test_frontmatter_yaml_lists_not_detected() {
    // Test for issue #35 - YAML lists in frontmatter should not be detected as Markdown lists
    let rule = MD005ListIndent::default();
    let content = "\
---
layout: post
title: \"title\"
creator:
  - 'user1'
  - 'user2'
creator_num:
  - 1253217
  - 1615089
tags: [tag1, tag2, tag3]
---

# TITLE

## Heading

Whatever

And a list:

- Item1
- Item2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should not flag YAML lists in frontmatter
    assert!(result.is_empty(), "MD005 should not check lists in frontmatter");
}

#[test]
fn test_invalid_unordered_indent() {
    let rule = MD005ListIndent::default();
    let content = "\
* Item 1
 * Item 2
   * Nested 1";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Dynamic detection: line 2 has 1 space, treated as top-level with wrong indent
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* Item 1\n* Item 2\n   * Nested 1");
}

#[test]
fn test_invalid_ordered_indent() {
    let rule = MD005ListIndent::default();
    let content = "\
1. Item 1
 2. Item 2
    1. Nested 1";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    // Dynamic detection: line 2 has 1 space, treated as top-level with wrong indent
    assert_eq!(fixed, "1. Item 1\n2. Item 2\n    1. Nested 1");
}

#[test]
fn test_mixed_list_types() {
    let rule = MD005ListIndent::default();
    let content = "\
* Item 1
  1. Nested ordered
  * Nested unordered
* Item 2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_levels() {
    let rule = MD005ListIndent::default();
    let content = "\
* Level 1
   * Level 2
      * Level 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Dynamic detection accepts 3-space pattern
    assert_eq!(result.len(), 0, "Should accept consistent 3-space indentation");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "No changes needed for consistent indentation");
}

#[test]
fn test_empty_lines() {
    let rule = MD005ListIndent::default();
    let content = "\
* Item 1

  * Nested 1

* Item 2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_lists() {
    let rule = MD005ListIndent::default();
    let content = "\
Just some text
More text
Even more text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_complex_nesting() {
    let rule = MD005ListIndent::default();
    let content = "\
* Level 1
  * Level 2
    * Level 3
  * Back to 2
    1. Ordered 3
    2. Still 3
* Back to 1";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_complex_nesting() {
    let rule = MD005ListIndent::default();
    let content = "\
* Level 1
   * Level 2
     * Level 3
   * Back to 2
      1. Ordered 3
     2. Still 3
* Back to 1";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // MD005 groups items by (parent_content_column, is_ordered) to prevent oscillation with MD007.
    // Level 3 ordered items (lines 5-6) are checked separately from level 3 unordered (line 3).
    // First-established for ordered group = 6 spaces (from line 5), so line 6 (5 spaces) is flagged.
    assert_eq!(
        result.len(),
        1,
        "Should flag line 6 which is inconsistent with other ordered items"
    );
    let fixed = rule.fix(&ctx).unwrap();
    // Line 6 is fixed to match line 5's indentation (6 spaces)
    assert_eq!(
        fixed,
        "* Level 1\n   * Level 2\n     * Level 3\n   * Back to 2\n      1. Ordered 3\n      2. Still 3\n* Back to 1"
    );
}

// ============================================================================
// Tab-indented list detection tests (issue #254)
//
// Issue #254: Tab-indented nested lists were not detected because pulldown-cmark
// reports item events at the newline position before the tab, not at the tab itself.
// This caused MD004, MD005, MD007 to miss nested items entirely.
// ============================================================================

/// Regression test: Tab-indented nested lists must be detected.
///
/// Before the fix, pulldown-cmark reported the nested item at byte 8 (the newline),
/// which mapped to line 1 instead of line 2. The fix detects this and advances
/// to the correct line.
#[test]
fn test_tab_indented_list_detection_regression() {
    let content = "* Item 1\n\t- Nested with tab";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Both items MUST be detected - this was broken before the fix
    let detected: Vec<_> = ctx
        .lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.list_item.as_ref().map(|item| (i + 1, &item.marker)))
        .collect();

    assert_eq!(
        detected.len(),
        2,
        "Regression: Both list items must be detected. Got: {detected:?}"
    );
    assert_eq!(detected[0], (1, &"*".to_string()), "Line 1 should have '*' marker");
    assert_eq!(detected[1], (2, &"-".to_string()), "Line 2 should have '-' marker");
}

/// Verify that rules actually WORK with tab-indented lists, not just detection.
#[test]
fn test_tab_indented_list_rules_work() {
    use rumdl_lib::rules::MD004UnorderedListStyle;

    // MD004 should detect inconsistent markers in tab-indented lists
    let rule = MD004UnorderedListStyle::default();
    let content = "- Item 1\n\t* Nested with wrong marker";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(
        warnings.len(),
        1,
        "MD004 should detect wrong marker on tab-indented nested item"
    );
    assert_eq!(warnings[0].line, 2, "Warning should be on line 2");
}

#[test]
fn test_tab_indented_nested_lists() {
    // Tab indentation should be detected correctly
    let content = "* Item 1\n\t- Nested with tab\n\t\t+ Double tab nested";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // All 3 items should be detected as list items
    let list_item_count = ctx.lines.iter().filter(|l| l.list_item.is_some()).count();
    assert_eq!(list_item_count, 3, "All 3 tab-indented list items should be detected");
}

#[test]
fn test_mixed_tab_and_space_indentation() {
    // Mixed tab/space indentation should be detected
    let content = "- Item 1\n  - Space nested\n\t- Tab nested";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let list_item_count = ctx.lines.iter().filter(|l| l.list_item.is_some()).count();
    assert_eq!(
        list_item_count, 3,
        "Mixed tab/space nested lists should all be detected"
    );
}

#[test]
fn test_toml_frontmatter_lists_not_detected() {
    // TOML frontmatter lists should not be detected as Markdown lists
    let content = r#"+++
title = "Test"
tags = ["tag1", "tag2"]
[[items]]
name = "item1"
[[items]]
name = "item2"
+++

# Heading

- Actual list item"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Only the actual Markdown list should be detected
    let list_lines: Vec<_> = ctx
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.list_item.is_some())
        .collect();

    assert_eq!(
        list_lines.len(),
        1,
        "Only the Markdown list after frontmatter should be detected"
    );
}

#[test]
fn test_blockquote_with_tab_indented_list() {
    // Blockquoted lists with tab indentation
    let content = "> - Item 1\n>\t- Tab nested in blockquote";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let list_item_count = ctx.lines.iter().filter(|l| l.list_item.is_some()).count();
    assert_eq!(list_item_count, 2, "Blockquoted tab-indented lists should be detected");
}

#[test]
fn test_deeply_indented_content_not_list() {
    // Lines with 8+ spaces of indentation should be code blocks, not lists
    // Per CommonMark, 4+ spaces of indentation (after accounting for list context) creates a code block
    let content = "- Item 1\n        - 8 spaces should be code block";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Only the first item should be detected as a list item
    // The second line is treated as an indented code block per CommonMark
    let list_item_count = ctx.lines.iter().filter(|l| l.list_item.is_some()).count();
    assert_eq!(
        list_item_count, 1,
        "8+ space indentation should be treated as code block, not list"
    );
}

// ============================================================================
// MD005/MD007 oscillation prevention tests
//
// MD005 groups items by (parent_content_column, is_ordered), treating ordered and
// unordered lists as separate concerns for indentation consistency. This prevents
// oscillation where MD007 fixes bullet indent and MD005 reverts it as "inconsistent"
// with ordered items at the same level.
// ============================================================================

/// MD005 should NOT flag bullets as inconsistent with ordered items at the same
/// level - they're separate semantic constructs.
#[test]
fn test_ordered_and_unordered_in_separate_groups() {
    use rumdl_lib::rules::MD007ULIndent;

    let md005 = MD005ListIndent::default();
    let md007 = MD007ULIndent::default();

    // Minimal reproduction: ordered item at one indent, bullet at different indent
    // Under same parent - MD005 should not consider them "inconsistent"
    let content = "* Parent\n  1. ordered at 2 spaces\n   - bullet at 3 spaces";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD005 should NOT flag bullet because ordered/unordered are separate groups
    let md005_warnings = md005.check(&ctx).unwrap();
    assert!(
        md005_warnings.is_empty(),
        "MD005 should not flag bullet as inconsistent with ordered item - separate groups. Got: {md005_warnings:?}"
    );

    // MD007 may or may not flag the bullet (depends on expected indent)
    // The key test is that after any fix, we don't oscillate
    let md007_warnings = md007.check(&ctx).unwrap();

    if !md007_warnings.is_empty() {
        // Apply MD007 fix
        let fixed = md007.fix(&ctx).unwrap();
        let ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);

        // After MD007 fix, MD005 should still be happy (no oscillation)
        let md005_after = md005.check(&ctx_fixed).unwrap();
        assert!(
            md005_after.is_empty(),
            "MD005 should not try to revert MD007's fix - no oscillation. Got: {md005_after:?}"
        );
    }
}

/// Bullets stay separate from ordered items regardless of which comes first.
#[test]
fn test_bullet_before_ordered_in_separate_groups() {
    let md005 = MD005ListIndent::default();

    // Bullet first, then ordered - also separate groups
    let content = "* Parent\n  - bullet at 2 spaces\n   1. ordered at 3 spaces";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD005 should NOT flag ordered item as inconsistent with bullet
    let md005_warnings = md005.check(&ctx).unwrap();
    assert!(
        md005_warnings.is_empty(),
        "MD005 should not flag ordered as inconsistent with bullet - separate groups. Got: {md005_warnings:?}"
    );
}

/// Multiple ordered/unordered siblings at same level stay in separate groups.
#[test]
fn test_multiple_mixed_siblings_in_separate_groups() {
    let md005 = MD005ListIndent::default();

    // Multiple ordered items, then multiple bullets at different indent
    let content = "* Parent\n  1. First ordered\n  2. Second ordered\n   - First bullet\n   - Second bullet";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // MD005 should not flag anything - ordered items are consistent (2 spaces),
    // bullets are consistent (3 spaces), and they're in separate groups
    let md005_warnings = md005.check(&ctx).unwrap();
    assert!(
        md005_warnings.is_empty(),
        "MD005 should not flag - each type is internally consistent. Got: {md005_warnings:?}"
    );
}

/// MD005 still flags inconsistency within same list type.
#[test]
fn test_inconsistency_within_same_type_detected() {
    let md005 = MD005ListIndent::default();

    // Two bullets at different indents under same parent - SHOULD be flagged
    let content = "* Parent\n  - First bullet at 2 spaces\n   - Second bullet at 3 spaces";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let md005_warnings = md005.check(&ctx).unwrap();
    assert!(
        !md005_warnings.is_empty(),
        "MD005 should flag inconsistent bullets (same type, different indents). Got: {md005_warnings:?}"
    );
}
