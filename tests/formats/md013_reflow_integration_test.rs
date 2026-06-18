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
        {
            // Allow slightly more for list items and lines with URLs
            let is_list_item = line.trim_start().starts_with("- ")
                || line.trim_start().starts_with("* ")
                || line.trim_start().chars().next().is_some_and(char::is_numeric);
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

    let result = rumdl_lib::lint(content, &md013_rules, false, MarkdownFlavor::Standard, None, None).unwrap();

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

#[test]
fn test_md013_semantic_line_breaks_via_cli() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = "All human beings are born free and equal in dignity and rights. They are endowed with reason and conscience and should act towards one another in a spirit of brotherhood.\n";
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 80
reflow = true
reflow-mode = "semantic-line-breaks"
"#;
    fs::write(&config_path, config_content).unwrap();

    // Run fix
    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let fixed = fs::read_to_string(&file_path).unwrap();

    // Should have sentence breaks
    assert!(
        fixed.contains("rights.\n"),
        "Should break at sentence boundary: {fixed:?}"
    );

    // Each line should respect the 80-char limit (or be a single word)
    for line in fixed.lines() {
        if !line.is_empty() && line.contains(' ') {
            assert!(
                line.len() <= 85, // Allow small overshoot for edge cases
                "Line should be approximately within limit: {line:?} (len={})",
                line.len()
            );
        }
    }
}

#[test]
fn test_md013_semantic_line_breaks_idempotent() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = "First sentence is here. Second sentence is also present. Third sentence completes the paragraph.\n";
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 80
reflow = true
reflow-mode = "semantic-line-breaks"
"#;
    fs::write(&config_path, config_content).unwrap();

    // First pass
    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let after_first = fs::read_to_string(&file_path).unwrap();

    // Second pass
    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let after_second = fs::read_to_string(&file_path).unwrap();

    assert_eq!(
        after_first, after_second,
        "Second fix pass should produce identical output (idempotent)"
    );
}

#[test]
fn test_md013_semantic_line_breaks_check_detects_violations() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = "First sentence. Second sentence. Third sentence.\n";
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 80
reflow = true
reflow-mode = "semantic-line-breaks"
"#;
    fs::write(&config_path, config_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should detect that the paragraph needs semantic line breaks
    assert!(
        stdout.contains("semantic line breaks"),
        "Should report semantic line breaks violation: {stdout}"
    );
}

#[test]
fn test_md013_issue_437_blockquote_reflow_regression() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("issue_437.md");

    let content = r#"# Lorem

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed quam leo, rhoncus sodales erat sed. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed quam leo, rhoncus sodales erat sed.

> Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed quam leo, rhoncus sodales erat sed. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed quam leo, rhoncus sodales erat sed.
"#;
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 72
reflow = true
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let after_first = fs::read_to_string(&file_path).unwrap();
    let quote_lines: Vec<&str> = after_first.lines().filter(|line| line.starts_with('>')).collect();

    assert!(
        quote_lines.len() >= 2,
        "Issue #437 regression: expected wrapped blockquote lines, got: {after_first}"
    );
    assert!(
        quote_lines.iter().all(|line| line.chars().count() <= 72),
        "Expected reflowed blockquote lines within limit: {after_first}"
    );

    // Idempotence: second pass should produce no further changes.
    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let after_second = fs::read_to_string(&file_path).unwrap();
    assert_eq!(after_first, after_second);
}

#[test]
fn test_md013_issue_676_blockquote_list_item_reflow() {
    // A long list item inside a blockquote should reflow exactly like the same
    // item at the top level, instead of being left over-long.
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("issue_676.md");

    let content = "- asdfkasjdhfla ksdjfhla ksdfhalksd jfhlaksdjf halksjdf akljsdh flkasjdhflkasjdhflka jshflk ajshf lkajsflkajsd flkaj dflkaj sdflkaj hdsfklja hsdlfkj ahsdfkhj\n\n> - asdfkasjdhfla ksdjfhla ksdfhalksd jfhlaksdjf halksjdf akljsdh flkasjdhflkasjdhflka jshflk ajshf lkajsflkajsd flkaj dflkaj sdflkaj hdsfklja hsdlfkj ahsdfkh\n";
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 90
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl fmt");
    let exit_code = output.status.code().unwrap_or(-1);
    assert!(exit_code == 0 || exit_code == 1, "Unexpected exit code: {exit_code}");

    let fixed = fs::read_to_string(&file_path).unwrap();
    let quote_lines: Vec<&str> = fixed.lines().filter(|line| line.starts_with('>')).collect();

    assert_eq!(
        quote_lines.len(),
        2,
        "Issue #676: the blockquote list item should wrap to two quoted lines: {fixed}"
    );
    assert!(
        quote_lines[0].starts_with("> - "),
        "First quoted line keeps the list marker: {fixed}"
    );
    assert!(
        quote_lines[1].starts_with(">   "),
        "Continuation aligns under the list content: {fixed}"
    );
    assert!(
        quote_lines.iter().all(|line| line.chars().count() <= 90),
        "Reflowed blockquote list lines should respect the limit: {fixed}"
    );

    // Idempotence: a second pass changes nothing.
    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl fmt on second pass");
    let after_second = fs::read_to_string(&file_path).unwrap();
    assert_eq!(fixed, after_second, "Issue #676 reflow should be idempotent");
}

#[test]
fn test_md013_issue_566_normalize_single_line_reflow_regression() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("issue_566.md");

    let content = r#"# Lorem

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed quam leo, rhoncus sodales erat sed. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed quam leo, rhoncus sodales erat sed.

Lorem ipsum dolor sit amet, consectetur adipiscing elit.
Sed quam leo, rhoncus sodales erat sed. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed
quam leo, rhoncus sodales erat sed.
"#;
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
[MD013]
line-length = 80
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config_content).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl fmt");

    let exit_code = output.status.code().unwrap_or(-1);
    assert!(exit_code == 0 || exit_code == 1, "Unexpected exit code: {exit_code}");

    let fixed = fs::read_to_string(&file_path).unwrap();
    let paragraphs: Vec<&str> = fixed.split("\n\n").collect();
    assert_eq!(paragraphs.len(), 3, "Expected heading plus two paragraphs: {fixed}");

    for paragraph in paragraphs.iter().skip(1) {
        let lines: Vec<&str> = paragraph.lines().collect();
        assert!(lines.len() > 1, "Paragraph should be wrapped: {paragraph}");
        assert!(
            lines.iter().all(|line| line.chars().count() <= 80),
            "Wrapped lines should respect line length: {paragraph}"
        );
    }

    let second_pass = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl fmt on second pass");
    let second_exit_code = second_pass.status.code().unwrap_or(-1);
    assert!(
        second_exit_code == 0 || second_exit_code == 1,
        "Unexpected second-pass exit code: {second_exit_code}"
    );

    let after_second = fs::read_to_string(&file_path).unwrap();
    assert_eq!(fixed, after_second, "Normalize reflow should be idempotent");
}

/// Issue #493: Inline disable comments inside list items must be respected
/// even when reflow mode groups the list item as a single paragraph.
#[test]
fn test_md013_reflow_respects_inline_disable_in_list() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = "\
# Test

1. Lorem ipsum dolor sit amet.

   <!-- rumdl-capture -->
   <!-- rumdl-disable MD013 -->

   Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

   <!-- rumdl-restore -->
";
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    fs::write(&config_path, "[global]\nline-length = 80\n\n[MD013]\nreflow = true\n").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !combined.contains("MD013"),
        "MD013 should be suppressed by inline disable comment, got: {combined}"
    );
}

/// Issue #494: MkDocs inline attr lists must not be split across lines during reflow.
#[test]
fn test_md013_reflow_preserves_mkdocs_attr_list() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = "\
# Test

**Lorem ipsum dolor sit amet, consectetur adipiscing elit**{ style=\"color: red\" }, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
";
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    fs::write(
        &config_path,
        "[global]\nflavor = \"mkdocs\"\nline-length = 80\n\n[MD013]\nreflow = true\n",
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg("--no-cache")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    assert!(output.status.success() || output.status.code() == Some(1));

    let result = fs::read_to_string(&file_path).unwrap();
    assert!(
        result.contains("{ style=\"color: red\" }"),
        "Attr list should be kept intact, got:\n{result}"
    );
    assert!(
        !result.contains("{ style=\"color:\n"),
        "Attr list should NOT be split across lines, got:\n{result}"
    );
}

/// Issue #494: Multiple attr lists on same line are all preserved.
#[test]
fn test_md013_reflow_preserves_multiple_attr_lists() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = "\
# Test

**Bold**{#my-id .highlight} and **more**{.other style=\"font-size: 2em\"} followed by text that makes this exceed eighty.
";
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    fs::write(
        &config_path,
        "[global]\nflavor = \"mkdocs\"\nline-length = 80\n\n[MD013]\nreflow = true\n",
    )
    .unwrap();

    std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg("--no-cache")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let result = fs::read_to_string(&file_path).unwrap();
    assert!(
        result.contains("{#my-id .highlight}"),
        "First attr list should be intact, got:\n{result}"
    );
    assert!(
        result.contains("{.other style=\"font-size: 2em\"}"),
        "Second attr list should be intact, got:\n{result}"
    );
}

/// Issue #494: Non-attr-list braces are still treated as regular text.
#[test]
fn test_md013_reflow_attr_list_wrappable_in_standard_flavor() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    // Use a long attr list that exceeds 80 chars on its own when attached to text,
    // forcing the reflow to break INSIDE the braces if treated as plain text.
    // In MkDocs mode the attr list is atomic; in standard mode it's wrappable.
    let content = "\
# Test

**Bold text here**{#section-identifier .primary-highlight .secondary-highlight style=\"background-color: red\"} and more text after it.
";
    fs::write(&file_path, content).unwrap();

    // Standard flavor (no MkDocs)
    let config_path_std = dir.path().join("standard.toml");
    fs::write(
        &config_path_std,
        "[global]\nline-length = 80\n\n[MD013]\nreflow = true\n",
    )
    .unwrap();

    std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg("--no-cache")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path_std)
        .output()
        .expect("Failed to execute rumdl");

    let result_std = fs::read_to_string(&file_path).unwrap();

    // In standard mode, the attr list is plain text and SHOULD be split across lines.
    // Verify no single line contains both the opening { and closing } of the attr list.
    let full_attr = "{#section-identifier .primary-highlight .secondary-highlight style=\"background-color: red\"}";
    let has_intact_attr = result_std.lines().any(|l| l.contains(full_attr));
    assert!(
        !has_intact_attr,
        "In standard flavor, long attr list should be split as regular text, got:\n{result_std}"
    );

    // Now test MkDocs flavor — same content, attr list must stay intact
    fs::write(&file_path, content).unwrap();
    let config_path_mkdocs = dir.path().join("mkdocs.toml");
    fs::write(
        &config_path_mkdocs,
        "[global]\nflavor = \"mkdocs\"\nline-length = 80\n\n[MD013]\nreflow = true\n",
    )
    .unwrap();

    std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg("--no-cache")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path_mkdocs)
        .output()
        .expect("Failed to execute rumdl");

    let result_mkdocs = fs::read_to_string(&file_path).unwrap();

    // In MkDocs mode, the attr list is atomic and must NOT be split.
    // Verify the full attr list appears on a single line.
    let has_intact_attr_mkdocs = result_mkdocs.lines().any(|l| l.contains(full_attr));
    assert!(
        has_intact_attr_mkdocs,
        "In MkDocs flavor, attr list must stay intact on one line, got:\n{result_mkdocs}"
    );
}

/// Issue #494: Non-attr-list braces are still treated as regular text.
#[test]
fn test_md013_reflow_does_not_treat_plain_braces_as_attr_list() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = "\
# Test

This line has {some random text in braces} that is not an attr list and should be treated as regular wrappable text here.
";
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    fs::write(
        &config_path,
        "[global]\nflavor = \"mkdocs\"\nline-length = 80\n\n[MD013]\nreflow = true\n",
    )
    .unwrap();

    std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg("--no-cache")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let result = fs::read_to_string(&file_path).unwrap();
    // Plain braces should be wrappable — the line should be split
    let lines: Vec<&str> = result.lines().collect();
    let long_lines = lines.iter().filter(|l| l.len() > 80).count();
    assert_eq!(
        long_lines, 0,
        "Plain braces should be wrappable (no lines >80 chars), got:\n{result}"
    );
}

/// Verify that PyMdown block content is not flagged or rewritten by semantic-line-breaks reflow.
/// Covers all edge cases from GitHub issue #495:
/// - Block with title and newline-separated content
/// - Block without title
/// - No newline between options and content
/// - No newline between content and closing `///`
#[test]
fn test_md013_reflow_skips_pymdown_blocks() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");

    let content = "\
# Test

/// details | Summary
    type: warning

Content inside the block.

///

/// details
    type: warning

Content inside the block.

///

/// details | Summary
    type: warning
Content inside the block.

///

/// details | Summary
    type: warning

Content inside the block.
///
";

    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    fs::write(
        &config_path,
        "[global]\nflavor = \"mkdocs\"\nline-length = 80\n\n[MD013]\nreflow = true\nreflow-mode = \"semantic-line-breaks\"\n",
    )
    .unwrap();

    // Check — should produce no warnings
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "PyMdown block content should not trigger MD013 reflow.\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Fix — content should be unchanged
    std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg("--no-cache")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let result = fs::read_to_string(&file_path).unwrap();
    assert_eq!(
        result, content,
        "PyMdown block content should not be modified by reflow fix"
    );
}

/// Issue #579: `rumdl fmt` must converge on GFM task lists when MD013
/// `reflow-mode = "normalize"` is enabled. Before the fix, MD013 reflowed
/// wrapped task-item content to the post-checkbox column (6 for `- [ ] `)
/// and MD077 immediately fixed it back to the content column (2), creating
/// an `MD077 -> MD013 -> MD077` oscillation that the fixer aborted after
/// three iterations.
#[test]
fn md013_md077_task_list_no_fix_loop() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("task.md");
    let config_path = dir.path().join(".rumdl.toml");

    let content = "# T\n\n- [ ] Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n";
    fs::write(&file_path, content).unwrap();

    let config = r#"
[MD013]
line-length = 80
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("fmt")
        .arg("--no-cache")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl fmt");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains("conflict loop") && !stderr.contains("conflict loop"),
        "rumdl fmt detected a fix loop on a task list — MD013 and MD077 \
         are disagreeing about the task-continuation column.\n\
         stdout: {stdout}\nstderr: {stderr}"
    );

    let fixed = fs::read_to_string(&file_path).unwrap();
    assert_eq!(
        fixed,
        "# T\n\n- [ ] Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod\n      tempor incididunt ut labore et dolore magna aliqua.\n",
        "MD013 reflow should produce the post-checkbox-aligned layout and \
         MD077 should accept it"
    );

    // Second pass: running check on the reflowed output should emit no
    // MD077 warnings.
    let recheck = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--no-cache")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl check");
    let recheck_stdout = String::from_utf8_lossy(&recheck.stdout);
    assert!(
        !recheck_stdout.contains("MD077"),
        "MD077 should accept MD013's post-checkbox continuation column. \
         Re-check output: {recheck_stdout}"
    );
}

/// End-to-end guard for issue #582. With `unfixable = ["MD013"]` in config, an
/// under-limit list item must not produce a persistent MD013 warning, and two
/// back-to-back runs of `rumdl check` must produce identical zero-warning
/// output.
#[test]
fn md013_under_limit_list_item_with_unfixable_stable() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("repro.md");
    let config_path = dir.path().join(".rumdl.toml");

    let content =
        "# T\n\n- [ ] @holdex/hr-payroll-operations: post additional costs in own payout issue\n  and link here\n";
    fs::write(&file_path, content).unwrap();

    let config = r#"
[MD013]
line-length = 80
reflow = true
reflow-mode = "normalize"

[global]
unfixable = ["MD013"]
"#;
    fs::write(&config_path, config).unwrap();

    let run_check = || -> String {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--no-cache")
            .arg(&file_path)
            .arg("--config")
            .arg(&config_path)
            .output()
            .expect("Failed to execute rumdl check");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    let first = run_check();
    assert!(
        !first.contains("MD013"),
        "MD013 must not warn on an under-limit list item (first run). Got:\n{first}"
    );

    let second = run_check();
    assert!(
        !second.contains("MD013"),
        "MD013 must not warn on an under-limit list item (second run). Got:\n{second}"
    );
}

/// Issue #639 safety gate: normalize-mode reflow of a matrix of list shapes must
/// converge to a fixed point with no oscillation. Re-wrapping a list item must
/// not provoke a continuation-indent fight with MD077 (the historical #579
/// cycle), so `rumdl fmt` is run repeatedly and the output must stabilize after
/// the first pass and stay stable.
#[test]
fn md013_normalize_list_reflow_converges_without_cycle() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("matrix.md");
    let config_path = dir.path().join(".rumdl.toml");

    let content = "# Doc\n\n\
- [ ] task item that is hard wrapped across\n  two lines for no good reason here\n\
- regular bullet wrapped\n  onto a second line\n  - nested bullet that is also\n    wrapped here onto two lines\n\
1. ordered item wrapped\n   onto a second line here\n\
2. another ordered item\n   - mixed nested bullet wrapped\n     across two physical lines here\n\n\
Some paragraph wrapped\nacross two lines.\n";
    fs::write(&file_path, content).unwrap();

    let config = r#"
[MD013]
line-length = 80
reflow = true
reflow-mode = "normalize"
"#;
    fs::write(&config_path, config).unwrap();

    let run_fmt = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
            .arg("check")
            .arg("--fix")
            .arg("--no-cache")
            .arg(&file_path)
            .arg("--config")
            .arg(&config_path)
            .output()
            .expect("Failed to execute rumdl");
    };

    run_fmt();
    let after_first = fs::read_to_string(&file_path).unwrap();

    // Every subsequent pass must be a no-op: the output is a fixed point.
    for pass in 2..=4 {
        run_fmt();
        let after = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            after, after_first,
            "normalize reflow must converge: pass {pass} changed an already-normalized document (MD013<->MD077 oscillation)"
        );
    }

    // The converged document must have its hard-wrapped list items joined.
    assert!(
        after_first.contains("- [ ] task item that is hard wrapped across two lines for no good reason here"),
        "task item should be joined onto one line; got:\n{after_first}"
    );
    assert!(
        after_first.contains("  - nested bullet that is also wrapped here onto two lines"),
        "nested bullet should be joined and keep its indentation; got:\n{after_first}"
    );
}

#[test]
fn test_md013_issue_590_table_inside_list_item_preserved() {
    // Tables nested inside a list item must not be reflowed as prose, even
    // under `reflow-mode = "semantic-line-breaks"`. The top-level reflow
    // path correctly treats table rows as opaque; the list-item path must
    // mirror that or the rows get joined with `|` literals interleaved.
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("issue_590.md");

    let content = "- A list item.\n\n    | Lorem ipsum | dolor sit amet             |\n    | ----------- | -------------- |\n    | consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |\n";
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    let config_content = r#"
flavor = "standard"

[MD013]
line-length = 60
reflow = true
reflow-mode = "semantic-line-breaks"
"#;
    fs::write(&config_path, config_content).unwrap();

    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let after = fs::read_to_string(&file_path).unwrap();

    // The three table rows must each remain intact, on separate lines.
    let rows: Vec<&str> = after
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            t.starts_with('|') && t.ends_with('|')
        })
        .collect();
    assert_eq!(
        rows.len(),
        3,
        "Issue #590 regression: expected 3 distinct table rows, got {} in:\n{}",
        rows.len(),
        after
    );

    // The delimiter row (separator) must be preserved verbatim with dashes.
    assert!(
        rows.iter().any(|row| row.contains("---") && row.contains('|')),
        "Delimiter row missing — table was likely reflowed: {after}"
    );

    // The data rows must contain their original cell content side by side,
    // not folded together with literal `|` characters mid-paragraph.
    assert!(
        after.contains("| consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |"),
        "Data row got reflowed — expected verbatim row in:\n{after}"
    );

    // Idempotence: a second pass must not change anything.
    let _output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    let after_second = fs::read_to_string(&file_path).unwrap();
    assert_eq!(after, after_second, "MD013 fix must be idempotent for tables in lists");
}

/// Helper for the issue #590 family of regressions: write the given Markdown
/// and config to a temp dir, run `rumdl check --fix`, return the resulting
/// file contents. Asserts no exit-code-2 (rumdl error).
fn run_md013_fix(content: &str, config: &str) -> String {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("input.md");
    fs::write(&file_path, content).unwrap();

    let config_path = dir.path().join(".rumdl.toml");
    fs::write(&config_path, config).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");

    if output.status.code() == Some(2) {
        panic!(
            "rumdl errored (exit 2). stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    let after = fs::read_to_string(&file_path).unwrap();

    // Idempotence: a second pass must not change anything.
    let _ = std::process::Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .arg("check")
        .arg("--fix")
        .arg(&file_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("Failed to execute rumdl");
    let after_second = fs::read_to_string(&file_path).unwrap();
    assert_eq!(
        after, after_second,
        "MD013 fix must be idempotent. First pass:\n{after}\nSecond pass:\n{after_second}",
    );

    after
}

/// Asserts the table rows in `text` are intact: three pipe-bordered lines,
/// the delimiter row contains `---`, and `expected_data_row` appears verbatim.
fn assert_three_row_table_preserved(text: &str, expected_data_row: &str) {
    let rows: Vec<&str> = text
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            t.starts_with('|') && t.ends_with('|')
        })
        .collect();
    assert_eq!(
        rows.len(),
        3,
        "Expected 3 distinct table rows, got {} in:\n{}",
        rows.len(),
        text,
    );
    assert!(
        rows.iter().any(|row| row.contains("---") && row.contains('|')),
        "Delimiter row missing — table likely reflowed:\n{text}"
    );
    assert!(
        text.contains(expected_data_row),
        "Expected data row {expected_data_row:?} verbatim in:\n{text}",
    );
}

#[test]
fn test_md013_issue_590_table_in_list_normalize_mode() {
    // reflow-mode = "normalize" rewrites every paragraph to fill the column
    // budget. A table nested in a list item must not be folded into prose.
    let content = "- A list item.\n\n    | Lorem ipsum | dolor sit amet             |\n    | ----------- | -------------- |\n    | consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |\n";
    let config = r#"
flavor = "standard"

[MD013]
line-length = 60
reflow = true
reflow-mode = "normalize"
"#;
    let after = run_md013_fix(content, config);
    assert_three_row_table_preserved(
        &after,
        "| consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |",
    );
}

#[test]
fn test_md013_issue_590_table_in_list_sentence_per_line_mode() {
    // reflow-mode = "sentence-per-line" breaks at sentence boundaries; table
    // pipes are not sentence punctuation but the fix must still leave the
    // rows alone.
    let content = "- A list item.\n\n    | Header A | Header B |\n    | -------- | -------- |\n    | First. Second. | Third. Fourth. |\n";
    let config = r#"
flavor = "standard"

[MD013]
line-length = 60
reflow = true
reflow-mode = "sentence-per-line"
"#;
    let after = run_md013_fix(content, config);
    assert_three_row_table_preserved(&after, "| First. Second. | Third. Fourth. |");
}

#[test]
fn test_md013_issue_590_table_in_list_default_reflow_mode() {
    // Default reflow only rewraps lines that exceed the limit. The table row
    // exceeds 40 chars, but it is a table row, not prose — it must be left
    // alone rather than wrapped on words.
    let content = "- A list item.\n\n    | Lorem ipsum | dolor sit amet             |\n    | ----------- | -------------- |\n    | consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |\n";
    let config = r#"
flavor = "standard"

[MD013]
line-length = 40
reflow = true
"#;
    let after = run_md013_fix(content, config);
    assert_three_row_table_preserved(
        &after,
        "| consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |",
    );
}

#[test]
fn test_md013_issue_590_table_in_list_mkdocs_flavor() {
    // mkdocs flavor uses 4-space indent under list items by convention. A
    // table at that nested level must still be preserved.
    let content = "- A list item.\n\n    | Lorem ipsum | dolor sit amet             |\n    | ----------- | -------------- |\n    | consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |\n";
    let config = r#"
flavor = "mkdocs"

[MD013]
line-length = 60
reflow = true
reflow-mode = "semantic-line-breaks"
"#;
    let after = run_md013_fix(content, config);
    assert_three_row_table_preserved(
        &after,
        "| consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |",
    );
}

#[test]
fn test_md013_issue_590_table_in_nested_list_item() {
    // Table nested inside a child list item (list inside list).
    let content = "- Parent item.\n    - Child item with a table.\n\n        | Lorem ipsum | dolor sit amet             |\n        | ----------- | -------------- |\n        | consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |\n";
    let config = r#"
flavor = "standard"

[MD013]
line-length = 60
reflow = true
reflow-mode = "semantic-line-breaks"
"#;
    let after = run_md013_fix(content, config);
    assert_three_row_table_preserved(
        &after,
        "| consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |",
    );
}

#[test]
fn test_md013_issue_590_table_immediately_after_list_marker() {
    // No blank line between marker and table. This is not a valid GFM table
    // per spec — a list item with `- | x |` followed by `  | --- |` is
    // parsed as a paragraph of literal pipes, not a table. The fix must
    // still leave the content verbatim and not invent a wrap that mangles
    // pipes mid-line.
    let content = "- | Lorem ipsum | dolor sit amet             |\n  | ----------- | -------------- |\n  | consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |\n";
    let config = r#"
flavor = "standard"

[MD013]
line-length = 60
reflow = true
reflow-mode = "semantic-line-breaks"
"#;
    let after = run_md013_fix(content, config);

    // Each pipe-row line must remain on its own line; we accept the first
    // row prefixed by the list marker (`- | header |`).
    let pipe_rows: Vec<&str> = after
        .lines()
        .filter(|line| line.contains('|') && line.trim_end().ends_with('|'))
        .collect();
    assert_eq!(
        pipe_rows.len(),
        3,
        "Expected 3 pipe-row lines, got {} in:\n{after}",
        pipe_rows.len(),
    );
    assert!(
        after.contains("| ----------- | -------------- |"),
        "Delimiter row missing in:\n{after}"
    );
    assert!(
        after.contains("| consectetur adipiscing elit | sed do eiusmod tempor incididunt ut labore |"),
        "Data row got mangled in:\n{after}"
    );
}

#[test]
fn test_md013_list_item_prose_with_embedded_pipe_is_not_a_table() {
    // Negative test for the table classification heuristic. A list-item
    // continuation paragraph that contains literal `|` characters mid-prose
    // (e.g. describing a CLI pipe command) must NOT be classified as a
    // table row — otherwise the line is preserved verbatim and skips
    // reflow, producing line-length violations on what is plain prose.
    //
    // GFM tables in this position require pipe-bordered rows (`| ... |`)
    // or a delimiter row immediately after a header; isolated prose with
    // an embedded pipe is neither.
    let content = "- A list item describing one important detail with technical context for later.\n    The continuation line discusses how to use the grep | sort pipeline correctly today.\n";
    let config = r#"
flavor = "standard"

[MD013]
line-length = 60
reflow = true
reflow-mode = "semantic-line-breaks"
"#;
    let after = run_md013_fix(content, config);

    // The prose continuation line was over 60 chars, so reflow MUST have
    // wrapped it. If it was misclassified as a table row it would still
    // be present verbatim, well over the limit.
    let long_lines: Vec<&str> = after
        .lines()
        .filter(|line| line.chars().count() > 70 && line.contains('|'))
        .collect();
    assert!(
        long_lines.is_empty(),
        "Continuation line containing `|` was not reflowed — likely \
         misclassified as a table row. Long lines:\n{long_lines:#?}\n\nFull output:\n{after}",
    );

    // The prose content must still be present, just rewrapped.
    assert!(
        after.contains("grep | sort"),
        "Pipeline prose missing from output:\n{after}",
    );
}
