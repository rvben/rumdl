use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD041FirstLineHeading;

#[test]
fn test_valid_first_line_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "# First heading\nSome text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    println!("Valid test result: {result:?}");
    assert!(result.is_empty());
}

#[test]
fn test_missing_first_line_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "Some text\n# Not first heading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_wrong_level_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "## Second level heading\nSome text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_with_front_matter() {
    let rule = MD041FirstLineHeading::new(1, true);
    let content = "---\ntitle: Test\n---\n# First heading\nSome text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_with_front_matter_no_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "---\ntitle: Test\n---\nSome text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_fix_missing_heading() {
    let rule = MD041FirstLineHeading::new(1, false);
    let content = "Some text\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert!(result.starts_with("# "));
}

#[test]
fn test_custom_level() {
    let rule = MD041FirstLineHeading::new(2, false);
    let content = "## Second level heading\nSome text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    println!("Custom level test result: {result:?}");
    assert!(result.is_empty());
}

#[test]
fn test_html_headings() {
    let rule = MD041FirstLineHeading::new(1, false);

    // Test valid HTML h1 heading
    let content = "<h1>First Level Heading</h1>\nSome text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "HTML h1 should be recognized as valid first heading");

    // Test wrong level HTML heading
    let content = "<h2>Second Level Heading</h2>\nSome text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "HTML h2 should fail when h1 is required");

    // Test HTML heading with attributes
    let content = "<h1 class=\"title\" id=\"main\">First Heading</h1>\nContent";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "HTML h1 with attributes should be valid");

    // Test custom level with HTML
    let rule = MD041FirstLineHeading::new(3, false);
    let content = "<h3>Third Level</h3>\nContent";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "HTML h3 should be valid when level 3 is required");
}

#[test]
fn test_front_matter_title_pattern() {
    // Test custom pattern matching
    let rule = MD041FirstLineHeading::with_pattern(1, true, Some("^(title|header):".to_string()));

    // Should pass with "title:"
    let content = "---\ntitle: My Document\n---\nContent";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should pass with title: in front matter");

    // Should pass with "header:"
    let content = "---\nheader: My Document\n---\nContent";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should pass with header: in front matter");

    // Should fail with "name:" (not matching pattern)
    let content = "---\nname: My Document\n---\nContent";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should fail with name: not matching pattern");

    // Test case-sensitive pattern
    let rule = MD041FirstLineHeading::with_pattern(1, true, Some("^Title:".to_string()));
    let content = "---\nTitle: My Document\n---\nContent";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should match case-sensitive Title:");

    let content = "---\ntitle: My Document\n---\nContent";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should fail with lowercase title when pattern expects Title:"
    );
}

#[test]
fn test_skip_non_content_lines() {
    let rule = MD041FirstLineHeading::new(1, false);

    // Test reference definitions before heading
    let content = "[ref]: https://example.com\n# First Heading\nContent";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should skip reference definitions");

    // Test abbreviation definitions
    let content = "*[HTML]: HyperText Markup Language\n# First Heading\nContent";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should skip abbreviation definitions");

    // Test HTML comments - should NOT be skipped, as per MD041 spec
    let content = "<!-- Comment -->\n# First Heading\nContent";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "HTML comments should not be skipped");
}
