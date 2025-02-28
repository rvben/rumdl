use rumdl::rules::MD034NoBareUrls;
use rumdl::rule::Rule;

#[test]
fn test_valid_urls() {
    let rule = MD034NoBareUrls::default();
    let content = "[Link](https://example.com)\n<https://example.com>";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_bare_urls() {
    let rule = MD034NoBareUrls::default();
    let content = "Visit https://example.com for more info";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Visit <https://example.com> for more info");
}

#[test]
fn test_multiple_urls() {
    let rule = MD034NoBareUrls::default();
    let content = "Visit https://example.com and http://another.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Visit <https://example.com> and <http://another.com>");
}

#[test]
fn test_urls_in_code_block() {
    let rule = MD034NoBareUrls::default();
    let content = "```\nhttps://example.com\n```\nhttps://outside.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "```\nhttps://example.com\n```\n<https://outside.com>");
}

#[test]
fn test_urls_in_inline_code() {
    let rule = MD034NoBareUrls::default();
    let content = "`https://example.com`\nhttps://outside.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "`https://example.com`\n<https://outside.com>");
}

#[test]
fn test_urls_in_markdown_links() {
    let rule = MD034NoBareUrls::default();
    let content = "[Example](https://example.com)\nhttps://bare.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "[Example](https://example.com)\n<https://bare.com>");
}

#[test]
fn test_ftp_urls() {
    let rule = MD034NoBareUrls::default();
    let content = "Download from ftp://example.com/file";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Download from <ftp://example.com/file>");
}

#[test]
fn test_complex_urls() {
    let rule = MD034NoBareUrls::default();
    let content = "Visit https://example.com/path?param=value#fragment";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Visit <https://example.com/path?param=value#fragment>");
}

#[test]
fn test_multiple_protocols() {
    let rule = MD034NoBareUrls::default();
    let content = "http://example.com\nhttps://secure.com\nftp://files.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "<http://example.com>\n<https://secure.com>\n<ftp://files.com>");
}

#[test]
fn test_mixed_content() {
    let rule = MD034NoBareUrls::default();
    let content = "# Heading\nVisit https://example.com\n> Quote with https://another.com";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading\nVisit <https://example.com>\n> Quote with <https://another.com>");
}

#[test]
fn test_not_urls() {
    let rule = MD034NoBareUrls::default();
    let content = "Text with example.com and just://something";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
} 