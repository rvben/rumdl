//! Tests for MD034 handling of URLs with parentheses (Issue #240)
//!
//! Wikipedia-style URLs contain parentheses in the path, e.g.,
//! https://en.wikipedia.org/wiki/Rust_(programming_language)
//!
//! These should be detected and fixed correctly.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD034NoBareUrls;

/// Test that Wikipedia-style URLs with parentheses are detected fully
#[test]
fn test_wikipedia_url_with_parentheses_detected() {
    let content = "https://en.wikipedia.org/wiki/Rust_(programming_language)\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = MD034NoBareUrls;

    let warnings = rule.check(&ctx).unwrap();

    assert_eq!(warnings.len(), 1, "Should detect one bare URL");
    let warning = &warnings[0];

    // The full URL including parentheses should be detected
    assert!(
        warning.message.contains("Rust_(programming_language)"),
        "URL in warning should include parentheses: {}",
        warning.message
    );
}

/// Test that Wikipedia-style URLs are fixed correctly
#[test]
fn test_wikipedia_url_with_parentheses_fixed() {
    let content = "https://en.wikipedia.org/wiki/Rust_(programming_language)\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = MD034NoBareUrls;

    let fixed = rule.fix(&ctx).unwrap();

    assert_eq!(
        fixed.trim(),
        "<https://en.wikipedia.org/wiki/Rust_(programming_language)>",
        "Fixed URL should have angle brackets around the full URL"
    );
}

/// Test that balanced parentheses in URL path are preserved
#[test]
fn test_balanced_parentheses_in_url_path() {
    let content = "https://example.com/path_(foo)_(bar)\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = MD034NoBareUrls;

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].message.contains("path_(foo)_(bar)"),
        "URL should include all balanced parentheses"
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed.trim(),
        "<https://example.com/path_(foo)_(bar)>",
        "Fixed URL should preserve balanced parentheses"
    );
}

/// Test that sentence parentheses after URL are NOT included
#[test]
fn test_sentence_parentheses_after_url_excluded() {
    let content = "Check https://example.com (it's great)\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = MD034NoBareUrls;

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);

    // The parenthetical comment should NOT be part of the URL
    assert!(
        !warnings[0].message.contains("(it's great)"),
        "Sentence parentheses should not be part of URL: {}",
        warnings[0].message
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("<https://example.com>"),
        "Fixed should have angle brackets around just the URL"
    );
    assert!(
        fixed.contains("(it's great)"),
        "Sentence text should be preserved outside the brackets"
    );
}

/// Test that URL inside parentheses has surrounding parens excluded
#[test]
fn test_url_inside_parentheses() {
    let content = "See (https://example.com) for more\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = MD034NoBareUrls;

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed.trim(),
        "See (<https://example.com>) for more",
        "Surrounding parentheses should be preserved outside angle brackets"
    );
}

/// Test that unbalanced trailing parenthesis is excluded
#[test]
fn test_unbalanced_trailing_paren_excluded() {
    let content = "https://example.com)\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = MD034NoBareUrls;

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);

    // Unbalanced ) should not be part of URL
    assert!(
        !warnings[0].message.contains("example.com)"),
        "Unbalanced paren should not be in URL: {}",
        warnings[0].message
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("<https://example.com>)"),
        "Unbalanced paren should remain outside: {fixed}",
    );
}

/// Test that URLs with multi-byte characters and unbalanced parentheses don't panic
/// This tests the fix for a panic when char indices vs byte indices were confused
#[test]
fn test_multibyte_url_with_unbalanced_paren() {
    // Chinese Wikipedia URL with closing paren - this used to panic
    let content = "https://zh.wikipedia.org/wiki/百分号编码)\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = MD034NoBareUrls;

    // Should not panic and should detect the URL correctly
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);

    // The unbalanced paren should be excluded from the URL
    assert!(
        !warnings[0].message.contains("编码)"),
        "Unbalanced paren should not be in URL: {}",
        warnings[0].message
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("<https://zh.wikipedia.org/wiki/百分号编码>)"),
        "Fixed URL should have angle brackets, paren outside: {fixed}"
    );
}

/// Test that balanced parentheses in multi-byte URLs are preserved
#[test]
fn test_multibyte_url_with_balanced_parens() {
    // URL with Chinese characters AND balanced parentheses in path
    let content = "https://example.com/路径_(测试)\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let rule = MD034NoBareUrls;

    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].message.contains("路径_(测试)"),
        "URL should include balanced parentheses with multi-byte chars"
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed.trim(),
        "<https://example.com/路径_(测试)>",
        "Fixed URL should preserve balanced parentheses"
    );
}
