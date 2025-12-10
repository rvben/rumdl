//! Tests for cross-file validation (MD051)
//!
//! These tests verify that cross-file link validation works correctly,
//! especially when target files don't have links themselves (which would
//! cause content-characteristic filtering to skip them).

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::rule::{CrossFileScope, Rule};
use rumdl_lib::rules::MD051LinkFragments;
use rumdl_lib::workspace_index::WorkspaceIndex;
use std::path::PathBuf;

/// Regression test: Ensure headings are indexed from files without links.
///
/// This test catches the bug where `contribute_to_index` was only called
/// for "applicable" rules (rules that match content characteristics).
/// Files without links would skip link rules, causing their headings
/// to not be indexed, which broke cross-file link validation.
#[test]
fn test_cross_file_link_to_file_without_links() {
    // Source file has links to target
    let source_content = r#"# Source

[valid link](./target.md#features)
[invalid link](./target.md#missing)
"#;

    // Target file has headings but NO links
    // This is the key: without links, content-characteristic filtering
    // would skip MD051 for this file, but we still need its headings indexed.
    let target_content = r#"# Target File

## Features

Here are the features.
"#;

    let source_path = PathBuf::from("/test/source.md");
    let target_path = PathBuf::from("/test/target.md");

    // Get all rules
    let rules = rumdl_lib::rules::all_rules(&Config::default());

    // Lint and index both files
    let (_, source_index) = rumdl_lib::lint_and_index(source_content, &rules, false, MarkdownFlavor::default(), None);
    let (_, target_index) = rumdl_lib::lint_and_index(target_content, &rules, false, MarkdownFlavor::default(), None);

    // Build workspace index
    let mut workspace_index = WorkspaceIndex::new();
    workspace_index.insert_file(source_path.clone(), source_index.clone());
    workspace_index.insert_file(target_path.clone(), target_index.clone());

    // Verify target.md's headings were indexed (this was the bug - they weren't)
    let target_file_index = workspace_index.get_file(&target_path).unwrap();
    assert!(
        !target_file_index.headings.is_empty(),
        "Target file headings should be indexed even though it has no links"
    );

    // Find the "features" heading
    let has_features_heading = target_file_index.headings.iter().any(|h| h.auto_anchor == "features");
    assert!(
        has_features_heading,
        "Target file should have 'features' heading indexed"
    );

    // Run cross-file checks on source file
    let md051 = MD051LinkFragments::default();
    let warnings = md051
        .cross_file_check(&source_path, &source_index, &workspace_index)
        .unwrap();

    // Should only have 1 warning for the invalid "missing" link
    // NOT 2 warnings (which would include the valid "features" link)
    assert_eq!(
        warnings.len(),
        1,
        "Should only flag the broken link, not the valid one. Got: {warnings:?}"
    );
    assert!(
        warnings[0].message.contains("missing"),
        "Warning should be about 'missing' fragment, got: {}",
        warnings[0].message
    );
}

/// Test that cross-file rules have Workspace scope
#[test]
fn test_md051_has_workspace_scope() {
    let rule = MD051LinkFragments::default();
    assert_eq!(
        rule.cross_file_scope(),
        CrossFileScope::Workspace,
        "MD051 should have Workspace cross-file scope"
    );
}

/// Test that headings are indexed correctly with auto-anchors
#[test]
fn test_heading_anchor_generation_in_index() {
    let content = r#"# Main Title

## Getting Started

### Step One {#step-1}
"#;

    let rules = rumdl_lib::rules::all_rules(&Config::default());
    let (_, file_index) = rumdl_lib::lint_and_index(content, &rules, false, MarkdownFlavor::default(), None);

    // Should have 3 headings indexed
    let unique_anchors: std::collections::HashSet<_> =
        file_index.headings.iter().map(|h| h.auto_anchor.as_str()).collect();

    assert!(unique_anchors.contains("main-title"), "Should have 'main-title' anchor");
    assert!(
        unique_anchors.contains("getting-started"),
        "Should have 'getting-started' anchor"
    );
    assert!(unique_anchors.contains("step-one"), "Should have 'step-one' anchor");

    // Check custom anchor is preserved
    let step_one = file_index
        .headings
        .iter()
        .find(|h| h.auto_anchor == "step-one")
        .expect("Should find step-one heading");
    assert_eq!(
        step_one.custom_anchor,
        Some("step-1".to_string()),
        "Custom anchor should be preserved"
    );
}

/// Test cross-file links are extracted correctly
#[test]
fn test_cross_file_links_extraction() {
    let content = r#"# Document

[Local](#local)
[Same file](./other.md)
[With fragment](./guide.md#install)
[External](https://example.com)
"#;

    let rules = rumdl_lib::rules::all_rules(&Config::default());
    let (_, file_index) = rumdl_lib::lint_and_index(content, &rules, false, MarkdownFlavor::default(), None);

    // Should have 1 cross-file link with fragment (./guide.md#install)
    // Local anchors (#local) and links without fragments are not included
    let cross_file_links_with_fragment: Vec<_> = file_index
        .cross_file_links
        .iter()
        .filter(|l| !l.fragment.is_empty())
        .collect();

    assert_eq!(
        cross_file_links_with_fragment.len(),
        1,
        "Should have 1 cross-file link with fragment"
    );
    assert_eq!(cross_file_links_with_fragment[0].target_path, "./guide.md");
    assert_eq!(cross_file_links_with_fragment[0].fragment, "install");
}

/// Test that inline config data is stored in FileIndex during linting.
/// This data allows cross-file rules to respect inline disable comments.
#[test]
fn test_inline_config_stored_in_file_index() {
    let content = r#"# Test Document

<!-- rumdl-disable MD051 -->
[Link to missing fragment](./other.md#nonexistent)
<!-- rumdl-enable MD051 -->

[Link to another missing fragment](./other.md#also-nonexistent)
"#;

    let rules = rumdl_lib::rules::all_rules(&Config::default());
    let (_, file_index) = rumdl_lib::lint_and_index(content, &rules, false, MarkdownFlavor::default(), None);

    // Verify that inline config data was stored in FileIndex
    // Line 4 should have MD051 disabled (line numbers are 1-indexed)
    assert!(
        file_index.is_rule_disabled_at_line("MD051", 4),
        "MD051 should be disabled at line 4"
    );

    // Line 7 should NOT have MD051 disabled
    assert!(
        !file_index.is_rule_disabled_at_line("MD051", 7),
        "MD051 should NOT be disabled at line 7"
    );
}

/// Test that file-wide disable is stored in FileIndex.
#[test]
fn test_file_wide_disable_in_file_index() {
    let content = r#"<!-- rumdl-disable-file MD051 -->
# Test Document

[Link to missing fragment](./other.md#nonexistent)
"#;

    let rules = rumdl_lib::rules::all_rules(&Config::default());
    let (_, file_index) = rumdl_lib::lint_and_index(content, &rules, false, MarkdownFlavor::default(), None);

    // All lines should have MD051 disabled with file-wide disable
    assert!(
        file_index.is_rule_disabled_at_line("MD051", 1),
        "MD051 should be disabled at line 1"
    );
    assert!(
        file_index.is_rule_disabled_at_line("MD051", 4),
        "MD051 should be disabled at line 4"
    );
}

/// Test that disable-next-line stores data correctly.
#[test]
fn test_disable_next_line_in_file_index() {
    let content = r#"# Test Document

<!-- rumdl-disable-next-line MD051 -->
[Link to missing fragment](./other.md#nonexistent)

[Link to another missing fragment](./other.md#also-nonexistent)
"#;

    let rules = rumdl_lib::rules::all_rules(&Config::default());
    let (_, file_index) = rumdl_lib::lint_and_index(content, &rules, false, MarkdownFlavor::default(), None);

    // Line 4 (the link after the disable-next-line comment) should have MD051 disabled
    assert!(
        file_index.is_rule_disabled_at_line("MD051", 4),
        "MD051 should be disabled at line 4 (via disable-next-line)"
    );

    // Line 6 should NOT have MD051 disabled
    assert!(
        !file_index.is_rule_disabled_at_line("MD051", 6),
        "MD051 should NOT be disabled at line 6"
    );
}

/// Test cross-file rule filtering respects inline disable.
/// This verifies that run_cross_file_checks() filters warnings based
/// on inline config stored in FileIndex.
#[test]
fn test_cross_file_rules_respect_inline_disable() {
    let source_content = r#"# Source

<!-- rumdl-disable MD051 -->
[disabled link](./target.md#missing)
<!-- rumdl-enable MD051 -->

[enabled link](./target.md#also-missing)
"#;

    let target_content = r#"# Target File

## Features

Here are the features.
"#;

    let source_path = PathBuf::from("/test/source.md");
    let target_path = PathBuf::from("/test/target.md");

    let rules = rumdl_lib::rules::all_rules(&Config::default());

    // Lint and index both files
    let (_, source_index) = rumdl_lib::lint_and_index(source_content, &rules, false, MarkdownFlavor::default(), None);
    let (_, target_index) = rumdl_lib::lint_and_index(target_content, &rules, false, MarkdownFlavor::default(), None);

    // Build workspace index
    let mut workspace_index = WorkspaceIndex::new();
    workspace_index.insert_file(source_path.clone(), source_index.clone());
    workspace_index.insert_file(target_path.clone(), target_index.clone());

    // Run cross-file checks on source file
    let md051 = MD051LinkFragments::default();
    let raw_warnings = md051
        .cross_file_check(&source_path, &source_index, &workspace_index)
        .unwrap();

    // Without filtering, should have 2 warnings (both links point to missing fragments)
    assert_eq!(
        raw_warnings.len(),
        2,
        "Raw cross_file_check should return 2 warnings before filtering"
    );

    // Filter warnings using inline config stored in FileIndex
    let filtered_warnings: Vec<_> = raw_warnings
        .into_iter()
        .filter(|w| !source_index.is_rule_disabled_at_line("MD051", w.line))
        .collect();

    // After filtering, should have 1 warning (only the enabled link)
    assert_eq!(
        filtered_warnings.len(),
        1,
        "After inline config filtering, should have 1 warning. Got: {filtered_warnings:?}"
    );

    // The remaining warning should be for line 7 (the enabled link)
    assert_eq!(
        filtered_warnings[0].line, 7,
        "Warning should be for line 7 (enabled link), got line {}",
        filtered_warnings[0].line
    );
}
