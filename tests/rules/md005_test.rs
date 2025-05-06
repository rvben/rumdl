use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD005ListIndent;

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
    assert!(result.is_empty());
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
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* Item 1\n  * Item 2\n    * Nested 1");
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
    assert_eq!(fixed, "1. Item 1\n  2. Item 2\n    1. Nested 1");
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
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "* Level 1\n  * Level 2\n    * Level 3\n  * Back to 2\n      1. Ordered 3\n    2. Still 3\n* Back to 1");
}
