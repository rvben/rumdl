use rumdl::rule::Rule;
use rumdl::rules::MD033NoInlineHtml;

#[test]
fn test_no_html() {
    let rule = MD033NoInlineHtml::default();
    let content = "Just regular markdown\n\n# Heading\n\n* List item";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_simple_html_tag() {
    let rule = MD033NoInlineHtml::default();
    let content = "Some <b>bold</b> text";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_self_closing_tag() {
    let rule = MD033NoInlineHtml::default();
    let content = "An image: <img src=\"test.png\" />";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_allowed_elements() {
    let rule = MD033NoInlineHtml::with_allowed(vec!["b".to_string(), "i".to_string()]);
    let content = "Some <b>bold</b> and <i>italic</i> but not <u>underlined</u>";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2); // Only <u> tags should be flagged
}

#[test]
fn test_html_in_code_block() {
    let rule = MD033NoInlineHtml::default();
    let content = "Normal text\n```\n<div>This is in a code block</div>\n```\nMore text";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_html_tags() {
    let rule = MD033NoInlineHtml::default();
    let content = "Some <b>bold</b> text";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Some bold text");
}

#[test]
fn test_fix_self_closing_tags() {
    let rule = MD033NoInlineHtml::default();
    let content = "Line break<br/>here";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Line breakhere");
}

#[test]
fn test_multiple_tags() {
    let rule = MD033NoInlineHtml::default();
    let content = "<div><p>Nested</p></div>";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
}

#[test]
fn test_attributes() {
    let rule = MD033NoInlineHtml::default();
    let content = "<div class=\"test\" id=\"main\">Content</div>";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_mixed_content() {
    let rule = MD033NoInlineHtml::default();
    let content = "# Heading\n\n<div>HTML content</div>\n\n* List item\n\n<span>More HTML</span>";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "# Heading\n\nHTML content\n\n* List item\n\nMore HTML"
    );
}

#[test]
fn test_preserve_content() {
    let rule = MD033NoInlineHtml::default();
    let content = "Text with <strong>important</strong> content";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Text with important content");
}

#[test]
fn test_multiline_html() {
    let rule = MD033NoInlineHtml::default();
    let content = "<div>\nMultiline\ncontent\n</div>";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_ignore_code_spans() {
    let rule = MD033NoInlineHtml::default();
    let content = "Use `<div>` for a block element";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_complex_code_block_patterns() {
    let rule = MD033NoInlineHtml::default();

    // Test with mixed fence styles
    let content = "Text\n```\n<div>Code block 1</div>\n```\nMore text\n~~~\n<span>Code block 2</span>\n~~~\nEnd text";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());

    // Test with code block at start of document
    let content = "```\n<div>Starts with code</div>\n```\nText with <b>bold</b>";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2); // Only the <b> tags outside the code block

    // Test with code block at end of document
    let content = "Text with <i>italic</i>\n```\n<div>Ends with code</div>\n```";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2); // Only the <i> tags outside the code block

    // Test adjacent code blocks
    let content = "```\n<div>Block 1</div>\n```\n```\n<span>Block 2</span>\n```";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_code_span_binary_search() {
    let rule = MD033NoInlineHtml::default();

    // Test HTML tag immediately before a code span
    let content = "<span>`code`</span>";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2); // Both span tags should be detected

    // Test HTML tag immediately after a code span
    let content = "`code`<div>text</div>";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2); // Both div tags should be detected

    // Test HTML tag exactly at position boundaries
    let content = "Text `<div>` more text";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());

    // Test many code spans to trigger binary search optimization
    let content = "`1` `2` `3` `4` `5` `6` `7` `8` `9` `10` `11` `12` <span>text</span>";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2); // Both span tags should be detected
}

#[test]
fn test_fix_preserves_structure_html() {
    let rule = MD033NoInlineHtml::default();

    // Verify HTML fix preserves code blocks
    let content = "Normal <b>bold</b>\n```\n<div>Code block</div>\n```\nMore <i>italic</i>";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(
        fixed,
        "Normal bold\n```\n<div>Code block</div>\n```\nMore italic"
    );

    // Verify HTML fix preserves code spans
    let content = "Text with `<span>` and <div>block</div>";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Text with `<span>` and block");

    // Verify HTML fix handles adjacent tags
    let content = "<div><p>Nested content</p></div>";
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "Nested content");
}

#[test]
fn test_markdown_comments() {
    let rule = MD033NoInlineHtml::default();

    // Test with markdownlint comments
    let content = "Some content\n<!-- markdownlint-disable -->\nIgnored content\n<!-- markdownlint-enable -->\nMore content";
    let result = rule.check(content).unwrap();

    // These should not be flagged as HTML tags
    assert!(
        result.is_empty(),
        "Markdown comments should not be flagged as HTML"
    );

    // Test with regular HTML comments
    let content = "Some content\n<!-- This is a comment -->\nMore content";
    let result = rule.check(content).unwrap();

    // Comments should not be flagged
    assert!(
        result.is_empty(),
        "HTML comments should not be flagged as HTML tags"
    );
}
