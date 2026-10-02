use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD034NoBareUrls;

#[test]
fn test_valid_urls() {
    let rule = MD034NoBareUrls;
    let content = "[Link](https://example.com)\n<https://example.com>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_bare_urls() {
    let rule = MD034NoBareUrls;
    let content = "This is a bare URL: https://example.com/foobar";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Bare URLs should be flagged");
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].rule_name.as_deref(), Some("MD034"));
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "This is a bare URL: <https://example.com/foobar>");
}

#[test]
fn test_multiple_urls() {
    let rule = MD034NoBareUrls;
    let content = "Visit https://example.com and http://another.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Bare URLs should be flagged");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Visit <https://example.com> and <http://another.com>");
}

#[test]
fn test_urls_in_code_block() {
    let rule = MD034NoBareUrls;
    let content = "```
https://example.com
```
https://outside.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Only https://outside.com should be flagged (URL in code block is ignored)
    assert_eq!(result.len(), 1, "Bare URL outside code block should be flagged");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```\nhttps://example.com\n```\n<https://outside.com>");
}

#[test]
fn test_urls_in_inline_code() {
    let rule = MD034NoBareUrls;
    let content = "`https://example.com`\nhttps://outside.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // https://outside.com should be flagged (URL in inline code is ignored)
    assert_eq!(result.len(), 1, "Bare URL outside inline code should be flagged");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "`https://example.com`\n<https://outside.com>");
}

#[test]
fn test_urls_in_markdown_links() {
    let rule = MD034NoBareUrls;
    let content = "[Example](https://example.com)\nhttps://bare.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // https://bare.com should be flagged (URL in markdown link is ignored)
    assert_eq!(result.len(), 1, "Bare URL should be flagged");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[Example](https://example.com)\n<https://bare.com>");
}

#[test]
fn test_ftp_urls() {
    let rule = MD034NoBareUrls;
    let content = "Download from ftp://example.com/file";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Download from <ftp://example.com/file>");
}

#[test]
fn test_complex_urls() {
    let rule = MD034NoBareUrls;
    let content = "Visit https://example.com/path?param=value#fragment";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Bare URL should be flagged");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Visit <https://example.com/path?param=value#fragment>");
}

#[test]
fn test_multiple_protocols() {
    let rule = MD034NoBareUrls;
    let content = "http://example.com\nhttps://secure.com\nftp://files.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3, "All bare URLs should be flagged");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "<http://example.com>\n<https://secure.com>\n<ftp://files.com>");
}

#[test]
fn test_mixed_content() {
    let rule = MD034NoBareUrls;
    let content = "# Heading\nVisit https://example.com\n> Quote with https://another.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Bare URLs should be flagged");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Heading\nVisit <https://example.com>\n> Quote with <https://another.com>"
    );
}

#[test]
fn test_not_urls() {
    let rule = MD034NoBareUrls;
    let content = "Text with example.com and just://something";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_badge_links_not_flagged() {
    let rule = MD034NoBareUrls;
    let content =
        "[![npm version](https://img.shields.io/npm/v/react.svg?style=flat)](https://www.npmjs.com/package/react)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Badge links should not be flagged as bare URLs");
}

#[test]
fn test_multiple_badges_and_links_on_one_line() {
    let rule = MD034NoBareUrls;
    let content = "# [React](https://react.dev/) \
&middot; [![GitHub license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/facebook/react/blob/main/LICENSE) \
[![npm version](https://img.shields.io/npm/v/react.svg?style=flat)](https://www.npmjs.com/package/react) \
[![(Runtime) Build and Test](https://github.com/facebook/react/actions/workflows/runtime_build_and_test.yml/badge.svg)](https://github.com/facebook/react/actions/workflows/runtime_build_and_test.yml) \
[![(Compiler) TypeScript](https://github.com/facebook/react/actions/workflows/compiler_typescript.yml/badge.svg?branch=main)](https://github.com/facebook/react/actions/workflows/compiler_typescript.yml) \
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://legacy.reactjs.org/docs/how-to-contribute.html#your-first-pull-request)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Multiple badges and links on one line should not be flagged as bare URLs"
    );
}

#[test]
fn test_md034_edge_cases() {
    let rule = MD034NoBareUrls;
    let cases = [
        // URL inside inline code - should not be flagged
        ("`https://example.com`", 0),
        // URL inside code block - should not be flagged
        ("```\nhttps://example.com\n```", 0),
        // Malformed URL - should not be flagged
        ("This is not a URL: htp://example.com", 0),
        // Custom scheme - should not be flagged (not http/https/ftp)
        ("custom://example.com", 0),
        // URL with trailing period - should be flagged (period should not be part of URL)
        ("See https://example.com.", 1),
        // URL with space in the middle - the valid part before space should be flagged
        ("https://example .com", 1),
        // URL in blockquote - should be flagged
        ("> https://example.com", 1),
        // URL in list item - should be flagged
        ("- https://example.com", 1),
        // URL with non-ASCII character - should be flagged
        ("https://exämple.com", 1),
        // Valid http URL with non-standard port - should be flagged
        ("http://example.com:8080", 1),
        // Valid URL with query string and fragment - should be flagged
        ("https://example.com/path?query=1#frag", 1),
        // URL with missing scheme - should be flagged (markdownlint flags www.example.com)
        ("www.example.com", 1),
        // URL in table cell - should be flagged
        ("| https://example.com |", 1),
        // URL in heading - should be flagged
        ("# https://example.com", 1),
        // URL in reference definition - should not be flagged
        ("[ref]: https://example.com", 0),
        // URL in markdown image - should not be flagged
        ("![alt](https://example.com/image.png)", 0),
        // URL in markdown link - should not be flagged
        ("[link](https://example.com)", 0),
        // True bare URL with non-standard scheme - should not be flagged (not http/https/ftp)
        ("foo://example.com", 0),
        // True bare URL with typo in scheme - should not be flagged (invalid scheme)
        ("htps://example.com", 0),
        // True bare URL with valid scheme but inside code span - should not be flagged
        ("`http://example.com`", 0),
        // True bare URL with valid scheme - should be flagged
        ("http://example.com", 1),
    ];
    for (content, expected) in cases.iter() {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), *expected, "Failed for content: {content}");
        let fixed = rule.fix(&ctx).unwrap();

        // If we expect warnings, the fix should change the content
        if *expected > 0 {
            assert_ne!(fixed, *content, "Fix should change content with warnings: {content}");
            // The fixed version should have no warnings
            let ctx_fixed = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
            let result_fixed = rule.check(&ctx_fixed).unwrap();
            assert_eq!(result_fixed.len(), 0, "Fixed content should have no warnings: {fixed}");
        } else {
            assert_eq!(
                fixed, *content,
                "Fix should not change content without warnings: {content}"
            );
        }
    }
}

// #[test]
// fn test_performance_md034() {
//     use std::time::Instant;
//     let rule = MD034NoBareUrls;

//     // Generate a large document with a mix of bare URLs, proper links, and code blocks
//     let mut content = String::with_capacity(500_000);

//     // Add a mix of content with URLs in various contexts
//     for i in 0..1000 {
//         // Regular text with bare URLs
//         if i % 5 == 0 {
//             content.push_str(&format!(
//                 "Paragraph {} with a bare URL https://example.com/page{} and some text.\n\n",
//                 i, i
//             ));
//         }
//         // Proper markdown links
//         else if i % 5 == 1 {
//             content.push_str(&format!(
//                 "Paragraph {} with a [proper link](https://example.com/page{}) and some text.\n\n",
//                 i, i
//             ));
//         }
//         // Auto-linked URLs
//         else if i % 5 == 2 {
//             content.push_str(&format!(
//                 "Paragraph {} with an auto-linked <https://example.com/page{}> and some text.\n\n",
//                 i, i
//             ));
//         }
//         // Code blocks with URLs
//         else if i % 5 == 3 {
//             content.push_str(&format!(
//                 "```\ncode block {} with https://example.com/page{} url\n```\n\n",
//                 i, i
//             ));
//         }
//         // Inline code with URLs
//         else {
//             content.push_str(&format!(
//                 "Paragraph {} with `https://example.com/page{}` in code and some text.\n\n",
//                 i, i
//             ));
//         }
//     }

//     // Add a section with multiple URLs on the same line
//     content.push_str("\n## Multiple URLs on same line\n\n");
//     for i in 0..200 {
//         content.push_str(&format!(
//             "Line with multiple bare URLs: https://example1.com/page{} and https://example2.com/page{} and https://example3.com/page{}\n",
//             i, i+1, i+2
//         ));
//     }

//     // Add some content without URLs to test the fast path
//     content.push_str("\n## Content without URLs\n\n");
//     for i in 0..100 {
//         content.push_str(&format!(
//             "This is paragraph {} without any URLs or links.\n\n",
//             i
//         ));
//     }

//     println!("Generated test content of {} bytes", content.len());

//     // Measure performance of check method
//     let start = Instant::now();
//     let ctx = LintContext::new(&content);
//     let result = rule.check(&ctx).unwrap();
//     let check_duration = start.elapsed();

//     // Measure performance of fix method
//     let start = Instant::now();
//     let fixed = rule.fix(&ctx).unwrap();
//     let fix_duration = start.elapsed();

//     println!(
//         "MD034 check duration: {:?} for content length {}",
//         check_duration,
//         content.len()
//     );
//     println!("MD034 fix duration: {:?}", fix_duration);
//     println!("Found {} warnings", result.len());

//     // Verify results
//     assert!(!result.is_empty(), "Should have found bare URLs");
//     assert!(
//         fixed.contains("<https://example.com/page0>"),
//         "Should have fixed bare URLs"
//     );
//     assert!(
//         !fixed.contains("code block 3 with <https://"),
//         "Should not fix URLs in code blocks"
//     );
//     assert!(
//         !fixed.contains("with `<https://"),
//         "Should not fix URLs in inline code"
//     );

//     // Performance assertion - should complete in a reasonable time
//     assert!(
//         check_duration.as_millis() < 150,
//         "Check should complete in under 100ms ({}ms)",
//         check_duration.as_millis()
//     );
//     assert!(
//         fix_duration.as_millis() < 100,
//         "Fix should complete in under 100ms ({}ms)",
//         fix_duration.as_millis()
//     );
// }

// CRITICAL PARITY TESTS: Email Detection Enhancement
// These tests cover the major MD034 improvement that added email detection
// which increased parity by +5 warnings

#[test]
fn test_bare_email_addresses() {
    let rule = MD034NoBareUrls;
    let content = "Contact us at support@example.com or admin@test.org";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Bare email addresses should be flagged as bare URLs");
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 1);

    assert!(
        result[0]
            .message
            .contains("Email address without angle brackets or link formatting")
    );
    assert!(
        result[1]
            .message
            .contains("Email address without angle brackets or link formatting")
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Contact us at <support@example.com> or <admin@test.org>");
}

#[test]
fn test_email_addresses_various_formats() {
    let rule = MD034NoBareUrls;
    let test_cases = [
        ("Email: user@domain.com", 1, "Email: <user@domain.com>"),
        (
            "Complex email: user.name+tag@sub.domain.co.uk",
            1,
            "Complex email: <user.name+tag@sub.domain.co.uk>",
        ),
        (
            "Email with numbers: user123@example123.com",
            1,
            "Email with numbers: <user123@example123.com>",
        ),
        (
            "Email with hyphens: user-name@sub-domain.example-site.org",
            1,
            "Email with hyphens: <user-name@sub-domain.example-site.org>",
        ),
        ("Short TLD: user@example.co", 1, "Short TLD: <user@example.co>"),
        ("Long TLD: user@example.museum", 1, "Long TLD: <user@example.museum>"),
    ];

    for (content, expected_count, expected_fix) in test_cases.iter() {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), *expected_count, "Failed for content: {content}");

        if *expected_count > 0 {
            assert!(
                result.iter().any(|w| w
                    .message
                    .contains("Email address without angle brackets or link formatting")),
                "Email detection failed for: {content}"
            );

            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, *expected_fix, "Fix failed for: {content}");
        }
    }
}

#[test]
fn test_email_exclusions() {
    let rule = MD034NoBareUrls;
    let test_cases = [
        // Emails in markdown links should not be flagged
        ("[Contact](mailto:user@example.com)", 0),
        // Emails in angle brackets (already auto-linked) should not be flagged
        ("<user@example.com>", 0),
        // Emails in code spans should not be flagged
        ("`user@example.com`", 0),
        // Emails in code blocks should not be flagged
        ("```\nuser@example.com\n```", 0),
        // Emails in HTML attributes should not be flagged
        ("<a href=\"mailto:user@example.com\">Contact</a>", 0),
    ];

    for (content, expected_count) in test_cases.iter() {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), *expected_count, "Failed for content: {content}");
    }
}

// CRITICAL PARITY TESTS: Localhost URL Support Enhancement
// These tests cover the major MD034 improvement that added localhost URL detection
// which increased parity by +5 warnings (combined with email detection = +10 total)

#[test]
fn test_localhost_urls() {
    let rule = MD034NoBareUrls;
    let content = "Visit http://localhost:3000 and https://localhost:8080/api";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Localhost URLs should be flagged as bare URLs");
    assert!(
        result
            .iter()
            .any(|w| w.message.contains("URL without angle brackets or link formatting")),
        "Localhost URL detection failed"
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Visit <http://localhost:3000> and <https://localhost:8080/api>");
}

#[test]
fn test_localhost_variations() {
    let rule = MD034NoBareUrls;
    let test_cases = [
        ("http://localhost", 1, "<http://localhost>"),
        ("https://localhost", 1, "<https://localhost>"),
        ("http://localhost:8080", 1, "<http://localhost:8080>"),
        ("https://localhost:3000", 1, "<https://localhost:3000>"),
        ("http://localhost/path", 1, "<http://localhost/path>"),
        ("https://localhost:9090/api/v1", 1, "<https://localhost:9090/api/v1>"),
        ("ftp://localhost", 1, "<ftp://localhost>"), // FTP is also supported
    ];

    for (content, expected_count, expected_fix) in test_cases.iter() {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), *expected_count, "Failed for content: {content}");

        if *expected_count > 0 {
            assert!(
                result
                    .iter()
                    .any(|w| w.message.contains("URL without angle brackets or link formatting")),
                "Localhost/protocol detection failed for: {content}"
            );

            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, *expected_fix, "Fix failed for: {content}");
        }
    }
}

#[test]
fn test_ip_address_urls() {
    let rule = MD034NoBareUrls;
    let content = "Connect to http://127.0.0.1:8080 or https://192.168.1.100";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "IP address URLs should be flagged as bare URLs");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Connect to <http://127.0.0.1:8080> or <https://192.168.1.100>");
}

#[test]
fn test_combined_emails_and_localhost() {
    let rule = MD034NoBareUrls;
    let content = "Contact admin@localhost.com or visit http://localhost:9090\nAlso try user@example.org and https://192.168.1.1:3000";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4, "Should detect both emails and localhost URLs");

    let fixed = rule.fix(&ctx).unwrap();
    let expected = "Contact <admin@localhost.com> or visit <http://localhost:9090>\nAlso try <user@example.org> and <https://192.168.1.1:3000>";
    assert_eq!(fixed, expected);
}

// REGRESSION TESTS: Prevent false positives that were previously fixed

#[test]
fn test_multiline_markdown_links_not_flagged() {
    let rule = MD034NoBareUrls;
    // This is the exact pattern that was causing false positives before the fix
    let content = "Details about each issue type and the issue lifecycle are discussed in the [MLflow Issue\nPolicy](https://github.com/mlflow/mlflow/blob/master/ISSUE_POLICY.md).\n\nAfter you have agreed upon an implementation strategy for your feature\nor patch with an MLflow committer, the next step is to introduce your\nchanges (see [developing\nchanges](https://github.com/mlflow/mlflow/blob/master/CONTRIBUTING.md#developing-and-testing-mlflow))\nas a pull request against the MLflow Repository.";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not flag any URLs since they are all properly formatted as markdown links
    assert!(
        result.is_empty(),
        "Multi-line markdown links should not be flagged as bare URLs. Found {} warnings: {:#?}",
        result.len(),
        result
    );

    // Fix should not change anything since there are no bare URLs
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, content,
        "Fix should not change content with properly formatted multi-line markdown links"
    );
}

#[test]
fn test_issue_48_url_in_link_text() {
    // Issue #48: URL within link text should not be flagged as a bare URL
    let rule = MD034NoBareUrls;
    let content = "Also don't forget that the next time you need to figure out which `datetime` format you need, **[use the strptime tool at https://pym.dev/strptime](https://www.pythonmorsels.com/strptime/)**!";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // The URL https://pym.dev/strptime is part of the link text and should NOT be flagged
    assert!(
        result.is_empty() || result.iter().all(|w| !w.message.contains("URL")),
        "URL within link text should not be flagged as bare URL. Found {} warnings: {:#?}",
        result.len(),
        result
    );
}

#[test]
fn test_issue_47_urls_emails_in_html_attributes() {
    // Issue #47: Email addresses and URLs in HTML attributes should not be flagged
    let rule = MD034NoBareUrls;
    let content = r#"# Example

This is **some text**.

<input type="email" name="fields[email]" id="drip-email" placeholder="email@domain.com">
<input name="fields[url]" value="https://www.example.com">"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Neither the email in placeholder nor the URL in value should be flagged
    assert!(
        result.is_empty(),
        "Emails and URLs within HTML attributes should not be flagged. Found {} warnings: {:#?}",
        result.len(),
        result
    );
}

#[test]
fn test_mixed_multiline_links_and_bare_urls() {
    let rule = MD034NoBareUrls;
    // Test content with both multi-line markdown links (should not be flagged) and bare URLs (should be flagged)
    let content = "This has a [multi-line\nlink](https://github.com/example/repo) which should not be flagged.\n\nBut this bare URL should be flagged: https://bare-url.com\n\nAnd this [another multi-line\nlink with long URL](https://github.com/very/long/repository/path/that/spans/multiple/lines) should also not be flagged.";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the one bare URL
    assert_eq!(
        result.len(),
        1,
        "Should only flag the bare URL, not the multi-line markdown links. Found {} warnings: {:#?}",
        result.len(),
        result
    );

    // Verify the flagged URL is the correct one
    assert!(
        result[0]
            .message
            .contains("URL without angle brackets or link formatting"),
        "Should flag bare URL with correct message"
    );

    // Check that the fix only wraps the bare URL
    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("<https://bare-url.com>"),
        "Should wrap the bare URL in angle brackets"
    );
    assert!(
        fixed.contains("[multi-line\nlink](https://github.com/example/repo)"),
        "Should not modify the multi-line markdown link"
    );
    assert!(
        fixed.contains("[another multi-line\nlink with long URL](https://github.com/very/long/repository/path/that/spans/multiple/lines)"),
        "Should not modify the second multi-line markdown link"
    );
}

#[test]
fn test_issue_104_url_in_empty_link() {
    // Issue #104: URL in link text with empty URL part [url]()
    // This is the pattern from issue #104: [https://github.com/pfeif/hx-complete-generator]()
    // The URL is in the link text with empty URL part
    let rule = MD034NoBareUrls;
    let content = "check it out in its new repository at [https://github.com/pfeif/hx-complete-generator]().";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // The URL in [url]() should NOT be flagged because it's part of a markdown link construct
    // (even though the link is empty/invalid, it's still a link construct that should be handled by MD042)
    assert_eq!(
        result.len(),
        0,
        "URL in [url]() link text should not be flagged as bare URL. This is MD042 territory. Found {} warnings: {:#?}",
        result.len(),
        result
    );
}

#[test]
fn test_issue_104_url_in_empty_bracket_link() {
    // Issue #104: Similar pattern with [url][]
    let rule = MD034NoBareUrls;
    let content = "Visit [https://www.google.com][] for more info.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not be flagged - it's part of a markdown link reference construct
    assert_eq!(
        result.len(),
        0,
        "URL in [url][] should not be flagged as bare URL. Found {} warnings: {:#?}",
        result.len(),
        result
    );
}

#[test]
fn test_issue_104_full_paragraph_not_corrupted() {
    // Issue #104: Full regression test with the actual paragraph from the bug report
    // This tests that after MD042 fixes the empty link, MD034 doesn't corrupt the text
    let rule = MD034NoBareUrls;

    // This is what the content looks like AFTER MD042 has fixed the empty link
    // MD042 now intelligently uses the URL from the text as the destination
    let content_after_md042 = "I've never been one to implement hacky solutions because life is just easier\nwhen everything gets done \"by the book.\" So, if you're reading this and want to\nsee the code that creates this extension and prevents me from pouring needless\nhours into meticulously maintaining the files by hand, I welcome you to check it\nout in its new repository at [https://github.com/pfeif/hx-complete-generator](https://github.com/pfeif/hx-complete-generator).";

    let ctx = LintContext::new(content_after_md042, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // MD034 should NOT flag the URL because it's properly in a markdown link now
    assert_eq!(
        result.len(),
        0,
        "After MD042 fixes empty link, MD034 should not flag the URL. Found {} warnings: {:#?}",
        result.len(),
        result
    );

    // Verify MD034 fix produces exactly the expected output (no modifications)
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, content_after_md042,
        "MD034 should not modify content that has properly formatted links"
    );
}

// Issue #116: URLs in front matter should not be flagged
#[test]
fn test_urls_in_yaml_front_matter() {
    let rule = MD034NoBareUrls;
    let content = "---\nurl: http://example.com\ntitle: Test\n---\n\n# Content";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "URLs in YAML front matter should not be flagged");
}

#[test]
fn test_urls_in_toml_front_matter() {
    let rule = MD034NoBareUrls;
    let content = "+++\nurl = \"http://example.com\"\ntitle = \"Test\"\n+++\n\n# Content";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "URLs in TOML front matter should not be flagged");
}

#[test]
fn test_urls_in_json_front_matter() {
    let rule = MD034NoBareUrls;
    let content = "{\n\"url\": \"http://example.com\",\n\"title\": \"Test\"\n}\n\n# Content";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "URLs in JSON front matter should not be flagged");
}

#[test]
fn test_bare_url_after_front_matter() {
    let rule = MD034NoBareUrls;
    let content = "---\nurl: http://example.com\n---\n\nVisit http://bare-url.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Bare URL after front matter should be flagged");
    assert!(result[0].message.contains("http://bare-url.com"));

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("<http://bare-url.com>"),
        "Bare URL should be wrapped in angle brackets"
    );
    assert!(
        fixed.contains("url: http://example.com"),
        "URL in front matter should remain unchanged"
    );
}

#[test]
fn test_email_in_front_matter() {
    let rule = MD034NoBareUrls;
    let content = "---\nauthor_email: user@example.com\ncontact: admin@test.org\n---\n\n# Content";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Emails in front matter should not be flagged");
}

#[test]
fn test_multiple_urls_in_front_matter() {
    let rule = MD034NoBareUrls;
    let content = "---\nurl: http://example.com\nrepository: https://github.com/user/repo\nwebsite: ftp://files.example.org\n---\n\n# Content";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Multiple URLs in front matter should not be flagged");
}

#[test]
fn test_issue_116_exact_reproduction() {
    // This is the exact test case from issue #116
    let rule = MD034NoBareUrls;
    let content = "---\nurl: http://example.com\n---\n\n# Repro";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Issue #116: URL in front matter should not be flagged"
    );
}

#[test]
fn test_issue_151_urls_in_html_block_attributes() {
    // This is the exact test case from issue #151
    // URLs in HTML tag attributes should not be flagged
    let rule = MD034NoBareUrls;
    let content = r#"<figure>
  <img
    src="https://example.com/test.html"
  />
</figure>"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Issue #151: URL in HTML block attribute should not be flagged"
    );
}

#[test]
fn test_issue_151_single_line_html_tag_with_url() {
    let rule = MD034NoBareUrls;
    let content = r#"<img src="https://example.com/image.png" alt="test" />"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Single-line HTML tag with URL in attribute should not be flagged"
    );
}

#[test]
fn test_issue_151_multiple_urls_in_html_block() {
    let rule = MD034NoBareUrls;
    let content = r#"<div>
  <img src="https://example.com/image1.png" />
  <img src="https://example.com/image2.png" />
  <a href="https://example.com/page.html">Link</a>
</div>"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Multiple URLs in HTML block attributes should not be flagged"
    );
}

#[test]
fn test_issue_151_various_html_tag_types() {
    let rule = MD034NoBareUrls;
    let content = r#"<section>
  <div data-url="https://example.com/api">
    <iframe src="https://example.com/embed.html"></iframe>
  </div>
</section>"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "URLs in various HTML tag types should not be flagged"
    );
}

#[test]
fn test_issue_151_nested_html_blocks_with_urls() {
    let rule = MD034NoBareUrls;
    let content = r#"<article>
  <header>
    <img src="https://example.com/logo.png" />
  </header>
  <div class="content">
    <a href="https://example.com/link1.html">Link 1</a>
    <figure>
      <img src="https://example.com/image.png" />
    </figure>
  </div>
</article>"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Nested HTML blocks with URLs should not be flagged");
}

#[test]
fn test_issue_151_html_block_with_mixed_content() {
    let rule = MD034NoBareUrls;
    let content = r#"<div>
  Some text content
  <img src="https://example.com/image.png" />
  More text
</div>

Outside HTML: https://example.com/should-flag.html"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Only bare URL outside HTML block should be flagged");
    assert_eq!(result[0].line, 7);
}

/// Regression test for issue #178: Multi-byte Unicode characters before code spans
/// caused byte-vs-character position mismatch, leading to false positives
#[test]
fn test_issue_178_unicode_before_inline_code_url() {
    let rule = MD034NoBareUrls;

    // Curly apostrophe (U+2019) is 3 bytes in UTF-8, causing byte offset mismatch
    let content = "- Some code\u{2019}s example `https://example.com` containing a URL";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "URL in inline code after curly apostrophe should NOT be flagged, got {result:?}"
    );

    // Multiple lines with curly apostrophe
    let content2 = "- [Some normal URL](https://example.com)\n- Some code\u{2019}s example `https://example.com` containing an URL\n- Some code\u{2019}s repro example `https://example.com`";
    let ctx2 = LintContext::new(content2, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result2 = rule.check(&ctx2).unwrap();
    assert!(
        result2.is_empty(),
        "URLs in inline code should NOT be flagged, got {result2:?}"
    );
}

/// Test various multi-byte Unicode characters before inline code with URLs
#[test]
fn test_unicode_multibyte_chars_before_inline_code_url() {
    let rule = MD034NoBareUrls;

    // Various multi-byte characters
    let test_cases = [
        ("Left curly quote", "Text \u{2018}quoted\u{2019} `https://example.com`"),
        ("Em dash", "Text\u{2014}dash `https://example.com`"),
        ("Euro sign", "Price 100\u{20AC} `https://example.com`"),
        ("Japanese", "\u{3042}\u{3044}\u{3046} `https://example.com`"),
        ("Emoji", "\u{1F600} happy `https://example.com`"),
        ("Chinese", "\u{4E2D}\u{6587} `https://example.com`"),
    ];

    for (name, content) in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "{name}: URL in inline code after multi-byte chars should NOT be flagged, got {result:?}"
        );
    }
}

#[test]
fn test_reference_definitions_with_titles_not_flagged() {
    let rule = MD034NoBareUrls;

    // Reference definitions should NOT be flagged - they are valid markdown link syntax
    let test_cases = [
        // Basic reference definition without title
        "[example]: https://example.com",
        // Reference definition with double-quoted title
        "[example]: https://example.com \"Title here\"",
        // Reference definition with single-quoted title
        "[example]: https://example.com 'Title here'",
        // Reference definition with parenthesized title
        "[example]: https://example.com (Title here)",
        // Reference with backticks in label
        "[`maturin`]: https://github.com/PyO3/maturin \"Build and publish crates\"",
        // Reference with angle brackets
        "[example]: <https://example.com> \"Title\"",
        // Real-world example from pyo3
        "[feature flags]: https://doc.rust-lang.org/cargo/reference/features.html \"Features - The Cargo Book\"",
        // Multiple reference definitions
        "[ref1]: https://example.com\n[ref2]: https://test.com \"Test title\"",
        // Indented reference definition
        "  [example]: https://example.com \"Indented\"",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Reference definition should NOT be flagged as bare URL:\n{content}\nGot: {result:?}"
        );
    }
}

#[test]
fn test_ref_like_paragraph_with_trailing_prose_still_flags_urls() {
    // A line that only *starts* like a reference definition but has trailing prose
    // is paragraph text in CommonMark, not a definition, so rumdl's parser does not
    // treat it as one and its bare URLs are flagged - inside and outside a blockquote.
    let rule = MD034NoBareUrls;

    for content in [
        "[x]: https://a.example.com and https://b.example.com",
        "> [x]: https://a.example.com and https://b.example.com",
    ] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "ref-like paragraph with trailing prose should flag its bare URLs:\n{content}\nGot: {result:?}"
        );
    }
}

#[test]
fn test_reference_definitions_in_blockquotes_not_flagged() {
    // Issue #674: a link reference definition inside a blockquote is valid
    // CommonMark and must not be flagged as a bare URL.
    let rule = MD034NoBareUrls;

    let test_cases = [
        "> [example]: https://example.com",
        "> [example]: https://example.com \"Title here\"",
        "> [example]: <https://example.com>",
        // Nested blockquote, both compact and spaced syntaxes
        ">> [example]: https://example.com",
        "> > [example]: https://example.com \"Title\"",
        // Indented before the marker
        "  > [example]: https://example.com",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Reference definition in a blockquote should NOT be flagged:\n{content}\nGot: {result:?}"
        );
    }
}

#[test]
fn test_reference_definition_with_escaped_title_delimiter_not_flagged() {
    // A reference-definition title may contain an escaped delimiter; such a line
    // is still a valid definition and must not be flagged.
    let rule = MD034NoBareUrls;

    let test_cases = [
        r#"[x]: https://example.com "a \" quote""#,
        r#"[x]: https://example.com 'a \' quote'"#,
        r#"> [x]: https://example.com "a \" quote""#,
    ];

    for content in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Reference definition with an escaped title delimiter should NOT be flagged:\n{content}\nGot: {result:?}"
        );
    }
}

#[test]
fn test_reference_definition_with_escaped_bracket_label_not_flagged() {
    // Issue #814: a link label ends at the first `]` that is not
    // backslash-escaped, so each of these is a valid definition whose
    // destination must not be reported as a bare URL.
    let rule = MD034NoBareUrls;

    let content = "# Reference Example\n\n\
        * [this is a link to example.com][ref1\\[\\]]\n\
        * [this is also a link to example.com][ref2\\[]\n\
        * [this is also a link to example.com][ref3]\n\n\
        [ref1\\[\\]]: https://example.com/ref1\n\
        [ref2\\[]: https://example.com/ref2\n\
        [ref3]: https://example.com/ref3\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Definitions with escaped brackets in the label should NOT be flagged:\nGot: {result:?}"
    );
}

#[test]
fn test_unterminated_label_is_not_a_reference_definition() {
    // The counterpart bound: in `[a\]: url` the `\]` is escaped, so the label
    // never closes and the line is a paragraph, not a definition. pulldown-cmark
    // agrees (it resolves no reference here), so the destination is a genuine
    // bare URL and MD034 must still report it.
    let rule = MD034NoBareUrls;

    let content = "[a\\]: https://example.com/trailing\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "A label with no unescaped closing bracket is not a definition:\nGot: {result:?}"
    );
}

#[test]
fn test_bare_url_in_blockquote_still_flagged() {
    // The blockquote ref-def exemption must not suppress a genuine bare URL
    // that merely sits inside a blockquote.
    let rule = MD034NoBareUrls;

    let content = "> See https://bare.example.com for details";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "A bare URL inside a blockquote should still be flagged:\nGot: {result:?}"
    );
}

#[test]
fn test_bare_urls_still_flagged_with_reference_definitions() {
    let rule = MD034NoBareUrls;

    // Mix of reference definitions (ok) and bare URLs (should be flagged)
    let content = r#"# Test Document

This has a bare URL: https://bare.example.com

[example]: https://example.com "This is fine"

Another bare URL: https://another.bare.url
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag exactly 2 bare URLs, not the reference definition
    assert_eq!(
        result.len(),
        2,
        "Expected 2 bare URLs flagged, got {}:\n{:?}",
        result.len(),
        result
    );

    // Verify the flagged URLs
    assert!(result[0].message.contains("https://bare.example.com"));
    assert!(result[1].message.contains("https://another.bare.url"));
}

#[test]
fn test_www_urls_without_protocol() {
    let rule = MD034NoBareUrls;

    // www URLs should be detected as bare URLs (matching markdownlint behavior)
    let content = "# Test\n\nVisit www.example.com for info.";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        result.len(),
        1,
        "www URL should be flagged as bare URL. Got: {result:?}"
    );
    assert!(
        result[0].message.contains("www.example.com"),
        "Message should contain the www URL"
    );
}

// =============================================================================
// URL boundary detection tests
// =============================================================================

/// Test that URLs inside markdown links are not flagged (basic case)
#[test]
fn test_url_inside_markdown_link_not_flagged() {
    let rule = MD034NoBareUrls;

    let content = "[Link text](https://example.com)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "URL inside markdown link should NOT be flagged: {result:?}"
    );
}

/// Test URL inside markdown link followed by text
#[test]
fn test_url_inside_markdown_link_with_trailing_text() {
    let rule = MD034NoBareUrls;

    let content = "See [here](https://example.com) for details.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "URL inside markdown link should NOT be flagged even with trailing text: {result:?}"
    );
}

/// Test multiple markdown links on the same line
#[test]
fn test_multiple_markdown_links_same_line() {
    let rule = MD034NoBareUrls;

    let content = "[Link1](https://example.com) and [Link2](https://test.com) are both valid.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Multiple URLs inside markdown links should NOT be flagged: {result:?}"
    );
}

/// Test URL inside image syntax
#[test]
fn test_url_inside_image_not_flagged() {
    let rule = MD034NoBareUrls;

    let content = "![Alt text](https://example.com/image.png)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "URL inside image syntax should NOT be flagged: {result:?}"
    );
}

/// Test URL inside nested parentheses (complex boundary)
#[test]
fn test_url_with_nested_parentheses_in_link() {
    let rule = MD034NoBareUrls;

    // Wikipedia-style URL inside a markdown link
    let content = "[Rust](https://en.wikipedia.org/wiki/Rust_(programming_language))";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "URL with nested parens inside markdown link should NOT be flagged: {result:?}"
    );
}

/// Test that bare URLs outside links ARE still flagged
#[test]
fn test_bare_url_outside_link_still_flagged() {
    let rule = MD034NoBareUrls;

    let content = "Visit https://example.com for more info.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1, "Bare URL outside markdown link SHOULD be flagged");
    assert!(result[0].message.contains("https://example.com"));
}

/// Test mixed: markdown link and bare URL on same line
#[test]
fn test_markdown_link_and_bare_url_same_line() {
    let rule = MD034NoBareUrls;

    let content = "[Good link](https://example.com) but also https://bare.url here";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the bare URL, not the one in the markdown link
    assert_eq!(result.len(), 1, "Should flag only the bare URL, got: {result:?}");
    assert!(
        result[0].message.contains("https://bare.url"),
        "Should flag the bare URL, not the markdown link URL"
    );
}

/// Test URL starting inside link construct (boundary edge case)
#[test]
fn test_url_starting_inside_link_boundary() {
    let rule = MD034NoBareUrls;

    // URL detection might find a URL that extends beyond the link boundary
    // if the link has complex structure. The fix ensures we check if the URL
    // *starts* inside the construct, not if it's entirely contained.
    let content = "[Link](https://example.com/path)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "URL starting inside link should NOT be flagged: {result:?}"
    );
}

/// Test URL in angle brackets (autolink) not flagged
#[test]
fn test_url_in_angle_brackets_not_flagged() {
    let rule = MD034NoBareUrls;

    let content = "Contact us at <https://example.com>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "URL in angle brackets (autolink) should NOT be flagged: {result:?}"
    );
}

/// Test URL in reference definition not flagged
#[test]
fn test_url_in_reference_definition_boundary() {
    let rule = MD034NoBareUrls;

    let content = "[ref]: https://example.com\n\nSee [ref] for details.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "URL in reference definition should NOT be flagged: {result:?}"
    );
}

// =============================================================================
// XMPP URI tests (GFM extended autolinks)
// =============================================================================

/// Test bare XMPP URIs are flagged
#[test]
fn test_bare_xmpp_uri() {
    let rule = MD034NoBareUrls;

    let content = "Contact me at xmpp:user@example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1, "Bare XMPP URI should be flagged");
    assert!(
        result[0].message.contains("xmpp:user@example.com"),
        "Message should contain the XMPP URI"
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Contact me at <xmpp:user@example.com>");
}

/// Test XMPP URI with resource path
#[test]
fn test_xmpp_uri_with_resource() {
    let rule = MD034NoBareUrls;

    let content = "My chat address: xmpp:foo@bar.baz/txt";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1, "Bare XMPP URI with resource should be flagged");
    assert!(
        result[0].message.contains("xmpp:foo@bar.baz/txt"),
        "Message should contain the XMPP URI with resource"
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "My chat address: <xmpp:foo@bar.baz/txt>");
}

/// Test XMPP URI in angle brackets (properly formatted) is not flagged
#[test]
fn test_xmpp_uri_in_angle_brackets() {
    let rule = MD034NoBareUrls;

    let content = "Contact me at <xmpp:user@example.com>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "XMPP URI in angle brackets should NOT be flagged: {result:?}"
    );
}

/// Test XMPP URI in markdown link is not flagged
#[test]
fn test_xmpp_uri_in_markdown_link() {
    let rule = MD034NoBareUrls;

    let content = "[Chat with me](xmpp:user@example.com)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "XMPP URI in markdown link should NOT be flagged: {result:?}"
    );
}

/// Test multiple XMPP URIs
#[test]
fn test_multiple_xmpp_uris() {
    let rule = MD034NoBareUrls;

    let content = "Contact xmpp:alice@example.com or xmpp:bob@example.org/work";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 2, "Both bare XMPP URIs should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Contact <xmpp:alice@example.com> or <xmpp:bob@example.org/work>");
}

/// Test XMPP URI mixed with regular URLs and emails
#[test]
fn test_xmpp_uri_mixed_with_urls_and_emails() {
    let rule = MD034NoBareUrls;

    let content = "Website: https://example.com\nEmail: user@example.com\nXMPP: xmpp:chat@example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 3, "URL, email, and XMPP URI should all be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "Website: <https://example.com>\nEmail: <user@example.com>\nXMPP: <xmpp:chat@example.com>"
    );
}

/// Test XMPP URI in code block is not flagged
#[test]
fn test_xmpp_uri_in_code_block() {
    let rule = MD034NoBareUrls;

    let content = "```\nxmpp:user@example.com\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "XMPP URI in code block should NOT be flagged: {result:?}"
    );
}

/// Test XMPP URI in inline code is not flagged
#[test]
fn test_xmpp_uri_in_inline_code() {
    let rule = MD034NoBareUrls;

    let content = "Use `xmpp:user@example.com` for chat.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "XMPP URI in inline code should NOT be flagged: {result:?}"
    );
}

/// Test XMPP URI variations per GFM spec
#[test]
fn test_xmpp_uri_variations() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        // Basic XMPP URI
        ("xmpp:user@domain.com", 1, "<xmpp:user@domain.com>"),
        // With subdomain
        ("xmpp:chat@chat.example.org", 1, "<xmpp:chat@chat.example.org>"),
        // With resource
        ("xmpp:user@domain.net/mobile", 1, "<xmpp:user@domain.net/mobile>"),
        // With complex resource
        (
            "xmpp:user@domain.com/resource/path",
            1,
            "<xmpp:user@domain.com/resource/path>",
        ),
        // With numbers
        ("xmpp:user123@domain456.com", 1, "<xmpp:user123@domain456.com>"),
        // With dots in username
        ("xmpp:first.last@domain.com", 1, "<xmpp:first.last@domain.com>"),
    ];

    for (content, expected_count, expected_fix) in test_cases.iter() {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), *expected_count, "Failed for XMPP URI: {content}");

        if *expected_count > 0 {
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, *expected_fix, "Fix failed for XMPP URI: {content}");
        }
    }
}

/// Test www URLs with query strings (GFM autolink extension)
#[test]
fn test_www_urls_with_query_string() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        (
            "www.example.com?param=value",
            1,
            "<https://www.example.com?param=value>",
        ),
        ("www.example.com?a=1&b=2", 1, "<https://www.example.com?a=1&b=2>"),
        (
            "www.example.com/path?query=test",
            1,
            "<https://www.example.com/path?query=test>",
        ),
    ];

    for (content, expected_count, expected_fix) in test_cases.iter() {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), *expected_count, "Failed for www URL: {content}");

        if *expected_count > 0 {
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, *expected_fix, "Fix failed for www URL: {content}");
        }
    }
}

/// Test www URLs with fragment identifiers
#[test]
fn test_www_urls_with_fragment() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        ("www.example.com#section", 1, "<https://www.example.com#section>"),
        (
            "www.example.com/page#anchor",
            1,
            "<https://www.example.com/page#anchor>",
        ),
        ("www.example.com?q=1#frag", 1, "<https://www.example.com?q=1#frag>"),
    ];

    for (content, expected_count, expected_fix) in test_cases.iter() {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), *expected_count, "Failed for www URL: {content}");

        if *expected_count > 0 {
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, *expected_fix, "Fix failed for www URL: {content}");
        }
    }
}

/// Test www URLs with port numbers
#[test]
fn test_www_urls_with_port() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        ("www.example.com:8080", 1, "<https://www.example.com:8080>"),
        ("www.example.com:3000/path", 1, "<https://www.example.com:3000/path>"),
        ("www.example.com:443?q=1", 1, "<https://www.example.com:443?q=1>"),
    ];

    for (content, expected_count, expected_fix) in test_cases.iter() {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), *expected_count, "Failed for www URL: {content}");

        if *expected_count > 0 {
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, *expected_fix, "Fix failed for www URL: {content}");
        }
    }
}

/// Test www URLs in context (embedded in sentences)
#[test]
fn test_www_urls_in_context() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        ("Visit www.example.com for more info.", 1),
        ("Check out www.example.com/docs#getting-started today!", 1),
        ("Server at www.internal.example.com:8080/api is ready.", 1),
    ];

    for (content, expected_count) in test_cases.iter() {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), *expected_count, "Failed for: {content}");
    }
}

/// Test www URLs properly formatted (should NOT be flagged)
#[test]
fn test_www_urls_not_flagged_when_formatted() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        "<https://www.example.com>",
        "[link](https://www.example.com)",
        "[www.example.com](https://www.example.com)",
        "`www.example.com`",
    ];

    for content in test_cases.iter() {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Formatted www URL should NOT be flagged: {content}");
    }
}

/// Test mixed www and protocol URLs
#[test]
fn test_www_and_protocol_urls_mixed() {
    let rule = MD034NoBareUrls;

    let content = "Visit www.example.com and https://other.com for info.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Both www and https URLs should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "Visit <https://www.example.com> and <https://other.com> for info."
    );
}

/// Test that multi-byte UTF-8 characters before emails don't cause panics
/// Regression test for kubernetes/website Bengali text issue
#[test]
fn test_email_detection_with_multibyte_utf8() {
    let rule = MD034NoBareUrls;

    // Bengali text followed by email - the email address starts at a byte offset
    // that could land inside a multi-byte character if we subtract 5 naively
    let content = "কুবারনেটিস কমিউনিটির মধ্যে ঘটে যাওয়া ঘটনাগুলির জন্য, conduct@kubernetes.io মাধ্যমে";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // The email should be detected
    assert_eq!(result.len(), 1, "Email should be detected in Bengali text");
    assert!(
        result[0].message.contains("Email address without angle brackets"),
        "Should flag bare email"
    );
}

/// Test various multi-byte UTF-8 edge cases with emails
#[test]
fn test_email_detection_various_scripts() {
    let rule = MD034NoBareUrls;

    let test_cases = [
        // Japanese
        ("日本語テキスト user@example.com 日本語", 1),
        // Chinese
        ("中文文本 user@example.com 更多中文", 1),
        // Arabic
        ("نص عربي user@example.com نص آخر", 1),
        // Emoji
        ("🎉 email@test.com 🎉", 1),
        // Mixed scripts
        ("日本語 中文 العربية user@example.com more", 1),
    ];

    for (content, expected_count) in test_cases.iter() {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            *expected_count,
            "Failed for multi-byte content: {content}"
        );
    }
}

/// Emphasis or strikethrough delimiters wrapped around a bare URL close the
/// emphasis; they are not part of the URL. Wrapping them into the autolink both
/// breaks the emphasis and changes the link target.
#[test]
fn test_fix_leaves_emphasis_delimiters_outside_the_autolink() {
    let rule = MD034NoBareUrls;
    let cases = [
        ("See _http://127.0.0.1:7878_ now.", "See _<http://127.0.0.1:7878>_ now."),
        ("See *https://example.com/a* now.", "See *<https://example.com/a>* now."),
        (
            "See **https://example.com/b** now.",
            "See **<https://example.com/b>** now.",
        ),
        (
            "See __https://example.com/b__ now.",
            "See __<https://example.com/b>__ now.",
        ),
        (
            "See ~~https://example.com/c~~ now.",
            "See ~~<https://example.com/c>~~ now.",
        ),
        ("See (_https://example.com/d_).", "See (_<https://example.com/d>_)."),
        // Interior delimiters and balanced parens stay in the URL
        (
            "See https://example.com/a_b*c~d now.",
            "See <https://example.com/a_b*c~d> now.",
        ),
        (
            "See _https://example.com/(a)_ now.",
            "See _<https://example.com/(a)>_ now.",
        ),
    ];

    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    let render = |markdown: &str| {
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new_ext(markdown, options));
        html
    };

    for (content, expected) in cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{content}");
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected, "{content}");

        let html = render(&fixed);
        for tag in ["em", "strong", "del"] {
            assert_eq!(
                render(content).contains(&format!("<{tag}>")),
                html.contains(&format!("<{tag}>")),
                "fix changed <{tag}> rendering of {content:?}: {html}"
            );
        }
        assert!(
            rule.check(&LintContext::new(
                &fixed,
                rumdl_lib::config::MarkdownFlavor::Standard,
                None
            ))
            .unwrap()
            .is_empty()
        );
    }
}

#[test]
fn test_nested_url_forms_are_one_finding_and_fix() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for url in [
        "https://example.com/user@example.com",
        "https://user@example.com/path",
        "https://example.com/?url=www.nested.example.com/path",
        "https://example.com/xmpp:user@example.com",
        "www.example.com/xmpp:user@example.com",
        "xmpp:user@example.com/https://nested.example.com",
        "https://[::1]/user@example.com",
        "https://example.com/?url=https://nested.example.com/path",
    ] {
        let content = format!("日本語 café {url} here\n");
        let destination = if url.starts_with("www.") {
            format!("https://{url}")
        } else {
            url.to_string()
        };
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MDX, MarkdownFlavor::MDG] {
            let ctx = LintContext::new(&content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "{flavor:?}: {url} must be one URL finding");
            let label = if url == "https://[::1]/user@example.com" {
                "https://\\[::1\\]/user@example.com"
            } else {
                url
            };
            let expected = match flavor {
                MarkdownFlavor::MDX => format!("日本語 café [{label}]({destination}) here\n"),
                MarkdownFlavor::MDG => content.clone(),
                _ => format!("日本語 café <{destination}> here\n"),
            };
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(
                fixed, expected,
                "{flavor:?}: {url} must be fixed without overlapping edits"
            );
            let fixed_ctx = LintContext::new(&fixed, flavor, None);
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed, "fix must be idempotent");
            if flavor != MarkdownFlavor::MDG {
                let links: Vec<_> = pulldown_cmark::Parser::new(&fixed)
                    .filter_map(|event| match event {
                        pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) => {
                            Some(dest_url.into_string())
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    links,
                    vec![destination.clone()],
                    "fix must produce exactly the outer link"
                );
                assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            }
        }
    }
}

#[test]
fn test_nested_url_does_not_hide_separate_bare_addresses() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    let content = "日本語 https://example.com/user@example.com then standalone@example.com and www.other.example.com\n";
    for (flavor, expected) in [
        (
            MarkdownFlavor::Standard,
            "日本語 <https://example.com/user@example.com> then <standalone@example.com> and <https://www.other.example.com>\n",
        ),
        (
            MarkdownFlavor::MDX,
            "日本語 [https://example.com/user@example.com](https://example.com/user@example.com) then [standalone@example.com](mailto:standalone@example.com) and [www.other.example.com](https://www.other.example.com)\n",
        ),
    ] {
        let ctx = LintContext::new(content, flavor, None);
        assert_eq!(
            rule.check(&ctx).unwrap().len(),
            3,
            "{flavor:?}: separate addresses must remain findings"
        );
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            expected,
            "{flavor:?}: each independent address must be fixed once"
        );
        let fixed_ctx = LintContext::new(expected, flavor, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
    }
}

#[test]
fn test_nested_email_unicode_url_fix_does_not_panic() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for url in [
        "https://example.com/user@example.com/日本語",
        "https://example.com/user@example.com/🦀",
    ] {
        let content = format!("café {url}\n");
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MDX] {
            let ctx = LintContext::new(&content, flavor, None);
            let expected = if flavor == MarkdownFlavor::MDX {
                format!("café [{url}]({url})\n")
            } else {
                format!("café <{url}>\n")
            };
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                expected,
                "{flavor:?}: UTF-8 URL must be fixed once"
            );
            let fixed_ctx = LintContext::new(&expected, flavor, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        }
    }
}

#[test]
fn test_emails_in_html_comments_are_not_rewritten() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    let content = "Before <!-- hidden@example.com --> visible@example.com\n\n<!--\nhidden@example.com\n-->\n";
    let expected = "Before <!-- hidden@example.com --> <visible@example.com>\n\n<!--\nhidden@example.com\n-->\n";
    for flavor in [
        MarkdownFlavor::Standard,
        MarkdownFlavor::MkDocs,
        MarkdownFlavor::Pandoc,
        MarkdownFlavor::Obsidian,
        MarkdownFlavor::Quarto,
        MarkdownFlavor::Hugo,
    ] {
        let ctx = LintContext::new(content, flavor, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{flavor:?}: only the visible address is a finding");
        assert!(warnings[0].message.contains("visible@example.com"));
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            expected,
            "{flavor:?}: comment contents must be unchanged"
        );
        let fixed_ctx = LintContext::new(expected, flavor, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
    }
}

#[test]
fn test_emails_in_shortcode_arguments_are_not_rewritten() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    let content = "Before {{< contact email=\"hidden@example.com\" >}} visible@example.com\n";
    let expected = "Before {{< contact email=\"hidden@example.com\" >}} <visible@example.com>\n";
    for flavor in [MarkdownFlavor::Hugo, MarkdownFlavor::Quarto] {
        let ctx = LintContext::new(content, flavor, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "{flavor:?}: a shortcode argument is not bare prose");
        assert!(warnings[0].message.contains("visible@example.com"));
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            expected,
            "{flavor:?}: shortcode argument must be unchanged"
        );
        let fixed_ctx = LintContext::new(expected, flavor, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
    }
}

#[test]
fn test_mdx_esm_strings_and_import_specifiers_are_not_rewritten() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for esm in [
        "export const email = \"hidden@example.com\";\n",
        "export const url = \"https://example.com/api\";\n",
        "import data from \"https://example.com/module.js\";\n",
        "export {default} from \"https://example.com/module.js\";\n",
        "export const settings = {\n  email: \"hidden@example.com\",\n  url: \"https://example.com/api\"\n};\n",
    ] {
        let content = format!("{esm}\nVisible user@example.com and https://other.example.com\n");
        let expected = format!(
            "{esm}\nVisible [user@example.com](mailto:user@example.com) and [https://other.example.com](https://other.example.com)\n"
        );
        let ctx = LintContext::new(&content, MarkdownFlavor::MDX, None);
        assert!(
            ctx.lines[0].in_esm_block,
            "fixture must be recognized as MDX ESM: {esm}"
        );
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2, "only visible prose is checked: {esm}");
        assert!(warnings.iter().all(|warning| warning.line > esm.lines().count()));
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            expected,
            "module code must be preserved verbatim"
        );
        let fixed_ctx = LintContext::new(&expected, MarkdownFlavor::MDX, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
    }
}

#[test]
fn test_standard_flavor_does_not_treat_export_text_as_esm() {
    let rule = MD034NoBareUrls;
    let content = "export const url = \"https://example.com\"\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert!(!ctx.lines[0].in_esm_block);
    assert_eq!(rule.check(&ctx).unwrap().len(), 1);
    assert_eq!(
        rule.fix(&ctx).unwrap(),
        "export const url = \"<https://example.com>\"\n"
    );
}

#[test]
fn test_uri_authority_addresses_are_not_rewritten_as_emails() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for uri in [
        "ssh://user@example.com",
        "git+ssh://user@example.com/path",
        "redis://user@example.com:6379/db",
        "custom+v2://user.name@example.com/path",
        "SSH://user@example.com",
    ] {
        let content = format!("café {uri} visible@example.com\n");
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MDX, MarkdownFlavor::MDG] {
            let ctx = LintContext::new(&content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(
                warnings.len(),
                1,
                "{flavor:?}: userinfo and host are not a bare email: {uri}"
            );
            assert!(warnings[0].message.contains("visible@example.com"));
            let expected = match flavor {
                MarkdownFlavor::MDX => format!("café {uri} [visible@example.com](mailto:visible@example.com)\n"),
                MarkdownFlavor::MDG => content.clone(),
                _ => format!("café {uri} <visible@example.com>\n"),
            };
            assert_eq!(rule.fix(&ctx).unwrap(), expected, "{flavor:?}: URI must stay unchanged");
            let fixed_ctx = LintContext::new(&expected, flavor, None);
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        }
    }
}

#[test]
fn test_uri_authority_guard_does_not_hide_bare_emails() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for prefix in ["", "Contact: ", "3://", "://", "ssh:// ", "ssh://host ", "ssh//"] {
        let content = format!("{prefix}user@example.com\n");
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MDX] {
            let ctx = LintContext::new(&content, flavor, None);
            assert_eq!(
                rule.check(&ctx).unwrap().len(),
                1,
                "{flavor:?}: {prefix:?} is not an adjacent URI scheme"
            );
            let expected = if flavor == MarkdownFlavor::MDX {
                format!("{prefix}[user@example.com](mailto:user@example.com)\n")
            } else {
                format!("{prefix}<user@example.com>\n")
            };
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
        }
    }
}

#[test]
fn test_urls_and_emails_in_math_are_not_rewritten() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for math in [
        "$$\n\\href{https://example.com}{link}\n$$",
        "$$\n\\text{hidden@example.com}\n$$",
        "$\\href{https://example.com}{link}$",
        "$\\text{hidden@example.com}$",
    ] {
        let content = format!("日本語 {math}\n\nVisible https://other.example.com and visible@example.com\n");
        let expected = format!("日本語 {math}\n\nVisible <https://other.example.com> and <visible@example.com>\n");
        for flavor in [
            MarkdownFlavor::Standard,
            MarkdownFlavor::Pandoc,
            MarkdownFlavor::Quarto,
            MarkdownFlavor::MkDocs,
        ] {
            let ctx = LintContext::new(&content, flavor, None);
            assert!(
                !ctx.math_spans().is_empty(),
                "fixture must be recognized as math: {math}"
            );
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(
                warnings.len(),
                2,
                "{flavor:?}: math contents are not bare prose: {math}"
            );
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                expected,
                "{flavor:?}: math must be preserved verbatim"
            );
            let fixed_ctx = LintContext::new(&expected, flavor, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        }
    }
}

#[test]
fn test_unpaired_and_escaped_dollars_do_not_hide_bare_urls() {
    let rule = MD034NoBareUrls;
    for content in [
        "Price $5 and https://example.com\n",
        "An escaped dollar \\$ before https://example.com\n",
        "Query https://example.com/?price=$5\n",
    ] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(ctx.math_spans().is_empty());
        assert_eq!(
            rule.check(&ctx).unwrap().len(),
            1,
            "non-math dollar must not suppress a URL: {content}"
        );
        let fixed = rule.fix(&ctx).unwrap();
        assert_ne!(fixed, content);
        let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed);
    }
}

#[test]
fn test_protocol_text_in_url_paths_and_queries_does_not_hide_urls() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for url in [
        "https://example.com/grpc://service",
        "https://example.com/?return=ssh://host/path",
        "https://example.com/path/file://data",
        "http://example.com/?next=custom://app",
        "ftp://example.com/path/redis://server",
        "www.example.com/path/ws://server",
        "https://[::1]/path/grpc://service",
    ] {
        let content = format!("日本語 See {url} here\n");
        let destination = if url.starts_with("www.") {
            format!("https://{url}")
        } else {
            url.to_string()
        };
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MDX, MarkdownFlavor::MDG] {
            let ctx = LintContext::new(&content, flavor, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1, "{flavor:?}: {url}");
            let expected = match flavor {
                MarkdownFlavor::MDG => content.clone(),
                MarkdownFlavor::MDX => format!(
                    "日本語 See [{}]({destination}) here\n",
                    url.replace('[', "\\[").replace(']', "\\]")
                ),
                _ => format!("日本語 See <{destination}> here\n"),
            };
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, expected);
            if flavor != MarkdownFlavor::MDG {
                let fixed_ctx = LintContext::new(&fixed, flavor, None);
                assert!(rule.check(&fixed_ctx).unwrap().is_empty());
                assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed);
                let hrefs: Vec<_> = pulldown_cmark::Parser::new(&fixed)
                    .filter_map(|event| match event {
                        pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) => {
                            Some(dest_url.to_string())
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    hrefs,
                    vec![destination.clone()],
                    "the entire URL must render as one link"
                );
            }
        }
    }
}

#[test]
fn test_custom_protocol_links_remain_unchanged() {
    let rule = MD034NoBareUrls;
    for content in [
        "See grpc://service here\n",
        "See ssh://host/path here\n",
        "See custom://app here\n",
        "See file://path/to/file here\n",
        "See ws://server/path here\n",
    ] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }
}

#[test]
fn test_mdx_inline_code_does_not_hide_visible_bare_addresses() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for (content, expected) in [
        (
            "日本語 Before {2 + 2} visible@example.com\n",
            "日本語 Before {2 + 2} [visible@example.com](mailto:visible@example.com)\n",
        ),
        (
            "Visible https://example.com before {2 + 2}\n",
            "Visible [https://example.com](https://example.com) before {2 + 2}\n",
        ),
        (
            "Before {\"hidden@example.com\"} visible@example.com\n",
            "Before {\"hidden@example.com\"} [visible@example.com](mailto:visible@example.com)\n",
        ),
        (
            "Before {/* hidden@example.com https://hidden.example.com */} visible@example.com\n",
            "Before {/* hidden@example.com https://hidden.example.com */} [visible@example.com](mailto:visible@example.com)\n",
        ),
        (
            "Before {/* hidden@example.com\nhttps://hidden.example.com\n*/} visible@example.com\n",
            "Before {/* hidden@example.com\nhttps://hidden.example.com\n*/} [visible@example.com](mailto:visible@example.com)\n",
        ),
        (
            "Before {(\n\"https://hidden.example.com\"\n)} visible@example.com\n",
            "Before {(\n\"https://hidden.example.com\"\n)} [visible@example.com](mailto:visible@example.com)\n",
        ),
        (
            "Before {\"}\"} visible@example.com\n",
            "Before {\"}\"} [visible@example.com](mailto:visible@example.com)\n",
        ),
        (
            "Before {2}{3}https://example.com\n",
            "Before {2}{3}[https://example.com](https://example.com)\n",
        ),
    ] {
        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "only the visible address is bare prose: {content}");
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            expected,
            "expression and comment source must be preserved"
        );
        let fixed_ctx = LintContext::new(expected, MarkdownFlavor::MDX, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
    }
}

#[test]
fn test_mdx_dynamic_urls_and_expression_strings_are_not_rewritten() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for content in [
        "See https://example.com/a{2}c here\n",
        "See https://example.com/a{/*comment*/}c here\n",
        "Before {\"https://example.com\"} after\n",
        "Before {\"hidden@example.com\"} after\n",
        "Before {\"https://example.com\"}{\"hidden@example.com\"} after\n",
        "{\"https://example.com\"}\n",
    ] {
        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
        assert!(rule.check(&ctx).unwrap().is_empty(), "{content}");
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }
}

#[test]
fn test_jinja_expression_and_statement_strings_are_not_rewritten() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for template in [
        "{{ \"hidden@example.com\" }}",
        "{{ \"https://hidden.example.com\" }}",
        "{{- 'https://hidden.example.com' -}}",
        "{% set address = \"hidden@example.com\" %}{{ address }}",
        "{% set endpoint = \"https://hidden.example.com\" %}{{ endpoint }}",
        "{% if \"hidden@example.com\" %}Shown{% endif %}",
        "{{ \"prefix hidden@example.com suffix\" }}",
        "{{ \"prefix \\\" hidden@example.com\" }}",
        "{{ 'prefix \\' hidden@example.com' }}",
        "{{ \"日本語 hidden@example.com\" }}",
    ] {
        let content = format!("日本語 Before {template} visible@example.com\n");
        let expected = format!("日本語 Before {template} <visible@example.com>\n");
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MkDocs, MarkdownFlavor::Quarto] {
            let ctx = LintContext::new(&content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "only visible prose is a bare address: {template}");
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                expected,
                "Jinja string values must be unchanged"
            );
            let fixed_ctx = LintContext::new(&expected, flavor, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        }
    }
}

#[test]
fn test_jinja_tags_do_not_hide_adjacent_bare_addresses() {
    let rule = MD034NoBareUrls;
    for (content, expected) in [
        (
            "Before {{ value }}visible@example.com\n",
            "Before {{ value }}<visible@example.com>\n",
        ),
        (
            "Before {% if value %}visible@example.com{% endif %}\n",
            "Before {% if value %}<visible@example.com>{% endif %}\n",
        ),
        (
            "Before {{ \"hidden@example.com\" }}https://example.com\n",
            "Before {{ \"hidden@example.com\" }}<https://example.com>\n",
        ),
    ] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1, "{content}");
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
        let fixed_ctx = LintContext::new(expected, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
    }
}

#[test]
fn test_gh_aw_directives_still_require_their_selected_flavor() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    let content = "{{#runtime-import https://example.com/shared.md}}\n";
    let standard = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(
        rule.check(&standard).unwrap().len(),
        1,
        "flavors must remain opt-in: {content}"
    );
    let selected = LintContext::new(content, MarkdownFlavor::GhAw, None);
    assert!(rule.check(&selected).unwrap().is_empty());
    assert_eq!(rule.fix(&selected).unwrap(), content);
}

#[test]
fn test_multiline_links_and_images_do_not_hide_adjacent_bare_addresses() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for formatted in [
        "[a link\nlabel](https://hidden.example.com)",
        "![an image\nlabel](https://hidden.example.com/image.png)",
        "[a link\nlabel](https://hidden.example.com \"a title\")",
        "[a link](\nhttps://hidden.example.com\n)",
        "[a link\nhttps://label.example.com\n](https://hidden.example.com)",
        "[a link\nlabel@example.com\n](https://hidden.example.com)",
    ] {
        for (address, mdx_link) in [
            (
                "https://visible.example.com",
                "[https://visible.example.com](https://visible.example.com)",
            ),
            (
                "visible@example.com",
                "[visible@example.com](mailto:visible@example.com)",
            ),
        ] {
            let content = format!("日本語 Before {formatted} and {address}\n");
            for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MDX, MarkdownFlavor::MDG] {
                let ctx = LintContext::new(&content, flavor, None);
                assert_eq!(
                    rule.check(&ctx).unwrap().len(),
                    1,
                    "only adjacent prose is bare: {content}"
                );
                let expected = match flavor {
                    MarkdownFlavor::MDG => content.clone(),
                    MarkdownFlavor::MDX => format!("日本語 Before {formatted} and {mdx_link}\n"),
                    _ => format!("日本語 Before {formatted} and <{address}>\n"),
                };
                assert_eq!(
                    rule.fix(&ctx).unwrap(),
                    expected,
                    "the existing link or image must remain intact"
                );
                if flavor != MarkdownFlavor::MDG {
                    let fixed_ctx = LintContext::new(&expected, flavor, None);
                    assert!(rule.check(&fixed_ctx).unwrap().is_empty());
                    assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
                    let destinations: Vec<_> = pulldown_cmark::Parser::new(&expected)
                        .filter_map(|event| match event {
                            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link {
                                dest_url, link_type, ..
                            }) => Some(if link_type == pulldown_cmark::LinkType::Email {
                                format!("mailto:{dest_url}")
                            } else {
                                dest_url.to_string()
                            }),
                            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) => {
                                Some(dest_url.to_string())
                            }
                            _ => None,
                        })
                        .collect();
                    let original_destination = if formatted.starts_with('!') {
                        "https://hidden.example.com/image.png"
                    } else {
                        "https://hidden.example.com"
                    };
                    let visible_destination = if address.contains('@') {
                        format!("mailto:{address}")
                    } else {
                        address.to_string()
                    };
                    assert_eq!(
                        destinations,
                        vec![original_destination.to_string(), visible_destination]
                    );
                }
            }
        }
    }
}

#[test]
fn test_multiline_link_lookalikes_do_not_hide_prose_urls() {
    let rule = MD034NoBareUrls;
    let content = "日本語 label](https://example.com) and https://visible.example.com\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert_eq!(rule.check(&ctx).unwrap().len(), 2);
    let expected = "日本語 label](<https://example.com>) and <https://visible.example.com>\n";
    assert_eq!(rule.fix(&ctx).unwrap(), expected);
    let fixed_ctx = LintContext::new(expected, rumdl_lib::config::MarkdownFlavor::Standard, None);
    assert!(rule.check(&fixed_ctx).unwrap().is_empty());
}

#[test]
fn test_urls_crossing_jinja_boundaries_are_reported_without_unsafe_fixes() {
    let rule = MD034NoBareUrls;
    for content in [
        "{% if true %}https://example.com{% endif %}\n",
        "{% if false %}https://example.com{%endif%}\n",
        "Before https://example.com/{{ \"path\" }} after\n",
        "Before https://example.com/{{'path'}} after\n",
        "Before https://example.com/{{ value }}/more after\n",
        "日本語 https://example.com/{{ \"path\" }}\n",
    ] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "the bare URL should still be reported: {content}");
        assert!(
            warnings[0].fix.is_none(),
            "fixing a partial template can corrupt it: {content}"
        );
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }
}

#[test]
fn test_template_boundary_guard_preserves_safe_fixes_and_code_exclusions() {
    let rule = MD034NoBareUrls;
    for content in [
        "Before https://example.com/{{value}} after\n",
        "Before https://example.com/{literal} after\n",
        "Before https://example.com visible@example.com\n",
    ] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().iter().all(|warning| warning.fix.is_some()));
        let fixed = rule.fix(&ctx).unwrap();
        assert_ne!(fixed, content);
        let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&fixed_ctx).unwrap().is_empty());
    }
    for content in [
        "日本語 `https://example.com/{{ \"path\" }}`\n",
        "日本語 `start\nhttps://example.com/{{ \"path\" }}\nend`\n",
        "```text\nhttps://example.com/{{ \"path\" }}\n```\n",
    ] {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty(), "code remains excluded: {content}");
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }
}

#[test]
fn test_myst_comments_preserve_bare_addresses() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for indent in 0..=3 {
        for separator in ["", " ", "\t"] {
            let comment = format!(
                "{}%{separator}hidden@example.com https://hidden.example.com",
                " ".repeat(indent)
            );
            let content = format!("{comment}\n\nVisible visible@example.com\n");
            let ctx = LintContext::new(&content, MarkdownFlavor::MyST, None);
            assert!(ctx.line_info(1).unwrap().is_myst_comment);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "only visible prose should be linted: {content}");
            assert_eq!(warnings[0].line, 3);
            let expected = format!("{comment}\n\nVisible <visible@example.com>\n");
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
            let fixed_ctx = LintContext::new(&expected, MarkdownFlavor::MyST, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        }
    }
}

#[test]
fn test_percent_signs_in_prose_and_other_flavors_remain_linted() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for content in [
        "Price 50% https://visible.example.com visible@example.com\n",
        "Escaped \\% https://visible.example.com visible@example.com\n",
    ] {
        let ctx = LintContext::new(content, MarkdownFlavor::MyST, None);
        assert!(!ctx.line_info(1).unwrap().is_myst_comment);
        assert_eq!(rule.check(&ctx).unwrap().len(), 2);
    }
    for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MkDocs, MarkdownFlavor::MDX] {
        let ctx = LintContext::new("%hidden@example.com https://hidden.example.com\n", flavor, None);
        assert!(!ctx.line_info(1).unwrap().is_myst_comment);
        assert_eq!(rule.check(&ctx).unwrap().len(), 2);
    }
    for content in [
        "    %hidden@example.com https://hidden.example.com\n",
        "```text\n%hidden@example.com https://hidden.example.com\n```\n",
    ] {
        let ctx = LintContext::new(content, MarkdownFlavor::MyST, None);
        assert!(rule.check(&ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }
}

#[test]
fn test_multiline_and_delimiter_containing_jinja_strings_are_preserved() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for template in [
        "{{\n\"hidden@example.com\"\n}}",
        "{{\r\n\"https://hidden.example.com\"\r\n}}",
        "{% set target =\n\"https://hidden.example.com\"\n%}{{target}}",
        "{{ \"https://example.com/{{value}}\" }}",
        "{{ \"hidden@example.com }} later\" }}",
        "{% set target = 'hidden@example.com %} later' %}{{target}}",
        "{{ \"escaped \\\" }} hidden@example.com\" }}",
        "{{{\"key\":\"hidden@example.com\"}}}",
    ] {
        let content = format!("日本語 {template}\n\nVisible visible@example.com\n");
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MkDocs, MarkdownFlavor::Quarto] {
            let ctx = LintContext::new(&content, flavor, None);
            assert_eq!(rule.check(&ctx).unwrap().len(), 1, "only prose is bare: {content}");
            let expected = format!("日本語 {template}\n\nVisible <visible@example.com>\n");
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
            let fixed_ctx = LintContext::new(&expected, flavor, None);
            assert!(rule.check(&fixed_ctx).unwrap().is_empty());
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        }
    }
}

#[test]
fn test_addresses_in_custom_uri_components_are_preserved() {
    use rumdl_lib::config::MarkdownFlavor;

    let rule = MD034NoBareUrls;
    for uri in [
        "ssh://host/path/user@example.com",
        "grpc://host/?contact=user@example.com",
        "custom+v2://host/#user@example.com",
        "git+ssh://3user:token@example.com/path",
        "ssh://host/(user@example.com)",
        "ssh://[::1]/user@example.com",
        "custom://host/日本語/user@example.com",
        "custom://host/a@example.com/b@example.com",
    ] {
        assert!(url::Url::parse(uri).is_ok(), "the fixture is a URI: {uri}");
        let content = format!("日本語 {uri} visible@example.com\n");
        for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::MDX, MarkdownFlavor::MDG] {
            let ctx = LintContext::new(&content, flavor, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "only adjacent prose is bare: {content}");
            assert!(warnings[0].message.contains("visible@example.com"));
            let expected = match flavor {
                MarkdownFlavor::MDX => format!("日本語 {uri} [visible@example.com](mailto:visible@example.com)\n"),
                MarkdownFlavor::MDG => content.clone(),
                _ => format!("日本語 {uri} <visible@example.com>\n"),
            };
            assert_eq!(rule.fix(&ctx).unwrap(), expected);
            let fixed_ctx = LintContext::new(&expected, flavor, None);
            assert_eq!(rule.fix(&fixed_ctx).unwrap(), expected);
        }
    }
}

#[test]
fn test_custom_uri_components_do_not_hide_emails_outside_the_uri() {
    let rule = MD034NoBareUrls;
    for prefix in [
        "3ssh://host/path/",
        "3://host/path/",
        "://host/path/",
        "ssh://host/path ",
        "(ssh://host/path)",
        "[ssh://host/path]",
        "\"ssh://host/path\"",
        "'ssh://host/path'",
        "ssh://host/path`",
    ] {
        let content = format!("{prefix}visible@example.com\n");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        assert_eq!(
            rule.check(&ctx).unwrap().len(),
            1,
            "the email is outside a URI: {content}"
        );
        assert_eq!(rule.fix(&ctx).unwrap(), format!("{prefix}<visible@example.com>\n"));
    }
}
