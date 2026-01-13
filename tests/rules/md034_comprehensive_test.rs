//! Comprehensive edge case tests for MD034 (No Bare URLs)
//!
//! These tests cover edge cases not covered by the main test file,
//! focusing on URL boundary detection, special characters, and
//! complex markdown contexts.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD034NoBareUrls;

// =============================================================================
// URL with Special Characters and Encoding
// =============================================================================

/// Test URL-encoded characters in paths
#[test]
fn test_url_encoded_characters() {
    let rule = MD034NoBareUrls;
    let test_cases = [
        (
            "https://example.com/path%20with%20spaces",
            1,
            "<https://example.com/path%20with%20spaces>",
        ),
        (
            "https://example.com/search?q=hello%20world",
            1,
            "<https://example.com/search?q=hello%20world>",
        ),
        (
            "https://example.com/%E4%B8%AD%E6%96%87", // Chinese URL encoded
            1,
            "<https://example.com/%E4%B8%AD%E6%96%87>",
        ),
    ];

    for (content, expected_count, expected_fix) in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), expected_count, "URL encoded: {content}");
        if expected_count > 0 {
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, expected_fix, "Fix for: {content}");
        }
    }
}

/// Test URLs with special query parameters
#[test]
fn test_urls_with_complex_query_strings() {
    let rule = MD034NoBareUrls;
    let test_cases = [
        // Multiple query parameters
        ("https://example.com?a=1&b=2&c=3", 1),
        // Query with special chars
        ("https://example.com?url=https%3A%2F%2Fother.com", 1),
        // Query with array notation
        ("https://example.com?ids[]=1&ids[]=2", 1),
        // Query with JSON-like content
        ("https://example.com?data={\"key\":\"value\"}", 1),
    ];

    for (content, expected_count) in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), expected_count, "Query string URL: {content}");
    }
}

/// Test URLs with fragments containing special characters
#[test]
fn test_urls_with_special_fragments() {
    let rule = MD034NoBareUrls;
    let test_cases = [
        ("https://example.com#section-1", 1),
        ("https://example.com#L123-L456", 1), // GitHub line range
        ("https://example.com#user-content-heading", 1),
        ("https://example.com/page#this.is.a.fragment", 1),
    ];

    for (content, expected_count) in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), expected_count, "Fragment URL: {content}");
    }
}

// =============================================================================
// URLs in Nested Markdown Contexts
// =============================================================================

/// Test URLs in nested blockquotes
#[test]
fn test_urls_in_nested_blockquotes() {
    let rule = MD034NoBareUrls;

    // Single level blockquote
    let content1 = "> Visit https://example.com for info";
    let ctx1 = LintContext::new(content1, MarkdownFlavor::Standard, None);
    let result1 = rule.check(&ctx1).unwrap();
    assert_eq!(result1.len(), 1, "Single blockquote should flag URL");

    // Double nested blockquote
    let content2 = "> > Nested quote with https://example.com URL";
    let ctx2 = LintContext::new(content2, MarkdownFlavor::Standard, None);
    let result2 = rule.check(&ctx2).unwrap();
    assert_eq!(result2.len(), 1, "Nested blockquote should flag URL");

    // Triple nested blockquote
    let content3 = "> > > Deep nesting https://example.com";
    let ctx3 = LintContext::new(content3, MarkdownFlavor::Standard, None);
    let result3 = rule.check(&ctx3).unwrap();
    assert_eq!(result3.len(), 1, "Deep nested blockquote should flag URL");
}

/// Test URLs in various list contexts
#[test]
fn test_urls_in_list_items() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        // Unordered list with different markers
        ("- https://example.com", 1),
        ("* https://example.com", 1),
        ("+ https://example.com", 1),
        // Ordered list
        ("1. https://example.com", 1),
        ("10. https://example.com", 1),
        ("99. https://example.com", 1),
        // Nested list
        ("- Item\n  - Nested https://example.com", 1),
        // List with multiple URLs
        ("- First https://one.com\n- Second https://two.com", 2),
    ];

    for (content, expected_count) in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), expected_count, "List item: {content}");
    }
}

/// Test URLs in bold/italic contexts
#[test]
fn test_urls_with_emphasis() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        // URL after bold text
        ("**Bold** https://example.com", 1),
        // URL before bold text
        ("https://example.com **Bold**", 1),
        // URL after italic text
        ("*Italic* https://example.com", 1),
        // URL in same line as bold
        ("This is **important**: https://example.com", 1),
    ];

    for (content, expected_count) in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), expected_count, "Emphasis context: {content}");
    }
}

/// Test URLs adjacent to closing emphasis markers - should not be flagged as bare URLs
#[test]
fn test_urls_inside_emphasis_in_links() {
    let rule = MD034NoBareUrls;

    // URL inside bold link text - should not be flagged
    let content = "[**https://example.com**](https://example.com)";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "URL inside bold link text should not be flagged: {result:?}"
    );
}

// =============================================================================
// Punctuation Boundary Tests
// =============================================================================

/// Test URLs followed by various punctuation
#[test]
fn test_urls_with_trailing_punctuation() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        // Common sentence-ending punctuation
        ("Visit https://example.com.", 1, "<https://example.com>"),
        ("Visit https://example.com!", 1, "<https://example.com>"),
        ("Visit https://example.com?", 1, "<https://example.com>"),
        ("Visit https://example.com,", 1, "<https://example.com>"),
        ("Visit https://example.com;", 1, "<https://example.com>"),
        // Note: Trailing colon is preserved (could be interpreted as port number prefix)
        // This is intentional behavior to avoid breaking URLs like https://example.com:8080
        ("Visit https://example.com:", 1, "<https://example.com:>"),
        // Multiple punctuation
        ("Visit https://example.com...", 1, "<https://example.com>"),
        ("Visit https://example.com!!", 1, "<https://example.com>"),
    ];

    for (content, expected_count, expected_url) in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), expected_count, "Punctuation: {content}");

        if expected_count > 0 {
            let fixed = rule.fix(&ctx).unwrap();
            assert!(
                fixed.contains(expected_url),
                "Fix should contain {expected_url}: got {fixed}"
            );
        }
    }
}

/// Test URLs at document boundaries
#[test]
fn test_urls_at_document_boundaries() {
    let rule = MD034NoBareUrls;

    // URL at very start of document
    let content1 = "https://example.com is the link";
    let ctx1 = LintContext::new(content1, MarkdownFlavor::Standard, None);
    let result1 = rule.check(&ctx1).unwrap();
    assert_eq!(result1.len(), 1, "URL at start should be flagged");
    assert_eq!(result1[0].column, 1, "Should start at column 1");

    // URL at very end of document (no newline)
    let content2 = "Visit https://example.com";
    let ctx2 = LintContext::new(content2, MarkdownFlavor::Standard, None);
    let result2 = rule.check(&ctx2).unwrap();
    assert_eq!(result2.len(), 1, "URL at end should be flagged");

    // URL as only content
    let content3 = "https://example.com";
    let ctx3 = LintContext::new(content3, MarkdownFlavor::Standard, None);
    let result3 = rule.check(&ctx3).unwrap();
    assert_eq!(result3.len(), 1, "URL as only content should be flagged");
}

// =============================================================================
// Unusual TLDs and Domain Patterns
// =============================================================================

/// Test URLs with unusual but valid TLDs
#[test]
fn test_urls_with_unusual_tlds() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        ("https://example.museum", 1),
        ("https://example.technology", 1),
        ("https://example.photography", 1),
        ("https://example.international", 1),
        ("https://example.co.uk", 1),
        ("https://example.com.au", 1),
        ("https://example.gov.uk", 1),
    ];

    for (content, expected_count) in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), expected_count, "Unusual TLD: {content}");
    }
}

/// Test internationalized domain names (IDN)
#[test]
fn test_internationalized_domain_names() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        // Punycode domains
        ("https://xn--n3h.com", 1), // Emoji domain in punycode
        // Direct Unicode domains
        ("https://exämple.com", 1),
        ("https://例え.jp", 1),  // Japanese
        ("https://مثال.com", 1), // Arabic
    ];

    for (content, expected_count) in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), expected_count, "IDN: {content}");
    }
}

// =============================================================================
// Edge Cases for Skip Contexts
// =============================================================================

/// Test URLs in inline HTML comments
#[test]
fn test_urls_in_inline_html_comments() {
    let rule = MD034NoBareUrls;

    // URL in HTML comment - should not be flagged
    let content = "Text <!-- https://example.com --> more text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "URL in HTML comment should not be flagged: {result:?}"
    );
}

/// Test URLs in multiline HTML comments
#[test]
fn test_urls_in_multiline_html_comments() {
    let rule = MD034NoBareUrls;

    let content = "Text\n<!--\nhttps://example.com\nhttps://another.com\n-->\nMore text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "URLs in multiline HTML comment should not be flagged: {result:?}"
    );
}

/// Test that URL after HTML comment IS flagged
#[test]
fn test_url_after_html_comment_is_flagged() {
    let rule = MD034NoBareUrls;

    let content = "<!-- comment --> https://example.com";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "URL after HTML comment should be flagged");
}

// =============================================================================
// Shortcut Reference Link Patterns
// =============================================================================

/// Test that shortcut reference links are not flagged
#[test]
fn test_shortcut_reference_links_not_flagged() {
    let rule = MD034NoBareUrls;

    // [URL] pattern - user intent is to use reference link
    let content = "[https://example.com]";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Shortcut reference link [URL] should not be flagged: {result:?}"
    );
}

/// Test that collapsed reference links are not flagged
#[test]
fn test_collapsed_reference_links_not_flagged() {
    let rule = MD034NoBareUrls;

    // [URL][] pattern - collapsed reference link
    let content = "[https://example.com][]";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Collapsed reference link [URL][] should not be flagged: {result:?}"
    );
}

// =============================================================================
// Table Context Tests
// =============================================================================

/// Test URLs in table cells
#[test]
fn test_urls_in_table_cells() {
    let rule = MD034NoBareUrls;

    // URL in table cell
    let content = "| Column 1 | Column 2 |\n|----------|----------|\n| https://example.com | text |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "URL in table cell should be flagged");

    // Multiple URLs in different cells
    let content2 = "| https://one.com | https://two.com |\n|-----------------|-----------------|";
    let ctx2 = LintContext::new(content2, MarkdownFlavor::Standard, None);
    let result2 = rule.check(&ctx2).unwrap();
    assert_eq!(result2.len(), 2, "URLs in multiple table cells should be flagged");
}

/// Test URLs in table headers
#[test]
fn test_urls_in_table_headers() {
    let rule = MD034NoBareUrls;

    let content = "| https://example.com | Header 2 |\n|---------------------|----------|\n| data | data |";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "URL in table header should be flagged");
}

// =============================================================================
// Empty and Whitespace Content
// =============================================================================

/// Test empty content handling
#[test]
fn test_empty_content() {
    let rule = MD034NoBareUrls;

    let content = "";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Empty content should produce no warnings");
}

/// Test whitespace-only content
#[test]
fn test_whitespace_only_content() {
    let rule = MD034NoBareUrls;

    let content = "   \n\n   \t\t\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Whitespace-only content should produce no warnings");
}

/// Test content without any URL-like patterns
#[test]
fn test_content_without_urls() {
    let rule = MD034NoBareUrls;

    let content = "# Heading\n\nThis is a paragraph without any URLs.\n\n- List item\n- Another item";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Content without URLs should produce no warnings");
}

// =============================================================================
// Very Long URLs
// =============================================================================

/// Test very long URLs
#[test]
fn test_very_long_urls() {
    let rule = MD034NoBareUrls;

    // URL with many path segments
    let long_path = (0..20).map(|i| format!("segment{i}")).collect::<Vec<_>>().join("/");
    let content = format!("https://example.com/{long_path}");
    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Very long URL should be flagged");

    // URL with very long query string
    let long_query = (0..50)
        .map(|i| format!("param{i}=value{i}"))
        .collect::<Vec<_>>()
        .join("&");
    let content2 = format!("https://example.com?{long_query}");
    let ctx2 = LintContext::new(&content2, MarkdownFlavor::Standard, None);
    let result2 = rule.check(&ctx2).unwrap();
    assert_eq!(result2.len(), 1, "URL with long query should be flagged");
}

// =============================================================================
// URLs with Credentials (complex behavior)
// =============================================================================

/// Test URLs with username in host
/// Note: MD034 detects both the URL and the email-like pattern (user@example.com)
/// This is expected behavior since both patterns appear in the content
#[test]
fn test_urls_with_credentials() {
    let rule = MD034NoBareUrls;

    // URL with username (deprecated but valid)
    // Both the full URL AND the email-like part are detected
    let content = "https://user@example.com/path";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // The rule may detect 1-2 warnings: the URL and/or the email-like pattern
    // This documents that the rule handles this without panicking
    assert!(!result.is_empty(), "URL with credentials should be detected");
}

// =============================================================================
// Protocol-relative URLs
// =============================================================================

/// Test protocol-relative URLs (//example.com)
#[test]
fn test_protocol_relative_urls_not_flagged() {
    let rule = MD034NoBareUrls;

    // Protocol-relative URLs are not http/https/ftp URLs
    // They should not be flagged by MD034
    let content = "//example.com/path";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Protocol-relative URLs should not be flagged as bare URLs: {result:?}"
    );
}

// =============================================================================
// Custom Protocol Exclusions
// =============================================================================

/// Test that custom protocols are not flagged
/// Note: Some custom protocol URLs may contain email-like patterns (e.g., git@github.com)
/// which will be detected separately. This test excludes those cases.
#[test]
fn test_custom_protocols_not_flagged() {
    let rule = MD034NoBareUrls;

    // Custom protocols without email-like patterns
    let test_cases = [
        "grpc://example.com:50051",
        "ws://example.com/socket",
        "wss://example.com/socket",
        "git://github.com/user/repo.git",
        "vscode://file/path/to/file",
        "slack://channel?id=123",
        "discord://invite/abc123",
        "redis://localhost:6379",
        "mongodb://localhost:27017/db",
        "postgresql://localhost:5432/db",
        "mysql://localhost:3306/db",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Custom protocol {content} should not be flagged: {result:?}"
        );
    }
}

/// Test that custom protocols with embedded email-like patterns are handled
/// The email pattern (e.g., git@github.com) is detected separately from the protocol
#[test]
fn test_custom_protocols_with_email_patterns() {
    let rule = MD034NoBareUrls;

    // ssh:// URLs often contain user@host patterns
    let content = "ssh://git@github.com/repo.git";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // The email-like pattern git@github.com may be detected
    // This is documenting actual behavior - the ssh protocol itself is excluded
    // but the embedded email pattern may still be flagged
    assert!(
        result.is_empty() || result.iter().all(|w| w.message.contains("Email")),
        "ssh:// URL should only flag email pattern if anything: {result:?}"
    );
}

// =============================================================================
// Fix Correctness Tests
// =============================================================================

/// Test that fix produces valid markdown
#[test]
fn test_fix_produces_valid_markdown() {
    let rule = MD034NoBareUrls;

    let content = "Visit https://example.com for more info.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Visit <https://example.com> for more info.");

    // Verify the fix itself doesn't produce new warnings
    let ctx_fixed = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
    let result_fixed = rule.check(&ctx_fixed).unwrap();
    assert!(result_fixed.is_empty(), "Fixed content should have no warnings");
}

/// Test fix with multiple URLs on same line
#[test]
fn test_fix_multiple_urls_same_line() {
    let rule = MD034NoBareUrls;

    let content = "Visit https://one.com and https://two.com today";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Visit <https://one.com> and <https://two.com> today");

    // Verify no warnings after fix
    let ctx_fixed = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
    let result_fixed = rule.check(&ctx_fixed).unwrap();
    assert!(result_fixed.is_empty());
}

/// Test fix preserves surrounding markdown structure
#[test]
fn test_fix_preserves_markdown_structure() {
    let rule = MD034NoBareUrls;

    let content = "# Heading\n\n> Blockquote with https://example.com\n\n- List item https://test.com\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Verify structure is preserved
    assert!(fixed.starts_with("# Heading\n"));
    assert!(fixed.contains("> Blockquote with <https://example.com>"));
    assert!(fixed.contains("- List item <https://test.com>"));
}
