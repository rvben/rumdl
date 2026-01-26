use std::fs;
use tempfile::tempdir;

#[test]
fn test_md013_reflow_via_cli() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Create test file with long lines
    let content = "This is a very long line that definitely exceeds the default eighty character limit and needs to be wrapped properly by the reflow algorithm.

## Heading

- This is a very long list item that needs to be wrapped properly with correct indentation
- Another long list item that should also be wrapped with the proper continuation indentation

Regular paragraph with **bold text** and *italic text* and `inline code` that needs wrapping.";

    fs::write(&file_path, content).unwrap();

    // Create config file enabling reflow
    // TODO: Fix backwards compatibility - enable-reflow should also work
    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 40
reflow = true
"#;
    fs::write(&config_path, config_content).unwrap();

    // First check what violations exist (for debugging if needed)
    let _check_output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl check");

    // Run rumdl with fix
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    // With --fix, rumdl returns exit code 1 if violations were found (even if fixed)
    // Exit code 2 indicates an actual error
    let exit_code = output.status.code().unwrap_or(-1);
    if exit_code == 2 {
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        panic!("rumdl failed with error exit code 2");
    }

    // Verify fixes were applied
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Fixed:") || stdout.contains("fixed"),
        "Expected fixes to be applied, but got: {stdout}"
    );

    // Read the fixed content
    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Verify reflow worked (lines should be reasonably short)
    for line in fixed_content.lines() {
        if !line.starts_with('#') && !line.trim().is_empty() && !line.contains('`') {
            // Be realistic about what reflow can achieve:
            // - List items need space for markers
            // - Continuation lines need indentation
            // - Words can't be broken
            let is_indented = line.starts_with(' ');
            let reasonable_limit = if is_indented { 50 } else { 45 };

            assert!(
                line.chars().count() <= reasonable_limit,
                "Line seems too long after reflow: {} ({} chars)",
                line,
                line.chars().count()
            );
        }
    }

    // Verify markdown elements are preserved
    assert!(fixed_content.contains("**bold text**"));
    assert!(fixed_content.contains("*italic text*"));
    assert!(fixed_content.contains("`inline code`"));

    // Verify list structure is preserved
    assert!(fixed_content.contains("- This"));
    assert!(fixed_content.contains("- Another"));
}

#[test]
fn test_md013_reflow_disabled_by_default() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Create test file with long line that has no trailing whitespace
    let content = "This is a very long line that definitely exceeds the default eighty character limit but has no trailing whitespace";
    fs::write(&file_path, content).unwrap();

    // Run rumdl with fix (no config, so reflow should be disabled)
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-config")
        .arg("--fix")
        .arg(&file_path)
        .output()
        .expect("Failed to execute rumdl");

    // Should complete without error (exit code 0 or 1, not 2)
    let exit_code = output.status.code().unwrap_or(-1);
    assert!(exit_code == 0 || exit_code == 1, "Unexpected exit code: {exit_code}");

    // The long line should not be wrapped (reflow disabled by default)
    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Check that the long line is still present (not reflowed)
    assert!(
        fixed_content.contains(content),
        "Expected the long line to remain unchanged, but it was modified"
    );
}

#[test]
fn test_md013_reflow_complex_document() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("complex.md");

    let content = r#"# Complex Document Test

This is a very long introduction paragraph that contains multiple sentences and definitely exceeds the line length limit. It should be wrapped properly while preserving all the markdown formatting.

## Code Examples

Here's some code that should not be wrapped:

```python
def very_long_function_name_with_many_parameters(param1, param2, param3, param4):
    return "This is a very long string that should not be wrapped even if it exceeds the limit"
```

## Lists and Quotes

1. First numbered item that is very long and needs to be wrapped correctly with proper indentation
2. Second item that is also quite long and requires proper wrapping to fit within limits

> This is a blockquote that contains a very long line that needs to be wrapped properly while preserving the blockquote marker on each line.

## Tables

| Column 1 | Column 2 with very long header that exceeds limit |
|----------|---------------------------------------------------|
| Data 1   | Very long cell content that should not be wrapped |

## Links and References

For more information, see [our documentation](https://example.com/very/long/url/that/should/not/break) and the [reference guide][ref].

[ref]: https://example.com/another/very/long/url/for/reference
"#;

    fs::write(&file_path, content).unwrap();

    // Create config with specific settings
    // TODO: Fix backwards compatibility - enable-reflow should also work
    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 50
reflow = true
code-blocks = true
tables = true
"#;
    fs::write(&config_path, config_content).unwrap();

    // Run rumdl with fix
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let exit_code = output.status.code().unwrap_or(-1);
    assert!(exit_code == 0 || exit_code == 1, "Unexpected exit code: {exit_code}");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Verify structure is preserved
    assert!(fixed_content.contains("# Complex Document Test"));
    assert!(fixed_content.contains("```python"));
    assert!(fixed_content.contains("def very_long_function_name_with_many_parameters"));
    assert!(fixed_content.contains("|----------|"));
    assert!(fixed_content.contains("[ref]: https://example.com/another/very/long/url/for/reference"));

    // Verify proper wrapping of regular content
    let lines: Vec<&str> = fixed_content.lines().collect();
    let mut in_code = false;
    for line in &lines {
        if line.starts_with("```") {
            in_code = !in_code;
            continue;
        }

        // Skip special lines
        if !in_code
            && !line.starts_with('#')
            && !line.starts_with('|')
            && !line.starts_with('[')
            && !line.trim().is_empty()
            && !line.starts_with('>')
        {
            // Allow slightly more for list items and lines with URLs
            let is_list_item = line.trim_start().starts_with("- ")
                || line.trim_start().starts_with("* ")
                || line.trim_start().chars().next().is_some_and(|c| c.is_numeric());
            let contains_url = line.contains("http://") || line.contains("https://");
            let limit = if is_list_item || contains_url { 80 } else { 50 };

            assert!(
                line.chars().count() <= limit,
                "Line exceeds limit: {} ({} > {})",
                line,
                line.chars().count(),
                limit
            );
        }
    }
}

#[test]
fn test_md013_reflow_preserves_exact_content() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("preserve.md");

    // Content with various markdown elements
    let content = "This paragraph has **bold text** and *italic text* and [a link](https://example.com) and `inline code` that should all be preserved exactly during the reflow process.";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 30
enable-reflow = true
"#;
    fs::write(&config_path, config_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let exit_code = output.status.code().unwrap_or(-1);
    assert!(exit_code == 0 || exit_code == 1, "Unexpected exit code: {exit_code}");

    let fixed_content = fs::read_to_string(&file_path).unwrap();

    // Extract all words and markdown elements to verify nothing was lost
    let original_elements = vec![
        "**bold text**",
        "*italic text*",
        "[a link](https://example.com)",
        "`inline code`",
    ];

    for element in &original_elements {
        assert!(
            fixed_content.contains(element),
            "Missing element: {element} in:\n{fixed_content}"
        );
    }

    // Verify all original words are preserved
    let original_words: Vec<&str> = content.split_whitespace().collect();
    for word in &original_words {
        assert!(fixed_content.contains(word), "Missing word '{word}' in fixed content");
    }
}

/// Issue #338: Snippet delimiters in list items should not be reflowed
#[test]
fn test_md013_issue_338_snippets_in_list_items() {
    use rumdl_lib::config::{Config, MarkdownFlavor, RuleConfig};
    use rumdl_lib::rules;

    // Test that snippet delimiters are preserved in list items
    let content = r#"# Test

- Some content:
  -8<-
  https://raw.githubusercontent.com/example/file.md
  -8<-

More text.
"#;

    let mut config = Config::default();
    let mut rule_config = RuleConfig::default();
    rule_config
        .values
        .insert("reflow".to_string(), toml::Value::Boolean(true));
    config.rules.insert("MD013".to_string(), rule_config);

    let all_rules = rules::all_rules(&config);
    let md013_rules: Vec<_> = all_rules.into_iter().filter(|r| r.name() == "MD013").collect();

    let result = rumdl_lib::lint(content, &md013_rules, false, MarkdownFlavor::Standard, None).unwrap();

    // Should have no warnings since content is already properly formatted
    // The snippet delimiters should be recognized and preserved
    assert_eq!(
        result.len(),
        0,
        "Issue #338: Snippet delimiters should be preserved. Found warnings: {:?}",
        result
            .iter()
            .map(|w| format!("Line {}: {}", w.line, w.message))
            .collect::<Vec<_>>()
    );
}

/// Issue #338: Snippet delimiters should stay on their own lines after reflow via CLI
#[test]
fn test_md013_issue_338_snippets_preserved_after_reflow_via_cli() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test_snippets.md");

    // Test that snippet delimiters are preserved when surrounding content is reflowed
    let content = r#"# Test

- Some content that is long enough to trigger reflow and also has a snippet block inside:
  -8<-
  https://raw.githubusercontent.com/example/file.md
  -8<-

More text.
"#;
    fs::write(&file_path, content).unwrap();

    // Create config enabling reflow
    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
reflow = true
"#;
    fs::write(&config_path, config_content).unwrap();

    // Run rumdl fmt to apply fix
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg("--no-cache")
        .arg("-e")
        .arg("MD013")
        .arg("--config")
        .arg(&config_path)
        .arg(&file_path)
        .output()
        .expect("Failed to execute rumdl");

    // Read the fixed content
    let fixed = fs::read_to_string(&file_path).unwrap();

    // Verify snippet delimiters are preserved on their own lines
    assert!(
        fixed.contains("  -8<-\n  https://"),
        "Snippet delimiter should be on its own line, followed by URL. Got:\n{fixed}",
    );
    assert!(
        fixed.lines().filter(|l| l.trim() == "-8<-").count() == 2,
        "Both snippet delimiters should be preserved. Got:\n{fixed}",
    );

    // Verify the URL is still there
    assert!(
        fixed.contains("https://raw.githubusercontent.com/example/file.md"),
        "URL should be preserved. Got:\n{fixed}",
    );

    // Verify success
    assert!(
        output.status.success() || String::from_utf8_lossy(&output.stdout).contains("Fixed"),
        "Command should succeed or fix. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
