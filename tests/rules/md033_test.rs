use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD033NoInlineHtml;

#[test]
fn test_no_html() {
    let rule = MD033NoInlineHtml::default();
    let content = "Just regular markdown\n\n# Heading\n\n* List item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_simple_html_tag() {
    let rule = MD033NoInlineHtml::default();
    let content = "Some <b>bold</b> text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Current implementation detects both opening and closing HTML tags
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 6); // <b>
    assert_eq!(result[1].line, 1);
    assert_eq!(result[1].column, 13); // </b>
}

#[test]
fn test_self_closing_tag() {
    let rule = MD033NoInlineHtml::default();
    let content = "An image: <img src=\"test.png\" />";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Current implementation detects self-closing HTML tags
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 11); // <img src="test.png" />
}

#[test]
fn test_allowed_elements() {
    let rule = MD033NoInlineHtml::with_allowed(vec!["b".to_string(), "i".to_string()]);
    let content = "Some <b>bold</b> and <i>italic</i> but not <u>underlined</u>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_html_in_code_block() {
    let rule = MD033NoInlineHtml::default();
    let content = "Normal text\n```\n<div>This is in a code block</div>\n```\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_html_tags() {
    let rule = MD033NoInlineHtml::default();
    let content = "Some <b>bold</b> text";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_fix_self_closing_tags() {
    let rule = MD033NoInlineHtml::default();
    let content = "Line break<br/>here";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_multiple_tags() {
    let rule = MD033NoInlineHtml::default();
    let content = "<div><p>Nested</p></div>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Current implementation detects all 4 HTML tags: <div>, <p>, </p>, </div>
    assert_eq!(result.len(), 4);
}

#[test]
fn test_attributes() {
    let rule = MD033NoInlineHtml::default();
    let content = "<div class=\"test\" id=\"main\">Content</div>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_mixed_content() {
    let rule = MD033NoInlineHtml::default();
    let content = "# Heading\n\n<div>HTML content</div>\n\n* List item\n\n<span>More HTML</span>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_preserve_content() {
    let rule = MD033NoInlineHtml::default();
    let content = "Text with <strong>important</strong> content";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_multiline_html() {
    let rule = MD033NoInlineHtml::default();
    let content = "<div>\nMultiline\ncontent\n</div>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Now detects both opening and closing tags (improved behavior)
    assert_eq!(result.len(), 2);
}

#[test]
fn test_ignore_code_spans() {
    let rule = MD033NoInlineHtml::default();
    let content = "Use `<div>` for a block element";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_complex_code_block_patterns() {
    let rule = MD033NoInlineHtml::default();

    // Test with mixed fence styles
    let content = "Text\n```\n<div>Code block 1</div>\n```\nMore text\n~~~\n<span>Code block 2</span>\n~~~\nEnd text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Test with code block at start of document
    let content = "```\n<div>Starts with code</div>\n```\nText with <b>bold</b>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // <b> and </b> outside code block

    // Test with code block at end of document
    let content = "Text with <i>italic</i>\n```\n<div>Ends with code</div>\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // <i> and </i> outside code block

    // Test adjacent code blocks
    let content = "```\n<div>Block 1</div>\n```\n```\n<span>Block 2</span>\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_code_span_binary_search() {
    let rule = MD033NoInlineHtml::default();

    // Test HTML tag immediately before a code span
    let content = "<span>`code`</span>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // <span> and </span> outside code span

    // Test HTML tag immediately after a code span
    let content = "`code`<div>text</div>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // <div> and </div> outside code span

    // Test HTML tag exactly at position boundaries
    let content = "Text `<div>` more text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // <div> is inside code span

    // Test many code spans to trigger binary search optimization
    let content = "`1` `2` `3` `4` `5` `6` `7` `8` `9` `10` `11` `12` <span>text</span>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // <span> and </span> outside code spans
}

#[test]
fn test_fix_preserves_structure_html() {
    let rule = MD033NoInlineHtml::default();

    // Verify HTML fix is a no-op (output equals input)
    let content = "Normal <b>bold</b>\n```\n<div>Code block</div>\n```\nMore <i>italic</i>";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);

    let content = "Text with `<span>` and <div>block</div>";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);

    let content = "<div><p>Nested content</p></div>";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_markdown_comments() {
    let rule = MD033NoInlineHtml::default();

    // Test with markdownlint comments
    let content = "Some content\n<!-- markdownlint-disable -->\nIgnored content\n<!-- markdownlint-enable -->\nMore content";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // These should not be flagged as HTML tags
    assert!(
        result.is_empty(),
        "Markdown comments should not be flagged as HTML"
    );

    // Test with regular HTML comments
    let content = "Some content\n<!-- This is a comment -->\nMore content";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Comments should not be flagged
    assert!(
        result.is_empty(),
        "HTML comments should not be flagged as HTML tags"
    );
}
