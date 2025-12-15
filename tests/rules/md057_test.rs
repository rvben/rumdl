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
