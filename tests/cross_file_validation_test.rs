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
