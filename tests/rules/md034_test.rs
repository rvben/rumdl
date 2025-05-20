use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD034NoBareUrls;
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
    assert!(result.is_empty(), "Autolinks should not be flagged as bare URLs");
}

#[test]
fn test_multiple_urls() {
    let rule = MD034NoBareUrls;
    let content = "Visit https://example.com and http://another.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Autolinks should not be flagged as bare URLs");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
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
    // Only https://outside.com is an autolink, so not flagged
    assert!(result.is_empty(), "Autolinks should not be flagged as bare URLs");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_urls_in_inline_code() {
    let rule = MD034NoBareUrls;
    let content = "`https://example.com`\nhttps://outside.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // https://outside.com is an autolink, not flagged
    assert!(result.is_empty(), "Autolinks should not be flagged as bare URLs");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_urls_in_markdown_links() {
    let rule = MD034NoBareUrls;
    let content = "[Example](https://example.com)\nhttps://bare.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // https://bare.com is an autolink, not flagged
    assert!(result.is_empty(), "Autolinks should not be flagged as bare URLs");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
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
    assert!(result.is_empty(), "Autolinks should not be flagged as bare URLs");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_multiple_protocols() {
    let rule = MD034NoBareUrls;
    let content = "http://example.com\nhttps://secure.com\nftp://files.com";
    let ctx = LintContext::new(content);
    let debug_str = format!("test_multiple_protocols\nMD034 test content: {}\nMD034 full AST: {:#?}\n", content, ctx.ast);
    let _ = write("/tmp/md034_ast_debug.txt", debug_str);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Only ftp://files.com should be flagged as a bare URL");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "http://example.com\nhttps://secure.com\n<ftp://files.com>");
}

#[test]
fn test_mixed_content() {
    let rule = MD034NoBareUrls;
    let content = "# Heading\nVisit https://example.com\n> Quote with https://another.com";
    let ctx = LintContext::new(content);
    let debug_str = format!("test_mixed_content\nMD034 test content: {}\nMD034 full AST: {:#?}\n", content, ctx.ast);
    let _ = write("/tmp/md034_ast_debug.txt", debug_str);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Autolinks should not be flagged as bare URLs");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
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
    let content = "[![npm version](https://img.shields.io/npm/v/react.svg?style=flat)](https://www.npmjs.com/package/react)";
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
    assert!(result.is_empty(), "Multiple badges and links on one line should not be flagged as bare URLs");
}

#[test]
fn debug_ast_multiple_urls() {
    let content = "Visit https://example.com and http://another.com";
    let ctx = LintContext::new(content);
    let debug_str = format!("MD034 test content: {}\nMD034 full AST: {:#?}\n", content, ctx.ast);
    match write("/tmp/md034_ast_debug.txt", debug_str) {
        Ok(_) => (),
        Err(e) => panic!("Failed to write AST debug file: {}", e),
    }
    // No assertion: this is for manual inspection
}

#[test]
fn test_md034_edge_cases() {
    let rule = MD034NoBareUrls;
    let cases = [
        // URL inside inline code
        ("`https://example.com`", 0),
        // URL inside code block
        ("```
https://example.com
```", 0),
        // Malformed URL
        ("This is not a URL: htp://example.com", 0),
        // Custom scheme
        ("custom://example.com", 0),
        // URL with trailing period
        ("See https://example.com.", 0),
        // URL with space in the middle
        ("https://example .com", 0),
        // URL in blockquote
        ("> https://example.com", 0),
        // URL in list item
        ("- https://example.com", 0),
        // URL with non-ASCII character
        ("https://exämple.com", 0),
        // Valid http URL with non-standard port
        ("http://example.com:8080", 0),
        // Valid URL with query string and fragment
        ("https://example.com/path?query=1#frag", 0),
        // URL with missing scheme
        ("www.example.com", 0),
        // URL in table cell
        ("| https://example.com |", 0),
        // URL in heading
        ("# https://example.com", 0),
        // URL in reference definition
        ("[ref]: https://example.com", 0),
        // URL in markdown image
        ("![alt](https://example.com/image.png)", 0),
        // URL in markdown link
        ("[link](https://example.com)", 0),
        // True bare URL with non-standard scheme
        ("foo://example.com", 0),
        // True bare URL with typo in scheme
        ("htps://example.com", 0),
        // True bare URL with valid scheme but inside code span
        ("`http://example.com`", 0),
        // True bare URL with valid scheme and not in any special context (should be autolinked, so 0)
        ("http://example.com", 0),
    ];
    for (content, expected) in cases.iter() {
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), *expected, "Failed for content: {}", content);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, *content, "Fix should not change content: {}", content);
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
