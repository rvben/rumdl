//!
//! Rule MD035: Horizontal rule style
//!
//! See [docs/md035.md](../../docs/md035.md) for full documentation, configuration, and examples.

use crate::utils::range_utils::{calculate_line_range, LineIndex};

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;
use toml;

lazy_static! {
    static ref HR_DASH: Regex = Regex::new(r"^\-{3,}\s*$").unwrap();
    static ref HR_ASTERISK: Regex = Regex::new(r"^\*{3,}\s*$").unwrap();
    static ref HR_UNDERSCORE: Regex = Regex::new(r"^_{3,}\s*$").unwrap();
    static ref HR_SPACED_DASH: Regex = Regex::new(r"^(\-\s+){2,}\-\s*$").unwrap();
    static ref HR_SPACED_ASTERISK: Regex = Regex::new(r"^(\*\s+){2,}\*\s*$").unwrap();
    static ref HR_SPACED_UNDERSCORE: Regex = Regex::new(r"^(_\s+){2,}_\s*$").unwrap();
}

/// Represents the style for horizontal rules
#[derive(Clone)]
pub struct MD035HRStyle {
    style: String,
}

impl Default for MD035HRStyle {
    fn default() -> Self {
        Self {
            style: "---".to_string(),
        }
    }
}

impl MD035HRStyle {
    pub fn new(style: String) -> Self {
        Self { style }
    }

    /// Determines if a line is a horizontal rule
    fn is_horizontal_rule(line: &str) -> bool {
        let line = line.trim();

        HR_DASH.is_match(line)
            || HR_ASTERISK.is_match(line)
            || HR_UNDERSCORE.is_match(line)
            || HR_SPACED_DASH.is_match(line)
            || HR_SPACED_ASTERISK.is_match(line)
            || HR_SPACED_UNDERSCORE.is_match(line)
    }

    /// Check if a line might be a Setext heading underline
    fn is_potential_setext_heading(lines: &[&str], i: usize) -> bool {
        if i == 0 {
            return false; // First line can't be a Setext heading underline
        }

        let line = lines[i].trim();
        let prev_line = lines[i - 1].trim();

        let is_dash_line = !line.is_empty() && line.chars().all(|c| c == '-');
        let is_equals_line = !line.is_empty() && line.chars().all(|c| c == '=');
        let prev_line_has_content = !prev_line.is_empty() && !Self::is_horizontal_rule(prev_line);
        (is_dash_line || is_equals_line) && prev_line_has_content
    }

    /// Find the most prevalent HR style in the document (excluding setext headings)
    fn most_prevalent_hr_style(lines: &[&str]) -> Option<String> {
        use std::collections::HashMap;
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let mut order: Vec<&str> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if Self::is_horizontal_rule(line) && !Self::is_potential_setext_heading(lines, i) {
                let style = line.trim();
                let counter = counts.entry(style).or_insert(0);
                *counter += 1;
                if *counter == 1 {
                    order.push(style);
                }
            }
        }
        // Find the style with the highest count, breaking ties by first encountered
        counts
            .iter()
            .max_by_key(|&(style, count)| {
                (
                    *count,
                    -(order
                        .iter()
                        .position(|&s| s == *style)
                        .unwrap_or(usize::MAX) as isize),
                )
            })
            .map(|(style, _)| style.to_string())
    }
}

impl Rule for MD035HRStyle {
    fn name(&self) -> &'static str {
        "MD035"
    }

    fn description(&self) -> &'static str {
        "Horizontal rule style"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Use the configured style or find the most prevalent HR style
        let expected_style = if self.style.is_empty() || self.style == "consistent" {
            Self::most_prevalent_hr_style(&lines).unwrap_or_else(|| "---".to_string())
        } else {
            self.style.clone()
        };

        for (i, line) in lines.iter().enumerate() {
            // Skip if this is a potential Setext heading underline
            if Self::is_potential_setext_heading(&lines, i) {
                continue;
            }

            if Self::is_horizontal_rule(line) {
                // Check if this HR matches the expected style
                let has_indentation = line.len() > line.trim_start().len();
                let style_mismatch = line.trim() != expected_style;

                if style_mismatch || has_indentation {
                    // Calculate precise character range for the entire horizontal rule
                    let (start_line, start_col, end_line, end_col) =
                        calculate_line_range(i + 1, line);

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line,
                        column: start_col,
                        end_line,
                        end_column: end_col,
                        message: if has_indentation {
                            "Horizontal rule should not be indented".to_string()
                        } else {
                            format!("Horizontal rule style should be \"{}\"", expected_style)
                        },
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: expected_style.clone(),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let _line_index = LineIndex::new(content.to_string());

        let mut result = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Use the configured style or find the most prevalent HR style
        let expected_style = if self.style.is_empty() || self.style == "consistent" {
            Self::most_prevalent_hr_style(&lines).unwrap_or_else(|| "---".to_string())
        } else {
            self.style.clone()
        };

        for (i, line) in lines.iter().enumerate() {
            // Skip if this is a potential Setext heading underline
            if Self::is_potential_setext_heading(&lines, i) {
                result.push(line.to_string());
                continue;
            }

            if Self::is_horizontal_rule(line) {
                // Here we have a proper horizontal rule - replace it with the expected style
                result.push(expected_style.clone());
            } else {
                // Not a horizontal rule, keep the original line
                result.push(line.to_string());
            }
        }

        Ok(result.join("\n"))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert("style".to_string(), toml::Value::String(self.style.clone()));
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD035", "style")
            .unwrap_or_else(|| "consistent".to_string());
        Box::new(MD035HRStyle::new(style))
    }
}
