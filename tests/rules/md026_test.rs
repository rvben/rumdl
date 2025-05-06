use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD026NoTrailingPunctuation;

#[test]
fn test_md026_valid() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md026_invalid() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);
}

#[test]
fn test_md026_mixed() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1\n## Heading 2!\n### Heading 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_md026_fix() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1\n## Heading 2\n### Heading 3\n");
}

#[test]
fn test_md026_custom_punctuation() {
    let rule = MD026NoTrailingPunctuation::new(Some("!?".to_string()));
    let content = "# Heading 1!\n## Heading 2?\n### Heading 3.\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Only ! and ? should be detected, not .
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
}

#[test]
fn test_md026_setext_headings() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "Heading 1!\n=======\nHeading 2?\n-------\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_md026_closed_atx() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading 1! #\n## Heading 2? ##\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##\n");
}

#[test]
fn test_md026_empty_document() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Empty documents should not produce warnings"
    );
}

#[test]
fn test_md026_with_code_blocks() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Valid heading\n\n```\n# This is a code block with heading syntax!\n```\n\n```rust\n# This is another code block with a punctuation mark.\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Content in code blocks should be ignored"
    );
}

#[test]
fn test_md026_with_front_matter() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "---\ntitle: This is a title with punctuation!\ndate: 2023-01-01\n---\n\n# Correct heading\n## Heading with punctuation!\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Only the heading outside front matter should be detected"
    );
    assert_eq!(result[0].line, 7);

    let fixed = rule.fix(&ctx).unwrap();
    assert!(
        fixed.contains("# Correct heading\n## Heading with punctuation\n"),
        "Fix should preserve front matter and only modify headings outside it"
    );
}

#[test]
fn test_md026_multiple_trailing_punctuation() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "# Heading with multiple marks!!!???\n## Another heading.....";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading with multiple marks\n## Another heading");
}

#[test]
fn test_md026_indented_headings() {
    let rule = MD026NoTrailingPunctuation::default();
    // In Markdown, content indented with 4+ spaces is considered a code block
    let content = "  # Indented heading!\n    ## Deeply indented heading?";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Only the first heading is detected, the second is treated as a code block
    // due to 4+ spaces indentation according to Markdown spec
    assert_eq!(
        result.len(),
        1,
        "The implementation should detect only the lightly indented heading"
    );
    assert_eq!(result[0].line, 1);

    let fixed = rule.fix(&ctx).unwrap();
    // Verify the first heading gets fixed but the second remains untouched
    // since it's considered a code block
    assert_eq!(
        fixed,
        "  # Indented heading\n    ## Deeply indented heading?"
    );
}

#[test]
fn test_md026_fix_setext_headings() {
    let rule = MD026NoTrailingPunctuation::default();
    let content = "Heading 1!\n=======\nHeading 2?\n-------";
    let ctx = LintContext::new(content);

    let fixed = rule.fix(&ctx).unwrap();

    // The correct behavior for a Markdown-compliant implementation
    let expected = "Heading 1\n=======\nHeading 2\n-------";
    assert_eq!(
        fixed, expected,
        "The implementation handles setext headings correctly"
    );
}

#[test]
fn test_md026_performance() {
    let rule = MD026NoTrailingPunctuation::default();

    // Create a large document with many headings, but smaller than before
    let mut content = String::new();
    for i in 1..=100 {
        content.push_str(&format!(
            "# Heading {}{}\n\nSome content paragraph.\n\n",
            i,
            if i % 3 == 0 { "!" } else { "" }
        ));
    }

    // Measure performance
    use std::time::Instant;
    let start = Instant::now();
    let ctx = LintContext::new(&content);
    let result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    // Verify correctness
    assert_eq!(
        result.len(),
        33,
        "Should detect exactly 33 headings with punctuation"
    );

    // Verify performance
    println!("MD026 performance test completed in {:?}", duration);
    assert!(
        duration.as_millis() < 1000,
        "Performance check should complete in under 1000ms"
    );
}

#[test]
fn test_md026_non_standard_punctuation() {
    let rule = MD026NoTrailingPunctuation::new(Some("@$%".to_string()));
    let content =
        "# Heading 1@\n## Heading 2$\n### Heading 3%\n#### Heading 4#\n##### Heading 5!\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Heading 1\n## Heading 2\n### Heading 3\n#### Heading 4#\n##### Heading 5!\n"
    );
}
