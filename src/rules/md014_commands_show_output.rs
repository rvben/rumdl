//!
//! Rule MD014: Commands should show output
//!
//! See [docs/md014.md](../../docs/md014.md) for full documentation, configuration, and examples.

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::range_utils::{LineIndex, calculate_match_range};
use lazy_static::lazy_static;
use regex::Regex;
use toml;

mod md014_config;
use md014_config::MD014Config;

lazy_static! {
    static ref COMMAND_PATTERN: Regex = Regex::new(r"^\s*[$>]\s+\S+").unwrap();
    static ref SHELL_LANG_PATTERN: Regex = Regex::new(r"^(?i)(bash|sh|shell|console|terminal)").unwrap();
    static ref DOLLAR_PROMPT_PATTERN: Regex = Regex::new(r"^\s*([$>])").unwrap();
}

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
        COMMAND_PATTERN.is_match(line)
    }

    fn is_shell_language(&self, lang: &str) -> bool {
        SHELL_LANG_PATTERN.is_match(lang)
    }

    fn is_output_line(&self, line: &str) -> bool {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('$') && !trimmed.starts_with('>') && !trimmed.starts_with('#')
    }

    fn is_no_output_command(&self, cmd: &str) -> bool {
        let cmd = cmd.trim().to_lowercase();
        cmd.contains("cd ")
            || cmd.contains("mkdir ")
            || cmd.contains("touch ")
            || cmd.contains("rm ")
            || cmd.contains("mv ")
            || cmd.contains("cp ")
            || cmd.contains("export ")
            || cmd.contains("set ")
    }

    fn is_command_without_output(&self, block: &[&str], lang: &str) -> bool {
        if !self.config.show_output || !self.is_shell_language(lang) {
            return false;
        }

        let mut has_command = false;
        let mut has_output = false;
        let mut last_command = String::new();

        for line in block {
            let trimmed = line.trim();
            if self.is_command_line(line) {
                has_command = true;
                last_command = trimmed[1..].trim().to_string();
            } else if self.is_output_line(line) {
                has_output = true;
            }
        }

        has_command && !has_output && !self.is_no_output_command(&last_command)
    }

    fn get_command_from_block(&self, block: &[&str]) -> String {
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

    fn find_first_command_line<'a>(&self, block: &[&'a str]) -> Option<(usize, &'a str)> {
        for (i, line) in block.iter().enumerate() {
            if self.is_command_line(line) {
                return Some((i, line));
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
        let _line_index = LineIndex::new(content.to_string());

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
                            if let Some(cap) = DOLLAR_PROMPT_PATTERN.captures(cmd_line) {
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
                                    rule_name: Some(self.name()),
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
        let _line_index = LineIndex::new(content.to_string());

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
        ctx.content.is_empty() || !ctx.content.contains("```")
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
        assert!(rule.is_no_output_command("cd /home"));
        assert!(rule.is_no_output_command("mkdir test"));
        assert!(rule.is_no_output_command("touch file.txt"));
        assert!(rule.is_no_output_command("rm -rf dir"));
        assert!(rule.is_no_output_command("mv old new"));
        assert!(rule.is_no_output_command("cp src dst"));
        assert!(rule.is_no_output_command("export VAR=value"));
        assert!(rule.is_no_output_command("set -e"));
        assert!(rule.is_no_output_command("CD /HOME"));
        assert!(rule.is_no_output_command("MKDIR TEST"));
        assert!(!rule.is_no_output_command("ls -la"));
        assert!(!rule.is_no_output_command("echo test"));
        assert!(!rule.is_no_output_command("pwd"));
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
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Bare $ with only space doesn't match command pattern"
        );

        // Test empty code block
        let empty_content = "```bash\n```";
        let ctx2 = LintContext::new(empty_content);
        let result2 = rule.check(&ctx2).unwrap();
        assert!(result2.is_empty(), "Empty code block should not be flagged");

        // Test minimal command
        let minimal = "```bash\n$ a\n```";
        let ctx3 = LintContext::new(minimal);
        let result3 = rule.check(&ctx3).unwrap();
        assert_eq!(result3.len(), 1, "Minimal command should be flagged");
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
