use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD026NoTrailingPunctuation;

#[test]
fn test_md026_with_kramdown_header_ids() {
    let rule = MD026NoTrailingPunctuation::default();

    // Header with punctuation and custom ID - should still trigger
    let content = "# This is a heading. {#custom-id}";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].message.contains("punctuation"));

    // Header with punctuation and no ID - should trigger
    let content = "# This is a heading.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].message.contains("punctuation"));

    // Multiple headers with IDs - should still flag punctuation
    let content = r#"# First Header. {#first}

## Second Header! {#second}

### Third Header: {#third}

#### Fourth Header; {#fourth}"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4, "Should flag all headers with trailing punctuation");

    // Headers with IDs but no punctuation - should not trigger
    let content = r#"# First Header {#first}

## Second Header {#second}"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not flag headers without punctuation");
}

#[test]
fn test_md026_fix_preserves_kramdown_ids() {
    let rule = MD026NoTrailingPunctuation::default();

    // Test that fix removes punctuation but preserves the header ID
    let content = "# Header with period. {#my-id}";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Header with period {#my-id}");

    // Test fix on header without ID
    let content = "# Header with period.";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Header with period");
}

#[test]
fn test_md026_complex_kramdown_ids() {
    let rule = MD026NoTrailingPunctuation::default();

    // Complex ID patterns - all should trigger due to punctuation
    let content = r#"# Heading. {#id-with-dashes}

## Another. {#id-with-colons:test}

### Yet Another. {#id_with_underscores}

#### Final. {#id123}"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4, "Should flag all headers with punctuation");

    // Test with invalid ID format (dots not allowed) - invalid IDs are not stripped
    // So the heading text is "Another. {#id.with.dots}" which ends with '}' not punctuation
    let content = "## Another. {#id.with.dots}";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Should not flag - invalid ID not stripped, doesn't end with punctuation"
    );
}

#[test]
fn test_md026_with_trailing_hashes_and_ids() {
    let rule = MD026NoTrailingPunctuation::default();

    // ATX closed heading with ID (no punctuation, correct format)
    let content = "# Heading # {#id}";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not flag when no punctuation");

    // ATX closed heading with punctuation and ID
    // Even though the heading has a custom ID, we still flag the trailing punctuation
    let content = "# Heading. # {#id}";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should flag punctuation even with custom ID");

    // ATX closed heading without ID (should trigger)
    let content = "# Heading. #";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_md026_false_positives() {
    let rule = MD026NoTrailingPunctuation::default();

    // Not actually a header ID (missing #)
    // The heading text is "Heading. {custom-id}" which ends with '}' not punctuation
    let content = "# Heading. {custom-id}";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Should not flag - ends with curly brace not punctuation"
    );

    // Header ID in middle of text
    let content = "# Heading {#id} with more text.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should flag - ID is not at the end");
}
