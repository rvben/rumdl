use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD005ListIndent;

#[test]
fn test_valid_unordered_list() {
    let rule = MD005ListIndent;
    let content = "\
* Item 1
* Item 2
  * Nested 1
  * Nested 2
* Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_valid_ordered_list() {
    let rule = MD005ListIndent;
    let content = "\
1. Item 1
2. Item 2
   1. Nested 1
   2. Nested 2
3. Item 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // With dynamic alignment, nested items should align with parent's text content
    // Ordered items starting with "1. " have text at column 3, so nested items need 3 spaces
    assert!(result.is_empty());
}

#[test]
fn test_frontmatter_yaml_lists_not_detected() {
    // Test for issue #35 - YAML lists in frontmatter should not be detected as Markdown lists
    let rule = MD005ListIndent;
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
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Should not flag YAML lists in frontmatter
    assert!(result.is_empty(), "MD005 should not check lists in frontmatter");
}

#[test]
fn test_invalid_unordered_indent() {
    let rule = MD005ListIndent;
    let content = "\
* Item 1
 * Item 2
   * Nested 1";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // With dynamic alignment, line 3 correctly aligns with line 2's text position
    // Only line 2 is incorrectly indented
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* Item 1\n  * Item 2\n   * Nested 1");
}

#[test]
fn test_invalid_ordered_indent() {
    let rule = MD005ListIndent;
    let content = "\
1. Item 1
 2. Item 2
    1. Nested 1";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    // With dynamic alignment, ordered items align with parent's text content
    // Line 1 text starts at col 3, so line 2 should have 3 spaces
    // Line 3 already correctly aligns with line 2's text position
    assert_eq!(fixed, "1. Item 1\n   2. Item 2\n    1. Nested 1");
}

#[test]
fn test_mixed_list_types() {
    let rule = MD005ListIndent;
    let content = "\
* Item 1
  1. Nested ordered
  * Nested unordered
* Item 2";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_levels() {
    let rule = MD005ListIndent;
    let content = "\
* Level 1
   * Level 2
      * Level 3";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    // With dynamic alignment:
    // Level 2 aligns with Level 1's text (2 spaces)
    // Level 3 aligns with Level 2's text (5 spaces: 2 + "* " + 1)
    assert_eq!(
        fixed,
        "\
* Level 1
  * Level 2
     * Level 3"
    );
}

#[test]
fn test_empty_lines() {
    let rule = MD005ListIndent;
    let content = "\
* Item 1

  * Nested 1

* Item 2";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_lists() {
    let rule = MD005ListIndent;
    let content = "\
Just some text
More text
Even more text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_complex_nesting() {
    let rule = MD005ListIndent;
    let content = "\
* Level 1
  * Level 2
    * Level 3
  * Back to 2
    1. Ordered 3
    2. Still 3
* Back to 1";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_complex_nesting() {
    let rule = MD005ListIndent;
    let content = "\
* Level 1
   * Level 2
     * Level 3
   * Back to 2
      1. Ordered 3
     2. Still 3
* Back to 1";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // With dynamic alignment, fewer items need correction
    // Lines 2,4: should align with Level 1's text (2 spaces)
    // Line 5: should align with "Back to 2"'s text (5 spaces)
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "* Level 1\n  * Level 2\n     * Level 3\n  * Back to 2\n     1. Ordered 3\n     2. Still 3\n* Back to 1"
    );
}
