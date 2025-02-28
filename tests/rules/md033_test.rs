use rustmark::rules::MD033NoInlineHtml;
use rustmark::rule::Rule;

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
    let rule = MD033NoInlineHtml::new(vec!["b".to_string(), "i".to_string()]);
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
    assert_eq!(fixed, "# Heading\n\nHTML content\n\n* List item\n\nMore HTML");
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