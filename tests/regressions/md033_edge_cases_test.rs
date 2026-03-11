use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD033NoInlineHtml;

#[test]
fn test_md033_inline_code_with_angle_brackets() {
    // Test for issue #90: <env> in backticks
    let rule = MD033NoInlineHtml::default();
    let content = "`<env>`";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Code spans with angle brackets should not be flagged as HTML"
    );
}

#[test]
fn test_md033_code_span_before_code_block() {
    let rule = MD033NoInlineHtml::default();
    let content = "`<env>`\n\n```diff\n- old\n+ new\n```";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Code span before code block should not be flagged");
}

#[test]
fn test_md033_multiple_code_spans_with_brackets() {
    let rule = MD033NoInlineHtml::default();
    let content = "`<one>` and `<two>` and `<three>`";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Multiple code spans with angle brackets should not be flagged"
    );
}

#[test]
fn test_md033_nested_angle_brackets_in_code() {
    let rule = MD033NoInlineHtml::default();
    let content = "`<<nested>>` and `List<List<T>>`";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Nested angle brackets in code spans should not be flagged"
    );
}

#[test]
fn test_md033_code_span_with_real_html_nearby() {
    let rule = MD033NoInlineHtml::default();
    let content = "`<code>` but <div>real html</div>";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should only flag real HTML (opening tag), not code spans"
    );
    assert_eq!(result[0].column, 14); // <div>
}

#[test]
fn test_md033_triple_backtick_code_spans() {
    let rule = MD033NoInlineHtml::default();
    let content = "```<not html>``` and ``<also not>``";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Multi-backtick code spans should not be flagged");
}

#[test]
fn test_md033_code_span_at_line_end() {
    let rule = MD033NoInlineHtml::default();
    let content = "Testing `<test>`\n```\ncode\n```";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Code span at end of line before code block should work"
    );
}

#[test]
fn test_md033_mixed_backticks_and_html() {
    let rule = MD033NoInlineHtml::default();
    let content = "Use `<div>` for blocks, but avoid <span>raw html</span> outside code";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should flag only HTML outside code spans (opening tag)"
    );
    assert!(
        result.iter().all(|w| w.column >= 35),
        "All warnings should be at or after position 35"
    );
}

#[test]
fn test_md033_unclosed_backticks() {
    let rule = MD033NoInlineHtml::default();
    let content = "Start `<span> but no closing backtick <div>test</div>";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Unclosed backticks don't create code spans, so both opening tags should be flagged
    assert_eq!(
        result.len(),
        2,
        "Unclosed backticks don't protect angle brackets (opening tags only)"
    );
}

#[test]
fn test_md033_empty_code_span() {
    let rule = MD033NoInlineHtml::default();
    let content = "Empty `` code span and `<test>`";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Both empty and non-empty code spans should work");
}

#[test]
fn test_md033_code_span_with_spaces() {
    let rule = MD033NoInlineHtml::default();
    let content = "` <tag> ` with spaces and `<tag>` without";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Code spans with or without spaces should work");
}

#[test]
fn test_md033_multiline_with_code_spans() {
    let rule = MD033NoInlineHtml::default();
    let content = "Line 1 `<code>`\n<div>html</div>\nLine 3 `<em>` test";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // HTML block on line 2 breaks paragraph parsing, so line 3 code span isn't detected
    // This is correct CommonMark behavior - <div> starts an HTML block
    // Only opening tags are reported
    assert_eq!(
        result.len(),
        2,
        "Should flag HTML on line 2 and line 3 (opening tags only)"
    );
    assert_eq!(result[0].line, 2); // <div>
    assert_eq!(result[1].line, 3); // <em> (not in code span due to HTML block)
}

#[test]
fn test_md033_generic_types_in_code() {
    let rule = MD033NoInlineHtml::default();
    // Common programming patterns that look like HTML
    let content = "`vector<int>`, `map<string, int>`, `Array<T>`, `Promise<User>`";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Generic type annotations in code spans should not be flagged"
    );
}

#[test]
fn test_md033_xml_like_in_code_spans() {
    let rule = MD033NoInlineHtml::default();
    let content = "XML example: `<user id=\"123\">John</user>`";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "XML-like content in code spans should not be flagged"
    );
}

#[test]
fn test_md033_comparison_operators_in_code() {
    let rule = MD033NoInlineHtml::default();
    let content = "Check `if (x < 5 && y > 3)` and `<<=` operator";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Comparison operators in code spans should not be flagged"
    );
}
