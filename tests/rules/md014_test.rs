use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD014CommandsShowOutput;

#[test]
fn test_valid_command() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -l\nfile1 file2\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_command() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -l\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2); // Command is on line 2, not line 1 (fence)
    assert_eq!(result[0].column, 1); // Highlights the $ character
    assert_eq!(result[0].end_line, 2);
    assert_eq!(result[0].end_column, 2); // End of $ character
}

#[test]
fn test_multiple_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -l\nfile1 file2\n$ pwd\n/home\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_command() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -l\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "```bash\nls -l\n```"); // No trailing newline in input, so none in output
}

#[test]
fn test_no_output_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ cd /home\n$ mkdir test\n$ touch file.txt\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // These commands don't require output
}

#[test]
fn test_mixed_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ cd /home\n$ ls -l\nfile1 file2\n$ touch test.txt\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_shell_prompt_variations() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```console\n$ ls -l\nfile1 file2\n> pwd\n/home\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_non_shell_code_blocks() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```python\n$ print('hello')\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Should ignore non-shell code blocks
}

#[test]
fn test_shell_language_variations() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```sh\n$ ls -l\nfile1 file2\n```\n```shell\n$ pwd\n/home\n```\n```console\n$ echo hello\nworld\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_disabled_output_check() {
    let rule = MD014CommandsShowOutput::with_show_output(false);
    let content = "```bash\n$ ls -l\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Should not check for output when disabled
}

#[test]
fn test_comments_in_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -l  # List files\nfile1 file2\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_indented_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n    $ ls -l\n    file1 file2\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_multiple_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -l\n$ pwd\n$ echo hello\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "```bash\nls -l\npwd\necho hello\n```"); // No trailing newline in input
}

#[test]
fn test_fix_indented_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n    $ ls -l\n    $ pwd\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "```bash\n    ls -l\n    pwd\n```"); // No trailing newline in input
}

#[test]
fn test_empty_lines_not_output() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ echo test\n\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Empty lines should not count as output");
}

#[test]
fn test_greater_than_prompt() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n> echo test\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Commands with > prompt should be flagged without output"
    );
}

#[test]
fn test_case_insensitive_languages() {
    let rule = MD014CommandsShowOutput::new();
    let content1 = "```BASH\n$ echo test\n```";
    let ctx1 = LintContext::new(content1, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result1 = rule.check(&ctx1).unwrap();
    assert_eq!(result1.len(), 1, "BASH (uppercase) should be recognized");

    let content2 = "```Shell\n$ echo test\n```";
    let ctx2 = LintContext::new(content2, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result2 = rule.check(&ctx2).unwrap();
    assert_eq!(result2.len(), 1, "Shell (mixed case) should be recognized");

    let content3 = "```CONSOLE\n$ echo test\n```";
    let ctx3 = LintContext::new(content3, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result3 = rule.check(&ctx3).unwrap();
    assert_eq!(result3.len(), 1, "CONSOLE (uppercase) should be recognized");

    let content4 = "```Terminal\n$ echo test\n```";
    let ctx4 = LintContext::new(content4, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result4 = rule.check(&ctx4).unwrap();
    assert_eq!(result4.len(), 1, "Terminal (mixed case) should be recognized");
}

#[test]
fn test_message_includes_command() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ git status\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert!(
        result[0].message.contains("git status"),
        "Error message should include the actual command"
    );
}

#[test]
fn test_no_output_commands_case_insensitive() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ CD /home\n$ MKDIR test\n$ Touch file.txt\n$ RM file.txt\n$ MV old new\n$ CP src dst\n$ EXPORT VAR=val\n$ SET -e\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "No-output commands should work case-insensitively");
}

#[test]
fn test_commands_only_without_other_content() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ pwd\n$ ls\n$ echo test\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Block with only commands (no output) should be flagged"
    );
}

#[test]
fn test_fix_preserves_trailing_newline() {
    let rule = MD014CommandsShowOutput::new();
    let content_with_newline = "```bash\n$ echo test\n```\n";
    let ctx1 = LintContext::new(content_with_newline, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed1 = rule.fix(&ctx1).unwrap();
    assert!(fixed1.ends_with('\n'), "Should preserve trailing newline");

    let content_without_newline = "```bash\n$ echo test\n```";
    let ctx2 = LintContext::new(
        content_without_newline,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let fixed2 = rule.fix(&ctx2).unwrap();
    assert!(!fixed2.ends_with('\n'), "Should not add trailing newline");
}

#[test]
fn test_complex_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ echo test | grep test\ntest\n$ find . -name '*.rs' | head -5\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Block has output so should not be flagged");
}

#[test]
fn test_only_commands_no_output() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls\n$ pwd\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Blocks with only commands (no output) should be flagged"
    );
}

#[test]
fn test_config_from_toml() {
    let mut config = rumdl_lib::config::Config::default();
    let mut rule_config = rumdl_lib::config::RuleConfig::default();
    rule_config
        .values
        .insert("show-output".to_string(), toml::Value::Boolean(false)); // kebab-case
    config.rules.insert("MD014".to_string(), rule_config);

    let rule = MD014CommandsShowOutput::from_config(&config);
    let content = "```bash\n$ echo test\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Rule should respect show_output=false from config");
}

#[test]
fn test_fix_range_calculation() {
    let rule = MD014CommandsShowOutput::new();
    let content = "# Header\n\n```bash\n$ echo test\n```\n\nMore content";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    // Verify the fix range is correct
    if let Some(_fix) = &result[0].fix {
        let fixed_content = rule.fix(&ctx).unwrap();
        assert!(
            fixed_content.contains("echo test\n```"),
            "Fix should produce correct output"
        );
    }
}

#[test]
fn test_multiple_blocks_in_document() {
    let rule = MD014CommandsShowOutput::new();
    let content = r#"# Doc

```bash
$ echo "has output"
has output
```

Some text

```bash
$ echo "no output"
```

More text

```python
print("not shell")
```

```console
$ pwd
/home
```
"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Only the bash block without output should be flagged");
    assert_eq!(result[0].line, 11, "Should flag line 11 (the command without output)");
}

#[test]
fn test_comments_not_treated_as_output() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -la\n# This is a comment\n# Another comment\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Comments starting with # should not count as output");
}

#[test]
fn test_whitespace_only_lines_not_output() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ echo test\n   \n\t\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Whitespace-only lines should not count as output");
}

#[test]
fn test_language_with_attributes() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash {.line-numbers}\n$ echo test\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should still recognize bash even with attributes");
}

#[test]
fn test_default_config() {
    let rule = MD014CommandsShowOutput::new();
    // By default, show_output should be true
    let content = "```bash\n$ echo test\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Default config should have show_output=true");
}
