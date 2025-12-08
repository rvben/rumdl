use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD028NoBlanksBlockquote;

#[test]
fn test_md028_valid() {
    let rule = MD028NoBlanksBlockquote;
    // With recent changes, blank lines between blockquotes are now flagged as ambiguous
    let content = "> Quote\n> Another line\n\n> New quote\n> Another line\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // This should now flag line 3 as ambiguous
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md028_lines_with_marker_are_valid() {
    let rule = MD028NoBlanksBlockquote;
    // Lines with just > should NOT be flagged
    let content = "> Quote\n> Another line\n>\n> Still same quote\n> Another line\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Lines with > marker should not be flagged");
}

#[test]
fn test_md028_invalid_blank_line() {
    let rule = MD028NoBlanksBlockquote;
    // Truly blank line (no >) should be flagged
    let content = "> Quote\n> Another line\n\n> Still same quote\n> Another line\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md028_multiple_blanks() {
    let rule = MD028NoBlanksBlockquote;
    // Multiple truly blank lines
    let content = "> Quote\n> Another line\n\n\n> Still same quote\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[1].line, 4);
}

#[test]
fn test_md028_fix() {
    let rule = MD028NoBlanksBlockquote;
    // Fix truly blank line
    let content = "> Quote\n> Another line\n\n> Still same quote\n> Another line\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "> Quote\n> Another line\n>\n> Still same quote\n> Another line\n"
    );
}

#[test]
fn test_md028_nested_blockquotes_with_marker() {
    let rule = MD028NoBlanksBlockquote;
    // Lines with >> should NOT be flagged
    let content = "> Outer quote\n>> Nested quote\n>>\n>> Still nested\n> Back to outer\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Lines with >> marker should not be flagged");
}

#[test]
fn test_md028_nested_blockquotes_blank() {
    let rule = MD028NoBlanksBlockquote;
    // Truly blank line in nested blockquote
    let content = "> Outer quote\n>> Nested quote\n\n>> Still nested\n> Back to outer\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed_result = rule.check(&fixed_ctx).unwrap();
    assert!(fixed_result.is_empty());
    assert_eq!(
        fixed,
        "> Outer quote\n>> Nested quote\n>>\n>> Still nested\n> Back to outer\n"
    );
}

#[test]
fn test_md028_indented_blockquotes() {
    let rule = MD028NoBlanksBlockquote;
    // Truly blank line in indented blockquote
    let content = "  > Indented quote\n  > Another line\n\n  > Still same quote\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed_result = rule.check(&fixed_ctx).unwrap();
    assert!(fixed_result.is_empty());
    assert_eq!(
        fixed,
        "  > Indented quote\n  > Another line\n  >\n  > Still same quote\n"
    );
}

#[test]
fn test_md028_multi_blockquotes() {
    let rule = MD028NoBlanksBlockquote;
    // With recent changes, all blank lines between blockquotes are flagged
    let content = "> First quote\n> Another line\n\n> Second quote\n> Another line\n\n> Still second quote\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Both blank lines (line 3 and line 6) should be flagged
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[1].line, 6);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed_result = rule.check(&fixed_ctx).unwrap();
    assert!(fixed_result.is_empty());
    assert_eq!(
        fixed,
        "> First quote\n> Another line\n>\n> Second quote\n> Another line\n>\n> Still second quote\n"
    );
}
