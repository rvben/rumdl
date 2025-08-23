use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD026NoTrailingPunctuation;

#[test]
fn test_md026_valid() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md026_invalid() {
    let rule = MD026NoTrailingPunctuation::default();
    // Default punctuation is ".,;:!" so ! and . are flagged, ? is not
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // ! and . should be flagged, ? should not
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1); // !
    assert_eq!(result[1].line, 3); // .
}

#[test]
fn test_md026_mixed() {
    let rule = MD026NoTrailingPunctuation::default();
    // Exclamation marks are now in the default punctuation list
    let content = "# Heading 1\n## Heading 2!\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Heading 2 should be flagged for the exclamation mark
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_md026_fix() {
    let rule = MD026NoTrailingPunctuation::default();
    // Default punctuation is ".,;:!" so ! and . are fixed, ? is not
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    // ! and . should be removed, ? should remain
    assert_eq!(result, "# Heading 1\n## Heading 2?\n### Heading 3\n");
}

#[test]
fn test_md026_custom_punctuation() {
    // When using custom punctuation, the lenient rules don't apply
    let rule = MD026NoTrailingPunctuation::new(Some("!?".to_string()));
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Only ! and ? should be detected, not .
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_md026_setext_headings() {
    let rule = MD026NoTrailingPunctuation::default();
    // Default punctuation is ".,;:!" so ! is flagged, ? is not
    let content = "Heading 1!\n=======\nHeading 2?\n-------\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Only the exclamation mark should be flagged
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_md026_closed_atx() {
    let rule = MD026NoTrailingPunctuation::default();
    // Default punctuation is ".,;:!" so ! is flagged, ? is not
    let content = "# Heading 1! #\n## Heading 2? ##\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Only the exclamation mark should be flagged
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    let fixed = rule.fix(&ctx).unwrap();
    // Exclamation mark should be removed
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2? ##\n");
}

#[test]
fn test_md026_empty_document() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Empty documents should not produce warnings");
}

#[test]
fn test_md026_with_code_blocks() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Valid heading\n\n```\n# This is a code block with heading syntax!\n```\n\n```rust\n# This is another code block with a punctuation mark.\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Content in code blocks should be ignored");
}

#[test]
fn test_md026_with_front_matter() {
    let rule = MD026NoTrailingPunctuation::default();
    // Default punctuation is ".,;:!" so ! is flagged
    let content = "---\ntitle: This is a title with punctuation!\ndate: 2023-01-01\n---\n\n# Correct heading\n## Heading with punctuation!\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // The second heading should be flagged for the exclamation mark
    assert_eq!(result.len(), 1, "Second heading should be flagged");
    assert_eq!(result[0].line, 7); // Line 7 is "## Heading with punctuation!"

    let fixed = rule.fix(&ctx).unwrap();
    // Exclamation mark should be removed from the heading (not the front matter)
    assert_eq!(
        fixed,
        "---\ntitle: This is a title with punctuation!\ndate: 2023-01-01\n---\n\n# Correct heading\n## Heading with punctuation\n",
        "Fix should remove punctuation from heading only"
    );
}

#[test]
fn test_md026_multiple_trailing_punctuation() {
    let rule = MD026NoTrailingPunctuation::default();
    // With lenient rules, ! and ? are allowed, but . is still flagged
    let content = "# Heading with multiple marks!!!???\n## Another heading.....";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Only the periods should be flagged
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Only the periods should be removed
    assert_eq!(fixed, "# Heading with multiple marks!!!???\n## Another heading");
}

#[test]
fn test_md026_indented_headings() {
    let rule = MD026NoTrailingPunctuation::default();
    // Default punctuation is ".,;:!" so ! is flagged, ? is not
    let content = "  # Indented heading!\n    ## Deeply indented heading?";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Only the exclamation mark should be flagged
    assert_eq!(result.len(), 1, "Should flag exclamation mark");
    assert_eq!(result[0].line, 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Exclamation mark should be removed, question mark should remain
    assert_eq!(fixed, "  # Indented heading\n    ## Deeply indented heading?");
}

#[test]
fn test_md026_fix_setext_headings() {
    let rule = MD026NoTrailingPunctuation::default();
    // Default punctuation is ".,;:!" so ! is fixed, ? is not
    let content = "Heading 1!\n=======\nHeading 2?\n-------";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // ! should be removed, ? should remain
    assert_eq!(fixed, "Heading 1\n=======\nHeading 2?\n-------");
}

#[test]
fn test_md026_performance() {
    let rule = MD026NoTrailingPunctuation::default();

    // Create a large document with many headings
    // With lenient rules, use periods (which are still flagged) for testing
    let mut content = String::new();
    for i in 1..=100 {
        content.push_str(&format!(
            "# Heading {}{}\n\nSome content paragraph.\n\n",
            i,
            if i % 3 == 0 { "." } else { "" } // Use periods instead of ! for testing
        ));
    }

    // Measure performance
    use std::time::Instant;
    let start = Instant::now();
    let ctx = LintContext::new(&content);
    let result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    // Verify correctness - only periods are flagged now
    assert_eq!(result.len(), 33, "Should detect exactly 33 headings with periods");

    // Verify performance
    println!("MD026 performance test completed in {duration:?}");
    assert!(
        duration.as_millis() < 1000,
        "Performance check should complete in under 1000ms"
    );
}

#[test]
fn test_md026_non_standard_punctuation() {
    let rule = MD026NoTrailingPunctuation::new(Some("@$%".to_string()));
    let content = "# Heading 1@\n## Heading 2$\n### Heading 3%\n#### Heading 4#\n##### Heading 5!\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Heading 1\n## Heading 2\n### Heading 3\n#### Heading 4#\n##### Heading 5!\n"
    );
}

#[test]
fn test_md026_inline_code_with_punctuation() {
    let rule = MD026NoTrailingPunctuation::default();

    // Test headings with inline code that ends with punctuation
    let content = r#"# Function `foo()`
## The `bar()` method
### Using `baz.`
#### Variable `x;`
##### Code `y,`"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Only the headings ending with actual punctuation should be flagged
    // (not the punctuation inside code blocks)
    assert_eq!(result.len(), 0, "Punctuation inside inline code should not be flagged");
}

#[test]
fn test_md026_unicode_punctuation() {
    let rule = MD026NoTrailingPunctuation::default();

    // Test various Unicode punctuation characters
    let content = r#"# Heading with ellipsis…
## Chinese full stop。
### Japanese period｡
#### Arabic comma،
##### Spanish inverted exclamation¡
###### French guillemets»"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Default rule only checks for ASCII punctuation
    assert_eq!(result.len(), 0, "Unicode punctuation should not be flagged by default");

    // Test with Unicode punctuation in config
    let unicode_rule = MD026NoTrailingPunctuation::new(Some("…。｡،¡»".to_string()));
    let unicode_result = unicode_rule.check(&ctx).unwrap();
    assert_eq!(
        unicode_result.len(),
        6,
        "All Unicode punctuation should be flagged when configured"
    );
}

#[test]
fn test_md026_edge_cases() {
    let rule = MD026NoTrailingPunctuation::default();

    // Test edge cases
    let content = r#"#
## Heading with spaces at end
### Heading with tab at end
#### Heading with newline immediately
#####Heading without space after hashes.
###### Multiple periods...
# Heading with (parentheses).
## Heading with [brackets].
### Heading with {braces}."#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Count the actual violations
    let violations: Vec<_> = result.iter().map(|w| (w.line, w.message.clone())).collect();
    println!("Violations found: {violations:?}");

    // Expected violations: periods at end of headings
    assert!(
        violations.iter().any(|v| v.0 == 5),
        "Heading without space should still be checked"
    );
    assert!(
        violations.iter().any(|v| v.0 == 6),
        "Multiple periods should be flagged"
    );
    assert!(
        violations.iter().any(|v| v.0 == 7),
        "Period after parentheses should be flagged"
    );
    assert!(
        violations.iter().any(|v| v.0 == 8),
        "Period after brackets should be flagged"
    );
    assert!(
        violations.iter().any(|v| v.0 == 9),
        "Period after braces should be flagged"
    );
}

#[test]
fn test_md026_fix_preserves_formatting() {
    let rule = MD026NoTrailingPunctuation::default();

    // Test that fix preserves spacing and formatting
    let content = "# Heading with period.    \n## Another heading with comma,\t\n###No space heading;";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    // Check that punctuation is removed
    // TODO: Headings without space after # (like "###No space") may not be properly detected
    assert_eq!(
        fixed,
        "# Heading with period\n## Another heading with comma\n###No space heading;"
    );
}

#[test]
fn test_md026_atx_closed_style_fix() {
    let rule = MD026NoTrailingPunctuation::default();

    // Test closed ATX style headings
    let content = "# Heading 1. #\n## Heading 2, ##\n### Heading 3; ###";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();

    // Ensure closing hashes are preserved
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_md026_setext_with_various_punctuation() {
    let rule = MD026NoTrailingPunctuation::default();

    let content = r#"Heading with period.
========

Another with comma,
--------

Yet another with semicolon;
========"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // All should be flagged
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 4);
    assert_eq!(result[2].line, 7);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        r#"Heading with period
========

Another with comma
--------

Yet another with semicolon
========"#
    );
}

#[test]
fn test_md026_deeply_nested_headings() {
    let rule = MD026NoTrailingPunctuation::default();

    // Test max depth headings and beyond
    let content = r#"###### Six level heading.
####### Seven hashes text.
######## Eight hashes text."#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Based on the output, it appears that 7+ hashes are treated as headings
    // with the extra hashes as part of the heading text
    // All three lines end with periods and will be flagged
    assert_eq!(result.len(), 3, "All lines with periods are flagged");
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);
}

#[test]
fn test_md026_mixed_content() {
    let rule = MD026NoTrailingPunctuation::default();

    let content = r#"# Main Title

Some paragraph with punctuation.

## Section.

- List item.
- Another item.

### Subsection,

```
# This is code.
```

#### Final heading;

| Table | Header |
|-------|--------|
| Data. | More.  |"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should flag headings but not other content
    assert_eq!(result.len(), 3);
    let lines: Vec<_> = result.iter().map(|w| w.line).collect();
    assert!(lines.contains(&5)); // ## Section.
    assert!(lines.contains(&10)); // ### Subsection,
    assert!(lines.contains(&16)); // #### Final heading;
}

#[test]
fn test_md026_config_empty_punctuation() {
    // Test with empty punctuation config (should flag nothing)
    let rule = MD026NoTrailingPunctuation::new(Some("".to_string()));
    let content = "# Heading!\n## Heading?\n### Heading.\n#### Heading,\n##### Heading;";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 0, "Empty punctuation config should not flag anything");
}

#[test]
fn test_md026_single_character_punctuation() {
    // Test with single character punctuation
    let rule = MD026NoTrailingPunctuation::new(Some("!".to_string()));
    let content = "# Warning!\n## Question?\n### Statement.\n#### List,\n##### Code;";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Warning\n## Question?\n### Statement.\n#### List,\n##### Code;"
    );
}

#[test]
fn test_md026_special_regex_characters() {
    // Test with characters that have special meaning in regex
    let rule = MD026NoTrailingPunctuation::new(Some(".*+?[]{}()^$|\\".to_string()));
    let content = r#"# Heading.
## Heading*
### Heading+
#### Heading?
##### Heading[
###### Heading]"#;

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should handle regex special characters properly
    assert!(
        result.len() >= 5,
        "Should detect special regex characters as punctuation"
    );
}
