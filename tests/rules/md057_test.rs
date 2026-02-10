use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD057ExistingRelativeLinks;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_missing_links() {
    // Create a temporary directory for test files
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create an existing file
    let exists_path = base_path.join("exists.md");
    File::create(&exists_path).unwrap().write_all(b"# Test File").unwrap();

    // Create test content with both existing and missing links
    let content = r#"
# Test Document

[Valid Link](exists.md)
[Invalid Link](missing.md)
"#;

    // Initialize rule with the base path
    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

    // Test the rule
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have one warning for the missing link
    assert_eq!(result.len(), 1, "Expected 1 warning, got {}", result.len());
    assert!(
        result[0].message.contains("missing.md"),
        "Expected warning about missing.md, got: {}",
        result[0].message
    );
}

#[test]
fn test_external_links() {
    // Create a temporary directory for test files
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create test content with external links
    let content = r#"
# Test Document with External Links

[Google](https://www.google.com)
[Example](http://example.com)
[Email](mailto:test@example.com)
[Domain](example.com)
"#;

    // Initialize rule with the base path
    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

    // Test the rule
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no warnings for external links
    assert_eq!(result.len(), 0, "Expected 0 warnings, got {}", result.len());
}

#[test]
fn test_special_uri_schemes() {
    // Issue #192: Special URI schemes should not be flagged as broken relative links
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# Test Document with Special URI Schemes

[Local file](file:///path/to/file)
[Network share](smb://example.com/path/to/share)
[Mac App Store](macappstores://apps.apple.com/)
[Phone](tel:+1234567890)
[Data URI](data:text/plain;base64,SGVsbG8=)
[SSH](ssh://git@github.com/repo)
[Git](git://github.com/repo.git)
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Expected no warnings for special URI schemes, got: {result:?}"
    );
}

#[test]
fn test_code_blocks() {
    // Create a temporary directory for test files
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create test content with links in code blocks
    let content = r#"
# Test Document

[Invalid Link](missing.md)

```markdown
[Another Invalid Link](also-missing.md)
```
"#;

    // Initialize rule with the base path
    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

    // Test the rule
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only have one warning for the link outside the code block
    assert_eq!(result.len(), 1, "Expected 1 warning, got {}", result.len());
    assert!(
        result[0].message.contains("missing.md"),
        "Expected warning about missing.md, got: {}",
        result[0].message
    );

    // Make sure the link in the code block is not flagged
    for warning in &result {
        assert!(
            !warning.message.contains("also-missing.md"),
            "Found unexpected warning for link in code block: {}",
            warning.message
        );
    }
}

#[test]
fn test_disabled_rule() {
    // Create a temporary directory for test files
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create test content with disabled rule
    let content = r#"
# Test Document

<!-- markdownlint-disable MD057 -->
[Invalid Link](missing.md)
<!-- markdownlint-enable MD057 -->

[Another Invalid Link](also-missing.md)
"#;

    // Initialize rule with the base path
    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

    // Test the rule - note: this tests the single-file check() method
    // which doesn't have access to inline config filtering (that happens
    // in lint_and_index()). The cross-file check in run_cross_file_checks()
    // now respects inline config stored in FileIndex.
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // The check() method returns all warnings; filtering happens later
    // when running through lint_and_index() or run_cross_file_checks()
    assert_eq!(
        result.len(),
        2,
        "Expected 2 warnings from check(), got {}",
        result.len()
    );

    // Check that both links are flagged
    let has_missing = result.iter().any(|w| w.message.contains("missing.md"));
    let has_also_missing = result.iter().any(|w| w.message.contains("also-missing.md"));

    assert!(has_missing, "Missing warning for 'missing.md'");
    assert!(has_also_missing, "Missing warning for 'also-missing.md'");
}

#[test]
fn test_complex_paths() {
    // Create a temporary directory for test files
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create a nested directory structure
    let nested_dir = base_path.join("docs");
    std::fs::create_dir(&nested_dir).unwrap();

    // Create some existing files
    let exists_path = nested_dir.join("exists.md");
    File::create(&exists_path).unwrap().write_all(b"# Test File").unwrap();

    // Create test content with various path formats
    let content = r#"
# Test Document with Complex Paths

[Valid Nested Link](docs/exists.md)
[Missing Nested Link](docs/missing.md)
[Missing Directory](missing-dir/file.md)
[Parent Directory Link](../file.md)
"#;

    // Initialize rule with the base path
    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

    // Test the rule
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have warnings for missing links but not for valid links
    assert_eq!(result.len(), 3, "Expected 3 warnings, got {}", result.len());

    // Check for specific warnings
    let has_missing_nested = result.iter().any(|w| w.message.contains("docs/missing.md"));
    let has_missing_dir = result.iter().any(|w| w.message.contains("missing-dir/file.md"));
    let has_parent_dir = result.iter().any(|w| w.message.contains("../file.md"));

    assert!(has_missing_nested, "Missing warning for 'docs/missing.md'");
    assert!(has_missing_dir, "Missing warning for 'missing-dir/file.md'");
    assert!(has_parent_dir, "Missing warning for '../file.md'");

    // Check that the valid link is not flagged
    for warning in &result {
        assert!(
            !warning.message.contains("docs/exists.md"),
            "Found unexpected warning for valid link: {}",
            warning.message
        );
    }
}

#[test]
fn test_no_base_path() {
    // Create test content with links
    let content = r#"
# Test Document

[Link](missing.md)
"#;

    // Initialize rule without setting a base path
    let rule = MD057ExistingRelativeLinks::new();

    // Test the rule
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no warnings when no base path is set
    assert_eq!(
        result.len(),
        0,
        "Expected 0 warnings when no base path is set, got {}",
        result.len()
    );
}

#[test]
fn test_fragment_links() {
    // Create a temporary directory for test files
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create a file with headings to link to
    let test_file_path = base_path.join("test_file.md");
    let test_content = r#"
# Main Heading

## Sub Heading One

Some content here.

## Sub Heading Two

More content.
"#;
    File::create(&test_file_path)
        .unwrap()
        .write_all(test_content.as_bytes())
        .unwrap();

    // Create content with internal fragment links to the same document
    let content = r#"
# Test Document

- [Link to Heading](#main-heading)
- [Link to Sub Heading](#sub-heading-one)
- [Link to External File](other_file.md#some-heading)
"#;

    // Initialize rule with the base path
    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

    // Test the rule
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have one warning for external file link only (fragment-only links are skipped)
    assert_eq!(
        result.len(),
        1,
        "Expected 1 warning for external file link, got {}",
        result.len()
    );

    // Check that the external link is flagged
    let has_other_file = result.iter().any(|w| w.message.contains("other_file.md"));
    assert!(has_other_file, "Missing warning for 'other_file.md'");
}

#[test]
fn test_combined_links() {
    // Create a temporary directory for test files
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create an existing file and a missing file with fragments
    let exists_path = base_path.join("exists.md");
    File::create(&exists_path).unwrap().write_all(b"# Test File").unwrap();

    // Create content with combined file and fragment links
    let content = r#"
# Test Document

- [Link to existing file with fragment](exists.md#section)
- [Link to missing file with fragment](missing.md#section)
- [Link to fragment only](#local-section)
"#;

    // Initialize rule with the base path
    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

    // Test the rule
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only have one warning for the missing file link with fragment
    assert_eq!(
        result.len(),
        1,
        "Expected 1 warning for missing file with fragment, got {}",
        result.len()
    );
    assert!(
        result[0].message.contains("missing.md"),
        "Expected warning about missing.md, got: {}",
        result[0].message
    );

    // Make sure the existing file with fragment and fragment-only links are not flagged
    for warning in &result {
        assert!(
            !warning.message.contains("exists.md#section"),
            "Found unexpected warning for existing file with fragment: {}",
            warning.message
        );
        assert!(
            !warning.message.contains("#local-section"),
            "Found unexpected warning for fragment-only link: {}",
            warning.message
        );
    }
}

/// Test that each file resolves links relative to its own directory.
/// This is a regression test for issue #190 where the base_path was being
/// cached in the rule instance and incorrectly reused across files.
#[test]
fn test_multi_file_base_path_isolation() {
    // Create a temporary directory structure:
    // temp/
    //   dir1/
    //     index.md  -> [link](sub/file.md)
    //     sub/
    //       file.md  <- EXISTS
    //   dir2/
    //     index.md  -> [link](sub/file.md)
    //     (sub/ does NOT exist)
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create dir1 structure with existing file
    let dir1 = base_path.join("dir1");
    let dir1_sub = dir1.join("sub");
    std::fs::create_dir_all(&dir1_sub).unwrap();
    File::create(dir1_sub.join("file.md"))
        .unwrap()
        .write_all(b"# File in dir1/sub")
        .unwrap();

    // Create dir2 structure WITHOUT the sub/file.md
    let dir2 = base_path.join("dir2");
    std::fs::create_dir_all(&dir2).unwrap();

    // Both files have the same relative link
    let content = "[Link](sub/file.md)\n";

    // Create a single rule instance (simulating how rules are reused across files)
    let rule = MD057ExistingRelativeLinks::new();

    // Test dir1/index.md - should have NO warnings (file exists)
    let dir1_file = dir1.join("index.md");
    let ctx1 = LintContext::new(
        content,
        rumdl_lib::config::MarkdownFlavor::Standard,
        Some(dir1_file.clone()),
    );
    let result1 = rule.check(&ctx1).unwrap();
    assert_eq!(
        result1.len(),
        0,
        "dir1/index.md should have no warnings because dir1/sub/file.md exists, got: {:?}",
        result1.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    // Test dir2/index.md - should have ONE warning (file does not exist)
    let dir2_file = dir2.join("index.md");
    let ctx2 = LintContext::new(
        content,
        rumdl_lib::config::MarkdownFlavor::Standard,
        Some(dir2_file.clone()),
    );
    let result2 = rule.check(&ctx2).unwrap();
    assert_eq!(
        result2.len(),
        1,
        "dir2/index.md should have 1 warning because dir2/sub/file.md does NOT exist, got: {:?}",
        result2.iter().map(|w| &w.message).collect::<Vec<_>>()
    );

    // Now test in reverse order to ensure no caching issues either way
    let rule2 = MD057ExistingRelativeLinks::new();

    // Test dir2 first this time
    let ctx2_again = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, Some(dir2_file));
    let result2_again = rule2.check(&ctx2_again).unwrap();
    assert_eq!(
        result2_again.len(),
        1,
        "dir2 should still have 1 warning when processed first"
    );

    // Then test dir1
    let ctx1_again = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, Some(dir1_file));
    let result1_again = rule2.check(&ctx1_again).unwrap();
    assert_eq!(
        result1_again.len(),
        0,
        "dir1 should still have 0 warnings when processed second (regression test for issue #190)"
    );
}

#[test]
fn test_query_parameters_stripped() {
    // Issue #198: URLs with query parameters like ?raw=true should be handled correctly
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create an existing image file
    let image_path = base_path.join("image.png");
    File::create(&image_path)
        .unwrap()
        .write_all(b"fake image data")
        .unwrap();

    // Test content with query parameters
    let content = r#"
# Test Document

![Embed link to raw image](image.png?raw=true)
![Another image](path/to/image.jpg?raw=true&version=1)
[Link with query](document.md?raw=true)
[Link with fragment](document.md#section)
[Link with both](document.md?raw=true#section)
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only warn about missing files, not about image.png?raw=true (file exists)
    // Should warn about path/to/image.jpg?raw=true (file doesn't exist)
    // Should warn about document.md (file doesn't exist)
    let messages: Vec<_> = result.iter().map(|w| w.message.as_str()).collect();

    // image.png exists, so no warning for that
    assert!(
        !messages.iter().any(|m| m.contains("image.png?raw=true")),
        "Should not warn about existing file with query parameter"
    );

    // path/to/image.jpg doesn't exist
    assert!(
        messages.iter().any(|m| m.contains("path/to/image.jpg?raw=true")),
        "Should warn about missing file with query parameter"
    );

    // document.md doesn't exist (should warn regardless of query/fragment)
    assert!(
        messages.iter().any(|m| m.contains("document.md")),
        "Should warn about missing document.md"
    );
}

// =============================================================================
// Extension-less links with fragments tests
// =============================================================================

/// Test that extension-less links with fragments resolve correctly when .md file exists
#[test]
fn test_extensionless_link_with_fragment_to_existing_md_file() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create docs/guide.md
    let docs_dir = base_path.join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    File::create(docs_dir.join("guide.md"))
        .unwrap()
        .write_all(b"# Guide\n\n## Installation\n")
        .unwrap();

    let content = r#"
# Test

[Link to guide](docs/guide#installation)
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Should not warn about extension-less link when .md file exists: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

/// Test multiple extension-less link patterns
#[test]
fn test_extensionless_link_patterns() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create various files
    File::create(base_path.join("readme.md"))
        .unwrap()
        .write_all(b"# Readme")
        .unwrap();
    File::create(base_path.join("CONTRIBUTING.md"))
        .unwrap()
        .write_all(b"# Contributing")
        .unwrap();

    let docs_dir = base_path.join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    File::create(docs_dir.join("api.md"))
        .unwrap()
        .write_all(b"# API")
        .unwrap();

    let content = r#"
# Test Extension-less Links

[Readme](readme#section)
[Contributing](CONTRIBUTING#how-to)
[API Docs](docs/api#methods)
[Missing](missing#section)
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only warn about the missing file
    assert_eq!(
        result.len(),
        1,
        "Expected 1 warning for missing file, got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(
        result[0].message.contains("missing"),
        "Expected warning about 'missing', got: {}",
        result[0].message
    );
}

/// Test extension-less links without fragments (should still work)
#[test]
fn test_extensionless_link_without_fragment() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    File::create(base_path.join("guide.md"))
        .unwrap()
        .write_all(b"# Guide")
        .unwrap();

    // Note: Extension-less links without fragments may or may not resolve
    // depending on the file system and context. This test documents current behavior.
    let content = "[Link](guide)\n";

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // With extension-less resolution, guide -> guide.md should work
    assert!(
        result.is_empty(),
        "Extension-less link to existing .md file should resolve: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

// =============================================================================
// Rustdoc backtick-wrapped URL tests
// =============================================================================

/// Test that rustdoc intra-doc links are not flagged
#[test]
fn test_rustdoc_backtick_links_not_flagged() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# Rust Documentation

See [`f32::is_subnormal`] for details on subnormal floats.

Check [`Vec::push`] for adding elements.

The [`Result::unwrap`] method panics on error.

[`f32::is_subnormal`]: https://doc.rust-lang.org/std/primitive.f32.html#method.is_subnormal
[`Vec::push`]: https://doc.rust-lang.org/std/vec/struct.Vec.html#method.push
[`Result::unwrap`]: https://doc.rust-lang.org/std/result/enum.Result.html#method.unwrap
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Rustdoc backtick links should not be flagged: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

/// Test various rustdoc link patterns
#[test]
fn test_rustdoc_link_patterns() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# Rustdoc Link Patterns

- [`panic!`] - macro
- [`Option`] - type
- [`std::fmt`] - module
- [`Iterator::next`] - trait method
- [`String::new`] - associated function
- [`Error`] - trait
- [`Err`] - enum variant
- [`Formatter`] - struct
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Rustdoc backtick reference links should not be flagged: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

/// Test that regular broken links are still flagged alongside rustdoc links
#[test]
fn test_rustdoc_links_mixed_with_regular_links() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    File::create(base_path.join("existing.md"))
        .unwrap()
        .write_all(b"# Existing")
        .unwrap();

    let content = r#"
# Mixed Links

See [`Vec::push`] for details.

[Valid link](existing.md)
[Broken link](missing.md)
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only warn about missing.md, not rustdoc links or existing.md
    assert_eq!(
        result.len(),
        1,
        "Expected 1 warning, got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(
        result[0].message.contains("missing.md"),
        "Expected warning about missing.md"
    );
}

/// Test inline rustdoc links (not reference style)
#[test]
fn test_rustdoc_inline_backtick_links() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# Inline Rustdoc Links

Check [`Option`](https://doc.rust-lang.org/std/option/enum.Option.html) for details.
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Inline rustdoc links with external URLs should not be flagged: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

// =============================================================================
// Duplicate warning prevention tests
// =============================================================================

/// Test that malformed links don't produce duplicate warnings
#[test]
fn test_no_duplicate_warnings_for_malformed_links() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Content with malformed link that could potentially match multiple patterns
    let content = r#"
# Test

[Broken link](missing.md)
[Another broken](also-missing.md)
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Count warnings per line
    let mut line_counts = std::collections::HashMap::new();
    for warning in &result {
        *line_counts.entry(warning.line).or_insert(0) += 1;
    }

    // Each line should have at most one warning
    for (line, count) in &line_counts {
        assert_eq!(*count, 1, "Line {line} has {count} warnings, expected at most 1");
    }
}

/// Test that reference-style links don't produce duplicate warnings
#[test]
fn test_no_duplicate_warnings_for_reference_links() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# Test

[Link text][ref]
[Another][ref]

[ref]: missing.md
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should not have duplicate warnings for the same reference definition
    let messages: Vec<_> = result.iter().map(|w| &w.message).collect();

    // Allow multiple warnings if they're on different lines, but not duplicates
    assert!(
        result.len() <= 3,
        "Too many warnings, possible duplicates: {messages:?}"
    );
}

/// Test complex document with multiple link types doesn't produce duplicates
#[test]
fn test_complex_document_no_duplicates() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    File::create(base_path.join("exists.md"))
        .unwrap()
        .write_all(b"# Exists")
        .unwrap();

    let content = r#"
# Complex Document

[Valid](exists.md)
[Missing](missing.md)
[Fragment only](#section)
[External](https://example.com)
[Missing with fragment](missing.md#section)

## References

[ref1]: missing.md
[ref2]: exists.md
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Count warnings per file reference
    let mut file_warnings: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for warning in &result {
        // Extract the file path from the warning message
        if warning.message.contains("missing.md") {
            *file_warnings.entry("missing.md".to_string()).or_insert(0) += 1;
        }
    }

    // missing.md appears multiple times in content but shouldn't produce excessive warnings
    let missing_count = file_warnings.get("missing.md").unwrap_or(&0);
    assert!(
        *missing_count <= 3,
        "Too many warnings for missing.md ({missing_count}), possible duplicates"
    );
}

// =============================================================================
// LaTeX math span tests (Issue #289)
// =============================================================================

/// Test that link-like patterns inside single-line display math are not flagged
/// Issue #289: MD057 incorrectly triggers on LaTeX math formulas like $[x](\zeta)$
#[test]
fn test_latex_single_line_display_math_not_flagged() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Z-transform formula that looks like a markdown link
    let content = r#"
# Z-Transform

$$X(\zeta) = \mathcal Z [x](\zeta) = \sum_k x(k) \zeta^{-k}$$
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "LaTeX display math should not trigger MD057. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

/// Test that link-like patterns inside inline math are not flagged
#[test]
fn test_latex_inline_math_not_flagged() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# Inline Math

The function $[x](\zeta)$ represents the evaluation.

Also check $f[n](x)$ and $g[k](t)$ for more examples.
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "LaTeX inline math should not trigger MD057. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

/// Test that link-like patterns inside multi-line math blocks are not flagged
#[test]
fn test_latex_multiline_math_block_not_flagged() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# Multi-line Math

$$
X(\zeta) = \mathcal Z [x](\zeta) = \sum_k x(k) \zeta^{-k}
$$

And another:

$$
[f](x) = \int_0^1 f(t) dt
$$
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "LaTeX multi-line math blocks should not trigger MD057. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

/// Test that real broken links outside math are still flagged
#[test]
fn test_latex_math_mixed_with_real_links() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create an existing file
    File::create(base_path.join("exists.md"))
        .unwrap()
        .write_all(b"# Test")
        .unwrap();

    let content = r#"
# Mixed Content

The formula $$[x](\zeta)$$ is LaTeX math.

[Valid link](exists.md)
[Broken link](missing.md)

Inline math $[f](x)$ in text.
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the real broken link
    assert_eq!(
        result.len(),
        1,
        "Expected 1 warning for missing.md, got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(
        result[0].message.contains("missing.md"),
        "Expected warning about missing.md"
    );
}

/// Test various LaTeX patterns that look like markdown links
#[test]
fn test_latex_link_like_patterns() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# LaTeX Patterns

- Evaluation: $[f](x)$
- Z-transform: $$\mathcal{Z}[x](z)$$
- Laplace: $\mathcal{L}[f](s)$
- Fourier: $$\mathcal{F}[g](\omega)$$
- Interval notation: $[a, b](c)$
- Set notation: $$[0, 1](x)$$
- Bracket function: $[n](k)$
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Various LaTeX patterns should not trigger MD057. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

/// Test that adjacent math and real links are handled correctly
#[test]
fn test_latex_adjacent_to_real_links() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    File::create(base_path.join("doc.md"))
        .unwrap()
        .write_all(b"# Doc")
        .unwrap();

    let content = r#"
# Adjacent Content

See $[f](x)$ and [doc](doc.md) for details.

The formula $$[g](y)$$ appears before [missing](missing.md).
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the real broken link, not math
    assert_eq!(
        result.len(),
        1,
        "Expected 1 warning for missing.md, got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(
        result[0].message.contains("missing.md"),
        "Expected warning about missing.md"
    );
}

/// Test math in code blocks is handled correctly (double protection)
#[test]
fn test_latex_math_in_code_block() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# Code Example

```latex
$$[x](\zeta) = \sum_k x(k)$$
```

Regular text here.
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Math in code blocks should not trigger MD057. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

/// Test complex document from issue #289
#[test]
fn test_issue_289_exact_example() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Exact content from issue #289
    let content = r#"$$X(\zeta) = \mathcal Z [x](\zeta) = \sum_k x(k) \zeta^{-k}$$"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Issue #289 example should not trigger MD057. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

/// Test that currency patterns are not treated as math (pulldown-cmark behavior)
#[test]
fn test_latex_currency_not_confused_with_math() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Currency patterns - pulldown-cmark requires balanced $ for math
    // $100 alone is not math, but $100$ would be
    let content = r#"
# Prices

The item costs $100 and [link](missing.md) is broken.

A range of $50-$100 is typical.
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // The broken link should still be flagged (currency $ doesn't hide it)
    assert_eq!(
        result.len(),
        1,
        "Currency patterns shouldn't affect link detection. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(result[0].message.contains("missing.md"));
}

/// Test escaped dollar signs
#[test]
fn test_latex_escaped_dollar_signs() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# Escaped Dollars

Use \$100 for escaped currency and [link](missing.md) is broken.

Real math: $[f](x)$ should be skipped.
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Only the broken link should be flagged
    assert_eq!(
        result.len(),
        1,
        "Only missing.md should be flagged. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(result[0].message.contains("missing.md"));
}

/// Test math inside HTML comments (should be ignored - HTML comment takes precedence)
#[test]
fn test_latex_math_in_html_comment() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# HTML Comment

<!-- This has math $[x](y)$ and link [z](missing.md) inside comment -->

Real broken [link](missing.md) here.
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Only the link outside the comment should be flagged
    assert_eq!(
        result.len(),
        1,
        "Only link outside HTML comment should be flagged. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

/// Test unbalanced/malformed math delimiters
#[test]
fn test_latex_unbalanced_delimiters() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Unbalanced $ should not be treated as math
    let content = r#"
# Unbalanced

Text with single $ sign and [link](missing.md) broken.

Also $$unclosed and [another](also-missing.md) broken.
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Both broken links should be flagged since math is unbalanced
    assert_eq!(
        result.len(),
        2,
        "Unbalanced math shouldn't hide broken links. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

/// Test environment variable patterns ($PATH, $HOME)
#[test]
fn test_latex_env_var_patterns() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# Environment Variables

Set $PATH and $HOME correctly. See [link](missing.md).

Real math $[f](x)$ is different.
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Only the broken link should be flagged
    assert_eq!(
        result.len(),
        1,
        "Env vars shouldn't affect link detection. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(result[0].message.contains("missing.md"));
}

/// Test math spans at document boundaries
#[test]
fn test_latex_math_at_boundaries() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Math at very start and end of document
    let content = r#"$[start](x)$ middle [link](missing.md) end $[end](y)$"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Only the broken link in the middle should be flagged
    assert_eq!(
        result.len(),
        1,
        "Math at boundaries should work correctly. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(result[0].message.contains("missing.md"));
}

/// Test nested-looking math patterns
#[test]
fn test_latex_nested_brackets() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"
# Nested Brackets in Math

Matrix: $$[[a](b)](c)$$

Function composition: $[f \circ g](x)$

Real broken: [link](missing.md)
"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        result.len(),
        1,
        "Nested brackets in math should be handled. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(result[0].message.contains("missing.md"));
}

/// Test consecutive math spans
#[test]
fn test_latex_consecutive_math_spans() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = r#"$[a](x)$$[b](y)$$[c](z)$ and [broken](missing.md)"#;

    let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        result.len(),
        1,
        "Consecutive math spans should all be detected. Got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
    assert!(result[0].message.contains("missing.md"));
}

// --- Tests for AbsoluteLinksOption::RelativeToDocs ---

use rumdl_lib::rules::{AbsoluteLinksOption, MD057Config};
use std::fs;

/// Helper to create a MkDocs project structure with mkdocs.yml and docs_dir.
/// Returns the path to the docs directory.
fn setup_mkdocs_project(temp_dir: &std::path::Path, docs_dir_name: &str) -> std::path::PathBuf {
    let mkdocs_content = if docs_dir_name != "docs" {
        format!("site_name: test\ndocs_dir: {docs_dir_name}\n")
    } else {
        "site_name: test\n".to_string()
    };
    fs::write(temp_dir.join("mkdocs.yml"), mkdocs_content).unwrap();

    let docs_dir = temp_dir.join(docs_dir_name);
    fs::create_dir_all(&docs_dir).unwrap();
    docs_dir
}

#[test]
fn test_relative_to_docs_resolves_valid_link() {
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    // Create the target file
    fs::write(docs_dir.join("getting-started.md"), "# Getting Started").unwrap();

    let content = "[Guide](/getting-started.md)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Expected no warnings for valid absolute link, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_flags_missing_link() {
    let temp_dir = tempdir().unwrap();
    let _docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    let content = "[Missing](/nonexistent.md)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config)
        .with_path(temp_dir.path().join("docs"));
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1, "Expected 1 warning, got: {result:?}");
    assert!(
        result[0].message.contains("does not exist"),
        "Expected 'does not exist' in message, got: {}",
        result[0].message
    );
    assert!(
        result[0].message.contains("/nonexistent.md"),
        "Expected '/nonexistent.md' in message, got: {}",
        result[0].message
    );
}

#[test]
fn test_relative_to_docs_extensionless_link() {
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    // Create the target file with .md extension
    fs::write(docs_dir.join("guide.md"), "# Guide").unwrap();

    let content = "[Guide](/guide)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Extensionless link should resolve to guide.md, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_fallback_without_mkdocs_yml() {
    // Create a temp dir without mkdocs.yml
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = "[Link](/some-page)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        result.len(),
        1,
        "Expected 1 warning (fallback to warn), got: {result:?}"
    );
    assert!(
        result[0].message.contains("no mkdocs.yml found"),
        "Expected fallback message, got: {}",
        result[0].message
    );
}

#[test]
fn test_relative_to_docs_custom_docs_dir() {
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "documentation");

    // Create the target file
    fs::write(docs_dir.join("index.md"), "# Home").unwrap();

    let content = "[Home](/index.md)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Custom docs_dir should be respected, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_with_fragment() {
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    // Create the target file
    fs::write(docs_dir.join("guide.md"), "# Guide\n## Section").unwrap();

    let content = "[Guide Section](/guide.md#section)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Fragment should be stripped before file check, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_image() {
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    // Create an image file
    let img_dir = docs_dir.join("assets");
    fs::create_dir_all(&img_dir).unwrap();
    fs::write(img_dir.join("logo.png"), "PNG").unwrap();

    let content = "![Logo](/assets/logo.png)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Image absolute link should resolve via docs_dir, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_image_missing() {
    let temp_dir = tempdir().unwrap();
    let _docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    let content = "![Missing](/assets/missing.png)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config)
        .with_path(temp_dir.path().join("docs"));
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1, "Expected 1 warning for missing image, got: {result:?}");
    assert!(result[0].message.contains("does not exist"));
}

#[test]
fn test_relative_to_docs_reference_definition() {
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    fs::write(docs_dir.join("api.md"), "# API").unwrap();

    let content = "[api]: /api.md\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Reference definition with valid absolute link should pass, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_directory_index() {
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    // Create a directory with index.md
    let sub_dir = docs_dir.join("getting-started");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("index.md"), "# Getting Started").unwrap();

    let content = "[Getting Started](/getting-started/)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Directory link with index.md should pass, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_directory_without_index() {
    // Directory exists but has no index.md — should warn
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    let sub_dir = docs_dir.join("empty-section");
    fs::create_dir_all(&sub_dir).unwrap();
    // No index.md inside

    let content = "[Empty](/empty-section/)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        result.len(),
        1,
        "Directory without index.md should warn, got: {result:?}"
    );
    assert!(
        result[0].message.contains("no index.md"),
        "Message should mention missing index.md, got: {}",
        result[0].message
    );
}

#[test]
fn test_relative_to_docs_directory_without_trailing_slash() {
    // Link to directory path without trailing slash — directory has index.md
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    let sub_dir = docs_dir.join("guide");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("index.md"), "# Guide").unwrap();

    let content = "[Guide](/guide)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Directory link without trailing slash should resolve via index.md, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_nested_subdirectory() {
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    let nested = docs_dir.join("api").join("v2");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("reference.md"), "# API v2 Reference").unwrap();

    let content = "[API Ref](/api/v2/reference.md)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Nested subdirectory link should resolve, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_url_encoded_path() {
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    fs::write(docs_dir.join("my guide.md"), "# My Guide").unwrap();

    let content = "[Guide](/my%20guide.md)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "URL-encoded absolute link should resolve, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_root_link() {
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    // Root link "/" should check for docs/index.md
    fs::write(docs_dir.join("index.md"), "# Home").unwrap();

    let content = "[Home](/)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Root link '/' should resolve to docs/index.md, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_root_link_missing_index() {
    let temp_dir = tempdir().unwrap();
    let _docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");
    // No index.md in docs/

    let content = "[Home](/)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config)
        .with_path(temp_dir.path().join("docs"));
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        result.len(),
        1,
        "Root link without index.md should warn, got: {result:?}"
    );
}

#[test]
fn test_existing_ignore_and_warn_unchanged() {
    // Regression test: existing ignore/warn behavior unchanged
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = "[Link](/absolute-path)\n";

    // Test ignore (default)
    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::Ignore,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Ignore should produce no warnings, got: {result:?}");

    // Test warn
    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::Warn,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Warn should produce 1 warning, got: {result:?}");
    assert!(
        result[0].message.contains("cannot be validated locally"),
        "Warn message should say 'cannot be validated locally', got: {}",
        result[0].message
    );
}

#[test]
fn test_absolute_links_config_deserialization() {
    // Verify all AbsoluteLinksOption variants deserialize correctly from TOML
    let toml_ignore: MD057Config = toml::from_str(r#"absolute-links = "ignore""#).unwrap();
    assert_eq!(toml_ignore.absolute_links, AbsoluteLinksOption::Ignore);

    let toml_warn: MD057Config = toml::from_str(r#"absolute-links = "warn""#).unwrap();
    assert_eq!(toml_warn.absolute_links, AbsoluteLinksOption::Warn);

    let toml_rtd: MD057Config = toml::from_str(r#"absolute-links = "relative_to_docs""#).unwrap();
    assert_eq!(toml_rtd.absolute_links, AbsoluteLinksOption::RelativeToDocs);

    // Also verify the snake_case alias works
    let toml_alias: MD057Config = toml::from_str(r#"absolute_links = "relative_to_docs""#).unwrap();
    assert_eq!(toml_alias.absolute_links, AbsoluteLinksOption::RelativeToDocs);

    // Default should be Ignore
    let toml_default: MD057Config = toml::from_str("").unwrap();
    assert_eq!(toml_default.absolute_links, AbsoluteLinksOption::Ignore);
}

#[test]
fn test_relative_to_docs_html_fallback() {
    // /page.html should resolve when page.md exists (HTML-to-markdown source fallback)
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    fs::write(docs_dir.join("about.md"), "# About").unwrap();

    let content = "[About](/about.html)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "HTML link should resolve via markdown source fallback, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_html_fallback_missing() {
    // /page.html should warn when neither page.html nor page.md exists
    let temp_dir = tempdir().unwrap();
    let _docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    let content = "[Missing](/nonexistent.html)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config)
        .with_path(temp_dir.path().join("docs"));
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(result.len(), 1, "Missing HTML link should warn, got: {result:?}");
    assert!(result[0].message.contains("does not exist"));
}

#[test]
fn test_relative_to_docs_query_parameter() {
    // Query parameters should be stripped before checking file existence
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    fs::write(docs_dir.join("page.md"), "# Page").unwrap();

    let content = "[Page](/page.md?v=2&ref=nav)\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Query parameters should be stripped, got: {result:?}"
    );
}

#[test]
fn test_relative_to_docs_mixed_links() {
    // Multiple absolute links: some valid, some broken
    let temp_dir = tempdir().unwrap();
    let docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    fs::write(docs_dir.join("exists.md"), "# Exists").unwrap();

    let content = "\
[Valid](/exists.md)
[Missing1](/does-not-exist.md)
[Missing2](/also-missing)
";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config).with_path(&docs_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        result.len(),
        2,
        "Expected 2 warnings for broken links, valid link should pass, got: {result:?}"
    );
    assert!(result.iter().any(|w| w.message.contains("does-not-exist")));
    assert!(result.iter().any(|w| w.message.contains("also-missing")));
}

#[test]
fn test_relative_to_docs_reference_definition_missing() {
    // Reference definition with broken absolute link
    let temp_dir = tempdir().unwrap();
    let _docs_dir = setup_mkdocs_project(temp_dir.path(), "docs");

    let content = "[missing-ref]: /nonexistent-page.md\n";

    let config = MD057Config {
        absolute_links: AbsoluteLinksOption::RelativeToDocs,
        ..Default::default()
    };
    let rule = rumdl_lib::rules::MD057ExistingRelativeLinks::from_config_struct(config)
        .with_path(temp_dir.path().join("docs"));
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    assert_eq!(
        result.len(),
        1,
        "Missing ref def absolute link should warn, got: {result:?}"
    );
    assert!(result[0].message.contains("does not exist"));
}

// =============================================================================
// compact-paths tests (Issue #391)
// =============================================================================

fn make_compact_paths_config() -> MD057Config {
    MD057Config {
        compact_paths: true,
        ..Default::default()
    }
}

#[test]
fn test_compact_paths_disabled_by_default() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create sub_dir/file.md
    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("file.md"), "# File").unwrap();

    // From sub_dir/, ../sub_dir/file.md is unnecessarily long
    let content = "[link](../sub_dir/file.md)\n";

    // Default config — compact_paths is false
    let rule = MD057ExistingRelativeLinks::new().with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no compact-paths warnings (feature disabled)
    assert!(
        result.iter().all(|w| !w.message.contains("simplified")),
        "Default config should not produce compact-paths warnings, got: {result:?}"
    );
}

#[test]
fn test_compact_paths_same_directory() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create sub_dir/file.md
    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("file.md"), "# File").unwrap();

    // From sub_dir/, link goes up then back into sub_dir
    let content = "[link](../sub_dir/file.md)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        1,
        "Expected 1 compact-paths warning, got: {compact_warnings:?}"
    );
    assert!(
        compact_warnings[0].message.contains("'file.md'"),
        "Should suggest 'file.md', got: {}",
        compact_warnings[0].message
    );
}

#[test]
fn test_compact_paths_dot_prefix() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    fs::write(base_path.join("file.md"), "# File").unwrap();

    // ./file.md can be simplified to file.md
    let content = "[link](./file.md)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        1,
        "Expected 1 compact-paths warning for ./file.md, got: {compact_warnings:?}"
    );
    assert!(
        compact_warnings[0].message.contains("'file.md'"),
        "Should suggest 'file.md', got: {}",
        compact_warnings[0].message
    );
}

#[test]
fn test_compact_paths_deep_traversal() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create a/sub/file.md
    let deep_dir = base_path.join("a").join("sub");
    fs::create_dir_all(&deep_dir).unwrap();
    fs::write(deep_dir.join("file.md"), "# File").unwrap();

    // From a/sub/, ../../a/sub/file.md → file.md
    let content = "[link](../../a/sub/file.md)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&deep_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        1,
        "Expected 1 compact-paths warning for deep traversal, got: {compact_warnings:?}"
    );
    assert!(
        compact_warnings[0].message.contains("'file.md'"),
        "Should suggest 'file.md', got: {}",
        compact_warnings[0].message
    );
}

#[test]
fn test_compact_paths_already_optimal() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create sibling/file.md
    let sub_dir = base_path.join("sub_dir");
    let sibling_dir = base_path.join("sibling");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::create_dir_all(&sibling_dir).unwrap();
    fs::write(sibling_dir.join("file.md"), "# File").unwrap();

    // From sub_dir/, ../sibling/file.md is already optimal
    let content = "[link](../sibling/file.md)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert!(
        compact_warnings.is_empty(),
        "Already optimal path should produce no compact-paths warnings, got: {compact_warnings:?}"
    );
}

#[test]
fn test_compact_paths_with_fragment() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("file.md"), "# File\n## Section").unwrap();

    // Fragment should be preserved in suggestion
    let content = "[link](../sub_dir/file.md#section)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        1,
        "Expected 1 compact-paths warning, got: {compact_warnings:?}"
    );
    assert!(
        compact_warnings[0].message.contains("'file.md#section'"),
        "Should preserve fragment in suggestion, got: {}",
        compact_warnings[0].message
    );
}

#[test]
fn test_compact_paths_with_query() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("file.md"), "# File").unwrap();

    // Query parameter should be preserved in suggestion
    let content = "[link](../sub_dir/file.md?v=1)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        1,
        "Expected 1 compact-paths warning, got: {compact_warnings:?}"
    );
    assert!(
        compact_warnings[0].message.contains("'file.md?v=1'"),
        "Should preserve query in suggestion, got: {}",
        compact_warnings[0].message
    );
}

#[test]
fn test_compact_paths_with_query_and_fragment() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("file.md"), "# File\n## Section").unwrap();

    // Both query and fragment should be preserved
    let content = "[link](../sub_dir/file.md?v=1#section)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        1,
        "Expected 1 compact-paths warning, got: {compact_warnings:?}"
    );
    assert!(
        compact_warnings[0].message.contains("'file.md?v=1#section'"),
        "Should preserve both query and fragment in suggestion, got: {}",
        compact_warnings[0].message
    );

    // Verify fix also preserves both
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, "[link](file.md?v=1#section)\n",
        "Fix should preserve query and fragment"
    );
}

#[test]
fn test_compact_paths_images() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("image.png"), "PNG data").unwrap();

    let content = "![img](../sub_dir/image.png)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        1,
        "Expected 1 compact-paths warning for image, got: {compact_warnings:?}"
    );
    assert!(
        compact_warnings[0].message.contains("'image.png'"),
        "Should suggest 'image.png', got: {}",
        compact_warnings[0].message
    );
}

#[test]
fn test_compact_paths_reference_defs() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("file.md"), "# File").unwrap();

    let content = "[ref]: ../sub_dir/file.md\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        1,
        "Expected 1 compact-paths warning for ref def, got: {compact_warnings:?}"
    );
    assert!(
        compact_warnings[0].message.contains("'file.md'"),
        "Should suggest 'file.md', got: {}",
        compact_warnings[0].message
    );
}

#[test]
fn test_compact_paths_nonexistent_file() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    // Do NOT create file.md — it doesn't exist

    // Should still warn about unnecessary traversal even if target is missing
    let content = "[link](../sub_dir/file.md)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    let broken_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("does not exist")).collect();

    assert_eq!(
        compact_warnings.len(),
        1,
        "Should still warn about path traversal, got: {compact_warnings:?}"
    );
    assert_eq!(
        broken_warnings.len(),
        1,
        "Should also warn about broken link, got: {broken_warnings:?}"
    );
}

#[test]
fn test_compact_paths_external_urls_skipped() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = "[link](https://example.com/../path)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "External URLs should not be checked for compact-paths, got: {result:?}"
    );
}

#[test]
fn test_compact_paths_fragment_only_skipped() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let content = "[link](#section)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Fragment-only links should not be checked, got: {result:?}"
    );
}

#[test]
fn test_compact_paths_plain_relative_no_warning() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    fs::write(base_path.join("file.md"), "# File").unwrap();

    // file.md has no traversal — should not warn
    let content = "[link](file.md)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Plain relative paths should not produce compact-paths warnings, got: {result:?}"
    );
}

#[test]
fn test_compact_paths_necessary_parent_traversal() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create structure: sub/child/ and file.md at root
    let sub_child = base_path.join("sub").join("child");
    fs::create_dir_all(&sub_child).unwrap();
    fs::write(base_path.join("file.md"), "# File").unwrap();

    // From sub/child/, ../../file.md is already the shortest path to root/file.md
    let content = "[link](../../file.md)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_child);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert!(
        compact_warnings.is_empty(),
        "Necessary parent traversal should not produce compact-paths warnings, got: {compact_warnings:?}"
    );
}

#[test]
fn test_compact_paths_mixed_traversal() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    // ./sub/../file.md normalizes to file.md
    fs::write(base_path.join("file.md"), "# File").unwrap();
    fs::create_dir_all(base_path.join("sub")).unwrap();

    let content = "[link](./sub/../file.md)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        1,
        "Mixed traversal should produce compact-paths warning, got: {compact_warnings:?}"
    );
    assert!(
        compact_warnings[0].message.contains("'file.md'"),
        "Should suggest 'file.md', got: {}",
        compact_warnings[0].message
    );
}

#[test]
fn test_compact_paths_fix_produces_correct_output() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("file.md"), "# File").unwrap();

    let content = "[link](../sub_dir/file.md)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    assert_eq!(fixed, "[link](file.md)\n", "Fix should replace path with compact form");
}

#[test]
fn test_compact_paths_fix_preserves_fragment() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("file.md"), "# File\n## Section").unwrap();

    let content = "[link](../sub_dir/file.md#section)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();

    assert_eq!(fixed, "[link](file.md#section)\n", "Fix should preserve fragment");
}

#[test]
fn test_compact_paths_config_deserialization() {
    // Default: compact-paths should be false
    let config: MD057Config = toml::from_str("").unwrap();
    assert!(!config.compact_paths, "compact_paths should default to false");

    // Kebab-case
    let config: MD057Config = toml::from_str("compact-paths = true").unwrap();
    assert!(config.compact_paths, "kebab-case should work");

    // Snake_case alias
    let config: MD057Config = toml::from_str("compact_paths = true").unwrap();
    assert!(config.compact_paths, "snake_case alias should work");
}

#[test]
fn test_compact_paths_warning_positions() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("file.md"), "# File").unwrap();

    // "Some text [link](../sub_dir/file.md) more" — URL starts at column 18
    let content = "Some text [link](../sub_dir/file.md) more\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(compact_warnings.len(), 1);
    assert_eq!(compact_warnings[0].line, 1, "Should be on line 1");
    // URL "../sub_dir/file.md" starts at byte 17 (0-indexed), column 18 (1-indexed)
    assert!(
        compact_warnings[0].column > 1,
        "Column should reflect URL position, not start of line, got: {}",
        compact_warnings[0].column
    );
}

#[test]
fn test_compact_paths_multiple_links_fix() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("a.md"), "# A").unwrap();
    fs::write(sub_dir.join("b.md"), "# B").unwrap();
    fs::write(sub_dir.join("c.md"), "# C").unwrap();

    let content = "\
[first](../sub_dir/a.md)
[second](../sub_dir/b.md)
[third](../sub_dir/c.md)
";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();
    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        3,
        "All three links should have compact-paths warnings"
    );

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, "[first](a.md)\n[second](b.md)\n[third](c.md)\n",
        "All three links should be compacted without corruption"
    );
}

#[test]
fn test_compact_paths_unicode_before_link() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    let sub_dir = base_path.join("sub_dir");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("file.md"), "# File").unwrap();

    // Multi-byte characters before the link test byte-offset calculation
    let content = "日本語テキスト [link](../sub_dir/file.md)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(&sub_dir);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();
    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(compact_warnings.len(), 1);

    // Fix should work correctly despite multi-byte characters preceding the link
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, "日本語テキスト [link](file.md)\n",
        "Fix should handle multi-byte characters correctly"
    );
}

#[test]
fn test_compact_paths_dot_prefix_image() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    fs::write(base_path.join("img.png"), "PNG").unwrap();

    let content = "![alt](./img.png)\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        1,
        "Image with ./ prefix should warn, got: {compact_warnings:?}"
    );
    assert!(
        compact_warnings[0].message.contains("'img.png'"),
        "Should suggest 'img.png', got: {}",
        compact_warnings[0].message
    );
}

#[test]
fn test_compact_paths_dot_prefix_reference_def() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path();

    fs::write(base_path.join("file.md"), "# File").unwrap();

    let content = "[ref]: ./file.md\n";

    let config = make_compact_paths_config();
    let rule = MD057ExistingRelativeLinks::from_config_struct(config).with_path(base_path);
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    let compact_warnings: Vec<_> = result.iter().filter(|w| w.message.contains("simplified")).collect();
    assert_eq!(
        compact_warnings.len(),
        1,
        "Reference def with ./ prefix should warn, got: {compact_warnings:?}"
    );
    assert!(
        compact_warnings[0].message.contains("'file.md'"),
        "Should suggest 'file.md', got: {}",
        compact_warnings[0].message
    );
}
