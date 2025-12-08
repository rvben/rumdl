use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD034NoBareUrls;

#[test]
fn test_ipv6_url_basic() {
    let rule = MD034NoBareUrls;
    let content = "Visit https://[::1]:8080 for local testing";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "IPv6 URL should be flagged as bare URL");
    assert_eq!(result[0].line, 1);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Visit <https://[::1]:8080> for local testing");
}

#[test]
fn test_ipv6_url_full_address() {
    let rule = MD034NoBareUrls;
    let content = "Server at http://[2001:db8::8a2e:370:7334]/path";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Full IPv6 URL should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Server at <http://[2001:db8::8a2e:370:7334]/path>");
}

#[test]
fn test_ipv6_localhost_variations() {
    let rule = MD034NoBareUrls;
    let test_cases = vec![
        ("http://[::1]", "<http://[::1]>"),
        ("https://[::1]", "<https://[::1]>"),
        ("http://[::1]:3000", "<http://[::1]:3000>"),
        ("https://[::1]:8080/api", "<https://[::1]:8080/api>"),
        ("http://[::ffff:127.0.0.1]", "<http://[::ffff:127.0.0.1]>"),
    ];

    for (input, expected) in test_cases {
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "IPv6 URL '{input}' should be flagged");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected, "IPv6 URL '{input}' should be wrapped correctly");
    }
}

#[test]
fn test_ipv6_with_zone_id() {
    let rule = MD034NoBareUrls;
    let content = "Connect to https://[fe80::1%eth0]:8080";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "IPv6 with zone ID should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Connect to <https://[fe80::1%eth0]:8080>");
}

#[test]
fn test_ipv6_mixed_with_ipv4() {
    let rule = MD034NoBareUrls;
    let content = "Try http://127.0.0.1 or https://[::1]:8080 or http://localhost";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3, "All three URLs should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "Try <http://127.0.0.1> or <https://[::1]:8080> or <http://localhost>"
    );
}

#[test]
fn test_ipv6_in_markdown_link() {
    let rule = MD034NoBareUrls;
    let content = "[IPv6 Server](https://[2001:db8::1]:8080) is already linked";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "IPv6 URL in markdown link should not be flagged");
}

#[test]
fn test_ipv6_in_angle_brackets() {
    let rule = MD034NoBareUrls;
    let content = "Already wrapped: <https://[::1]:8080>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "IPv6 URL already in angle brackets should not be flagged"
    );
}

#[test]
fn test_ipv6_edge_cases() {
    let rule = MD034NoBareUrls;

    // Test compressed zeros
    let content = "Visit http://[2001:db8:0:0:0:0:0:1] or http://[2001:db8::1]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Both IPv6 formats should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Visit <http://[2001:db8:0:0:0:0:0:1]> or <http://[2001:db8::1]>");
}

#[test]
fn test_ipv6_with_path_query_fragment() {
    let rule = MD034NoBareUrls;
    let content = "API at https://[2001:db8::1]:8080/api/v1?param=value#section";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "IPv6 URL with full path should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "API at <https://[2001:db8::1]:8080/api/v1?param=value#section>");
}

#[test]
fn test_ipv6_trailing_punctuation() {
    let rule = MD034NoBareUrls;
    let content = "Visit https://[::1]:8080.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "IPv6 URL with trailing period should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Visit <https://[::1]:8080>.");
}

#[test]
fn test_ipv6_ftp_protocol() {
    let rule = MD034NoBareUrls;
    let content = "FTP server at ftp://[2001:db8::ftp]:21";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "FTP IPv6 URL should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "FTP server at <ftp://[2001:db8::ftp]:21>");
}

#[test]
fn test_ipv6_multiple_on_line() {
    let rule = MD034NoBareUrls;
    let content = "Primary: https://[2001:db8::1] Secondary: https://[2001:db8::2]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2, "Both IPv6 URLs should be flagged");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "Primary: <https://[2001:db8::1]> Secondary: <https://[2001:db8::2]>"
    );
}

#[test]
fn test_ipv6_in_reference_definition() {
    let rule = MD034NoBareUrls;
    let content = "[ref]: https://[::1]:8080";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "IPv6 URL in reference definition should not be flagged"
    );
}

#[test]
fn test_ipv6_invalid_formats_not_flagged() {
    let rule = MD034NoBareUrls;
    // These are not valid URLs and should not be flagged
    let test_cases = vec![
        "Just brackets [::1] without protocol",
        "Missing closing bracket https://[::1:8080",
        "Missing opening bracket https://::1]:8080",
        "Empty brackets https://[]:8080",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Invalid format '{content}' should not be flagged");
    }
}
