use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::rules::heading_utils::HeadingUtils;

#[derive(Debug)]
pub struct MD008ULStyle {
    style: char,
    use_consistent: bool,
}

impl Default for MD008ULStyle {
    fn default() -> Self {
        Self { 
            style: '-',
            use_consistent: true
        }
    }
}

impl MD008ULStyle {
    pub fn new(style: char) -> Self {
        Self { 
            style,
            use_consistent: false 
        }
    }

    fn get_list_marker(line: &str) -> Option<char> {
        let trimmed = line.trim_start();
        
        // Skip empty lines
        if trimmed.is_empty() {
            return None;
        }
        
        // Check for actual list markers
        if trimmed.starts_with(['*', '+', '-']) && 
           (trimmed.len() == 1 || trimmed.chars().nth(1) == Some(' ')) {
            Some(trimmed.chars().next().unwrap())
        } else {
            None
        }
    }
    
    fn detect_first_marker_style(&self, content: &str) -> Option<char> {
        for (i, line) in content.lines().enumerate() {
            // Skip front matter and code blocks
            if FrontMatterUtils::is_in_front_matter(content, i) || HeadingUtils::is_in_code_block(content, i) {
                continue;
            }
            
            // Look for a list marker
            if let Some(marker) = Self::get_list_marker(line) {
                return Some(marker);
            }
        }
        None
    }
}

impl Rule for MD008ULStyle {
    fn name(&self) -> &'static str {
        "MD008"
    }

    fn description(&self) -> &'static str {
        "Unordered list style"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        
        // Determine the target style - use the first marker found or fall back to default
        let target_style = if self.use_consistent {
            self.detect_first_marker_style(content).unwrap_or(self.style)
        } else {
            self.style
        };

        for (line_num, line) in content.lines().enumerate() {
            // Skip front matter and code blocks
            if FrontMatterUtils::is_in_front_matter(content, line_num) || HeadingUtils::is_in_code_block(content, line_num) {
                continue;
            }
            
            if let Some(_marker) = Self::get_list_marker(line) {
                if _marker != target_style {
                    let message = if self.use_consistent {
                        format!(
                            "Unordered list item marker '{}' should be '{}' to match first marker style",
                            _marker, target_style
                        )
                    } else {
                        format!(
                            "Unordered list item marker '{}' should be '{}' (configured style)",
                            _marker, target_style
                        )
                    };
                    
                    warnings.push(LintWarning {
                        message,
                        line: line_num + 1,
                        column: line.find(_marker).unwrap() + 1,
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: line.find(_marker).unwrap() + 1,
                            replacement: line.replacen(_marker, &target_style.to_string(), 1),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Apply front matter fixes first if needed
        let content = FrontMatterUtils::fix_malformed_front_matter(content);
        
        // Determine the target style - use the first marker found or fall back to default
        let target_style = if self.use_consistent {
            self.detect_first_marker_style(&content).unwrap_or(self.style)
        } else {
            self.style
        };
        
        let mut result = String::new();

        for (i, line) in content.lines().enumerate() {
            // Skip modifying front matter and code blocks
            if FrontMatterUtils::is_in_front_matter(&content, i) || HeadingUtils::is_in_code_block(&content, i) {
                result.push_str(line);
            } else if let Some(_marker) = Self::get_list_marker(line) {
                if _marker != target_style {
                    result.push_str(&line.replacen(_marker, &target_style.to_string(), 1));
                } else {
                    result.push_str(line);
                }
            } else {
                result.push_str(line);
            }
            
            if i < content.lines().count() - 1 {
                result.push('\n');
            }
        }

        // Preserve the original trailing newline state
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }
} 