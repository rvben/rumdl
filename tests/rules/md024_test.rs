use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD024NoDuplicateHeading;
use std::io::Write;

#[test]
fn test_no_duplicate_headings() {
    let rule = MD024NoDuplicateHeading::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_duplicate_headings() {
    let rule = MD024NoDuplicateHeading::new(false, false); // siblings_only=false to check all duplicates
    let content = "# Heading\n## Heading\n### Heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_fix_duplicate_headings() {
    let rule = MD024NoDuplicateHeading::default();
    // Input with duplicate headings
    let content = "# Heading\n## Heading\n# Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    // There should be duplicate heading warnings before fixing
    let before = rule.check(&ctx).unwrap();
    assert!(!before.is_empty(), "Should detect duplicate headings before fix");

    // Apply the fix
    let fixed = rule.fix(&ctx).unwrap();
    // The fix should NOT change the content (MD024 does not support auto-fixing)
    assert_eq!(fixed, content, "Fix should not modify the content for MD024");
}

#[test]
fn test_md024_different_levels() {
    let rule = MD024NoDuplicateHeading::new(false, false); // siblings_only=false to check all duplicates
    let content = "# Heading\n## Heading\n### Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_md024_different_levels_with_allow_different_nesting() {
    let rule = MD024NoDuplicateHeading::new(true, false);
    let content = "# Heading\n## Heading\n### Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Expected 0 warnings for duplicated headings with allow_different_nesting=true"
    );
}

#[test]
fn test_md024_different_case() {
    let rule = MD024NoDuplicateHeading::default();
    let content = "# Heading\n## Subheading\n# heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_md024_with_setext_headings() {
    let rule = MD024NoDuplicateHeading::default();
    let content = "Heading 1\n=========\nSome text\n\nHeading 1\n=========\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
}

#[test]
fn test_md024_mixed_heading_styles() {
    let rule = MD024NoDuplicateHeading::default();
    let content = "# Heading\n\nHeading\n=======\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md024_with_empty_headings() {
    let rule = MD024NoDuplicateHeading::default();
    // Empty headings should be ignored by the rule
    let content = "#\n## \n###  \n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md024_in_code_blocks() {
    let rule = MD024NoDuplicateHeading::default();
    let content = "# Heading\n\n```markdown\n# Heading\n```\n# Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 6);
    println!("{result:?}");
    std::io::stdout().flush().unwrap();
}

#[test]
fn test_md024_with_front_matter() {
    let rule = MD024NoDuplicateHeading::default();
    let content = "---\ntitle: My Document\n---\n# Heading\n\nSome text\n\n# Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 8);
}

#[test]
fn test_md024_with_closed_atx_headings() {
    let rule = MD024NoDuplicateHeading::default();
    let content = "# Heading #\n\n## Subheading ##\n\n# Heading #\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
}

#[test]
fn test_md024_with_multiple_duplicates() {
    let rule = MD024NoDuplicateHeading::default();
    let content = "# Heading\n\n## Subheading\n\n# Heading\n\n## Subheading\n\n# Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 5);
    assert_eq!(result[1].line, 7);
    assert_eq!(result[2].line, 9);
}

#[test]
fn test_md024_with_trailing_whitespace() {
    let rule = MD024NoDuplicateHeading::default();
    let content = "# Heading \n\n# Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md024_performance_with_many_headings() {
    let rule = MD024NoDuplicateHeading::default();

    // Create a document with 100 unique headings
    let mut content = String::new();
    for i in 1..=100 {
        content.push_str(&format!("# Heading {i}\n\n"));
    }

    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let start = std::time::Instant::now();
    let result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    assert!(result.is_empty());
    assert!(
        duration.as_millis() < 100,
        "Checking 100 unique headings should take less than 100ms"
    );
}

#[test]
fn test_md024_fix() {
    let rule = MD024NoDuplicateHeading::default();
    let content = "# Heading\n## Subheading\n# Heading\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, content, "Fix method should not modify content");
}
