use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD025SingleTitle;

#[test]
fn test_md025_valid() {
    let rule = MD025SingleTitle::default();
    let content = "# Title\n## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md025_invalid() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n## Heading\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_md025_no_title() {
    let rule = MD025SingleTitle::default();
    let content = "## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md025_with_front_matter() {
    let rule = MD025SingleTitle::default();
    let content = "---\ntitle: Document Title\n---\n# Title\n## Heading 2\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should not flag a single title after front matter");
}

#[test]
fn test_md025_multiple_with_front_matter() {
    let rule = MD025SingleTitle::default();
    let content = "---\ntitle: Document Title\n---\n# Title 1\n## Heading 2\n# Title 2\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 6);
}

#[test]
fn test_md025_with_code_blocks() {
    let rule = MD025SingleTitle::default();
    let content = "# Title\n\n```markdown\n# This is not a real title\n```\n\n## Heading\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Should ignore titles in code blocks");
}

#[test]
fn test_md025_with_custom_level() {
    let rule = MD025SingleTitle::new(2, "");
    let content = "# Heading 1\n## Heading 2.1\n## Heading 2.2\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md025_indented_headings() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n\n  # Title 2\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md025_with_multiple_violations() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n\n# Title 2\n\n# Title 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[1].line, 5);
}

#[test]
fn test_md025_empty_document() {
    let rule = MD025SingleTitle::default();
    let content = "";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md025_closing_hashes() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1 #\n\n# Title 2 #\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md025_setext_headings() {
    let rule = MD025SingleTitle::default();
    // Setext headings (using === or ---) are now detected by this rule
    // Multiple level-1 setext headings should be flagged
    let content = "Title 1\n=======\n\nTitle 2\n=======\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Second title should be flagged
    assert_eq!(result[0].line, 4); // "Title 2" line
}

#[test]
fn test_md025_performance() {
    let rule = MD025SingleTitle::default();

    // Generate a large document with many headings
    let mut content = String::new();
    content.push_str("# Main Title\n\n");

    for i in 1..=100 {
        content.push_str(&format!("## Heading {i}\n\nSome text here.\n\n"));
    }

    let ctx = LintContext::new(&content);
    let start = std::time::Instant::now();
    let result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    assert!(result.is_empty());
    assert!(
        duration.as_millis() < 500,
        "Processing large document should take less than 500ms"
    );
}

#[test]
fn test_md025_fix() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n## Heading\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Title 1\n## Title 2\n## Heading\n");
}

#[test]
fn test_md025_fix_multiple() {
    let rule = MD025SingleTitle::default();
    let content = "# Title 1\n# Title 2\n# Title 3\n## Heading\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Title 1\n## Title 2\n## Title 3\n## Heading\n");
}

#[test]
fn test_md025_fix_with_indentation() {
    let rule = MD025SingleTitle::default();
    // In Markdown, content indented with 4+ spaces is considered a code block
    // so the heavily indented heading is not processed as a heading
    let content = "# Title 1\n  # Title 2\n    # Title 3\n";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed);

    // Expected behavior: verify the title is fixed properly
    assert!(fixed_ctx.content.contains("# Title 1"));
    assert!(fixed_ctx.content.contains("Title 2"));
    assert!(fixed_ctx.content.contains("Title 3"));

    // Ensure there are no duplicate H1 headings (the issue this rule checks for)
    let result = rule.check(&fixed_ctx).unwrap();
    assert!(result.is_empty(), "Fixed content should have no warnings");
}
