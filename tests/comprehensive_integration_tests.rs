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
    assert!(base_path.join(".rumdl.toml").exists(), "Config file not created");

    // Create a test file with a heading level increment issue (jumping from level 1 to level 3)
    fs::write(
        base_path.join("test.md"),
        "# Heading level 1\n### Heading level 3 (skipping level 2)\n",
    )
    .unwrap();

    // Run linter with default config (should detect MD001)
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["check", "test.md"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined_output = format!("{stdout}\n{stderr}");

    // Check for specific rules that should trigger:
    // - MD002: First heading should be level 1 (found level 3)
    // - MD022: Missing blank lines around headings
    // - MD041: First line in file should be a level 1 heading
    assert!(
        combined_output.contains("MD002") || combined_output.contains("MD022") || combined_output.contains("MD041"),
        "Should detect at least one of: MD002 (first heading h1), MD022 (blanks around headings), or MD041 (first line heading)"
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
        .args(["check", "complex.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{stdout}\n{stderr}");

    // Check for specific rules that should trigger in this complex markdown:
    // - MD001: Heading level jump from H1 to H4 (lines 94-96)
    // - MD013: Long line exceeding usual limits (line 88)
    // - MD042: Empty link with no URL (line 86)
    // - MD045: Image without alt text (line 84)
    // - MD046: Mixed code block styles (indented code block on line 90)
    assert!(
        combined_output.contains("MD001")
            || combined_output.contains("MD013")
            || combined_output.contains("MD042")
            || combined_output.contains("MD045")
            || combined_output.contains("MD046"),
        "Should detect at least one of: MD001 (heading jump), MD013 (line length), MD042 (empty link), MD045 (no alt text), or MD046 (code block style)"
    );

    // Run the fix operation
    let _fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["check", "complex.md", "--fix"])
        .output()
        .unwrap();

    // Verify file was modified in some way
    let fixed_content = fs::read_to_string(base_path.join("complex.md")).unwrap();
    assert!(complex_md != fixed_content, "File should be modified by fix operation");
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
        .args(["check", "test.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{stdout}\n{stderr}");

    // Check for specific rules that should trigger:
    // - MD003: Inconsistent heading style "## heading 2" (line 143)
    // - MD009: Trailing whitespace on line 147
    // - MD010: Hard tab on line 148
    // - MD012: Multiple consecutive blank lines (lines 151-152)
    // - MD013: Long line exceeding limit (line 149)
    // - MD024: Duplicate "Heading 1" content (lines 139-140)
    // - MD030: Inconsistent spaces after list markers (line 146)
    assert!(
        combined_output.contains("MD003")
            || combined_output.contains("MD009")
            || combined_output.contains("MD010")
            || combined_output.contains("MD012")
            || combined_output.contains("MD013")
            || combined_output.contains("MD024")
            || combined_output.contains("MD030"),
        "Should detect at least one of: MD003 (heading style), MD009 (trailing spaces), MD010 (tabs), MD012 (multiple blanks), MD013 (line length), MD024 (duplicate heading), or MD030 (list marker spaces)"
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
        .args(["check", "test.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{stdout}\n{stderr}");

    // Check for specific rules that should trigger:
    // - MD001: Heading level jump from H1 to H3 (lines 191-192)
    // - MD022: Missing blank lines around headings (lines 188-189)
    // - MD023: Indented heading (line 185)
    // - MD036: Emphasis used as heading (lines 182-183)
    assert!(
        combined_output.contains("MD001")
            || combined_output.contains("MD022")
            || combined_output.contains("MD023")
            || combined_output.contains("MD036"),
        "Should detect at least one of: MD001 (heading increment), MD022 (blanks around headings), MD023 (heading start left), or MD036 (emphasis as heading)"
    );

    // Test fix operation
    let fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["check", "test.md", "--fix", "--verbose"])
        .output()
        .unwrap();

    // Print debug output
    println!("Fix command stdout: {}", String::from_utf8_lossy(&fix_output.stdout));
    println!("Fix command stderr: {}", String::from_utf8_lossy(&fix_output.stderr));

    // Verify some issues were fixed
    let fixed_content = fs::read_to_string(base_path.join("test.md")).unwrap();
    println!("Original content:\n{test_content}");
    println!("Fixed content:\n{fixed_content}");
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
        .args(["check", "test.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{stdout}\n{stderr}");

    println!("Check command output:\n{combined_output}");

    // Check for specific link/URL rules that should trigger:
    // - MD034: Bare URL without angle brackets (lines 246, 256)
    // - MD039: Space inside link text (line 248)
    // - MD042: Empty link with no URL (line 250)
    // - MD045: Image without alt text (line 252)
    // - MD052: Undefined reference link (line 254)
    assert!(
        combined_output.contains("MD034")
            || combined_output.contains("MD039")
            || combined_output.contains("MD042")
            || combined_output.contains("MD045")
            || combined_output.contains("MD052"),
        "Should detect at least one of: MD034 (bare URL), MD039 (space in links), MD042 (empty link), MD045 (no alt text), or MD052 (undefined reference)"
    );

    // Test fix operation
    let fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["check", "test.md", "--fix", "--verbose"])
        .output()
        .unwrap();

    // Print debug output
    println!("Fix command stdout: {}", String::from_utf8_lossy(&fix_output.stdout));
    println!("Fix command stderr: {}", String::from_utf8_lossy(&fix_output.stderr));

    // Verify the file was modified
    let fixed_content = fs::read_to_string(base_path.join("test.md")).unwrap();
    println!("Original content:\n{test_content}");
    println!("Fixed content:\n{fixed_content}");
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
    fs::write(base_path.join("test.md"), "# Test Heading\n\nSimple content.\n").unwrap();

    // Run with verbose output that should include rule names
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["check", "test.md", "--verbose"])
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
        .args(["check", "test.md", "--verbose"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{stdout}\n{stderr}");

    // Check for specific rules that should trigger:
    // - MD026: Trailing punctuation in heading (line 332)
    // - MD028: Blank line inside blockquote (lines 337-339)
    // - MD029: Ordered list not using incremental numbers (lines 345-347)
    // - MD030: Extra space after list marker (line 342)
    // - MD056: Table column count inconsistent (line 353 has extra column)
    assert!(
        combined_output.contains("MD026")
            || combined_output.contains("MD028")
            || combined_output.contains("MD029")
            || combined_output.contains("MD030")
            || combined_output.contains("MD056"),
        "Should detect at least one of: MD026 (trailing punctuation), MD028 (blank in blockquote), MD029 (ordered list prefix), MD030 (list marker space), or MD056 (table columns)"
    );

    // Test fix for these rules
    let _fix_output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(base_path)
        .args(["check", "test.md", "--fix", "--verbose"])
        .output()
        .unwrap();

    // Verify that at least some issues were fixed
    let fixed_content = fs::read_to_string(base_path.join("test.md")).unwrap();
    assert!(
        fixed_content != test_content,
        "File should be modified by fix operation"
    );
}
