use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD019NoMultipleSpaceAtx;
use rumdl_lib::utils::fix_utils::apply_warning_fixes;

#[test]
fn test_valid_atx_headings() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_atx_headings() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "#  Heading 1\n##   Heading 2\n###    Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 2);
    assert_eq!(result[0].message, "Multiple spaces (2) after # in heading");
}

#[test]
fn test_mixed_atx_headings() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "# Heading 1\n##  Heading 2\n### Heading 3\n####   Heading 4";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_code_block() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "```markdown\n#  Not a heading\n##   Also not a heading\n```\n# Real Heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_atx_headings() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "#  Heading 1\n##   Heading 2\n###    Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1\n## Heading 2\n### Heading 3");
}

#[test]
fn test_fix_mixed_atx_headings() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "# Heading 1\n##  Heading 2\n### Heading 3\n####   Heading 4";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1\n## Heading 2\n### Heading 3\n#### Heading 4");
}

#[test]
fn test_preserve_code_blocks() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "# Real Heading\n```\n#  Not a heading\n```\n# Another Heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Real Heading\n```\n#  Not a heading\n```\n# Another Heading");
}

#[test]
fn test_heading_with_multiple_hashes() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "######  Heading 6";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].message, "Multiple spaces (2) after ###### in heading");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "###### Heading 6");
}

#[test]
fn test_not_a_heading() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "This is #  not a heading\nAnd this is also #   not a heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_closed_atx_headings() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "#  Heading 1 #\n##   Heading 2 ##\n###    Heading 3 ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_many_spaces() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "#     Heading with many spaces\n##      Another heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].message, "Multiple spaces (5) after # in heading");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading with many spaces\n## Another heading");
}

#[test]
fn test_empty_headings() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "#\n##\n###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_multiple_spaces() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "#  Multiple Spaces\n\nRegular content\n\n##   More Spaces";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 5);
}

#[test]
fn test_valid_single_space() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "# Single Space\n\n## Also correct";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_multiple_spaces() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "#  Multiple Spaces\n\n##   More Spaces";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Multiple Spaces\n\n## More Spaces");
}

#[test]
fn test_roundtrip_check_fixes_match_fix() {
    let rule = MD019NoMultipleSpaceAtx::new();

    let cases = vec![
        "#  Heading 1\n##   Heading 2\n###    Heading 3",
        "# Heading 1\n##  Heading 2\n### Heading 3\n####   Heading 4",
        "#  Multiple Spaces\n\nRegular content\n\n##   More Spaces",
        "######  Heading 6",
        "#     Heading with many spaces\n##      Another heading",
        "#  Heading 1 #\n##   Heading 2 ##\n###    Heading 3 ###",
        "# Real Heading\n```\n#  Not a heading\n```\n# Another Heading",
        "#  Heading\n\nSome text\n\n##   Another\n\nMore text\n",
    ];

    for content in cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        let fix_result = rule.fix(&ctx).unwrap();
        let check_based_fix = apply_warning_fixes(content, &warnings).unwrap();
        assert_eq!(fix_result, check_based_fix, "Mismatch for content: {:?}", content);
    }
}

#[test]
fn test_roundtrip_fix_is_idempotent() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "#  Heading 1\n##   Heading 2\n###    Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed_once = rule.fix(&ctx).unwrap();
    let ctx2 = LintContext::new(&fixed_once, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed_twice = rule.fix(&ctx2).unwrap();
    assert_eq!(fixed_once, fixed_twice);
    let warnings = rule.check(&ctx2).unwrap();
    assert!(warnings.is_empty(), "Fixed content should produce no warnings");
}

#[test]
fn test_roundtrip_fix_with_trailing_newline() {
    let rule = MD019NoMultipleSpaceAtx::new();
    let content = "#  Heading\n\nText\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();
    let fix_result = rule.fix(&ctx).unwrap();
    let check_based_fix = apply_warning_fixes(content, &warnings).unwrap();
    assert_eq!(fix_result, check_based_fix);
    assert!(fix_result.ends_with('\n'), "Should preserve trailing newline");
}
