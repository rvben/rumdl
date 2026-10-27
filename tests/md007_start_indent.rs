//! A configured first-level offset must carry through fixed list indentation.
use rumdl_lib::config::{Config, MarkdownFlavor, RuleConfig};
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD007ULIndent;
use rumdl_lib::rules::md007_ul_indent::md007_config::{IndentStyle, MD007Config};
use rumdl_lib::types::IndentSize;

fn fixed_rule(style_explicit: bool) -> MD007ULIndent {
    MD007ULIndent::from_config_struct(MD007Config {
        indent: IndentSize::from_const(4),
        start_indented: true,
        start_indent: IndentSize::from_const(2),
        style: IndentStyle::Fixed,
        style_explicit,
        indent_explicit: true,
    })
}

#[test]
fn fixed_start_indent_applies_to_every_depth() {
    let content = "# Title\n\nIntro.\n\n  - Parent\n      - Child\n          - Grandchild\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    for style_explicit in [false, true] {
        let rule = fixed_rule(style_explicit);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty(), "style_explicit={style_explicit}: {warnings:?}");
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }
}

#[test]
fn fixed_start_indent_fixes_nested_items_idempotently() {
    let content = "  - Parent\n    - Child\n        - Grandchild\n";
    let expected = "  - Parent\n      - Child\n          - Grandchild\n";
    for style_explicit in [false, true] {
        let rule = fixed_rule(style_explicit);
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].message, "Expected 6 spaces for indent depth 1, found 4");
        assert_eq!(warnings[1].message, "Expected 10 spaces for indent depth 2, found 8");
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let ctx = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), fixed);
    }
}

#[test]
fn fixed_start_indent_is_relative_to_the_blockquote() {
    let rule = fixed_rule(true);
    let content = ">   - Parent\n>       - Child\n>           - Grandchild\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(rule.check(&ctx).unwrap().is_empty());
    assert_eq!(rule.fix(&ctx).unwrap(), content);
}

#[test]
fn smart_style_accounts_for_the_parent_start_indent() {
    let rule = MD007ULIndent::from_config_struct(MD007Config {
        indent: IndentSize::from_const(4),
        start_indented: true,
        ..MD007Config::default()
    });
    let content = "  - Parent\n      - Child\n          - Grandchild\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(rule.check(&ctx).unwrap().is_empty());
}

#[test]
fn explicit_text_aligned_style_still_uses_parent_content_columns() {
    let rule = MD007ULIndent::from_config_struct(MD007Config {
        indent: IndentSize::from_const(4),
        start_indented: true,
        style: IndentStyle::TextAligned,
        style_explicit: true,
        ..MD007Config::default()
    });
    let content = "  - Parent\n    - Child\n      - Grandchild\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(rule.check(&ctx).unwrap().is_empty());
    assert_eq!(rule.fix(&ctx).unwrap(), content);
}

#[test]
fn fixed_start_indent_does_not_offset_an_ordered_root() {
    let content = "1. Parent\n   - Child\n";
    let expected = "1. Parent\n    - Child\n";
    for start_indent in [2, 4] {
        let rule = MD007ULIndent::from_config_struct(MD007Config {
            indent: IndentSize::from_const(4),
            start_indented: true,
            start_indent: IndentSize::from_const(start_indent),
            style: IndentStyle::Fixed,
            style_explicit: true,
            indent_explicit: true,
        });
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "Expected 4 spaces for indent depth 1, found 3");
        assert!(warnings[0].fix.is_some());
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
        let ctx = LintContext::new(expected, MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), expected);
    }
}

#[test]
fn mkdocs_ordered_root_does_not_receive_the_bullet_start_offset() {
    let rule = fixed_rule(false);
    let expected = "1. Parent\n    - Child\n";
    let ctx = LintContext::new(expected, MarkdownFlavor::MkDocs, None);
    assert!(rule.check(&ctx).unwrap().is_empty());
    let content = "1. Parent\n      - Child\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].message, "Expected 4 spaces for indent depth 1, found 6");
    assert_eq!(rule.fix(&ctx).unwrap(), expected);
}

#[test]
fn fixed_start_indent_uses_the_configured_offset_and_increment() {
    for (start_indented, content) in [
        (true, " - Parent\n    - Child\n       - Grandchild\n"),
        (false, "- Parent\n   - Child\n      - Grandchild\n"),
    ] {
        let rule = MD007ULIndent::from_config_struct(MD007Config {
            indent: IndentSize::from_const(3),
            start_indented,
            start_indent: IndentSize::from_const(1),
            style: IndentStyle::Fixed,
            style_explicit: true,
            indent_explicit: true,
        });
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }
}

#[test]
fn fixed_start_indent_preserves_mixed_nesting_and_the_ordered_parent_clamp() {
    let rule = fixed_rule(true);
    for (marker, actual_indent, expected_indent) in [("1.", 9, 10), ("100.", 12, 11)] {
        // A blank line lets a list starting at 100 interrupt the parent paragraph.
        let content = format!(
            "  - Parent\n\n      {marker} Ordered\n{}- Child\n",
            " ".repeat(actual_indent)
        );
        let expected = format!(
            "  - Parent\n\n      {marker} Ordered\n{}- Child\n",
            " ".repeat(expected_indent)
        );
        let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);
        assert!(ctx.list_item_on_line(3).is_some_and(|item| item.is_ordered()));
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "marker={marker}: {warnings:?}");
        assert_eq!(warnings[0].line, 4);
        assert_eq!(
            warnings[0].message,
            format!("Expected {expected_indent} spaces for indent depth 2, found {actual_indent}")
        );
        assert!(warnings[0].fix.is_some());
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected);
        let ctx = LintContext::new(&fixed, MarkdownFlavor::Standard, None);
        assert!(rule.check(&ctx).unwrap().is_empty());
        assert_eq!(rule.fix(&ctx).unwrap(), fixed);
    }
}

#[test]
fn mkdocs_implicit_fixed_indent_inherits_the_unordered_root_offset() {
    let mut config = Config::default();
    config.global.flavor = MarkdownFlavor::MkDocs;
    config.rules.insert(
        "MD007".to_string(),
        RuleConfig {
            severity: None,
            values: std::collections::BTreeMap::from([
                ("start-indented".to_string(), toml::Value::Boolean(true)),
                ("start-indent".to_string(), toml::Value::Integer(2)),
            ]),
        },
    );
    // Neither indent nor style is explicit: MkDocs supplies the four-space indent.
    let rule = MD007ULIndent::from_config(&config);
    let content = "  - Parent\n    - Child\n";
    let expected = "  - Parent\n      - Child\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let warnings = rule.check(&ctx).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].message, "Expected 6 spaces for indent depth 1, found 4");
    assert!(warnings[0].fix.is_some());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, expected);
    let ctx = LintContext::new(&fixed, MarkdownFlavor::MkDocs, None);
    assert!(rule.check(&ctx).unwrap().is_empty());
    assert_eq!(rule.fix(&ctx).unwrap(), fixed);
}
