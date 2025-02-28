use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;

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
        
        HR_DASH.is_match(line) || 
        HR_ASTERISK.is_match(line) || 
        HR_UNDERSCORE.is_match(line) || 
        HR_SPACED_DASH.is_match(line) || 
        HR_SPACED_ASTERISK.is_match(line) || 
        HR_SPACED_UNDERSCORE.is_match(line)
    }
    
    /// Gets the indentation of a line as a string
    fn get_indentation(line: &str) -> String {
        let indent_length = line.len() - line.trim_start().len();
        " ".repeat(indent_length)
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
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Use the configured style or find the first HR style
        let expected_style = if self.style.is_empty() {
            // Find the first HR in the document
            let mut first_style = "---".to_string(); // Default if none found
            for line in &lines {
                if Self::is_horizontal_rule(line) {
                    first_style = line.trim().to_string();
                    break;
                }
            }
            first_style
        } else {
            self.style.clone()
        };
        
        for (i, line) in lines.iter().enumerate() {
            if Self::is_horizontal_rule(line) {
                // Check if this HR matches the expected style
                let has_indentation = line.len() > line.trim_start().len();
                let style_mismatch = line.trim() != expected_style;
                
                if style_mismatch || has_indentation {
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: if has_indentation {
                            "Horizontal rule should not be indented".to_string()
                        } else {
                            format!("Horizontal rule style should be \"{}\"", expected_style)
                        },
                        fix: Some(Fix {
                            line: i + 1,
                            column: 1,
                            replacement: expected_style.clone(),
                        }),
                    });
                }
            }
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Use the configured style or find the first HR style
        let expected_style = if self.style.is_empty() {
            // Find the first HR in the document
            let mut first_style = "---".to_string(); // Default if none found
            for line in &lines {
                if Self::is_horizontal_rule(line) {
                    first_style = line.trim().to_string();
                    break;
                }
            }
            first_style
        } else {
            self.style.clone()
        };
        
        for line in lines {
            if Self::is_horizontal_rule(line) {
                // Replace with the correct style and remove indentation
                result.push(expected_style.clone());
            } else {
                result.push(line.to_string());
            }
        }
        
        Ok(result.join("\n"))
    }
} 