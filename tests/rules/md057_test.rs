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
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have no warnings for external links
    assert_eq!(result.len(), 0, "Expected 0 warnings, got {}", result.len());
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
    let ctx = LintContext::new(content);
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

    // Test the rule
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();

    // Should have two warnings even with disable comment
    assert_eq!(result.len(), 2, "Expected 2 warnings, got {}", result.len());

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
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
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
