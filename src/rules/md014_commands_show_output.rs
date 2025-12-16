//!
//! Rule MD014: Commands should show output
//!
//! See [docs/md014.md](../../docs/md014.md) for full documentation, configuration, and examples.

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::range_utils::calculate_match_range;
use crate::utils::regex_cache::get_cached_regex;
use toml;

mod md014_config;
use md014_config::MD014Config;

// Command detection patterns
const COMMAND_PATTERN: &str = r"^\s*[$>]\s+\S+";
const SHELL_LANG_PATTERN: &str = r"^(?i)(bash|sh|shell|console|terminal)";
const DOLLAR_PROMPT_PATTERN: &str = r"^\s*([$>])";

#[derive(Clone, Default)]
pub struct MD014CommandsShowOutput {
    config: MD014Config,
}

impl MD014CommandsShowOutput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_show_output(show_output: bool) -> Self {
        Self {
            config: MD014Config { show_output },
        }
    }

    pub fn from_config_struct(config: MD014Config) -> Self {
        Self { config }
    }

    fn is_command_line(&self, line: &str) -> bool {
        get_cached_regex(COMMAND_PATTERN)
            .map(|re| re.is_match(line))
            .unwrap_or(false)
    }

    fn is_shell_language(&self, lang: &str) -> bool {
        get_cached_regex(SHELL_LANG_PATTERN)
            .map(|re| re.is_match(lang))
            .unwrap_or(false)
    }

    fn is_output_line(&self, line: &str) -> bool {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('$') && !trimmed.starts_with('>') && !trimmed.starts_with('#')
    }

    fn is_no_output_command(&self, cmd: &str) -> bool {
        let cmd = cmd.trim().to_lowercase();

        // Only skip commands that produce NO output by design.
        // Commands that produce output (even if verbose) should NOT be skipped -
        // the rule's intent is to encourage showing output when using $ prompts.

        // Shell built-ins and commands that produce no terminal output
        cmd.starts_with("cd ")
            || cmd == "cd"
            || cmd.starts_with("mkdir ")
            || cmd.starts_with("touch ")
            || cmd.starts_with("rm ")
            || cmd.starts_with("mv ")
            || cmd.starts_with("cp ")
            || cmd.starts_with("export ")
            || cmd.starts_with("set ")
            || cmd.starts_with("alias ")
            || cmd.starts_with("unset ")
            || cmd.starts_with("source ")
            || cmd.starts_with(". ")
            || cmd == "true"
            || cmd == "false"
            || cmd.starts_with("sleep ")
            || cmd.starts_with("wait ")
            || cmd.starts_with("pushd ")
            || cmd.starts_with("popd")

            // Shell redirects (output goes to file, not terminal)
            || cmd.contains(" > ")
            || cmd.contains(" >> ")

            // Git commands that produce no output on success
            || cmd.starts_with("git add ")
            || cmd.starts_with("git checkout ")
            || cmd.starts_with("git stash")
            || cmd.starts_with("git reset ")
    }

    fn is_command_without_output(&self, block: &[&str], lang: &str) -> bool {
        if !self.config.show_output || !self.is_shell_language(lang) {
            return false;
        }

        // Check if block has any output
        let has_output = block.iter().any(|line| self.is_output_line(line));
        if has_output {
            return false; // Has output, don't flag
        }

        // Flag if there's at least one command that should produce output
        self.get_first_output_command(block).is_some()
    }

    /// Returns the first command in the block that should produce output.
    /// Skips no-output commands like cd, mkdir, etc.
    fn get_first_output_command(&self, block: &[&str]) -> Option<(usize, String)> {
        for (i, line) in block.iter().enumerate() {
            if self.is_command_line(line) {
                let cmd = line.trim()[1..].trim().to_string();
                if !self.is_no_output_command(&cmd) {
                    return Some((i, cmd));
                }
            }
        }
        None // All commands are no-output commands
    }

    fn get_command_from_block(&self, block: &[&str]) -> String {
        // Return the first command that should produce output
        if let Some((_, cmd)) = self.get_first_output_command(block) {
            return cmd;
        }
        // Fallback to first command (for backwards compatibility)
        for line in block {
            let trimmed = line.trim();
            if self.is_command_line(line) {
                return trimmed[1..].trim().to_string();
            }
        }
        String::new()
    }

    fn fix_command_block(&self, block: &[&str]) -> String {
        block
            .iter()
            .map(|line| {
                let trimmed = line.trim_start();
                if self.is_command_line(line) {
                    let spaces = line.len() - line.trim_start().len();
                    let cmd = trimmed.chars().skip(1).collect::<String>().trim_start().to_string();
                    format!("{}{}", " ".repeat(spaces), cmd)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn get_code_block_language(block_start: &str) -> String {
        block_start
            .trim_start()
            .trim_start_matches("```")
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    }

    /// Find the first command line that should produce output.
    /// Skips no-output commands (cd, mkdir, etc.) to report the correct position.
    fn find_first_command_line<'a>(&self, block: &[&'a str]) -> Option<(usize, &'a str)> {
        for (i, line) in block.iter().enumerate() {
            if self.is_command_line(line) {
                let cmd = line.trim()[1..].trim();
                if !self.is_no_output_command(cmd) {
                    return Some((i, line));
                }
            }
        }
        None
    }
}

impl Rule for MD014CommandsShowOutput {
    fn name(&self) -> &'static str {
        "MD014"
    }

    fn description(&self) -> &'static str {
        "Commands in code blocks should show output"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = &ctx.line_index;

        let mut warnings = Vec::new();

        let mut current_block = Vec::new();

        let mut in_code_block = false;

        let mut block_start_line = 0;

        let mut current_lang = String::new();

        for (line_num, line) in content.lines().enumerate() {
            if line.trim_start().starts_with("```") {
                if in_code_block {
                    // End of code block
                    if self.is_command_without_output(&current_block, &current_lang) {
                        // Find the first command line to highlight the dollar sign
                        if let Some((cmd_line_idx, cmd_line)) = self.find_first_command_line(&current_block) {
                            let cmd_line_num = block_start_line + 1 + cmd_line_idx + 1; // +1 for fence, +1 for 1-indexed

                            // Find and highlight the dollar sign or prompt
                            if let Ok(re) = get_cached_regex(DOLLAR_PROMPT_PATTERN)
                                && let Some(cap) = re.captures(cmd_line)
                            {
                                let match_obj = cap.get(1).unwrap(); // The $ or > character
                                let (start_line, start_col, end_line, end_col) =
                                    calculate_match_range(cmd_line_num, cmd_line, match_obj.start(), match_obj.len());

                                // Get the command for a more helpful message
                                let command = self.get_command_from_block(&current_block);
                                let message = if command.is_empty() {
                                    "Command should show output (add example output or remove $ prompt)".to_string()
                                } else {
                                    format!(
                                        "Command '{command}' should show output (add example output or remove $ prompt)"
                                    )
                                };

                                warnings.push(LintWarning {
                                    rule_name: Some(self.name().to_string()),
                                    line: start_line,
                                    column: start_col,
                                    end_line,
                                    end_column: end_col,
                                    message,
                                    severity: Severity::Warning,
                                    fix: Some(Fix {
                                        range: {
                                            // Replace the content line(s) between the fences
                                            let content_start_line = block_start_line + 1; // Line after opening fence (0-indexed)
                                            let content_end_line = line_num - 1; // Line before closing fence (0-indexed)

                                            // Calculate byte range for the content lines including their newlines
                                            let start_byte =
                                                _line_index.get_line_start_byte(content_start_line + 1).unwrap_or(0); // +1 for 1-indexed
                                            let end_byte = _line_index
                                                .get_line_start_byte(content_end_line + 2)
                                                .unwrap_or(start_byte); // +2 to include newline after last content line
                                            start_byte..end_byte
                                        },
                                        replacement: format!("{}\n", self.fix_command_block(&current_block)),
                                    }),
                                });
                            }
                        }
                    }
                    current_block.clear();
                } else {
                    // Start of code block
                    block_start_line = line_num;
                    current_lang = Self::get_code_block_language(line);
                }
                in_code_block = !in_code_block;
            } else if in_code_block {
                current_block.push(line);
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = &ctx.line_index;

        let mut result = String::new();

        let mut current_block = Vec::new();

        let mut in_code_block = false;

        let mut current_lang = String::new();

        for line in content.lines() {
            if line.trim_start().starts_with("```") {
                if in_code_block {
                    // End of code block
                    if self.is_command_without_output(&current_block, &current_lang) {
                        result.push_str(&self.fix_command_block(&current_block));
                        result.push('\n');
                    } else {
                        for block_line in &current_block {
                            result.push_str(block_line);
                            result.push('\n');
                        }
                    }
                    current_block.clear();
                } else {
                    current_lang = Self::get_code_block_language(line);
                }
                in_code_block = !in_code_block;
                result.push_str(line);
                result.push('\n');
            } else if in_code_block {
                current_block.push(line);
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        // Remove trailing newline if original didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no code blocks
        ctx.content.is_empty() || !ctx.likely_has_code()
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD014Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD014Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD014Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_is_command_line() {
        let rule = MD014CommandsShowOutput::new();
        assert!(rule.is_command_line("$ echo test"));
        assert!(rule.is_command_line("  $ ls -la"));
        assert!(rule.is_command_line("> pwd"));
        assert!(rule.is_command_line("   > cd /home"));
        assert!(!rule.is_command_line("echo test"));
        assert!(!rule.is_command_line("# comment"));
        assert!(!rule.is_command_line("output line"));
    }

    #[test]
    fn test_is_shell_language() {
        let rule = MD014CommandsShowOutput::new();
        assert!(rule.is_shell_language("bash"));
        assert!(rule.is_shell_language("BASH"));
        assert!(rule.is_shell_language("sh"));
        assert!(rule.is_shell_language("shell"));
        assert!(rule.is_shell_language("Shell"));
        assert!(rule.is_shell_language("console"));
        assert!(rule.is_shell_language("CONSOLE"));
        assert!(rule.is_shell_language("terminal"));
        assert!(rule.is_shell_language("Terminal"));
        assert!(!rule.is_shell_language("python"));
        assert!(!rule.is_shell_language("javascript"));
        assert!(!rule.is_shell_language(""));
    }

    #[test]
    fn test_is_output_line() {
        let rule = MD014CommandsShowOutput::new();
        assert!(rule.is_output_line("output text"));
        assert!(rule.is_output_line("   some output"));
        assert!(rule.is_output_line("file1 file2"));
        assert!(!rule.is_output_line(""));
        assert!(!rule.is_output_line("   "));
        assert!(!rule.is_output_line("$ command"));
        assert!(!rule.is_output_line("> prompt"));
        assert!(!rule.is_output_line("# comment"));
    }

    #[test]
    fn test_is_no_output_command() {
        let rule = MD014CommandsShowOutput::new();

        // Shell built-ins that produce no output
        assert!(rule.is_no_output_command("cd /home"));
        assert!(rule.is_no_output_command("cd"));
        assert!(rule.is_no_output_command("mkdir test"));
        assert!(rule.is_no_output_command("touch file.txt"));
        assert!(rule.is_no_output_command("rm -rf dir"));
        assert!(rule.is_no_output_command("mv old new"));
        assert!(rule.is_no_output_command("cp src dst"));
        assert!(rule.is_no_output_command("export VAR=value"));
        assert!(rule.is_no_output_command("set -e"));
        assert!(rule.is_no_output_command("source ~/.bashrc"));
        assert!(rule.is_no_output_command(". ~/.profile"));
        assert!(rule.is_no_output_command("alias ll='ls -la'"));
        assert!(rule.is_no_output_command("unset VAR"));
        assert!(rule.is_no_output_command("true"));
        assert!(rule.is_no_output_command("false"));
        assert!(rule.is_no_output_command("sleep 5"));
        assert!(rule.is_no_output_command("pushd /tmp"));
        assert!(rule.is_no_output_command("popd"));

        // Case insensitive (lowercased internally)
        assert!(rule.is_no_output_command("CD /HOME"));
        assert!(rule.is_no_output_command("MKDIR TEST"));

        // Shell redirects (output goes to file)
        assert!(rule.is_no_output_command("echo 'test' > file.txt"));
        assert!(rule.is_no_output_command("cat input.txt > output.txt"));
        assert!(rule.is_no_output_command("echo 'append' >> log.txt"));

        // Git commands that produce no output on success
        assert!(rule.is_no_output_command("git add ."));
        assert!(rule.is_no_output_command("git checkout main"));
        assert!(rule.is_no_output_command("git stash"));
        assert!(rule.is_no_output_command("git reset HEAD~1"));

        // Commands that PRODUCE output (should NOT be skipped)
        assert!(!rule.is_no_output_command("ls -la"));
        assert!(!rule.is_no_output_command("echo test")); // echo without redirect
        assert!(!rule.is_no_output_command("pwd"));
        assert!(!rule.is_no_output_command("cat file.txt")); // cat without redirect
        assert!(!rule.is_no_output_command("grep pattern file"));

        // Installation commands PRODUCE output (should NOT be skipped)
        assert!(!rule.is_no_output_command("pip install requests"));
        assert!(!rule.is_no_output_command("npm install express"));
        assert!(!rule.is_no_output_command("cargo install ripgrep"));
        assert!(!rule.is_no_output_command("brew install git"));

        // Build commands PRODUCE output (should NOT be skipped)
        assert!(!rule.is_no_output_command("cargo build"));
        assert!(!rule.is_no_output_command("npm run build"));
        assert!(!rule.is_no_output_command("make"));

        // Docker commands PRODUCE output (should NOT be skipped)
        assert!(!rule.is_no_output_command("docker ps"));
        assert!(!rule.is_no_output_command("docker compose up"));
        assert!(!rule.is_no_output_command("docker run myimage"));

        // Git commands that PRODUCE output (should NOT be skipped)
        assert!(!rule.is_no_output_command("git status"));
        assert!(!rule.is_no_output_command("git log"));
        assert!(!rule.is_no_output_command("git diff"));
    }

    #[test]
    fn test_get_command_from_block() {
        let rule = MD014CommandsShowOutput::new();
        let block = vec!["$ echo test", "output"];
        assert_eq!(rule.get_command_from_block(&block), "echo test");

        let block2 = vec!["  $ ls -la", "file1 file2"];
        assert_eq!(rule.get_command_from_block(&block2), "ls -la");

        let block3 = vec!["> pwd", "/home"];
        assert_eq!(rule.get_command_from_block(&block3), "pwd");

        let empty_block: Vec<&str> = vec![];
        assert_eq!(rule.get_command_from_block(&empty_block), "");
    }

    #[test]
    fn test_fix_command_block() {
        let rule = MD014CommandsShowOutput::new();
        let block = vec!["$ echo test", "$ ls -la"];
        assert_eq!(rule.fix_command_block(&block), "echo test\nls -la");

        let indented = vec!["    $ echo test", "  $ pwd"];
        assert_eq!(rule.fix_command_block(&indented), "    echo test\n  pwd");

        let mixed = vec!["> cd /home", "$ mkdir test"];
        assert_eq!(rule.fix_command_block(&mixed), "cd /home\nmkdir test");
    }

    #[test]
    fn test_get_code_block_language() {
        assert_eq!(MD014CommandsShowOutput::get_code_block_language("```bash"), "bash");
        assert_eq!(MD014CommandsShowOutput::get_code_block_language("```shell"), "shell");
        assert_eq!(
            MD014CommandsShowOutput::get_code_block_language("   ```console"),
            "console"
        );
        assert_eq!(
            MD014CommandsShowOutput::get_code_block_language("```bash {.line-numbers}"),
            "bash"
        );
        assert_eq!(MD014CommandsShowOutput::get_code_block_language("```"), "");
    }

    #[test]
    fn test_find_first_command_line() {
        let rule = MD014CommandsShowOutput::new();
        let block = vec!["# comment", "$ echo test", "output"];
        let result = rule.find_first_command_line(&block);
        assert_eq!(result, Some((1, "$ echo test")));

        let no_commands = vec!["output1", "output2"];
        assert_eq!(rule.find_first_command_line(&no_commands), None);
    }

    #[test]
    fn test_is_command_without_output() {
        let rule = MD014CommandsShowOutput::with_show_output(true);

        // Commands without output should be flagged
        let block1 = vec!["$ echo test"];
        assert!(rule.is_command_without_output(&block1, "bash"));

        // Commands with output should not be flagged
        let block2 = vec!["$ echo test", "test"];
        assert!(!rule.is_command_without_output(&block2, "bash"));

        // No-output commands should not be flagged
        let block3 = vec!["$ cd /home"];
        assert!(!rule.is_command_without_output(&block3, "bash"));

        // Disabled rule should not flag
        let rule_disabled = MD014CommandsShowOutput::with_show_output(false);
        assert!(!rule_disabled.is_command_without_output(&block1, "bash"));

        // Non-shell language should not be flagged
        assert!(!rule.is_command_without_output(&block1, "python"));
    }

    #[test]
    fn test_edge_cases() {
        let rule = MD014CommandsShowOutput::new();
        // Bare $ doesn't match command pattern (needs a command after $)
        let content = "```bash\n$ \n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Bare $ with only space doesn't match command pattern"
        );

        // Test empty code block
        let empty_content = "```bash\n```";
        let ctx2 = LintContext::new(empty_content, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert!(result2.is_empty(), "Empty code block should not be flagged");

        // Test minimal command
        let minimal = "```bash\n$ a\n```";
        let ctx3 = LintContext::new(minimal, crate::config::MarkdownFlavor::Standard, None);
        let result3 = rule.check(&ctx3).unwrap();
        assert_eq!(result3.len(), 1, "Minimal command should be flagged");
    }

    #[test]
    fn test_mixed_silent_and_output_commands() {
        let rule = MD014CommandsShowOutput::new();

        // Block with only silent commands should NOT be flagged
        let silent_only = "```bash\n$ cd /home\n$ mkdir test\n```";
        let ctx1 = LintContext::new(silent_only, crate::config::MarkdownFlavor::Standard, None);
        let result1 = rule.check(&ctx1).unwrap();
        assert!(
            result1.is_empty(),
            "Block with only silent commands should not be flagged"
        );

        // Block with silent commands followed by output-producing command
        // should flag with the OUTPUT-PRODUCING command in the message
        let mixed_silent_first = "```bash\n$ cd /home\n$ ls -la\n```";
        let ctx2 = LintContext::new(mixed_silent_first, crate::config::MarkdownFlavor::Standard, None);
        let result2 = rule.check(&ctx2).unwrap();
        assert_eq!(result2.len(), 1, "Mixed block should be flagged once");
        assert!(
            result2[0].message.contains("ls -la"),
            "Message should mention 'ls -la', not 'cd /home'. Got: {}",
            result2[0].message
        );

        // Block with mkdir followed by cat (which produces output)
        let mixed_mkdir_cat = "```bash\n$ mkdir test\n$ cat file.txt\n```";
        let ctx3 = LintContext::new(mixed_mkdir_cat, crate::config::MarkdownFlavor::Standard, None);
        let result3 = rule.check(&ctx3).unwrap();
        assert_eq!(result3.len(), 1, "Mixed block should be flagged once");
        assert!(
            result3[0].message.contains("cat file.txt"),
            "Message should mention 'cat file.txt', not 'mkdir'. Got: {}",
            result3[0].message
        );

        // Block with silent command followed by pip install (which produces output)
        // pip install is NOT a silent command - it produces verbose output
        let mkdir_pip = "```bash\n$ mkdir test\n$ pip install something\n```";
        let ctx3b = LintContext::new(mkdir_pip, crate::config::MarkdownFlavor::Standard, None);
        let result3b = rule.check(&ctx3b).unwrap();
        assert_eq!(result3b.len(), 1, "Block with pip install should be flagged");
        assert!(
            result3b[0].message.contains("pip install"),
            "Message should mention 'pip install'. Got: {}",
            result3b[0].message
        );

        // Block with output-producing command followed by silent command
        // should still flag with the FIRST output-producing command
        let mixed_output_first = "```bash\n$ echo hello\n$ cd /home\n```";
        let ctx4 = LintContext::new(mixed_output_first, crate::config::MarkdownFlavor::Standard, None);
        let result4 = rule.check(&ctx4).unwrap();
        assert_eq!(result4.len(), 1, "Mixed block should be flagged once");
        assert!(
            result4[0].message.contains("echo hello"),
            "Message should mention 'echo hello'. Got: {}",
            result4[0].message
        );
    }

    #[test]
    fn test_default_config_section() {
        let rule = MD014CommandsShowOutput::new();
        let config_section = rule.default_config_section();
        assert!(config_section.is_some());
        let (name, _value) = config_section.unwrap();
        assert_eq!(name, "MD014");
    }
}
