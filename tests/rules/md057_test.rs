use rumdl::rule::Rule;
use rumdl::rules::MD057ExistingRelativeLinks;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
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
    let result = rule.check(content).unwrap();
    
    // Should have one warning for the missing link
    assert_eq!(result.len(), 1, "Expected 1 warning, got {}", result.len());
    assert!(result[0].message.contains("missing.md"),
            "Expected warning about missing.md, got: {}", result[0].message);
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
    let result = rule.check(content).unwrap();
    
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
    let result = rule.check(content).unwrap();
    
    // Should only have one warning for the link outside the code block
    assert_eq!(result.len(), 1, "Expected 1 warning, got {}", result.len());
    assert!(result[0].message.contains("missing.md"),
            "Expected warning about missing.md, got: {}", result[0].message);
    
    // Make sure the link in the code block is not flagged
    for warning in &result {
        assert!(!warning.message.contains("also-missing.md"),
                "Found unexpected warning for link in code block: {}", warning.message);
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
    let result = rule.check(content).unwrap();
    
    // Should only have one warning for the link after enabling the rule
    assert_eq!(result.len(), 1, "Expected 1 warning, got {}", result.len());
    assert!(result[0].message.contains("also-missing.md"),
            "Expected warning about also-missing.md, got: {}", result[0].message);
    
    // Make sure the disabled link is not flagged
    for warning in &result {
        assert!(!warning.message.contains("'missing.md'"),
                "Found warning for disabled rule: {}", warning.message);
    }
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
    let result = rule.check(content).unwrap();
    
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
        assert!(!warning.message.contains("docs/exists.md"),
                "Found unexpected warning for valid link: {}", warning.message);
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
    let result = rule.check(content).unwrap();
    
    // Should have no warnings when no base path is set
    assert_eq!(result.len(), 0, "Expected 0 warnings when no base path is set, got {}", result.len());
} 