use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref COMMAND_PATTERN: Regex = Regex::new(r"^\s*[$>]\s+\S+").unwrap();
    static ref SHELL_LANG_PATTERN: Regex =
        Regex::new(r"^(?i)(bash|sh|shell|console|terminal)").unwrap();
}

#[derive(Debug)]
pub struct MD014CommandsShowOutput {
    pub show_output: bool,
}

impl Default for MD014CommandsShowOutput {
    fn default() -> Self {
        Self { show_output: true }
    }
}

impl MD014CommandsShowOutput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_show_output(show_output: bool) -> Self {
        Self { show_output }
    }

    fn is_command_line(&self, line: &str) -> bool {
        COMMAND_PATTERN.is_match(line)
    }

    fn is_shell_language(&self, lang: &str) -> bool {
        SHELL_LANG_PATTERN.is_match(lang)
    }

    fn is_output_line(&self, line: &str) -> bool {
        let trimmed = line.trim();
        !trimmed.is_empty()
            && !trimmed.starts_with('$')
            && !trimmed.starts_with('>')
            && !trimmed.starts_with('#')
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
        if !self.show_output || !self.is_shell_language(lang) {
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

    fn fix_command_block(&self, block: &[&str]) -> String {
        block
            .iter()
            .map(|line| {
                let trimmed = line.trim_start();
                if self.is_command_line(line) {
                    let spaces = line.len() - line.trim_start().len();
                    let cmd = trimmed
                        .chars()
                        .skip(1)
                        .collect::<String>()
                        .trim_start()
                        .to_string();
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
}

impl Rule for MD014CommandsShowOutput {
    fn name(&self) -> &'static str {
        "MD014"
    }

    fn description(&self) -> &'static str {
        "Commands in code blocks should show output"
    }

    fn check(&self, content: &str) -> LintResult {
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
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: block_start_line + 1,
                            column: 1,
                            message: "Commands in code blocks should show output".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index.line_col_to_byte_range(block_start_line + 1, 1),
                                replacement: self.fix_command_block(&current_block),
                            }),
                        });
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

    fn fix(&self, content: &str) -> Result<String, LintError> {
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
                result.push_str(line);
                result.push('\n');
                in_code_block = !in_code_block;
            } else if in_code_block {
                current_block.push(line);
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
