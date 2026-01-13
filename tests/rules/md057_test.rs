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
