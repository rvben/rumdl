use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD034NoBareUrls;
use std::fs::write;

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
    let debug_str = format!("test_multiple_protocols\nMD034 test content: {content}\n");
    let _ = write("/tmp/md034_ast_debug.txt", debug_str);
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
    let debug_str = format!("test_mixed_content\nMD034 test content: {content}\n");
    let _ = write("/tmp/md034_ast_debug.txt", debug_str);
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
fn debug_ast_multiple_urls() {
    let content = "Visit https://example.com and http://another.com";
    let _ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let debug_str = format!("MD034 test content: {content}\n");
    match write("/tmp/md034_ast_debug.txt", debug_str) {
        Ok(_) => (),
        Err(e) => panic!("Failed to write AST debug file: {e}"),
    }
    // No assertion: this is for manual inspection
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
