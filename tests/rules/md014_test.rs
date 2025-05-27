use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD014CommandsShowOutput;

#[test]
fn test_valid_command() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -l\nfile1 file2\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_command() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -l\n```";
    let ctx = LintContext::new(content);
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
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_command() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -l\n```";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "```bash\nls -l\n```\n"); // Fix method preserves trailing newline
}

#[test]
fn test_no_output_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ cd /home\n$ mkdir test\n$ touch file.txt\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // These commands don't require output
}

#[test]
fn test_mixed_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ cd /home\n$ ls -l\nfile1 file2\n$ touch test.txt\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_shell_prompt_variations() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```console\n$ ls -l\nfile1 file2\n> pwd\n/home\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_non_shell_code_blocks() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```python\n$ print('hello')\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Should ignore non-shell code blocks
}

#[test]
fn test_shell_language_variations() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```sh\n$ ls -l\nfile1 file2\n```\n```shell\n$ pwd\n/home\n```\n```console\n$ echo hello\nworld\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_disabled_output_check() {
    let rule = MD014CommandsShowOutput::with_show_output(false);
    let content = "```bash\n$ ls -l\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Should not check for output when disabled
}

#[test]
fn test_comments_in_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -l  # List files\nfile1 file2\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_indented_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n    $ ls -l\n    file1 file2\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_multiple_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n$ ls -l\n$ pwd\n$ echo hello\n```";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "```bash\nls -l\npwd\necho hello\n```\n"); // Fix method preserves trailing newline
}

#[test]
fn test_fix_indented_commands() {
    let rule = MD014CommandsShowOutput::new();
    let content = "```bash\n    $ ls -l\n    $ pwd\n```";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "```bash\n    ls -l\n    pwd\n```\n"); // Fix method preserves trailing newline
}
