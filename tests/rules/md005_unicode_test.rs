use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD005ListIndent;

#[test]
fn test_unicode_list_items_valid() {
    let rule = MD005ListIndent;
    let content = "\
* Item with Unicode café
* Item with emoji 🔥
  * Nested item with 汉字
  * Nested item with こんにちは
* Item with Arabic مرحبا";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Valid Unicode list items with proper indentation should not trigger warnings"
    );
}

#[test]
fn test_unicode_list_items_invalid() {
    let rule = MD005ListIndent;
    let content = "\
* Item with Unicode café
 * Item with emoji 🔥 (wrong indent)
   * Nested item with 汉字 (wrong indent)
  * Another nested with こんにちは (wrong indent)
* Item with Arabic مرحبا";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        3,
        "Unicode list items with incorrect indentation should trigger warnings"
    );

    // Check that we have violations on the expected lines (order may vary)
    let violation_lines: Vec<usize> = result.iter().map(|w| w.line).collect();
    assert!(
        violation_lines.contains(&2),
        "Should have violation on line 2 (1 space instead of 2)"
    );
    assert!(
        violation_lines.contains(&3),
        "Should have violation on line 3 (3 spaces instead of 4)"
    );
    assert!(
        violation_lines.contains(&4),
        "Should have violation on line 4 (2 spaces instead of 4)"
    );
}

#[test]
fn test_unicode_mixed_list_types() {
    let rule = MD005ListIndent;
    let content = "\
* Unicode café item
  1. Ordered item with 汉字
  2. Another ordered with 🔥
* Back to unordered with こんにちは
  - Dash item with مرحبا
  + Plus item with ñáéíóú";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Unicode mixed list types with proper indentation should not trigger warnings"
    );
}

#[test]
fn test_unicode_complex_nesting() {
    let rule = MD005ListIndent;
    let content = "\
* Level 1 with 汉字 café 🔥
  * Level 2 with مرحبا こんにちは
    * Level 3 with ñáéíóú
      * Level 4 with русский
        * Level 5 with עברית";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Deep Unicode nesting with correct indentation should not trigger warnings"
    );
}

#[test]
fn test_unicode_complex_nesting_invalid() {
    let rule = MD005ListIndent;
    let content = "\
* Level 1 with 汉字 café 🔥
   * Level 2 with مرحبا (wrong indent - 3 spaces)
  * Level 2 with こんにちは (correct indent - 2 spaces)
     * Level 3 with ñáéíóú (wrong indent - 5 spaces)
    * Level 3 with русский (correct indent - 4 spaces)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        4,
        "Unicode nesting with inconsistent indentation should trigger warnings"
    );
}

#[test]
fn test_unicode_fix_functionality() {
    let rule = MD005ListIndent;
    let content = "\
* Item with Unicode café
 * Wrong indent with 🔥
   * Also wrong with 汉字";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, "* Item with Unicode café\n  * Wrong indent with 🔥\n    * Also wrong with 汉字",
        "Fix should properly handle Unicode characters and correct indentation"
    );
}

#[test]
fn test_unicode_in_blockquotes() {
    let rule = MD005ListIndent;
    let content = "\
> List in blockquote with Unicode:
> * Item with café
>   * Nested with 汉字
>   * Another nested with 🔥
> * Back to level 1 with こんにちは";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Unicode lists in blockquotes with correct indentation should not trigger warnings"
    );
}

#[test]
fn test_unicode_with_continuation_text() {
    let rule = MD005ListIndent;
    let content = "\
* Item with Unicode café
  This is continuation text with 汉字
  More continuation with emoji 🔥
  * Nested item with こんにちは
    Nested continuation with مرحبا
* Another item with ñáéíóú";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Unicode lists with continuation text should not trigger warnings"
    );
}

#[test]
fn test_unicode_edge_cases() {
    let rule = MD005ListIndent;

    // Test with Unicode that might affect character counting
    let content = "\
* Unicode combining chars: é (é vs e + ´)
  * Nested with emoji: 👨‍👩‍👧‍👦 (family emoji)
  * Arabic with diacritics: مَرْحَبًا
* Unicode whitespace variants should still work";

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Complex Unicode edge cases should not affect indentation detection"
    );
}

#[test]
fn test_unicode_rtl_content() {
    let rule = MD005ListIndent;
    let content = "\
* Hebrew text: שלום עולם
  * Nested Hebrew: עוד טקסט עברי
  * Arabic text: مرحبا بالعالم
* Mixed RTL and LTR: Hello שלום مرحبا";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Right-to-left Unicode text should not affect indentation detection"
    );
}
