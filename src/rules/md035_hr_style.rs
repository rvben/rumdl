use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref HR_DASH: Regex = Regex::new(r"^\-{3,}\s*$").unwrap();
    static ref HR_ASTERISK: Regex = Regex::new(r"^\*{3,}\s*$").unwrap();
    static ref HR_UNDERSCORE: Regex = Regex::new(r"^_{3,}\s*$").unwrap();
    static ref HR_SPACED_DASH: Regex = Regex::new(r"^(\-\s+){2,}\-\s*$").unwrap();
    static ref HR_SPACED_ASTERISK: Regex = Regex::new(r"^(\*\s+){2,}\*\s*$").unwrap();
    static ref HR_SPACED_UNDERSCORE: Regex = Regex::new(r"^(_\s+){2,}_\s*$").unwrap();
}

#[derive(Debug)]
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

        // Check if the current line is all dashes or equals signs

        let is_dash_line = !line.is_empty() && line.chars().all(|c| c == '-');

        let is_equals_line = !line.is_empty() && line.chars().all(|c| c == '=');

        // Check if the previous line is not empty and not a horizontal rule

        let prev_line_has_content = !prev_line.is_empty() && !Self::is_horizontal_rule(prev_line);

        // If the current line is all dashes or equals signs and the previous line has content,
        // it's likely a Setext heading
        (is_dash_line || is_equals_line) && prev_line_has_content
    }
}

impl Rule for MD035HRStyle {
    fn name(&self) -> &'static str {
        "MD035"
    }

    fn description(&self) -> &'static str {
        "Horizontal rule style"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        // Use the configured style or find the first HR style

        let expected_style = if self.style.is_empty() {
            // Find the first HR in the document
            let mut first_style = "---".to_string(); // Default if none found
            for (i, line) in lines.iter().enumerate() {
                if Self::is_horizontal_rule(line) && !Self::is_potential_setext_heading(&lines, i) {
                    first_style = line.trim().to_string();
                    break;
                }
            }
            first_style
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
                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        line: i + 1,
                        column: 1,
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

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let mut result = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        // Use the configured style or find the first HR style

        let expected_style = if self.style.is_empty() {
            // Find the first HR in the document
            let mut first_style = "---".to_string(); // Default if none found
            for (i, line) in lines.iter().enumerate() {
                if Self::is_horizontal_rule(line) && !Self::is_potential_setext_heading(&lines, i) {
                    first_style = line.trim().to_string();
                    break;
                }
            }
            first_style
        } else {
            self.style.clone()
        };

        for (i, line) in lines.iter().enumerate() {
            // Skip if this is a potential Setext heading underline
            if Self::is_potential_setext_heading(&lines, i) {
                result.push(line.to_string());
                continue;
            }

            let _trimmed = line.trim();

            // Simplify the horizontal rule detection and replacement
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
}
