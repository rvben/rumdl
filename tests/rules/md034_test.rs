use rumdl::rule::Rule;
use rumdl::rules::MD034NoBareUrls;

#[test]
fn test_valid_urls() {
    let rule = MD034NoBareUrls;
    let content = "[Link](https://example.com)\n<https://example.com>";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_bare_urls() {
    let rule = MD034NoBareUrls;
    let content = "Visit https://example.com for more info";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Visit <https://example.com> for more info");
}

#[test]
fn test_multiple_urls() {
    let rule = MD034NoBareUrls;
    let content = "Visit https://example.com and http://another.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "Visit <https://example.com> and <http://another.com>"
    );
}

#[test]
fn test_urls_in_code_block() {
    let rule = MD034NoBareUrls;
    let content = "```\nhttps://example.com\n```\nhttps://outside.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "```\nhttps://example.com\n```\n<https://outside.com>"
    );
}

#[test]
fn test_urls_in_inline_code() {
    let rule = MD034NoBareUrls;
    let content = "`https://example.com`\nhttps://outside.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "`https://example.com`\n<https://outside.com>");
}

#[test]
fn test_urls_in_markdown_links() {
    let rule = MD034NoBareUrls;
    let content = "[Example](https://example.com)\nhttps://bare.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "[Example](https://example.com)\n<https://bare.com>");
}

#[test]
fn test_ftp_urls() {
    let rule = MD034NoBareUrls;
    let content = "Download from ftp://example.com/file";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Download from <ftp://example.com/file>");
}

#[test]
fn test_complex_urls() {
    let rule = MD034NoBareUrls;
    let content = "Visit https://example.com/path?param=value#fragment";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "Visit <https://example.com/path?param=value#fragment>"
    );
}

#[test]
fn test_multiple_protocols() {
    let rule = MD034NoBareUrls;
    let content = "http://example.com\nhttps://secure.com\nftp://files.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "<http://example.com>\n<https://secure.com>\n<ftp://files.com>"
    );
}

#[test]
fn test_mixed_content() {
    let rule = MD034NoBareUrls;
    let content = "# Heading\nVisit https://example.com\n> Quote with https://another.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "# Heading\nVisit <https://example.com>\n> Quote with <https://another.com>"
    );
}

#[test]
fn test_not_urls() {
    let rule = MD034NoBareUrls;
    let content = "Text with example.com and just://something";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_performance_md034() {
    use std::time::Instant;
    let rule = MD034NoBareUrls;

    // Generate a large document with a mix of bare URLs, proper links, and code blocks
    let mut content = String::with_capacity(500_000);

    // Add a mix of content with URLs in various contexts
    for i in 0..1000 {
        // Regular text with bare URLs
        if i % 5 == 0 {
            content.push_str(&format!(
                "Paragraph {} with a bare URL https://example.com/page{} and some text.\n\n",
                i, i
            ));
        }
        // Proper markdown links
        else if i % 5 == 1 {
            content.push_str(&format!(
                "Paragraph {} with a [proper link](https://example.com/page{}) and some text.\n\n",
                i, i
            ));
        }
        // Auto-linked URLs
        else if i % 5 == 2 {
            content.push_str(&format!(
                "Paragraph {} with an auto-linked <https://example.com/page{}> and some text.\n\n",
                i, i
            ));
        }
        // Code blocks with URLs
        else if i % 5 == 3 {
            content.push_str(&format!(
                "```\ncode block {} with https://example.com/page{} url\n```\n\n",
                i, i
            ));
        }
        // Inline code with URLs
        else {
            content.push_str(&format!(
                "Paragraph {} with `https://example.com/page{}` in code and some text.\n\n",
                i, i
            ));
        }
    }

    // Add a section with multiple URLs on the same line
    content.push_str("\n## Multiple URLs on same line\n\n");
    for i in 0..200 {
        content.push_str(&format!(
            "Line with multiple bare URLs: https://example1.com/page{} and https://example2.com/page{} and https://example3.com/page{}\n",
            i, i+1, i+2
        ));
    }

    // Add some content without URLs to test the fast path
    content.push_str("\n## Content without URLs\n\n");
    for i in 0..100 {
        content.push_str(&format!(
            "This is paragraph {} without any URLs or links.\n\n",
            i
        ));
    }

    println!("Generated test content of {} bytes", content.len());

    // Measure performance of check method
    let start = Instant::now();
    let result = rule.check(&content).unwrap();
    let check_duration = start.elapsed();

    // Measure performance of fix method
    let start = Instant::now();
    let fixed = rule.fix(&content).unwrap();
    let fix_duration = start.elapsed();

    println!(
        "MD034 check duration: {:?} for content length {}",
        check_duration,
        content.len()
    );
    println!("MD034 fix duration: {:?}", fix_duration);
    println!("Found {} warnings", result.len());

    // Verify results
    assert!(!result.is_empty(), "Should have found bare URLs");
    assert!(
        fixed.contains("<https://example.com/page0>"),
        "Should have fixed bare URLs"
    );
    assert!(
        !fixed.contains("code block 3 with <https://"),
        "Should not fix URLs in code blocks"
    );
    assert!(
        !fixed.contains("with `<https://"),
        "Should not fix URLs in inline code"
    );

    // Performance assertion - should complete in a reasonable time
    assert!(
        check_duration.as_millis() < 100,
        "Check should complete in under 100ms"
    );
    assert!(
        fix_duration.as_millis() < 100,
        "Fix should complete in under 100ms"
    );
}
