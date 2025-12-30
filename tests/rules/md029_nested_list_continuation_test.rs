/// Tests for MD029 nested list continuation (Issue #107)
/// Ensures MD029 correctly handles nested lists and complex continuation content
/// within list items according to CommonMark specification.
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD029OrderedListPrefix;

#[test]
fn test_md029_nested_bullets_continue_list() {
    // Test that nested bullets within a list item don't break list continuity
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

2. Second item:
   - Nested bullet 1
   - Nested bullet 2

3. Third item

4. Fourth item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should NOT report any MD029 errors - nested bullets are properly indented continuation
    assert_eq!(
        warnings.len(),
        0,
        "Should not report MD029 errors when nested bullets are properly indented (3+ spaces)"
    );
}

#[test]
fn test_md029_nested_bullets_with_code_block() {
    // Test nested bullets + code block within list item (Issue #107 scenario)
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. Install WSL, reboot

2. Install distro:
   - Install WSL, reboot
   - Install distro (I use Debian)
   - Configure distro (Create user account, etc.)

   Get into the distro, then:

   ```bash
   sudo apt-get update
   ```

3. Install cuda-toolkit
   NOTE: Important note

4. Final step"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should NOT report any MD029 errors - all content is properly indented
    assert_eq!(
        warnings.len(),
        0,
        "Should not report MD029 errors for nested bullets and code blocks in list continuation"
    );
}

#[test]
fn test_md029_under_indented_bullets_break_list() {
    // Under-indented bullets (2 spaces instead of 3) break list continuity per CommonMark.
    // Verified with both pulldown-cmark and markdownlint-cli.
    // Item "3." starts a new list according to CommonMark, and is correctly numbered.
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

2. Second item:
  - Bullet with 2 spaces (starts new list)

3. Third item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // No warnings - CommonMark parses the second list as starting at 3,
    // so "3. Third item" is correctly numbered for its list
    assert!(
        warnings.is_empty(),
        "Item 3 is correctly numbered for its list (which starts at 3)"
    );
}

#[test]
fn test_md029_nested_ordered_list_continues() {
    // Test that nested ordered lists within list items don't break continuity
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

2. Second item with nested ordered list:
   1. Nested item A
   2. Nested item B

3. Third item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should NOT report MD029 errors for parent list
    // (May report for nested list if it has issues, but parent should continue)
    let parent_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.line == 7) // Line 7 is "3. Third item"
        .collect();

    assert_eq!(
        parent_warnings.len(),
        0,
        "Parent list should continue correctly with nested ordered lists"
    );
}

#[test]
fn test_md029_complex_continuation_content() {
    // Test complex continuation: text + bullets + code block
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

2. Second item with complex content:

   Some paragraph text here.

   - Bullet 1
   - Bullet 2

   More text.

   ```bash
   echo "code block"
   ```

   Final paragraph.

3. Third item

4. Fourth item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should NOT report any MD029 errors - all content properly indented
    assert_eq!(
        warnings.len(),
        0,
        "Should handle complex continuation content (text + bullets + code)"
    );
}

#[test]
fn test_md029_unindented_text_breaks_list() {
    // Test that unindented text between list items breaks continuity
    // CommonMark parses item "3." as start of a new list, correctly numbered at 3
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

2. Second item

Unindented paragraph breaks the list.

3. Third item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // No warnings - CommonMark parses the second list as starting at 3,
    // so "3. Third item" is correctly numbered for its list
    assert!(
        warnings.is_empty(),
        "Item 3 is correctly numbered for its list (which starts at 3)"
    );
}

#[test]
fn test_md029_wider_marker_with_nested_list() {
    // Test that "10. " handles nested list items
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

10. Item with wide marker:
    - Nested bullet (4 spaces)

11. This item continues"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should report 2 errors:
    // 1. "10." should be "2." (wrong initial number)
    // 2. "11." should be "3." (continues from "10.")
    // Note: Nested bullets are list items and allow continuation
    assert_eq!(warnings.len(), 2, "Should report numbering errors");

    assert_eq!(warnings[0].line, 3);
    assert!(warnings[0].message.contains("expected 2"));

    assert_eq!(warnings[1].line, 6);
    assert!(warnings[1].message.contains("expected 3"));
}

#[test]
fn test_md029_wider_marker_with_under_indented_bullet() {
    // Test that "10. " with a 3-space indented bullet breaks the list
    // (3 spaces is insufficient for "10. " which requires 4+ spaces for continuation)
    // CommonMark parses this as two lists: [1, 10] and [11]
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

10. Item with wide marker:
   - Bullet with 3 spaces (breaks the list)

11. This item continues"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should report 1 error:
    // "10." should be "2." (wrong number in first list)
    // "11." is correctly numbered for its list (which starts at 11)
    assert_eq!(warnings.len(), 1, "Should report numbering error for 10");

    assert_eq!(warnings[0].line, 3); // "10." should be "2."
    assert!(warnings[0].message.contains("expected 2"));
    assert!(warnings[0].fix.is_some(), "Normal error should have fix");
}

#[test]
fn test_md029_multiple_nested_levels() {
    // Test multiple levels of nesting within a single list item
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

2. Second item with deep nesting:
   - Level 1 bullet
     - Level 2 bullet (6 spaces)
       - Level 3 bullet (9 spaces)

   Back to level 1 text

3. Third item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Should NOT report MD029 errors - all nesting properly indented
    assert_eq!(
        warnings.len(),
        0,
        "Should handle multiple nested levels within list item"
    );
}

#[test]
fn test_md029_fix_renumbers_correctly_after_nested_content() {
    // Test that the fix correctly renumbers items after nested content
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

2. Second item:
   - Nested bullet

1. Wrong number (should be 3)

2. Wrong number (should be 4)"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert_eq!(warnings.len(), 2, "Should detect 2 numbering errors");

    // Check first error
    assert_eq!(warnings[0].line, 6);
    assert!(warnings[0].message.contains("1") && warnings[0].message.contains("expected 3"));

    // Check second error
    assert_eq!(warnings[1].line, 8);
    assert!(warnings[1].message.contains("2") && warnings[1].message.contains("expected 4"));

    // Test the fix
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("3. Wrong number"));
    assert!(fixed.contains("4. Wrong number"));
}

#[test]
fn test_md029_commonmark_example_248() {
    // Based on CommonMark spec example 248: list items with indented code
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. First item

2. Second item

       indented code

3. Third item"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Indented code (4+ spaces from margin = 7+ from "2. ") continues list
    assert_eq!(
        warnings.len(),
        0,
        "Should follow CommonMark: indented code continues list"
    );
}

#[test]
fn test_md029_lazy_continuation_is_valid_commonmark() {
    // Lazy continuation is valid CommonMark and should NOT break list continuity.
    // Using pulldown-cmark as the authoritative source for list membership means
    // we correctly handle lazy continuation as part of the list item.
    // Verified against markdownlint-cli which also treats this as one list.
    let rule = MD029OrderedListPrefix::default();
    let content = r#"1. Item one

2. Item two
Lazy continuation (not indented)

3. Item three"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    // Lazy continuation should NOT break list - pulldown-cmark correctly parses it as one list
    let numbering_errors: Vec<_> = warnings
        .iter()
        .filter(|w| w.rule_name.as_deref() == Some("MD029"))
        .collect();

    assert_eq!(
        numbering_errors.len(),
        0,
        "Lazy continuation should not break list continuity - this is valid CommonMark"
    );
}
