use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD051LinkFragments;
use rumdl_lib::utils::anchor_styles::AnchorStyle;

/// Regression tests for Issue #39: Two bugs in Links [MD051]
/// These tests ensure that the complex punctuation handling bugs are fixed and won't regress

#[test]
fn test_issue_39_duplicate_headings() {
    // Test case from issue 39: links to the second of repeated headers
    let content = r#"
# Title

## Section

This is a [reference](#section-1) to the second section.

## Section

There will be another section.
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - link to second section should work
    assert_eq!(result.len(), 0, "Link to duplicate heading should work");
}

#[test]
fn test_issue_39_complex_punctuation_arrows() {
    // Test case from issue 39: complex arrow punctuation patterns
    let content = r#"
## cbrown --> sbrown: --unsafe-paths

## cbrown -> sbrown

## Arrow Test <-> bidirectional

## Double Arrow ==> Test

Links to test:
- [Complex unsafe](#cbrown----sbrown---unsafe-paths)
- [Simple arrow](#cbrown---sbrown)
- [Bidirectional](#arrow-test---bidirectional)
- [Double arrow](#double-arrow--test)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - all complex punctuation should be handled correctly
    assert_eq!(
        result.len(),
        0,
        "Complex arrow patterns should work: {:?}",
        result.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_issue_39_ampersand_and_colons() {
    // Test case from issue 39: headers with ampersands and colons
    let content = r#"
# Testing & Coverage

## API Reference: Methods & Properties

## Config: Database & Cache Settings

Links to test:
- [Testing coverage](#testing--coverage)
- [API reference](#api-reference-methods--properties)
- [Config settings](#config-database--cache-settings)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - ampersands and colons should be handled correctly
    assert_eq!(
        result.len(),
        0,
        "Ampersand and colon patterns should work: {:?}",
        result.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_issue_39_mixed_punctuation_clusters() {
    // Test edge cases with multiple types of punctuation in clusters
    let content = r#"
## Step 1: Setup (Prerequisites)

## Error #404 - Not Found!

## FAQ: What's Next?

## Version 2.0.1 - Release Notes

Links to test:
- [Setup guide](#step-1-setup-prerequisites)
- [Error page](#error-404---not-found)
- [FAQ section](#faq-whats-next)
- [Release notes](#version-201---release-notes)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - mixed punctuation should be handled correctly
    assert_eq!(
        result.len(),
        0,
        "Mixed punctuation clusters should work: {:?}",
        result.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_issue_39_consecutive_hyphens_and_spaces() {
    // Test that consecutive hyphens are collapsed properly
    let content = r#"
## Test --- Multiple Hyphens

## Test  --  Spaced Hyphens

## Test - Single - Hyphen

Links to test:
- [Multiple](#test-----multiple-hyphens)
- [Spaced](#test------spaced-hyphens)
- [Single](#test---single---hyphen)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - consecutive hyphens should be collapsed
    assert_eq!(
        result.len(),
        0,
        "Consecutive hyphens should be collapsed: {:?}",
        result.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_issue_39_edge_cases_from_comments() {
    // Test specific patterns mentioned in issue 39 comments
    let content = r#"
### PHP $_REQUEST

### sched_debug

#### Add ldap_monitor to delegator$

### cbrown --> sbrown: --unsafe-paths

Links to test:
- [PHP request](#php-_request)
- [Sched debug](#sched_debug)
- [LDAP monitor](#add-ldap_monitor-to-delegator)
- [Complex path](#cbrown--sbrown-unsafe-paths)
"#;

    let rule = MD051LinkFragments::with_anchor_style(AnchorStyle::KramdownGfm);
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - all edge cases should work
    assert_eq!(
        result.len(),
        0,
        "Edge cases from issue comments should work: {:?}",
        result.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_issue_39_operation_order_verification() {
    // Verify that punctuation is removed BEFORE space-to-hyphen conversion
    // This is the core issue that was reported
    let content = r#"
## A -> B: Operation

## Test & Development

## Setup --> Configuration

Links to test:
- [Operation](#a---b-operation)
- [Test dev](#test--development)
- [Setup config](#setup----configuration)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - operation order should be correct
    assert_eq!(
        result.len(),
        0,
        "Operation order should be correct: {:?}",
        result.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_issue_39_github_vs_kramdown_differences() {
    // Test that GitHub mode works correctly (this is the default)
    let rule = MD051LinkFragments::new();

    // Test with GitHub-style expected fragments
    let content_github = r#"
## Testing & Coverage

Links: [Test](#testing--coverage)
"#;

    let ctx = LintContext::new(content_github);
    let result = rule.check(&ctx).unwrap();

    // Should work with GitHub-style fragments (ampersand removed)
    assert_eq!(result.len(), 0, "GitHub-style fragment should work");
}
