use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD002FirstHeadingH1;
use rumdl::utils::document_structure::DocumentStructure;

#[test]
fn test_custom_level() {
    let rule = MD002FirstHeadingH1::new(2);
    let content = "## Heading\n### Subheading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_first_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading\n### Subheading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_no_headings() {
    let rule = MD002FirstHeadingH1::default();
    let content = "This is a paragraph\nAnother paragraph";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_only_one_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "# Heading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_closed_atx_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading ##\n### Subheading ###";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading #\n### Subheading ###");
}

#[test]
fn test_fix_first_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading\n### Subheading";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading\n### Subheading");
}

#[test]
fn test_fix_closed_atx_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading ##\n### Subheading ###";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading #\n### Subheading ###");
}

#[test]
fn test_mixed_heading_styles() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading\n### Subheading ###\n#### Another heading";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "# Heading\n### Subheading ###\n#### Another heading"
    );
}

#[test]
fn test_indented_first_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "  ## Heading\n# Subheading";
    println!("Input: '{}'", content.replace("\n", "\\n"));
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    println!("Output: '{}'", result.replace("\n", "\\n"));
    println!(
        "Expected: '{}' (len {})",
        "  # Heading\n# Subheading".replace("\n", "\\n"),
        "  # Heading\n# Subheading".len()
    );
    println!(
        "Got:      '{}' (len {})",
        result.replace("\n", "\\n"),
        result.len()
    );

    // Print each character's byte value
    println!("Expected bytes: ");
    for (i, b) in "  # Heading\n# Subheading".bytes().enumerate() {
        print!("{}:{} ", i, b);
    }
    println!("\nGot bytes: ");
    for (i, b) in result.bytes().enumerate() {
        print!("{}:{} ", i, b);
    }
    println!();

    assert_eq!(result, "  # Heading\n# Subheading");
}

#[test]
fn test_setext_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "Heading\n-------\n\n### Subheading";
    let structure = DocumentStructure::new(content);
    println!(
        "[test_setext_heading] heading_lines: {:?}",
        structure.heading_lines
    );
    println!(
        "[test_setext_heading] heading_levels: {:?}",
        structure.heading_levels
    );
    let ctx = LintContext::new(content);
    let result = rule.check_with_structure(&ctx, &structure).unwrap();
    println!("[test_setext_heading] result: {:?}", result);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Heading\n=======\n\n### Subheading");
}

#[test]
fn test_with_front_matter() {
    let rule = MD002FirstHeadingH1::default();
    let content = "---\ntitle: Test\n---\n## Heading\n### Subheading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "---\ntitle: Test\n---\n# Heading\n### Subheading");
}

#[test]
fn test_setext_with_front_matter() {
    let rule = MD002FirstHeadingH1::default();
    let content = "---\ntitle: Test\n---\n\nHeading\n-------\n\n### Subheading";
    let structure = DocumentStructure::new(content);
    println!(
        "[test_setext_with_front_matter] heading_lines: {:?}",
        structure.heading_lines
    );
    println!(
        "[test_setext_with_front_matter] heading_levels: {:?}",
        structure.heading_levels
    );
    let ctx = LintContext::new(content);
    let result = rule.check_with_structure(&ctx, &structure).unwrap();
    println!("[test_setext_with_front_matter] result: {:?}", result);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "---\ntitle: Test\n---\n\nHeading\n=======\n\n### Subheading"
    );
}
