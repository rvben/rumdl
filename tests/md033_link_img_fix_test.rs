//! Tests for MD033 auto-fix of <a> and <img> tags to Markdown equivalents.
//!
//! These tests verify the safe conversion of simple HTML links and images to Markdown,
//! and ensure unsafe cases (dangerous URLs, extra attributes, nested HTML) are NOT converted.

use rumdl_lib::config::Config;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD033NoInlineHtml;

fn create_config_with_fix() -> Config {
    let mut config = Config::default();
    if let Some(rule_config) = config.rules.get_mut("MD033") {
        rule_config.values.insert("fix".to_string(), toml::Value::Boolean(true));
    } else {
        let mut rule_config = rumdl_lib::config::RuleConfig::default();
        rule_config.values.insert("fix".to_string(), toml::Value::Boolean(true));
        config.rules.insert("MD033".to_string(), rule_config);
    }
    config
}

// =============================================================================
// Basic <a> tag conversion tests
// =============================================================================

#[test]
fn test_basic_a_tag_conversion() {
    let content = r#"See <a href="https://example.com">Example</a> for details."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(!warnings.is_empty(), "Should detect inline HTML");
    assert!(warnings[0].fix.is_some(), "Should have a fix");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"See [Example](https://example.com) for details."#,
        "Should convert <a> to markdown link"
    );
}

#[test]
fn test_a_tag_with_single_quotes() {
    let content = r#"Click <a href='https://example.com'>here</a>."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"Click [here](https://example.com)."#,
        "Should handle single-quoted attributes"
    );
}

#[test]
fn test_a_tag_with_relative_url() {
    let content = r#"Read the <a href="./docs/guide.md">guide</a>."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"Read the [guide](./docs/guide.md)."#,
        "Should convert relative URLs"
    );
}

#[test]
fn test_a_tag_with_fragment() {
    let content = r##"Jump to <a href="#section-1">Section 1</a>."##;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r##"Jump to [Section 1](#section-1)."##,
        "Should convert fragment-only URLs"
    );
}

#[test]
fn test_a_tag_with_mailto() {
    let content = r#"Contact <a href="mailto:test@example.com">us</a>."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"Contact [us](mailto:test@example.com)."#,
        "Should convert mailto links"
    );
}

#[test]
fn test_a_tag_with_tel() {
    let content = r#"Call <a href="tel:+1234567890">us</a>."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, r#"Call [us](tel:+1234567890)."#, "Should convert tel links");
}

// =============================================================================
// <a> tag safety tests - should NOT convert
// =============================================================================

#[test]
fn test_a_tag_javascript_url_not_converted() {
    let content = r#"Click <a href="javascript:alert('xss')">here</a>."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(!warnings.is_empty(), "Should detect inline HTML");
    // Fix should be None for dangerous URLs
    assert!(warnings[0].fix.is_none(), "Should NOT provide fix for javascript: URLs");
}

#[test]
fn test_a_tag_data_url_not_converted() {
    let content = r#"See <a href="data:text/html,<script>alert(1)</script>">link</a>."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings[0].fix.is_none(), "Should NOT provide fix for data: URLs");
}

#[test]
fn test_a_tag_with_onclick_not_converted() {
    let content = r#"Click <a href="https://example.com" onclick="track()">here</a>."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT provide fix for tags with onclick"
    );
}

#[test]
fn test_a_tag_with_target_not_converted() {
    let content = r#"Open <a href="https://example.com" target="_blank">here</a>."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT provide fix for tags with target attribute"
    );
}

#[test]
fn test_a_tag_with_class_not_converted() {
    let content = r#"<a href="https://example.com" class="btn">Button</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT provide fix for tags with class attribute"
    );
}

#[test]
fn test_a_tag_with_nested_html_not_converted() {
    let content = r#"Click <a href="https://example.com"><strong>here</strong></a>."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    // The <a> tag warning should not have a fix because of nested HTML
    let warnings = rule.check(&ctx).unwrap();
    // Find the warning for the <a> tag
    let a_warning = warnings.iter().find(|w| w.message.contains("<a "));
    assert!(a_warning.is_some(), "Should warn about <a> tag");
    assert!(
        a_warning.unwrap().fix.is_none(),
        "Should NOT provide fix for <a> with nested HTML"
    );
}

#[test]
fn test_a_tag_without_href_not_converted() {
    let content = r#"This <a name="anchor">anchor</a> has no href."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings[0].fix.is_none(), "Should NOT provide fix for <a> without href");
}

// =============================================================================
// Basic <img> tag conversion tests
// =============================================================================

#[test]
fn test_basic_img_tag_conversion() {
    let content = r#"Here is an image: <img src="https://example.com/logo.png" alt="Logo">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(!warnings.is_empty(), "Should detect inline HTML");
    assert!(warnings[0].fix.is_some(), "Should have a fix");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"Here is an image: ![Logo](https://example.com/logo.png)"#,
        "Should convert <img> to markdown image"
    );
}

#[test]
fn test_img_tag_self_closing() {
    let content = r#"Image: <img src="logo.png" alt="Logo" />"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"Image: ![Logo](logo.png)"#,
        "Should handle self-closing img tag"
    );
}

#[test]
fn test_img_tag_without_alt() {
    let content = r#"<img src="photo.jpg">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"![](photo.jpg)"#,
        "Should convert img without alt to empty alt text"
    );
}

#[test]
fn test_img_tag_relative_path() {
    let content = r#"<img src="./images/photo.png" alt="Photo">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, r#"![Photo](./images/photo.png)"#, "Should handle relative paths");
}

// =============================================================================
// <img> tag safety tests - should NOT convert
// =============================================================================

#[test]
fn test_img_tag_javascript_src_not_converted() {
    let content = r#"<img src="javascript:alert(1)" alt="Bad">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings[0].fix.is_none(), "Should NOT provide fix for javascript: src");
}

#[test]
fn test_img_tag_data_url_not_converted() {
    let content = r#"<img src="data:image/png;base64,..." alt="Image">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT provide fix for data: URLs in img"
    );
}

#[test]
fn test_img_tag_with_width_not_converted() {
    let content = r#"<img src="logo.png" alt="Logo" width="100">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT provide fix for img with width attribute"
    );
}

#[test]
fn test_img_tag_with_style_not_converted() {
    let content = r#"<img src="logo.png" alt="Logo" style="border: 1px solid red">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT provide fix for img with style attribute"
    );
}

#[test]
fn test_img_tag_with_loading_not_converted() {
    let content = r#"<img src="logo.png" alt="Logo" loading="lazy">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT provide fix for img with loading attribute"
    );
}

#[test]
fn test_img_tag_without_src_not_converted() {
    let content = r#"<img alt="No source">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings[0].fix.is_none(), "Should NOT provide fix for img without src");
}

// =============================================================================
// Edge case tests
// =============================================================================

#[test]
fn test_special_characters_in_link_text() {
    let content = r#"<a href="https://example.com">Text with [brackets]</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"[Text with \[brackets\]](https://example.com)"#,
        "Should escape brackets in link text"
    );
}

#[test]
fn test_special_characters_in_url() {
    let content = r#"<a href="https://example.com/path(1)">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"[Link](https://example.com/path%281%29)"#,
        "Should escape parentheses in URL"
    );
}

#[test]
fn test_special_characters_in_alt_text() {
    let content = r#"<img src="logo.png" alt="Image with [brackets]">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"![Image with \[brackets\]](logo.png)"#,
        "Should escape brackets in alt text"
    );
}

#[test]
fn test_multiple_links_converted() {
    let content = r#"See <a href="https://a.com">A</a> and <a href="https://b.com">B</a> for more."#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"See [A](https://a.com) and [B](https://b.com) for more."#,
        "Should convert multiple links"
    );
}

#[test]
fn test_mixed_link_and_image() {
    let content = r#"<a href="https://example.com">Link</a> and <img src="logo.png" alt="Logo">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"[Link](https://example.com) and ![Logo](logo.png)"#,
        "Should convert both links and images"
    );
}

#[test]
fn test_fix_is_idempotent() {
    let content = r#"<a href="https://example.com">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);

    // First fix
    let ctx1 = LintContext::new(content, Default::default(), None);
    let fixed1 = rule.fix(&ctx1).unwrap();

    // Second fix on already fixed content
    let ctx2 = LintContext::new(&fixed1, Default::default(), None);
    let fixed2 = rule.fix(&ctx2).unwrap();

    assert_eq!(fixed1, fixed2, "Fix should be idempotent");
    assert_eq!(fixed2, "[Link](https://example.com)");
}

#[test]
fn test_case_insensitive_tag_names() {
    let content = r#"<A HREF="https://example.com">Link</A>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"[Link](https://example.com)"#,
        "Should handle uppercase tag names"
    );
}

#[test]
fn test_case_insensitive_attributes() {
    let content = r#"<a HREF="https://example.com">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"[Link](https://example.com)"#,
        "Should handle uppercase attribute names"
    );
}

#[test]
fn test_protocol_relative_url() {
    let content = r#"<a href="//example.com/page">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"[Link](//example.com/page)"#,
        "Should handle protocol-relative URLs"
    );
}

// =============================================================================
// Config tests
// =============================================================================

#[test]
fn test_no_fix_when_disabled() {
    let content = r#"<a href="https://example.com">Link</a>"#;

    // Default config has fix = false
    let config = Config::default();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings[0].fix.is_none(), "Should NOT provide fix when fix is disabled");
}

// =============================================================================
// URL safety edge cases
// =============================================================================

#[test]
fn test_unknown_scheme_not_converted() {
    let content = r#"<a href="unknown:data">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings[0].fix.is_none(), "Should NOT convert unknown URL schemes");
}

#[test]
fn test_vbscript_not_converted() {
    let content = r#"<a href="vbscript:msgbox('hi')">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings[0].fix.is_none(), "Should NOT convert vbscript: URLs");
}

#[test]
fn test_relative_path_without_dot() {
    let content = r#"<a href="path/to/file.html">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"[Link](path/to/file.html)"#,
        "Should convert relative paths without leading dot"
    );
}

// =============================================================================
// URL encoding bypass protection tests
// =============================================================================

#[test]
fn test_javascript_percent_encoded_not_converted() {
    // java%73cript: = javascript: with 's' encoded
    let content = r#"<a href="java%73cript:alert(1)">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT convert percent-encoded javascript: URLs"
    );
}

#[test]
fn test_javascript_html_entity_encoded_not_converted() {
    // javascript&#58; = javascript: with ':' as HTML entity
    let content = r#"<a href="javascript&#58;alert(1)">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT convert HTML entity encoded javascript: URLs"
    );
}

#[test]
fn test_javascript_hex_entity_encoded_not_converted() {
    // javascript&#x3a; = javascript: with ':' as hex HTML entity
    let content = r#"<a href="javascript&#x3a;alert(1)">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT convert hex HTML entity encoded javascript: URLs"
    );
}

#[test]
fn test_data_percent_encoded_not_converted() {
    // dat%61: = data: with 'a' encoded
    let content = r#"<a href="dat%61:text/html,<script>alert(1)</script>">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT convert percent-encoded data: URLs"
    );
}

// =============================================================================
// Proper attribute parsing tests
// =============================================================================

#[test]
fn test_a_tag_with_data_attribute_not_converted() {
    let content = r#"<a href="https://example.com" data-tracking="123">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT convert tags with data-* attributes"
    );
}

#[test]
fn test_a_tag_with_name_attribute_not_converted() {
    let content = r#"<a href="https://example.com" name="anchor">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(warnings[0].fix.is_none(), "Should NOT convert tags with name attribute");
}

#[test]
fn test_attribute_value_containing_dangerous_word_is_ok() {
    // The href value contains "onclick" but that's not an attribute name
    let content = r#"<a href="https://example.com/onclick-handler">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"[Link](https://example.com/onclick-handler)"#,
        "Should convert when dangerous word is in value, not attribute name"
    );
}

#[test]
fn test_attribute_parsing_no_space_before_second_attr() {
    // Malformed HTML: no space before title attribute - still should be detected
    let content = r#"<a href="https://example.com"title="tip">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    // Note: This malformed HTML may or may not be parsed correctly
    // The important thing is we don't crash and ideally detect the title attribute
    assert!(!warnings.is_empty(), "Should still detect the HTML tag");
}

// =============================================================================
// Title attribute support tests
// =============================================================================

#[test]
fn test_a_tag_with_title_converted() {
    let content = r#"<a href="https://example.com" title="Example Site">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"[Link](https://example.com "Example Site")"#,
        "Should convert <a> with title to markdown link with title"
    );
}

#[test]
fn test_img_tag_with_title_converted() {
    let content = r#"<img src="image.png" alt="Photo" title="A nice photo">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"![Photo](image.png "A nice photo")"#,
        "Should convert <img> with title to markdown image with title"
    );
}

#[test]
fn test_title_with_quotes_escaped() {
    let content = r#"<a href="https://example.com" title='Say "Hello"'>Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"[Link](https://example.com "Say \"Hello\"")"#,
        "Should escape quotes in title"
    );
}

#[test]
fn test_a_tag_title_only_no_class() {
    // title + href is OK, but title + href + class is NOT
    let content = r#"<a href="https://example.com" title="Tip" class="btn">Link</a>"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let warnings = rule.check(&ctx).unwrap();
    assert!(
        warnings[0].fix.is_none(),
        "Should NOT convert when extra attributes beyond href/title are present"
    );
}

#[test]
fn test_img_tag_title_with_url_parentheses() {
    let content = r#"<img src="https://example.com/image_(1).png" alt="Test" title="Image (1)">"#;

    let config = create_config_with_fix();
    let rule = MD033NoInlineHtml::from_config(&config);
    let ctx = LintContext::new(content, Default::default(), None);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, r#"![Test](https://example.com/image_%281%29.png "Image (1)")"#,
        "Should escape URL parentheses but preserve title parentheses"
    );
}
