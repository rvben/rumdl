//! Tests for MD013 reflow behavior with Quarto div syntax (:::)
//!
//! Quarto/Pandoc uses `:::` markers for div blocks (e.g., callouts, custom divs).
//! These markers must be preserved on their own lines during reflow, with content
//! between them reflowed normally.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD013LineLength;

fn create_quarto_config_with_reflow() -> Config {
    let mut config = Config::default();
    config.global.flavor = MarkdownFlavor::Quarto;
    if let Some(rule_config) = config.rules.get_mut("MD013") {
        rule_config
            .values
            .insert("reflow".to_string(), toml::Value::Boolean(true));
    } else {
        let mut rule_config = rumdl_lib::config::RuleConfig::default();
        rule_config
            .values
            .insert("reflow".to_string(), toml::Value::Boolean(true));
        config.rules.insert("MD013".to_string(), rule_config);
    }
    config
}

// ============================================================================
// LineInfo.is_div_marker architectural tests
// ============================================================================

#[test]
fn test_is_div_marker_opening() {
    let content = "::: {.callout-note}\nContent.\n:::\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(
        ctx.lines[0].is_div_marker,
        "Opening ::: {{.class}} should be a div marker"
    );
    assert!(!ctx.lines[1].is_div_marker, "Content inside div is not a marker");
    assert!(ctx.lines[2].is_div_marker, "Closing ::: should be a div marker");
}

#[test]
fn test_is_div_marker_nested_colons() {
    let content = "::::: {.panel}\n::: {.inner}\nContent.\n:::\n:::::\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(ctx.lines[0].is_div_marker, "::::: with 5 colons is a div marker");
    assert!(ctx.lines[1].is_div_marker, "::: with 3 colons is a div marker");
    assert!(!ctx.lines[2].is_div_marker, "Content is not a marker");
    assert!(ctx.lines[3].is_div_marker, "Closing ::: is a div marker");
    assert!(ctx.lines[4].is_div_marker, "Closing ::::: is a div marker");
}

#[test]
fn test_is_div_marker_not_in_code_block() {
    let content = "```\n::: {.callout-note}\n:::\n```\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(
        !ctx.lines[1].is_div_marker,
        "::: inside code block should not be a div marker"
    );
    assert!(
        !ctx.lines[2].is_div_marker,
        "::: inside code block should not be a div marker"
    );
}

#[test]
fn test_is_div_marker_not_in_frontmatter() {
    let content = "---\n::: something\n---\n::: {.callout}\nContent.\n:::\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(
        !ctx.lines[1].is_div_marker,
        "::: inside front matter should not be a div marker"
    );
    assert!(ctx.lines[3].is_div_marker, "::: after front matter IS a div marker");
}

#[test]
fn test_is_div_marker_indented() {
    let content = "  ::: {.callout-note}\n  Content.\n  :::\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(ctx.lines[0].is_div_marker, "Indented ::: should be a div marker");
    assert!(
        ctx.lines[2].is_div_marker,
        "Indented closing ::: should be a div marker"
    );
}

#[test]
fn test_is_div_marker_simple_class() {
    let content = "::: myclass\nContent.\n:::\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(ctx.lines[0].is_div_marker, "::: with simple class name is a div marker");
}

#[test]
fn test_is_div_marker_standard_flavor() {
    // is_div_marker is not flavor-gated — ::: is structural regardless
    let content = "::: {.callout-note}\nContent.\n:::\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(
        ctx.lines[0].is_div_marker,
        "::: should be detected even in standard flavor"
    );
}

// ============================================================================
// MD013 reflow integration tests
// ============================================================================

#[test]
fn test_quarto_div_markers_preserved_during_reflow() {
    let content = "::: {.callout-note}\nLorem ipsum dolor sit amet, consectetur adipiscing elit. Sed quam leo, rhoncus sodales erat sed.\n:::\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    assert!(
        fixed.starts_with("::: {.callout-note}\n"),
        "Opening div marker must be preserved on its own line, got:\n{fixed}"
    );
    assert!(
        fixed.trim_end().ends_with("\n:::"),
        "Closing div marker must be on its own line, got:\n{fixed}"
    );

    for line in fixed.lines() {
        if line.starts_with(":::") || line.is_empty() {
            continue;
        }
        assert!(
            line.len() <= 80,
            "Content line should be reflowed to <=80 chars, got ({} chars): {line:?}",
            line.len()
        );
    }
}

#[test]
fn test_quarto_nested_divs_preserved() {
    let content = "::::: {.panel}\n::: {.callout-note}\nLorem ipsum dolor sit amet, consectetur adipiscing elit. Sed quam leo, rhoncus sodales erat sed.\n:::\n:::::\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    let lines: Vec<&str> = fixed.lines().collect();
    assert_eq!(lines[0], "::::: {.panel}", "Outer opening marker preserved");
    assert_eq!(lines[1], "::: {.callout-note}", "Inner opening marker preserved");
    assert!(lines.contains(&":::"), "Inner closing marker preserved");
    assert!(lines.contains(&":::::"), "Outer closing marker preserved");
}

#[test]
fn test_quarto_empty_div_preserved() {
    let content = "::: {.callout-note}\n:::\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    assert_eq!(
        fixed.trim_end(),
        "::: {.callout-note}\n:::",
        "Empty div should be preserved as-is"
    );
}

#[test]
fn test_quarto_content_before_and_after_div_reflowed() {
    let content = "This is a very long paragraph before the div that should be reflowed to fit within the line length limit of eighty characters.\n\n::: {.callout-note}\nShort content.\n:::\n\nThis is a very long paragraph after the div that should also be reflowed to fit within the line length limit of eighty characters.\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    assert!(fixed.contains("::: {.callout-note}\n"));
    assert!(fixed.contains("\n:::"));

    for line in fixed.lines() {
        if line.starts_with(":::") || line.is_empty() {
            continue;
        }
        assert!(
            line.len() <= 80,
            "Line should be <=80 chars, got ({} chars): {line:?}",
            line.len()
        );
    }
}

#[test]
fn test_quarto_multiple_consecutive_divs() {
    let content =
        "::: {.callout-note}\nNote content here.\n:::\n\n::: {.callout-warning}\nWarning content here.\n:::\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    assert!(fixed.contains("::: {.callout-note}"), "First div marker preserved");
    assert!(fixed.contains("::: {.callout-warning}"), "Second div marker preserved");

    let marker_count = fixed.lines().filter(|l| l.trim().starts_with(":::")).count();
    assert_eq!(marker_count, 4, "All four div markers should be preserved");
}

#[test]
fn test_quarto_closing_marker_not_merged_with_text() {
    let content = "::: {.callout-note}\nSome text.\n:::\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    assert!(
        !fixed.contains("text.:::"),
        "Closing marker must not be merged with preceding text"
    );
    assert!(
        !fixed.contains("text. :::"),
        "Closing marker must not be merged with preceding text (with space)"
    );
}

#[test]
fn test_quarto_div_with_normalize_reflow_mode() {
    let content = "::: {.callout-note}\nLorem ipsum dolor sit amet, consectetur adipiscing elit. Sed quam leo, rhoncus sodales erat sed.\n:::\n";

    let mut config = create_quarto_config_with_reflow();
    if let Some(rule_config) = config.rules.get_mut("MD013") {
        rule_config
            .values
            .insert("reflow-mode".to_string(), toml::Value::String("normalize".to_string()));
    }
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    assert!(
        fixed.starts_with("::: {.callout-note}\n"),
        "Opening marker preserved in normalize mode"
    );
    assert!(
        fixed.trim_end().ends_with("\n:::"),
        "Closing marker preserved in normalize mode"
    );
}

#[test]
fn test_quarto_div_markers_not_treated_as_paragraph() {
    let content = "::: {.callout-note}\nShort.\n:::\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let warnings = rule.check(&ctx).unwrap();

    assert!(
        warnings.is_empty(),
        "Short div content should produce no warnings, got: {warnings:?}"
    );
}

#[test]
fn test_quarto_div_with_long_opening_marker() {
    // Long opening marker with attributes should be preserved, not reflowed
    let content = "::: {.callout-note title=\"This is a rather long title that might exceed eighty characters in width\"}\nContent.\n:::\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    // The div marker line should NOT be reflowed or broken
    let first_line = fixed.lines().next().unwrap();
    assert!(
        first_line.starts_with("::: {.callout-note title="),
        "Opening marker with attributes should be preserved intact"
    );
}

#[test]
fn test_quarto_reflow_idempotent() {
    // Running reflow twice should produce the same result
    let content = "::: {.callout-note}\nLorem ipsum dolor sit amet, consectetur adipiscing elit. Sed quam leo, rhoncus sodales erat sed.\n:::\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);

    let ctx1 = LintContext::new(content, MarkdownFlavor::Quarto, None);
    let fixed1 = rule.fix(&ctx1).unwrap();

    let ctx2 = LintContext::new(&fixed1, MarkdownFlavor::Quarto, None);
    let fixed2 = rule.fix(&ctx2).unwrap();

    assert_eq!(fixed1, fixed2, "Reflow should be idempotent");
}

#[test]
fn test_quarto_div_adjacent_to_paragraph_without_blank_line() {
    // Paragraph immediately followed by div (no blank line) — the div marker
    // must still be recognized as a block boundary, not merged into the paragraph
    let content = "Short paragraph.\n::: {.callout-note}\nDiv content.\n:::\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    assert!(
        fixed.contains("::: {.callout-note}"),
        "Div marker must not be merged with preceding paragraph"
    );
    assert!(
        !fixed.contains("paragraph.::: "),
        "Div marker must not be joined to paragraph text"
    );
}

// ============================================================================
// MD013 reflow: div markers inside list items
// ============================================================================

#[test]
fn test_quarto_div_inside_list_item_preserved() {
    // A list item containing a div block — the div markers must stay on their
    // own lines and content between them should be reflowed independently
    let content = "- This is a list item with a very long line of text that should be reflowed to fit within the eighty character line width\n\n  ::: {.callout-note}\n  Note text inside the div.\n  :::\n\n  More text after the div.\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Div markers must be on their own lines (with list continuation indent)
    let lines: Vec<&str> = fixed.lines().collect();
    assert!(
        lines.iter().any(|l| l.trim() == "::: {.callout-note}"),
        "Opening div marker must be preserved on its own line, got:\n{fixed}"
    );
    assert!(
        lines.iter().any(|l| l.trim() == ":::"),
        "Closing div marker must be preserved on its own line, got:\n{fixed}"
    );

    // Div markers must NOT be merged with paragraph text
    assert!(
        !fixed.contains("width::: "),
        "Opening div marker must not be merged with preceding text"
    );
    assert!(
        !fixed.contains("div.:::"),
        "Closing div marker must not be merged with preceding text"
    );
}

#[test]
fn test_quarto_div_inside_list_item_with_long_content() {
    // List item with a div containing long text that needs reflowing
    let content = "- Item text.\n\n  ::: {.callout-note}\n  Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed quam leo, rhoncus sodales erat sed.\n  :::\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    let lines: Vec<&str> = fixed.lines().collect();
    assert!(
        lines.iter().any(|l| l.trim() == "::: {.callout-note}"),
        "Opening div marker preserved in list item"
    );
    assert!(
        lines.iter().any(|l| l.trim() == ":::"),
        "Closing div marker preserved in list item"
    );
}

#[test]
fn test_quarto_div_inside_list_item_no_blank_lines() {
    // List item with div adjacent to text (no blank line separation)
    let content = "- Some text before.\n  ::: {.callout-note}\n  Note content.\n  :::\n  Text after.\n";

    let config = create_quarto_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let fixed = rule.fix(&ctx).unwrap();

    assert!(
        !fixed.contains("before.::: "),
        "Div marker must not merge with preceding text in list item"
    );
    assert!(
        !fixed.contains("content.:::"),
        "Closing div marker must not merge with preceding text in list item"
    );

    let lines: Vec<&str> = fixed.lines().collect();
    assert!(
        lines.iter().any(|l| l.trim() == "::: {.callout-note}"),
        "Opening div marker preserved"
    );
    assert!(lines.iter().any(|l| l.trim() == ":::"), "Closing div marker preserved");
}
