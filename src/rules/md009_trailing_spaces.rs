use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::{calculate_trailing_range, LineIndex};
use crate::utils::regex_cache::get_cached_regex;
use crate::rule_config_serde::RuleConfig;
use lazy_static::lazy_static;
use regex::Regex;

mod md009_config;
use md009_config::MD009Config;

lazy_static! {
    // Optimized regex patterns for fix operations
    static ref TRAILING_SPACES_REGEX: std::sync::Arc<Regex> = get_cached_regex(r"(?m) +$").unwrap();
}

#[derive(Debug, Clone)]
pub struct MD009TrailingSpaces {
    config: MD009Config,
}

impl Default for MD009TrailingSpaces {
    fn default() -> Self {
        Self {
            config: MD009Config::default(),
        }
    }
}

impl MD009TrailingSpaces {
    pub fn new(br_spaces: usize, strict: bool) -> Self {
        Self {
            config: MD009Config { br_spaces, strict },
        }
    }
    
    pub fn from_config_struct(config: MD009Config) -> Self {
        Self { config }
    }


    fn is_empty_blockquote_line(line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed.starts_with('>') && trimmed.trim_end() == ">"
    }

    fn count_trailing_spaces(line: &str) -> usize {
        let mut count = 0;
        for c in line.chars().rev() {
            if c == ' ' {
                count += 1;
            } else {
                break;
            }
        }
        count
    }
}

impl Rule for MD009TrailingSpaces {
    fn name(&self) -> &'static str {
        "MD009"
    }

    fn description(&self) -> &'static str {
        "Trailing spaces should be removed"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        for (line_num, &line) in lines.iter().enumerate() {
            let trailing_spaces = Self::count_trailing_spaces(line);

            // Skip if no trailing spaces
            if trailing_spaces == 0 {
                continue;
            }

            // Handle empty lines
            if line.trim().is_empty() {
                if trailing_spaces > 0 {
                    // Calculate precise character range for all trailing spaces on empty line
                    let (start_line, start_col, end_line, end_col) =
                        calculate_trailing_range(line_num + 1, line, 0);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: "Empty line has trailing spaces".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range_with_length(line_num + 1, 1, line.len()),
                            replacement: String::new(),
                        }),
                    });
                }
                continue;
            }

            // Handle code blocks if not in strict mode
            if !self.config.strict {
                // Use pre-computed line info
                if let Some(line_info) = ctx.line_info(line_num + 1) {
                    if line_info.in_code_block {
                        continue;
                    }
                }
            }

            // Check if it's a valid line break
            // Special handling: if the content ends with a newline, the last line from .lines() 
            // is not really the "last line" in terms of trailing spaces rules
            let is_truly_last_line = line_num == lines.len() - 1 && !content.ends_with('\n');
            if !self.config.strict && !is_truly_last_line && trailing_spaces == self.config.br_spaces {
                continue;
            }

            // Special handling for empty blockquote lines
            if Self::is_empty_blockquote_line(line) {
                let trimmed = line.trim_end();
                // Calculate precise character range for trailing spaces after blockquote marker
                let (start_line, start_col, end_line, end_col) =
                    calculate_trailing_range(line_num + 1, line, trimmed.len());

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: "Empty blockquote line needs a space after >".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range_with_length(line_num + 1, trimmed.len() + 1, line.len() - trimmed.len()),
                        replacement: " ".to_string(),
                    }),
                });
                continue;
            }

            let trimmed = line.trim_end();
            // Calculate precise character range for all trailing spaces
            let (start_line, start_col, end_line, end_col) =
                calculate_trailing_range(line_num + 1, line, trimmed.len());

            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: if trailing_spaces == 1 {
                    "Trailing space found".to_string()
                } else {
                    format!("{} trailing spaces found", trailing_spaces)
                },
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: _line_index.line_col_to_byte_range_with_length(line_num + 1, trimmed.len() + 1, trailing_spaces),
                    replacement: if !self.config.strict && !is_truly_last_line && trailing_spaces >= 1 {
                        " ".repeat(self.config.br_spaces)
                    } else {
                        String::new()
                    },
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // For simple cases (strict mode), use fast regex approach
        if self.config.strict {
            // In strict mode, remove ALL trailing spaces everywhere
            return Ok(TRAILING_SPACES_REGEX.replace_all(content, "").to_string());
        }

        // For complex cases, we need line-by-line processing but with optimizations
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::with_capacity(content.len()); // Pre-allocate capacity

        for (i, line) in lines.iter().enumerate() {
            // Fast path: if no trailing spaces, just add the line
            if !line.ends_with(' ') {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            let trimmed = line.trim_end();

            // Handle empty lines - fast regex replacement
            if trimmed.is_empty() {
                result.push('\n');
                continue;
            }

            // Handle code blocks if not in strict mode
            if let Some(line_info) = ctx.line_info(i + 1) {
                if line_info.in_code_block {
                    result.push_str(line);
                    result.push('\n');
                    continue;
                }
            }

            // Special handling for empty blockquote lines
            if Self::is_empty_blockquote_line(line) {
                result.push_str(trimmed);
                result.push(' '); // Add a space after the blockquote marker
                result.push('\n');
                continue;
            }

            // Handle lines with trailing spaces
            let trailing_spaces = Self::count_trailing_spaces(line);
            let is_truly_last_line = i == lines.len() - 1 && !content.ends_with('\n');
            if !self.config.strict && !is_truly_last_line && trailing_spaces >= 1 {
                // This is a line break (intentional trailing spaces)
                result.push_str(trimmed);
                result.push_str(&" ".repeat(self.config.br_spaces));
            } else {
                // Normal line, just use trimmed content
                result.push_str(trimmed);
            }
            result.push('\n');
        }

        // Preserve original ending (with or without final newline)
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD009Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;
        
        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD009Config::RULE_NAME.to_string(), toml::Value::Table(table)))
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
        let rule_config = crate::rule_config_serde::load_rule_config::<MD009Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}
