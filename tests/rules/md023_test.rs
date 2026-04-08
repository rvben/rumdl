use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD023HeadingStartLeft;
use rumdl_lib::utils::fix_utils::apply_warning_fixes;

#[test]
fn test_valid_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "# Heading 1\n## Heading 2\nHeading 3\n---";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_indented_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "  # Indented\n    ## Indented\n  Heading\n  ---";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_fix_indented_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "  # Indented\n    ## Indented\n  Heading\n  ---";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.contains("# Indented"));
    assert!(fixed.contains("## Indented"));
}

#[test]
fn test_mixed_content() {
    let rule = MD023HeadingStartLeft;
    let content = "# Good heading\n   # Bad heading\nNormal text\n  ## Another bad one";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_closed_atx_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "   # Heading 1 #\n  ## Heading 2 ##";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##");
}

#[test]
fn test_preserve_heading_content() {
    let rule = MD023HeadingStartLeft;
    let content = "   # Complex *heading* with **markdown**";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Complex *heading* with **markdown**");
}

#[test]
fn test_ignore_non_headings() {
    let rule = MD023HeadingStartLeft;
    let content = "   Not a heading\n  Also not a heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_heading_levels() {
    let rule = MD023HeadingStartLeft;
    let content = "   # H1\n  ## H2\n ### H3\n#### H4";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3); // Only the indented ones should be flagged
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# H1\n## H2\n### H3\n#### H4");
}

/// Helper: apply fixes via check() warnings to verify roundtrip consistency
fn roundtrip_fix(content: &str) -> String {
    let rule = MD023HeadingStartLeft;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();
    if warnings.is_empty() {
        return content.to_string();
    }
    apply_warning_fixes(content, &warnings).unwrap()
}

#[test]
fn test_roundtrip_atx_headings() {
    let rule = MD023HeadingStartLeft;

    // Use 1-3 space indentation (4+ spaces creates a code block, not a heading)
    let content = "  # Indented H1\n   ## Indented H2\n### Already fine";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fix_result = rule.fix(&ctx).unwrap();
    let roundtrip_result = roundtrip_fix(content);
    assert_eq!(fix_result, roundtrip_result, "ATX heading roundtrip mismatch");
    assert_eq!(roundtrip_result, "# Indented H1\n## Indented H2\n### Already fine");
}

#[test]
fn test_roundtrip_setext_headings() {
    let rule = MD023HeadingStartLeft;

    let content = "  Heading 1\n  =========\n  Heading 2\n  ---------";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fix_result = rule.fix(&ctx).unwrap();
    let roundtrip_result = roundtrip_fix(content);
    assert_eq!(fix_result, roundtrip_result, "Setext heading roundtrip mismatch");
    assert_eq!(roundtrip_result, "Heading 1\n=========\nHeading 2\n---------");
}

#[test]
fn test_roundtrip_mixed_headings() {
    let rule = MD023HeadingStartLeft;

    let content = "# Good heading\n   # Bad heading\nNormal text\n  ## Another bad one\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fix_result = rule.fix(&ctx).unwrap();
    let roundtrip_result = roundtrip_fix(content);
    assert_eq!(fix_result, roundtrip_result, "Mixed heading roundtrip mismatch");
}

#[test]
fn test_roundtrip_closed_atx() {
    let rule = MD023HeadingStartLeft;

    let content = "   # Heading 1 #\n  ## Heading 2 ##";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fix_result = rule.fix(&ctx).unwrap();
    let roundtrip_result = roundtrip_fix(content);
    assert_eq!(fix_result, roundtrip_result, "Closed ATX roundtrip mismatch");
    assert_eq!(roundtrip_result, "# Heading 1 #\n## Heading 2 ##");
}

#[test]
fn test_roundtrip_no_warnings() {
    let content = "# Heading 1\n## Heading 2\n### Heading 3\n";
    let roundtrip_result = roundtrip_fix(content);
    assert_eq!(roundtrip_result, content);
}

#[test]
fn test_roundtrip_idempotent() {
    // Fix should be idempotent: applying fix twice yields same result
    let content = "  # Indented\n    ## Also indented\n";
    let first_pass = roundtrip_fix(content);
    let second_pass = roundtrip_fix(&first_pass);
    assert_eq!(first_pass, second_pass, "Fix should be idempotent");
}
