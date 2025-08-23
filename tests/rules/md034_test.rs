use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD034NoBareUrls;
use std::fs::write;

#[test]
fn test_valid_urls() {
    let rule = MD034NoBareUrls;
    let content = "[Link](https://example.com)\n<https://example.com>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_bare_urls() {
    let rule = MD034NoBareUrls;
    let content = "This is a bare URL: https://example.com/foobar";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Bare URLs should be flagged");
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].rule_name, Some("MD034"));
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "This is a bare URL: <https://example.com/foobar>");
}

#[test]
fn test_multiple_urls() {
    let rule = MD034NoBareUrls;
    let content = "Visit https://example.com and http://another.com";
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Download from <ftp://example.com/file>");
}

#[test]
fn test_complex_urls() {
    let rule = MD034NoBareUrls;
    let content = "Visit https://example.com/path?param=value#fragment";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Bare URL should be flagged");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Visit <https://example.com/path?param=value#fragment>");
}

#[test]
fn test_multiple_protocols() {
    let rule = MD034NoBareUrls;
    let content = "http://example.com\nhttps://secure.com\nftp://files.com";
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_badge_links_not_flagged() {
    let rule = MD034NoBareUrls;
    let content =
        "[![npm version](https://img.shields.io/npm/v/react.svg?style=flat)](https://www.npmjs.com/package/react)";
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Multiple badges and links on one line should not be flagged as bare URLs"
    );
}

#[test]
fn debug_ast_multiple_urls() {
    let content = "Visit https://example.com and http://another.com";
    let _ctx = LintContext::new(content);
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
        // URL with missing scheme - should not be flagged
        ("www.example.com", 0),
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), *expected, "Failed for content: {content}");
        let fixed = rule.fix(&ctx).unwrap();

        // If we expect warnings, the fix should change the content
        if *expected > 0 {
            assert_ne!(fixed, *content, "Fix should change content with warnings: {content}");
            // The fixed version should have no warnings
            let ctx_fixed = LintContext::new(&fixed);
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
    let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
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
        let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "IP address URLs should be flagged as bare URLs");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Connect to <http://127.0.0.1:8080> or <https://192.168.1.100>");
}

#[test]
fn test_combined_emails_and_localhost() {
    let rule = MD034NoBareUrls;
    let content = "Contact admin@localhost.com or visit http://localhost:9090\nAlso try user@example.org and https://192.168.1.1:3000";
    let ctx = LintContext::new(content);
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

    let ctx = LintContext::new(content);
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

    let ctx = LintContext::new(content);
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

    let ctx = LintContext::new(content);
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

    let ctx = LintContext::new(content);
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
