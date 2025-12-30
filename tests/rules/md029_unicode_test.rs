use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD029OrderedListPrefix;

#[test]
fn test_unicode_ordered_list_valid() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. First item with Unicode café
2. Second item with emoji 🔥
3. Third item with 汉字
4. Fourth item with こんにちは
5. Fifth item with Arabic مرحبا";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Valid Unicode ordered list items should not trigger warnings"
    );
}

#[test]
fn test_unicode_ordered_list_invalid() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. First item with Unicode café
3. Wrong number with emoji 🔥
5. Another wrong number with 汉字";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        2,
        "Unicode ordered list with wrong numbering should trigger warnings"
    );
    assert_eq!(result[0].line, 2);
    assert_eq!(result[1].line, 3);
}

#[test]
fn test_unicode_nested_ordered_lists() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. First level with 汉字 café 🔥
   1. Nested item with مرحبا
   2. Another nested with こんにちは
2. Back to first level with ñáéíóú
   1. New nested section with русский
   2. Final nested with עברית";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Unicode nested ordered lists with correct numbering should not trigger warnings"
    );
}

#[test]
fn test_unicode_nested_ordered_lists_invalid() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. First level with 汉字 café 🔥
   2. Wrong nested number with مرحبا
   3. Another wrong with こんにちは
3. Wrong first level with ñáéíóú";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // pulldown-cmark sees this as ONE top-level list with items at lines 1 and 4.
    // Lines 2-3 (with "2." and "3.") are NOT nested list items - they're continuation
    // text because 3-space indent is insufficient for a nested list.
    // The only error is "3." at line 4 should be "2.".
    // Verified with: npx markdownlint-cli -c '{"MD029": {"style": "ordered"}}' file.md
    assert_eq!(
        result.len(),
        1,
        "Only top-level list item '3.' should trigger warning (should be 2)"
    );
    assert_eq!(result[0].line, 4);
}

#[test]
fn test_unicode_with_code_blocks() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. First item with Unicode café
```
Code block with Unicode: 汉字
console.log('emoji 🔥');
```
2. Item after code block with こんにちは
3. Final item with مرحبا";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Code block at column 0 breaks the list. pulldown-cmark sees 2 lists:
    // - List 1: [1] at line 1 - correct
    // - List 2: [2, 3] at lines 6 and 7 - starts at 2, both correct
    // With CommonMark start value support, no warnings.
    assert!(
        result.is_empty(),
        "No warnings - each list correctly numbered from its start value"
    );
}

#[test]
fn test_unicode_fix_functionality() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. First item with Unicode café
3. Wrong number with emoji 🔥
5. Another wrong with 汉字";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, "1. First item with Unicode café\n2. Wrong number with emoji 🔥\n3. Another wrong with 汉字",
        "Fix should properly handle Unicode characters and correct numbering"
    );
}

#[test]
fn test_unicode_one_style() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::One);
    let content = "\
1. First item with Unicode café
2. Should be 1 with emoji 🔥
3. Should also be 1 with 汉字";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        2,
        "Unicode ordered list with One style should trigger warnings for non-1 numbers"
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, "1. First item with Unicode café\n1. Should be 1 with emoji 🔥\n1. Should also be 1 with 汉字",
        "Fix should properly handle Unicode characters with One style"
    );
}

#[test]
fn test_unicode_in_blockquotes() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
> Ordered list in blockquote with Unicode:
> 1. Item with café
> 2. Item with 汉字
> 3. Item with 🔥
> 4. Item with こんにちは";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Unicode ordered lists in blockquotes should not trigger warnings"
    );
}

#[test]
fn test_unicode_with_continuation_text() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. Item with Unicode café
   This is continuation text with 汉字
   More continuation with emoji 🔥
2. Another item with こんにちは
   Continuation with مرحبا
3. Final item with ñáéíóú";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Unicode ordered lists with continuation text should not trigger warnings"
    );
}

#[test]
fn test_unicode_complex_mixed_content() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. First item with mixed Unicode: 汉字 café 🔥

   Some paragraph text with مرحبا こんにちは

   ```
   Code block content
   ```

2. Second item after complex content with ñáéíóú
3. Third item with Russian русский and Hebrew עברית";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Unicode ordered lists with complex mixed content should maintain correct numbering"
    );
}

#[test]
fn test_unicode_edge_cases() {
    let rule = MD029OrderedListPrefix::default();

    // Test with Unicode that might affect character counting
    let content = "\
1. Unicode combining chars: é (é vs e + ´)
2. Emoji sequences: 👨‍👩‍👧‍👦 (family emoji)
3. Arabic with diacritics: مَرْحَبًا
4. Unicode whitespace should not affect numbering";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Complex Unicode edge cases should not affect numbering detection"
    );
}

#[test]
fn test_unicode_rtl_content() {
    let rule = MD029OrderedListPrefix::default();
    let content = "\
1. Hebrew text: שלום עולם
2. Arabic text: مرحبا بالعالم
3. Mixed RTL and LTR: Hello שלום مرحبا World";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Right-to-left Unicode text should not affect numbering detection"
    );
}

#[test]
fn test_unicode_ordered0_style() {
    let rule = MD029OrderedListPrefix::new(rumdl_lib::rules::ListStyle::Ordered0);
    let content = "\
0. First item (zero-based) with Unicode café
1. Second item with emoji 🔥
2. Third item with 汉字";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Unicode ordered list with Ordered0 style should not trigger warnings"
    );

    // Test invalid case
    let invalid_content = "\
1. Wrong start with Unicode café
2. Second item with emoji 🔥";
    let ctx = LintContext::new(invalid_content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        2,
        "Unicode ordered list with wrong Ordered0 start should trigger warnings"
    );
}
