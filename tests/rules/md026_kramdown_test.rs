use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD026NoTrailingPunctuation;

#[test]
fn test_md026_with_kramdown_header_ids() {
    let rule = MD026NoTrailingPunctuation::default();

    // Header with punctuation but also has Kramdown ID - should not trigger
    let content = "# This is a heading. {#custom-id}";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag punctuation when header has Kramdown ID"
    );

    // Header with punctuation and no ID - should trigger
    let content = "# This is a heading.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].message.contains("punctuation"));

    // Multiple headers with IDs
    let content = r#"# First Header. {#first}

## Second Header! {#second}

### Third Header: {#third}

#### Fourth Header; {#fourth}"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not flag any headers with Kramdown IDs");
}

#[test]
fn test_md026_fix_preserves_kramdown_ids() {
    let rule = MD026NoTrailingPunctuation::default();

    // Test that fix preserves the header ID
    let content = "# Header with period. {#my-id}";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    // Since we don't flag headers with IDs, the content should remain unchanged
    assert_eq!(fixed, content);

    // Test fix on header without ID
    let content = "# Header with period.";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Header with period");
}

#[test]
fn test_md026_complex_kramdown_ids() {
    let rule = MD026NoTrailingPunctuation::default();

    // Complex ID patterns
    let content = r#"# Heading. {#id-with-dashes}

## Another. {#id.with.dots}

### Yet Another. {#id_with_underscores}

#### Final. {#id123}"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should handle various ID formats");
}

#[test]
fn test_md026_with_trailing_hashes_and_ids() {
    let rule = MD026NoTrailingPunctuation::default();

    // ATX closed heading with ID
    let content = "# Heading. {#id} #";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should handle closed ATX with Kramdown ID");

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
    let content = "# Heading. {custom-id}";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should flag - not a valid Kramdown ID");

    // Header ID in middle of text
    let content = "# Heading {#id} with more text.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should flag - ID is not at the end");
}
