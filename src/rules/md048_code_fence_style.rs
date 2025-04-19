use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::code_fence_utils::CodeFenceStyle;
use crate::utils::range_utils::LineIndex;

/// Rule MD048: Code fence style should be consistent
pub struct MD048CodeFenceStyle {
    style: CodeFenceStyle,
}

impl MD048CodeFenceStyle {
    pub fn new(style: CodeFenceStyle) -> Self {
        Self { style }
    }

    fn detect_style(&self, content: &str) -> Option<CodeFenceStyle> {
        for line in content.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") {
                return Some(CodeFenceStyle::Backtick);
            } else if trimmed.starts_with("~~~") {
                return Some(CodeFenceStyle::Tilde);
            }
        }
        None
    }
}

impl Rule for MD048CodeFenceStyle {
    fn name(&self) -> &'static str {
        "MD048"
    }

    fn description(&self) -> &'static str {
        "Code fence style should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let target_style = match self.style {
            CodeFenceStyle::Consistent => self
                .detect_style(content)
                .unwrap_or(CodeFenceStyle::Backtick),
            _ => self.style,
        };

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") && target_style == CodeFenceStyle::Tilde {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Code fence style should use tildes".to_string(),
                    line: line_num + 1,
                    column: line.len() - trimmed.len() + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index
                            .line_col_to_byte_range(line_num + 1, line.len() - trimmed.len() + 1),
                        replacement: line.replace("```", "~~~"),
                    }),
                });
            } else if trimmed.starts_with("~~~") && target_style == CodeFenceStyle::Backtick {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Code fence style should use backticks".to_string(),
                    line: line_num + 1,
                    column: line.len() - trimmed.len() + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index
                            .line_col_to_byte_range(line_num + 1, line.len() - trimmed.len() + 1),
                        replacement: line.replace("~~~", "```"),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let target_style = match self.style {
            CodeFenceStyle::Consistent => self
                .detect_style(content)
                .unwrap_or(CodeFenceStyle::Backtick),
            _ => self.style,
        };

        let mut result = String::new();
        for line in content.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") && target_style == CodeFenceStyle::Tilde {
                result.push_str(&line.replace("```", "~~~"));
            } else if trimmed.starts_with("~~~") && target_style == CodeFenceStyle::Backtick {
                result.push_str(&line.replace("~~~", "```"));
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
