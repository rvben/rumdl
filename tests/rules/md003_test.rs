use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD003HeadingStyle;
use rumdl_lib::rules::heading_utils::HeadingStyle;

#[test]
fn test_consistent_atx() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_atx_closed() {
    let rule = MD003HeadingStyle::new(HeadingStyle::AtxClosed);
    let content = "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_styles() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n## Heading 2 ##\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_fix_mixed_styles() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n## Heading 2 ##\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1\n## Heading 2\n### Heading 3");
}

#[test]
fn test_fix_to_atx_closed() {
    let rule = MD003HeadingStyle::new(HeadingStyle::AtxClosed);
    let content = "# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_indented_headings() {
    let rule = MD003HeadingStyle::default();
    let content = "  # Heading 1\n  ## Heading 2\n  ### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_indentation() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1\n  ## Heading 2\n    ### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_preserve_content() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading with *emphasis* and **bold**\n## Another heading with [link](url)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, content);
}

#[test]
fn test_empty_headings() {
    let rule = MD003HeadingStyle::default();
    let content = "#\n##\n###";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_heading_with_trailing_space() {
    let rule = MD003HeadingStyle::default();
    let content = "# Heading 1  \n## Heading 2  \n### Heading 3  ";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_consistent_setext() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading 1\n=========\n\nHeading 2\n---------";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_setext_atx() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading 1\n=========\n\n## Heading 2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4);
}

#[test]
fn test_fix_to_setext() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "# Heading 1\n## Heading 2";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Heading 1\n=========\nHeading 2\n---------");
}

#[test]
fn test_setext_with_formatting() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading with *emphasis*\n====================\n\nHeading with **bold**\n--------------------";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_mixed_setext_atx() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Heading 1\n=========\n\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Heading 1\n=========\n\nHeading 2\n---------\n### Heading 3");
}

#[test]
fn test_setext_with_indentation() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "  Heading 1\n  =========\n\n  Heading 2\n  ---------";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_with_front_matter() {
    let rule = MD003HeadingStyle::default();
    let content = "---\ntitle: \"Test Document\"\nauthor: \"Test Author\"\ndate: \"2024-04-03\"\n---\n\n# Heading 1\n## Heading 2\n### Heading 3";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Expected no warnings with front matter followed by ATX headings, but got {} warnings",
        result.len()
    );
}

#[test]
fn test_yaml_like_content_detected_as_setext_heading() {
    // Per CommonMark and markdownlint-cli: `---` in mid-document is a Setext underline,
    // not frontmatter. This content creates a Setext heading "config: value" with the `---`.
    // markdownlint-cli flags this as MD003 (heading style mismatch).
    let rule = MD003HeadingStyle::default();
    let content = "# Real Heading\n\n---\nconfig: value\nsetting: another value\n---\n\nMore content.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // The `---` after "setting: another value" creates a Setext h2 heading,
    // which conflicts with the ATX style used for "# Real Heading"
    assert!(
        !result.is_empty(),
        "Expected warning for Setext heading style mismatch, but got none"
    );
}

#[test]
fn test_legitimate_setext_headings_still_work() {
    let rule = MD003HeadingStyle::new(HeadingStyle::Setext1);
    let content = "Main Title\n==========\n\nSubtitle\n--------\n\nContent here.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Legitimate Setext headings should still work, but got {} warnings: {:?}",
        result.len(),
        result
    );
}
