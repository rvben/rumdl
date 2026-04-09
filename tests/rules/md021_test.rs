use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD021NoMultipleSpaceClosedAtx;

#[test]
fn test_valid_closed_atx_headings() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_closed_atx_headings() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "#  Heading 1  #\n##  Heading 2 ##\n###   Heading 3   ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);
}

#[test]
fn test_mixed_closed_atx_headings() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "# Heading 1 #\n##  Heading 2  ##\n### Heading 3 ###\n####    Heading 4    ####";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_code_block() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "```markdown\n#  Not a heading  #\n##   Also not a heading   ##\n```\n# Real Heading #";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_closed_atx_headings() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "#  Heading 1  #\n##  Heading 2 ##\n###   Heading 3   ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_fix_mixed_closed_atx_headings() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "# Heading 1 #\n##  Heading 2  ##\n### Heading 3 ###\n####    Heading 4    ####";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###\n#### Heading 4 ####"
    );
}

#[test]
fn test_preserve_code_blocks() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "# Real Heading #\n```\n#  Not a heading  #\n```\n# Another Heading #";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "# Real Heading #\n```\n#  Not a heading  #\n```\n# Another Heading #"
    );
}

#[test]
fn test_heading_with_multiple_hashes() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "######  Heading 6  ######";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].message,
        "Multiple spaces (2 at start, 2 at end) inside hashes on closed heading (with ###### at start and end)"
    );
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "###### Heading 6 ######");
}

#[test]
fn test_not_a_heading() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "This is #  not a heading  #\nAnd this is also #   not a heading   #";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_indented_closed_atx_headings() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "  #  Heading 1  #\n    ##   Heading 2   ##\n      ###    Heading 3    ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "  # Heading 1 #\n    ##   Heading 2   ##\n      ###    Heading 3    ###"
    );
}

#[test]
fn test_empty_closed_atx_headings() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "# #\n## ##\n### ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_spaces_at_start() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "#   Heading 1 #\n##    Heading 2 ##\n###     Heading 3 ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(
        result[0].message,
        "Multiple spaces (3) after # at start of closed heading"
    );
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_multiple_spaces_at_end() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "# Heading 1   #\n## Heading 2    ##\n### Heading 3     ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(
        result[0].message,
        "Multiple spaces (3) before # at end of closed heading"
    );
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_fix_roundtrip_check_then_fix() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "#  Heading 1  #\n##  Heading 2 ##\n###   Heading 3   ###\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(!warnings.is_empty(), "should detect violations");
    for w in &warnings {
        assert!(w.fix.is_some(), "every warning must carry a Fix");
    }

    let fixed = rule.fix(&ctx).unwrap();
    let ctx2 = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
    let warnings2 = rule.check(&ctx2).unwrap();
    assert!(
        warnings2.is_empty(),
        "fix must resolve all warnings, got: {warnings2:?}"
    );
}

#[test]
fn test_fix_idempotent() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "#  Heading 1  #\n## Heading 2 ##\n###   Heading 3   ###\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let fixed1 = rule.fix(&ctx).unwrap();
    let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Standard, None);
    let fixed2 = rule.fix(&ctx2).unwrap();
    assert_eq!(fixed1, fixed2, "fix must be idempotent");
}

#[test]
fn test_fix_preserves_trailing_newline() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "#  Heading  #\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert!(fixed.ends_with('\n'), "trailing newline must be preserved");
    assert_eq!(fixed, "# Heading #\n");
}

#[test]
fn test_fix_clean_content_unchanged() {
    let rule = MD021NoMultipleSpaceClosedAtx::new();
    let content = "# Heading 1 #\n## Heading 2 ##\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "clean content must pass through unchanged");
}
