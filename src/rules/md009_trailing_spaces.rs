use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::LineIndex;
use crate::utils::regex_cache::get_cached_regex;
use lazy_static::lazy_static;
use regex::Regex;
use toml;


lazy_static! {
    // Use cached regex patterns for better performance
    static ref FENCED_CODE_REGEX: std::sync::Arc<Regex> = get_cached_regex(r"^(\s*)```").unwrap();
    static ref ALTERNATE_FENCED_CODE_REGEX: std::sync::Arc<Regex> = get_cached_regex(r"^(\s*)~~~").unwrap();
    static ref BLOCKQUOTE_REGEX: std::sync::Arc<Regex> = get_cached_regex(r"^\s*>\s*$").unwrap();
}

#[derive(Debug, Clone)]
pub struct MD009TrailingSpaces {
    pub br_spaces: usize,
    pub strict: bool,
}

impl Default for MD009TrailingSpaces {
    fn default() -> Self {
        Self {
            br_spaces: 2,
            strict: false,
        }
    }
}

impl MD009TrailingSpaces {
    pub fn new(br_spaces: usize, strict: bool) -> Self {
        Self { br_spaces, strict }
    }

    fn is_in_code_block(lines: &[&str], current_line: usize) -> bool {
        let mut fence_count = 0;
        for (i, line) in lines.iter().take(current_line + 1).enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                fence_count += 1;
            }
            if i == current_line && fence_count % 2 == 1 {
                return true;
            }
        }
        false
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
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: line_num + 1,
                        column: 1,
                        message: "Empty line should not have trailing spaces".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(line_num + 1, 1),
                            replacement: String::new(),
                        }),
                    });
                }
                continue;
            }

            // Handle code blocks if not in strict mode
            if !self.strict && Self::is_in_code_block(&lines, line_num) {
                continue;
            }

            // Check if it's a valid line break
            if !self.strict && trailing_spaces == self.br_spaces {
                continue;
            }

            // Special handling for empty blockquote lines
            if Self::is_empty_blockquote_line(line) {
                let trimmed = line.trim_end();
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: line_num + 1,
                    column: trimmed.len() + 1,
                    message: "Empty blockquote line should have a space after >".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: _line_index.line_col_to_byte_range(line_num + 1, trimmed.len() + 1),
                        replacement: format!("{} ", trimmed),
                    }),
                });
                continue;
            }

            let trimmed = line.trim_end();
            warnings.push(LintWarning {
                rule_name: Some(self.name()),
                line: line_num + 1,
                column: trimmed.len() + 1,
                message: if trailing_spaces == 1 {
                    "Trailing space found".to_string()
                } else {
                    format!("{} trailing spaces found", trailing_spaces)
                },
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: _line_index.line_col_to_byte_range(line_num + 1, trimmed.len() + 1),
                    replacement: if !self.strict && line_num < lines.len() - 1 {
                        format!("{}{}", trimmed, " ".repeat(self.br_spaces))
                    } else {
                        trimmed.to_string()
                    },
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut result = String::new();

        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_end();

            // Handle empty lines
            if trimmed.is_empty() {
                result.push('\n');
                continue;
            }

            // Handle code blocks if not in strict mode
            if !self.strict && Self::is_in_code_block(&lines, i) {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            // Special handling for empty blockquote lines
            if Self::is_empty_blockquote_line(line) {
                result.push_str(trimmed);
                result.push(' '); // Add a space after the blockquote marker
                result.push('\n');
                continue;
            }

            // Handle lines with trailing spaces
            if !self.strict && i < lines.len() - 1 && Self::count_trailing_spaces(line) >= 1 {
                // This is a line break (intentional trailing spaces)
                result.push_str(trimmed);
                result.push_str(&" ".repeat(self.br_spaces));
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
        let mut map = toml::map::Map::new();
        map.insert(
            "br_spaces".to_string(),
            toml::Value::Integer(self.br_spaces as i64),
        );
        map.insert("strict".to_string(), toml::Value::Boolean(self.strict));

        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        // get_rule_config_value now automatically tries both underscore and kebab-case variants
        let br_spaces = crate::config::get_rule_config_value::<u32>(config, "MD009", "br_spaces")
            .unwrap_or(2);

        let strict = crate::config::get_rule_config_value::<bool>(config, "MD009", "strict")
            .unwrap_or(false);

        let br_spaces_usize = br_spaces as usize;
        Box::new(MD009TrailingSpaces::new(br_spaces_usize, strict))
    }
}
