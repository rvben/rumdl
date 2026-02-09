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

#[test]
fn test_admonition_with_code_block_at_start_does_not_hang() {
    // Regression test: MkDocs admonition with code block as first content line
    // followed by a long line outside the admonition should not cause infinite loop.
    // The bug was that when container_lines was empty after breaking from the inner loop
    // (due to code block at start), the code would `continue` without incrementing `i`,
    // causing the outer loop to process the same line forever.
    let content = r#"!!! note
    ```
    x
    ```

aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // This should complete without hanging
    let warnings = rule.check(&ctx).unwrap();

    // Should have a warning for the long line outside the admonition
    assert!(!warnings.is_empty(), "Long line should generate a warning");

    // Fix should also complete without hanging
    let fixed = rule.fix(&ctx).unwrap();

    // The admonition with code block should be preserved
    assert!(fixed.contains("!!! note"), "Admonition marker should be preserved");
    assert!(fixed.contains("```"), "Code block should be preserved");
}

#[test]
fn test_admonition_with_empty_line_at_start_does_not_hang() {
    // Similar regression test: admonition with empty indented line at start
    let content = r#"!!! note

    Some content after blank line.

This is a very long line that exceeds the default line length limit and should trigger the reflow logic.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // This should complete without hanging
    let _warnings = rule.check(&ctx).unwrap();

    // Fix should also complete without hanging
    let fixed = rule.fix(&ctx).unwrap();

    // Content should be preserved
    assert!(fixed.contains("!!! note"), "Admonition marker should be preserved");
    assert!(fixed.contains("Some content"), "Admonition content should be preserved");
}

#[test]
fn test_admonition_with_list_at_start_does_not_hang() {
    // Similar regression test: admonition with list item at start
    let content = r#"!!! note
    - List item at start
    - Another item

This is a very long line that exceeds the default line length limit and should trigger the reflow logic in MD013.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // This should complete without hanging
    let _warnings = rule.check(&ctx).unwrap();

    // Fix should also complete without hanging
    let fixed = rule.fix(&ctx).unwrap();

    // Content should be preserved
    assert!(fixed.contains("!!! note"), "Admonition marker should be preserved");
    assert!(fixed.contains("- List item"), "List items should be preserved");
}

// ───── Bug #2: Compact admonition marker lines must not be reflowed ─────

#[test]
fn test_compact_admonition_marker_not_reflowed() {
    // Compact admonition: `!!! note` followed immediately by indented content (no blank line)
    // The marker line must NEVER be reflowed — only the indented content lines.
    let content = r#"!!! note
    This is a very long compact admonition content line that exceeds the default eighty character line length limit and should be wrapped.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Marker line must be exactly `!!! note` — never merged with content
    let first_line = fixed.lines().next().unwrap();
    assert_eq!(
        first_line, "!!! note",
        "Compact admonition marker must remain on its own line, got: {first_line:?}"
    );

    // Content must NOT appear on the marker line
    assert!(
        !first_line.contains("This is"),
        "Content must not be merged onto the admonition marker line"
    );

    // All content lines should be indented with 4 spaces
    for line in fixed.lines().skip(1) {
        if !line.is_empty() {
            assert!(
                line.starts_with("    "),
                "Content line should have 4-space indent: {line:?}"
            );
        }
    }
}

#[test]
fn test_compact_admonition_with_title_not_reflowed() {
    // Compact admonition with title: `!!! warning "Caution"` followed by indented content
    let content = r#"!!! warning "Caution"
    This is a long warning message inside a compact admonition with a custom title that exceeds the eighty character line length limit.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let fixed = rule.fix(&ctx).unwrap();

    let first_line = fixed.lines().next().unwrap();
    assert_eq!(
        first_line,
        "!!! warning \"Caution\"",
        "Admonition marker with title must remain intact: {first_line:?}"
    );
}

#[test]
fn test_compact_collapsible_admonition_not_reflowed() {
    // Collapsible admonition `???` format
    let content = r#"??? info "Details"
    This collapsible admonition has content that is very long and exceeds the line length limit and should be wrapped while preserving indentation.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let fixed = rule.fix(&ctx).unwrap();

    let first_line = fixed.lines().next().unwrap();
    assert_eq!(
        first_line,
        "??? info \"Details\"",
        "Collapsible marker must remain intact: {first_line:?}"
    );
}

#[test]
fn test_tab_marker_not_reflowed() {
    // Tab marker `=== "Tab"` followed by indented content
    let content = r#"=== "Configuration"
    This is a very long configuration description inside a tab that exceeds the default eighty character line length limit and should be wrapped.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let fixed = rule.fix(&ctx).unwrap();

    let first_line = fixed.lines().next().unwrap();
    assert_eq!(
        first_line,
        "=== \"Configuration\"",
        "Tab marker must remain intact: {first_line:?}"
    );
}

#[test]
fn test_compact_admonition_multi_paragraph_preserved() {
    // Multi-paragraph compact admonition: content spans multiple paragraphs
    // Each paragraph should be reflowed independently, maintaining structure
    let content = r#"!!! note
    First paragraph of the admonition that is long enough to need reflowing when the line length limit is set to eighty characters.

    Second paragraph of the admonition that is also long enough to trigger the reflow when the line length limit is eighty.
"#;

    let config = create_mkdocs_config_with_reflow();
    let rule = MD013LineLength::from_config(&config);
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    let fixed = rule.fix(&ctx).unwrap();

    // Marker line intact
    assert_eq!(fixed.lines().next().unwrap(), "!!! note");

    // Blank line separating paragraphs should be preserved
    let lines: Vec<&str> = fixed.lines().collect();
    let has_blank_between_paragraphs = lines.windows(3).any(|w| {
        w[0].starts_with("    ") && w[1].is_empty() && w[2].starts_with("    ")
    });
    assert!(
        has_blank_between_paragraphs,
        "Blank line between admonition paragraphs should be preserved. Got:\n{fixed}"
    );
}
