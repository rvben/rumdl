use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

/// Rule MD046: Code block style
///
/// This rule is triggered when code blocks do not use a consistent style (either fenced or indented).
pub struct MD046CodeBlockStyle {
    style: CodeBlockStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CodeBlockStyle {
    Consistent,
    Fenced,
    Indented,
}

impl MD046CodeBlockStyle {
    pub fn new(style: CodeBlockStyle) -> Self {
        Self { style }
    }

    fn is_fenced_code_block_start(&self, line: &str) -> bool {
        line.trim_start().starts_with("```")
    }

    fn is_indented_code_block(&self, line: &str) -> bool {
        line.starts_with("    ") && !line.trim_start().starts_with("```")
    }

    fn detect_style(&self, content: &str) -> Option<CodeBlockStyle> {
        for line in content.lines() {
            if self.is_fenced_code_block_start(line) {
                return Some(CodeBlockStyle::Fenced);
            }
            if self.is_indented_code_block(line) {
                return Some(CodeBlockStyle::Indented);
            }
        }
        None
    }
}

impl Rule for MD046CodeBlockStyle {
    fn name(&self) -> &'static str {
        "MD046"
    }

    fn description(&self) -> &'static str {
        "Code blocks should use a consistent style"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut in_fenced_block = false;
        let target_style = match self.style {
            CodeBlockStyle::Consistent => self.detect_style(content).unwrap_or(CodeBlockStyle::Fenced),
            _ => self.style.clone(),
        };

        for (line_num, line) in content.lines().enumerate() {
            if self.is_fenced_code_block_start(line) {
                in_fenced_block = !in_fenced_block;
                if target_style == CodeBlockStyle::Indented {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: "Code block style should be indented".to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: "    ".to_string() + line.trim_start(),
                        }),
                    });
                }
            } else if self.is_indented_code_block(line) && !in_fenced_block {
                if target_style == CodeBlockStyle::Fenced {
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: 1,
                        message: "Code block style should be fenced".to_string(),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: "```\n".to_string() + line.trim_start(),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let target_style = match self.style {
            CodeBlockStyle::Consistent => self.detect_style(content).unwrap_or(CodeBlockStyle::Fenced),
            _ => self.style.clone(),
        };

        let mut result = String::new();
        let mut in_fenced_block = false;
        let mut in_indented_block = false;
        let mut buffer = Vec::new();

        for line in content.lines() {
            if self.is_fenced_code_block_start(line) {
                if target_style == CodeBlockStyle::Indented {
                    // Convert fenced block to indented
                    in_fenced_block = !in_fenced_block;
                    if !in_fenced_block && !buffer.is_empty() {
                        for block_line in buffer.drain(..) {
                            result.push_str("    ");
                            result.push_str(block_line);
                            result.push('\n');
                        }
                    }
                } else {
                    // Keep fenced block as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else if self.is_indented_code_block(line) && !in_fenced_block {
                if target_style == CodeBlockStyle::Fenced {
                    // Convert indented block to fenced
                    if !in_indented_block {
                        result.push_str("```\n");
                        in_indented_block = true;
                    }
                    result.push_str(line.trim_start());
                    result.push('\n');
                } else {
                    // Keep indented block as is
                    result.push_str(line);
                    result.push('\n');
                }
            } else {
                if in_indented_block && target_style == CodeBlockStyle::Fenced {
                    result.push_str("```\n");
                    in_indented_block = false;
                }
                if in_fenced_block {
                    buffer.push(line);
                } else {
                    result.push_str(line);
                    result.push('\n');
                }
            }
        }

        // Close any remaining blocks
        if in_indented_block && target_style == CodeBlockStyle::Fenced {
            result.push_str("```\n");
        }

        Ok(result)
    }
} 