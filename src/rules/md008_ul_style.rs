use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

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
        if trimmed.starts_with(['*', '+', '-']) && 
           (trimmed.len() == 1 || trimmed.chars().nth(1) == Some(' ')) {
            Some(trimmed.chars().next().unwrap())
        } else {
            None
        }
    }
    
    fn detect_first_marker_style(&self, content: &str) -> Option<char> {
        let mut in_code_block = false;
        
        for line in content.lines() {
            let trimmed = line.trim_start();
            
            // Skip code blocks
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
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

        let mut in_code_block = false;

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            
            // Skip code blocks
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }
            
            if let Some(marker) = Self::get_list_marker(line) {
                if marker != target_style {
                    let message = if self.use_consistent {
                        format!(
                            "Unordered list item marker '{}' should be '{}' to match first marker style",
                            marker, target_style
                        )
                    } else {
                        format!(
                            "Unordered list item marker '{}' should be '{}' (configured style)",
                            marker, target_style
                        )
                    };
                    
                    warnings.push(LintWarning {
                        message,
                        line: line_num + 1,
                        column: line.find(marker).unwrap() + 1,
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: line.find(marker).unwrap() + 1,
                            replacement: line.replacen(marker, &target_style.to_string(), 1),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Determine the target style - use the first marker found or fall back to default
        let target_style = if self.use_consistent {
            self.detect_first_marker_style(content).unwrap_or(self.style)
        } else {
            self.style
        };
        
        let mut result = String::new();
        let mut in_code_block = false;

        for line in content.lines() {
            let trimmed = line.trim_start();
            
            // Handle code blocks
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                result.push_str(line);
                result.push('\n');
                continue;
            }
            if in_code_block {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if let Some(marker) = Self::get_list_marker(line) {
                if marker != target_style {
                    result.push_str(&line.replacen(marker, &target_style.to_string(), 1));
                } else {
                    result.push_str(line);
                }
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }

        // Remove the final newline if the original content didn't end with one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 