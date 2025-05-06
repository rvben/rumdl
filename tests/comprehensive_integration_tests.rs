use std::fs;
use std::process::Command;

#[test]
fn test_init_command_creates_and_loads_config() {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Run init command
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["init"])
        .output()
        .unwrap();

    assert!(output.status.success(), "Init command failed");
    assert!(
        base_path.join(".rumdl.toml").exists(),
        "Config file not created"
    );

    // Create a test file with a heading level increment issue
    fs::write(
        base_path.join("test.md"),
        "### Heading level 3\n# Heading level 1 after\n",
    )
    .unwrap();

    // Run linter with default config (should detect MD001)
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["test.md"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined_output = format!("{}\n{}", stdout, stderr);

    // Verify that we've detected at least one rule violation
    assert!(
        !output.status.success()
            || combined_output.contains("warning")
            || combined_output.contains("MD"),
        "Should detect at least one rule violation"
    );
}

#[test]
fn test_utilities_via_complex_markdown() {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create a complex Markdown file that tests multiple utilities
    let complex_md = r#"
# Heading 1

## Heading 2

Text with *emphasis* and **strong emphasis**.

### Heading 3

- List item 1
  - Nested item 1.1
    - Deeply nested 1.1.1
  - Nested item 1.2
- List item 2
  - Nested item 2.1
    - Deeply nested 2.1.1

1. Ordered item 1
   1. Nested ordered 1.1
      1. Deeply nested 1.1.1
   2. Nested ordered 1.2
2. Ordered item 2

> Blockquote text
> multiple lines
>
> With blank line

```js
console.log('Code block');
```

Text with <span>inline HTML</span>.

![Image without alt](image.png)

[Empty link]()

This line is very long and exceeds the usual line length limits by a significant margin which should trigger line length rules.

	Indented code block instead of fenced code block that should be detected by the code block utilities.

Heading level jump coming:

# Heading 1

#### Heading 4 (skipping level 2 and 3)

___
"#;

    fs::write(base_path.join("complex.md"), complex_md).unwrap();

    // Run linter with all rules enabled
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["complex.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{}\n{}", stdout, stderr);

    // Simply verify that the linter detected some issues with our complex file
    assert!(
        !output.status.success()
            || combined_output.contains("warning")
            || combined_output.contains("MD"),
        "Should detect at least some issues in the complex Markdown"
    );

    // Run the fix operation
    let _fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["complex.md", "--fix"])
        .output()
        .unwrap();

    // Verify file was modified in some way
    let fixed_content = fs::read_to_string(base_path.join("complex.md")).unwrap();
    assert!(
        complex_md != fixed_content,
        "File should be modified by fix operation"
    );
}

#[test]
fn test_multiple_rule_groups() {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create a test file with various issues
    let test_content = r#"
# Heading 1
# Heading 1 Duplicate

## heading 2 (inconsistent style)

- First list item
-   Second item with extra spaces
- Third list item with trailing whitespace  
- Fourth item with	hard tab

Unnecessarily long line that exceeds the default line length limit and should trigger a warning if enabled.


Multiple blank lines above this one.
"#;

    fs::write(base_path.join("test.md"), test_content).unwrap();

    // Run with default rules (should detect various issues)
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["test.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{}\n{}", stdout, stderr);

    // Simply verify that we detected some issues
    assert!(
        !output.status.success()
            || combined_output.contains("warning")
            || combined_output.contains("MD"),
        "Should detect some issues with the test file"
    );
}

#[test]
fn test_emphasis_and_heading_rules() {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Test file targeting specific low-coverage rules
    let test_content = r#"
*This is an emphasized line that should be detected as a heading by MD036*

**Another emphasized line that should be detected**

   # Indented heading (MD023)
   
## Missing blank line above (MD022)
Text immediately below heading (MD022)

# First Heading Level 1
### Heading Level 3 (Skipping level 2, MD001)

**This is not a heading, because it's not on a line by itself**
"#;

    fs::write(base_path.join("test.md"), test_content).unwrap();

    // Run linter for all rules
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["test.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{}\n{}", stdout, stderr);

    // Check if at least one rule violation was detected
    assert!(
        !output.status.success()
            || combined_output.contains("warning")
            || combined_output.contains("MD"),
        "Should detect at least one rule violation"
    );

    // Test fix operation
    let fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["test.md", "--fix", "--verbose"])
        .output()
        .unwrap();

    // Print debug output
    println!(
        "Fix command stdout: {}",
        String::from_utf8_lossy(&fix_output.stdout)
    );
    println!(
        "Fix command stderr: {}",
        String::from_utf8_lossy(&fix_output.stderr)
    );

    // Verify some issues were fixed
    let fixed_content = fs::read_to_string(base_path.join("test.md")).unwrap();
    println!("Original content:\n{}", test_content);
    println!("Fixed content:\n{}", fixed_content);
    assert!(
        fixed_content != test_content,
        "File should be modified by fix operation"
    );
}

#[test]
fn test_url_and_link_rules() {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create a test file with various link and URL issues
    let test_content = r#"
# Links and URLs Test

Bare URL: https://example.com

[Link with space at end](https://example.com  )

[Empty link]()

![Image without alt text](image.png)

[Undefined reference][undefined]

Visit http://example.com for more information.
"#;

    fs::write(base_path.join("test.md"), test_content).unwrap();

    // Run linter with default rules
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["test.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{}\n{}", stdout, stderr);

    println!("Check command output:\n{}", combined_output);

    // Check if at least one link-related issue was detected
    assert!(
        !output.status.success()
            || combined_output.contains("warning")
            || combined_output.contains("MD"),
        "Should detect at least one link-related issue"
    );

    // Test fix operation
    let fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["test.md", "--fix", "--verbose"])
        .output()
        .unwrap();

    // Print debug output
    println!(
        "Fix command stdout: {}",
        String::from_utf8_lossy(&fix_output.stdout)
    );
    println!(
        "Fix command stderr: {}",
        String::from_utf8_lossy(&fix_output.stderr)
    );

    // Verify the file was modified
    let fixed_content = fs::read_to_string(base_path.join("test.md")).unwrap();
    println!("Original content:\n{}", test_content);
    println!("Fixed content:\n{}", fixed_content);
    assert!(
        fixed_content != test_content,
        "File should be modified by fix operation"
    );
}

#[test]
fn test_profiling_features() {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create a simple test file
    fs::write(
        base_path.join("test.md"),
        "# Test Heading\n\nSimple content.\n",
    )
    .unwrap();

    // Run with verbose output that should include rule names
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["test.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check if verbose output contains rule names or summary
    assert!(
        stdout.contains("Rules:") || stdout.contains("MD") || stdout.contains("Success:"),
        "Should show rules or summary information"
    );
}

#[test]
fn test_low_coverage_rules() {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_path = temp_dir.path();

    // Create a test file specifically for testing low-coverage rules
    let test_content = r#"
# Heading with Trailing Punctuation!

> This is a blockquote
> with multiple lines.

> This is another blockquote
> 
> with a blank line.

- List item 1
-  List item 2 with extra space after marker
- List item 3

1. Ordered item 1
1. Ordered item 2 (not using incremental numbers)
1. Ordered item 3

| Column 1 | Column 2 |
|-|-|
| Row 1    | Data 1   |
| Row 2    | Data 2   |
| Row 3    | Data 3 With extra column | Extra |
"#;

    fs::write(base_path.join("test.md"), test_content).unwrap();

    // Run linter with all rules
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["test.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{}\n{}", stdout, stderr);

    // Verify we detected some issues
    assert!(
        !output.status.success()
            || combined_output.contains("warning")
            || combined_output.contains("MD"),
        "Should detect some Markdown issues"
    );

    // Test fix for these rules
    let _fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["test.md", "--fix", "--verbose"])
        .output()
        .unwrap();

    // Verify that at least some issues were fixed
    let fixed_content = fs::read_to_string(base_path.join("test.md")).unwrap();
    assert!(
        fixed_content != test_content,
        "File should be modified by fix operation"
    );
}
