use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use crate::rules::heading_utils::HeadingUtils;
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD023HeadingStartLeft;

impl Rule for MD023HeadingStartLeft {
    fn name(&self) -> &'static str {
        "MD023"
    }

    fn description(&self) -> &'static str {
        "Headings must start at the beginning of the line"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let atx_re = Regex::new(r"^(\s+)(#{1,6}(?:\s+.+?|\s*))(?:\s+(#+))?\s*$").unwrap();
        let setext_underline_re = Regex::new(r"^(=+|-+)\s*$").unwrap();

        let lines: Vec<&str> = content.lines().collect();
        for i in 0..lines.len() {
            let line = lines[i];
            
            // Check for indented ATX headings
            if let Some(caps) = atx_re.captures(line) {
                let indentation = caps[1].len();
                let heading_content = &caps[2];
                let closing_sequence = caps.get(3).map_or("", |m| m.as_str());
                
                let replacement = if !closing_sequence.is_empty() {
                    format!("{} {}", heading_content, closing_sequence)
                } else {
                    heading_content.to_string()
                };
                
                warnings.push(LintWarning {
                    line: i + 1,
                    column: 1,
                    message: format!("Heading should not be indented by {} spaces", indentation),
                    fix: Some(Fix {
                        line: i + 1,
                        column: 1,
                        replacement,
                    }),
                });
            }
            
            // Check for indented Setext headings
            if i > 0 && setext_underline_re.is_match(line) {
                let prev_line = lines[i - 1];
                let indentation = HeadingUtils::get_indentation(prev_line);
                if indentation > 0 {
                    warnings.push(LintWarning {
                        line: i,
                        column: 1,
                        message: format!("Setext heading should not be indented by {} spaces", indentation),
                        fix: Some(Fix {
                            line: i,
                            column: 1,
                            replacement: prev_line.trim_start().to_string(),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = Vec::new();
        let atx_re = Regex::new(r"^(\s+)(#{1,6}(?:\s+.+?|\s*))(?:\s+(#+))?\s*$").unwrap();
        let setext_underline_re = Regex::new(r"^(=+|-+)\s*$").unwrap();

        let lines: Vec<&str> = content.lines().collect();
        for i in 0..lines.len() {
            let line = lines[i];
            
            // Handle indented ATX headings
            if let Some(caps) = atx_re.captures(line) {
                let heading_content = &caps[2];
                let closing_sequence = caps.get(3).map_or("", |m| m.as_str());
                
                if !closing_sequence.is_empty() {
                    result.push(format!("{} {}", heading_content, closing_sequence));
                } else {
                    result.push(heading_content.to_string());
                }
            } 
            // Handle indented Setext headings
            else if i > 0 && setext_underline_re.is_match(line) {
                // Underline line - add it as is
                result.push(line.to_string());
                
                // Check if we need to fix the previous line (the heading text)
                let prev_line = lines[i - 1];
                let indentation = HeadingUtils::get_indentation(prev_line);
                if indentation > 0 {
                    // We already added the previous line, so replace it
                    result[i - 1] = prev_line.trim_start().to_string();
                }
            } else {
                // Regular line
                result.push(line.to_string());
            }
        }

        Ok(result.join("\n"))
    }
}