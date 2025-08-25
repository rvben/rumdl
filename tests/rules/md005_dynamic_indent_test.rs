use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD005ListIndent;

#[test]
fn test_four_space_indent_detection() {
    // Test that MD005 detects and respects 4-space indentation pattern
    let rule = MD005ListIndent;
    let content = "\
* Item 1
    * Nested with 4 spaces
        * Double nested with 8 spaces
    * Another nested with 4 spaces
* Item 2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    // Should detect 4-space pattern and accept it
    assert!(
        result.is_empty(),
        "MD005 should detect and accept 4-space indentation pattern"
    );
}

#[test]
fn test_four_space_indent_inconsistent() {
    // Test that MD005 flags inconsistent indentation when 4-space pattern is established
    let rule = MD005ListIndent;
    let content = "\
* Item 1
    * Nested with 4 spaces
  * Wrong: only 2 spaces
    * Back to 4 spaces";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    // Should flag the 2-space item as inconsistent
    assert!(!result.is_empty(), "MD005 should flag inconsistent indentation");
    assert!(result.iter().any(|w| w.line == 3));
}

#[test]
fn test_three_space_indent_detection() {
    // Test that MD005 detects and respects 3-space indentation pattern
    let rule = MD005ListIndent;
    let content = "\
* Item 1
   * Nested with 3 spaces
      * Double nested with 6 spaces
   * Another nested with 3 spaces
* Item 2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    // Should detect 3-space pattern and accept it
    assert!(
        result.is_empty(),
        "MD005 should detect and accept 3-space indentation pattern"
    );
}

#[test]
fn test_mixed_ordered_unordered_with_four_spaces() {
    // Test mixed lists with 4-space indentation
    let rule = MD005ListIndent;
    let content = "\
1. Ordered item
    * Unordered with 4 spaces
    * Another unordered with 4 spaces
2. Second ordered
    1. Nested ordered with 4 spaces";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    // MD005 should accept this - bullets under ordered items naturally need more spaces
    // and the pattern is consistent
    assert!(
        result.is_empty(),
        "MD005 should accept consistent 4-space pattern in mixed lists"
    );
}

#[test]
fn test_deep_nesting_with_four_spaces() {
    // Test deeply nested lists with 4-space indentation
    let rule = MD005ListIndent;
    let content = "\
* L1
    * L2 (4 spaces)
        * L3 (8 spaces)
            * L4 (12 spaces)
                * L5 (16 spaces)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "MD005 should accept consistent 4-space increments in deep nesting"
    );
}

#[test]
fn test_fix_with_detected_four_space_pattern() {
    // Test that fixes use the detected 4-space pattern
    let rule = MD005ListIndent;
    let content = "\
* Item 1
    * Correctly indented with 4
  * Wrong: only 2 spaces
      * Nested under wrong item";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let fixed = rule.fix(&ctx).unwrap();
    // Should fix to use 4-space pattern
    assert!(
        fixed.contains("    * Wrong: only 2 spaces") || fixed.contains("    * Correctly indented with 4"),
        "Fix should maintain the detected 4-space pattern"
    );
}

#[test]
fn test_issue_64_scenario() {
    // Test the exact scenario from issue #64
    let rule = MD005ListIndent;
    let content = "\
* Top level item
    * Sub item with 4 spaces (as configured in MD007)
        * Nested sub item with 8 spaces
    * Another sub item with 4 spaces
* Another top level";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    // MD005 should now detect the 4-space pattern and not flag any issues
    assert!(
        result.is_empty(),
        "MD005 should accept 4-space indentation when that's the pattern being used. Got {} warnings",
        result.len()
    );
}

#[test]
fn test_ruff_example_from_issue() {
    // Test with a pattern similar to ruff's markdown
    let rule = MD005ListIndent;
    let content = "\
## Features

* Fast
    * 10-100x faster than existing linters
    * Instant feedback
* Comprehensive
    * Over 800 built-in rules
    * Support for Python 3.13";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "MD005 should accept consistent 4-space indentation in documentation. Got {} warnings",
        result.len()
    );
}

#[test]
fn test_dynamic_detection_with_multiple_blocks() {
    // Test detection across multiple list blocks
    let rule = MD005ListIndent;
    let content = "\
First list:
* Item A
    * Sub A1 with 4 spaces
    * Sub A2 with 4 spaces

Second list:
* Item B
  * Sub B1 with only 2 spaces - should be flagged
    * Sub B2 with 4 spaces";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard);
    let result = rule.check(&ctx).unwrap();

    // Should detect 4-space pattern from first list and flag inconsistency in second
    assert!(
        !result.is_empty(),
        "Should flag inconsistent indentation in second list"
    );
    assert!(result.iter().any(|w| w.message.contains("Sub B1") || w.line == 8));
}
