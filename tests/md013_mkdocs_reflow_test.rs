//! Tests for MD013 reflow behavior with MkDocs constructs (admonitions, tabs)
//!
//! MkDocs uses 4-space indented content for admonitions (!!! note) and tabs (=== "Tab").
//! This content should be reflowed while preserving the 4-space indentation on all lines.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD013LineLength;

fn create_mkdocs_config_with_reflow() -> Config {
    let mut config = Config::default();
    config.global.flavor = MarkdownFlavor::MkDocs;
    // Enable reflow
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

#[test]
fn test_mkdocs_admonition_content_detected_correctly() {
    // MkDocs admonition content should be detected as in_admonition, NOT as code block
    let content = r#"!!! note

    This approach shares state between the composited efforts. This means that authentication works.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // Check that the admonition content is detected as in_admonition
    assert!(
        ctx.lines[2].in_admonition,
        "Line 3 should be detected as admonition content"
    );

    // Check that it's NOT marked as code block (this was the bug in issue #361)
    assert!(
        !ctx.lines[2].in_code_block,
        "Admonition content should not be marked as code block"
    );
}

#[test]
fn test_mkdocs_admonition_long_content_reflowed_with_indent() {
    // Long admonition content should be reflowed with the 4-space indent preserved
    let content = r#"!!! note

    This approach shares state between the composited efforts. This means that authentication, database pooling, and other things will be usable between components.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let warnings = rule.check(&ctx).unwrap();

    // Should have a warning for the long line in the admonition
    assert!(
        !warnings.is_empty(),
        "Long admonition content should generate a warning"
    );
    assert!(warnings[0].fix.is_some(), "Warning should have a fix");

    // Fix should reflow with preserved 4-space indent
    let fixed = rule.fix(&ctx).unwrap();

    // Admonition marker should be preserved
    assert!(fixed.contains("!!! note"), "Admonition marker should be preserved");

    // ALL content lines should have 4-space indent
    for line in fixed.lines() {
        if !line.is_empty() && !line.starts_with("!!!") {
            assert!(
                line.starts_with("    "),
                "All admonition content lines should have 4-space indent, but got: {line:?}"
            );
        }
    }

    // Content should be wrapped (multiple lines after reflow)
    let content_lines: Vec<_> = fixed
        .lines()
        .filter(|l| l.starts_with("    ") && !l.trim().is_empty())
        .collect();
    assert!(
        content_lines.len() > 1,
        "Long content should be wrapped into multiple lines, got: {content_lines:?}"
    );
}

#[test]
fn test_mkdocs_tab_content_detected_correctly() {
    // MkDocs tab content should be detected as in_content_tab, NOT as code block
    let content = r#"=== "Tab 1"

    This is tab content that should be preserved with its indentation.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // Check that the tab content is detected as in_content_tab
    assert!(ctx.lines[2].in_content_tab, "Line 3 should be detected as tab content");

    // Check that it's NOT marked as code block
    assert!(
        !ctx.lines[2].in_code_block,
        "Tab content should not be marked as code block"
    );
}

#[test]
fn test_mkdocs_tab_long_content_reflowed_with_indent() {
    // Long tab content should be reflowed with the 4-space indent preserved
    let content = r#"=== "Configuration"

    This is tab content with a very long line that would normally be reflowed by MD013 and should now be properly reflowed while preserving the 4-space indentation.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let warnings = rule.check(&ctx).unwrap();

    // Should have a warning for the long line
    assert!(!warnings.is_empty(), "Long tab content should generate a warning");

    // Fix should reflow with preserved 4-space indent
    let fixed = rule.fix(&ctx).unwrap();

    // Tab marker should be preserved
    assert!(
        fixed.contains("=== \"Configuration\""),
        "Tab marker should be preserved"
    );

    // ALL content lines should have 4-space indent
    for line in fixed.lines() {
        if !line.is_empty() && !line.starts_with("===") {
            assert!(
                line.starts_with("    "),
                "All tab content lines should have 4-space indent, but got: {line:?}"
            );
        }
    }
}

#[test]
fn test_mkdocs_nested_admonition_content() {
    // Nested content inside admonition should also be detected
    let content = r#"!!! warning "Important"

    This is a warning message.

    - List item inside admonition
    - Another list item

    More paragraph content here.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // All indented content should be detected as admonition
    for (i, line_info) in ctx.lines.iter().enumerate() {
        let line = ctx.content.lines().nth(i).unwrap_or("");
        if line.starts_with("    ") && !line.trim().is_empty() {
            assert!(
                line_info.in_admonition,
                "Line {} should be in admonition: {:?}",
                i + 1,
                line
            );
        }
    }
}

#[test]
fn test_regular_paragraph_still_reflowed_in_mkdocs() {
    // Regular paragraphs (not in admonitions) should still be reflowed normally
    let content = r#"# Heading

This is a regular paragraph that is quite long and should be reflowed by MD013 when the reflow option is enabled in the configuration file.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let warnings = rule.check(&ctx).unwrap();

    // Should have a warning for the long line
    assert!(
        warnings.iter().any(|w| w.fix.is_some()),
        "Regular paragraph should be flagged for reflow"
    );
}

#[test]
fn test_collapsible_admonition_content_detected() {
    // Collapsible admonitions (??? syntax) should also be detected
    let content = r#"??? info "Click to expand"

    This is hidden content that will be revealed when the user clicks. It should preserve its indentation.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // Check that the content is detected as admonition
    assert!(
        ctx.lines[2].in_admonition,
        "Collapsible admonition content should be detected"
    );
}

#[test]
fn test_short_admonition_content_not_modified() {
    // Short admonition content that doesn't exceed line length should not be modified
    let content = r#"!!! note

    Short content here.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let warnings = rule.check(&ctx).unwrap();

    // No warnings for short content
    assert!(
        warnings.is_empty(),
        "Short admonition content should not generate warnings"
    );

    // Fix should preserve content exactly
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "Short content should be preserved exactly");
}

#[test]
fn test_admonition_with_multiple_paragraphs() {
    // Multiple paragraphs in admonition should each be handled separately
    let content = r#"!!! note

    First paragraph with some content.

    Second paragraph with different content.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // Fix should preserve the structure
    let fixed = rule.fix(&ctx).unwrap();

    // Both paragraphs should be present
    assert!(fixed.contains("First paragraph"), "First paragraph should be preserved");
    assert!(
        fixed.contains("Second paragraph"),
        "Second paragraph should be preserved"
    );

    // Blank line between paragraphs should be preserved
    assert!(
        fixed.contains("\n\n    "),
        "Blank line between paragraphs should be preserved"
    );
}

#[test]
fn test_nested_admonition_preserves_deeper_indent() {
    // Nested admonitions have 8 spaces of indent - this must be preserved
    let content = r#"!!! note

    !!! warning

        This nested content has 8 spaces and is a very long line that should be reflowed while preserving all 8 spaces of indentation.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // Fix should reflow with preserved 8-space indent
    let fixed = rule.fix(&ctx).unwrap();

    // ALL nested content lines should have 8-space indent
    for line in fixed.lines() {
        // Skip the admonition markers and blank lines
        if line.trim().is_empty() || line.starts_with("!!!") || line.trim_start().starts_with("!!!") {
            continue;
        }
        // Only check lines that should be nested content (not the outer warning marker)
        if !line.starts_with("    !!!") {
            assert!(
                line.starts_with("        "),
                "Nested content should have 8-space indent, but got: {line:?}"
            );
        }
    }
}

#[test]
fn test_filtered_lines_skip_mkdocs_containers() {
    // Test the new skip_mkdocs_containers() filter
    use rumdl_lib::filtered_lines::FilteredLinesExt;

    let content = r#"# Heading

!!! note

    Admonition content here.

Regular paragraph.

=== "Tab"

    Tab content here.

Another paragraph.
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let filtered: Vec<_> = ctx.filtered_lines().skip_mkdocs_containers().into_iter().collect();

    // Should include heading and regular paragraphs
    assert!(filtered.iter().any(|l| l.content == "# Heading"));
    assert!(filtered.iter().any(|l| l.content == "Regular paragraph."));
    assert!(filtered.iter().any(|l| l.content == "Another paragraph."));

    // Should exclude admonition and tab content
    assert!(!filtered.iter().any(|l| l.content.contains("Admonition content")));
    assert!(!filtered.iter().any(|l| l.content.contains("Tab content")));
}
