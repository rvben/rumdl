use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::code_fence_utils::CodeFenceStyle;
use crate::utils::range_utils::{calculate_match_range, LineIndex};
use toml;

/// Rule MD048: Code fence style
///
/// See [docs/md048.md](../../docs/md048.md) for full documentation, configuration, and examples.
#[derive(Clone)]
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

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
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
                // Find the position and length of the backtick fence
                let fence_start = line.len() - trimmed.len();
                let fence_end =
                    fence_start + trimmed.find(|c: char| c != '`').unwrap_or(trimmed.len());

                // Calculate precise character range for the entire fence
                let (start_line, start_col, end_line, end_col) =
                    calculate_match_range(line_num + 1, line, fence_start, fence_end - fence_start);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Code fence style: use tildes instead of backticks".to_string(),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(line_num + 1, fence_start + 1),
                        replacement: line.replace("```", "~~~"),
                    }),
                });
            } else if trimmed.starts_with("~~~") && target_style == CodeFenceStyle::Backtick {
                // Find the position and length of the tilde fence
                let fence_start = line.len() - trimmed.len();
                let fence_end =
                    fence_start + trimmed.find(|c: char| c != '~').unwrap_or(trimmed.len());

                // Calculate precise character range for the entire fence
                let (start_line, start_col, end_line, end_col) =
                    calculate_match_range(line_num + 1, line, fence_start, fence_end - fence_start);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: "Code fence style: use backticks instead of tildes".to_string(),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(line_num + 1, fence_start + 1),
                        replacement: line.replace("~~~", "```"),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
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

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert(
            "style".to_string(),
            toml::Value::String(self.style.to_string()),
        );
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD048", "style")
            .unwrap_or_else(|| "consistent".to_string());
        let style = match style.as_str() {
            "backtick" => CodeFenceStyle::Backtick,
            "tilde" => CodeFenceStyle::Tilde,
            "consistent" => CodeFenceStyle::Consistent,
            _ => CodeFenceStyle::Consistent,
        };
        Box::new(MD048CodeFenceStyle::new(style))
    }
}
