use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug, Default)]
pub struct MD038NoSpaceInCode;

impl MD038NoSpaceInCode {
    fn is_in_code_block(&self, content: &str, line_num: usize) -> bool {
        let mut in_code_block = false;
        let mut fence_type = None;
        
        for (i, line) in content.lines().enumerate() {
            if i + 1 == line_num {
                // Count backticks in the current line up to this point
                let backticks = line.chars().filter(|&c| c == '`').count();
                return in_code_block || backticks % 2 == 1;
            }
            
            let trimmed = line.trim();
            if let Some(fence) = fence_type {
                if trimmed.starts_with(fence) {
                    in_code_block = false;
                    fence_type = None;
                }
            } else if trimmed.starts_with("```") {
                in_code_block = true;
                fence_type = Some("```");
            } else if trimmed.starts_with("~~~") {
                in_code_block = true;
                fence_type = Some("~~~");
            }
        }
        
        in_code_block
    }

    fn check_line(&self, line: &str) -> Vec<(usize, String, String)> {
        let mut issues = Vec::new();
        
        // Find all code spans and check for spaces
        let mut in_code = false;
        let mut start_pos = 0;
        let chars: Vec<char> = line.chars().collect();
        
        for (i, &c) in chars.iter().enumerate() {
            if c == '`' {
                if !in_code {
                    // Start of code span
                    start_pos = i;
                    in_code = true;
                } else {
                    // End of code span
                    in_code = false;
                    
                    // Skip if this span is part of a longer span (e.g. ``code``)
                    if i > 0 && chars[i - 1] == '`' {
                        continue;
                    }
                    if i < chars.len() - 1 && chars[i + 1] == '`' {
                        continue;
                    }
                    
                    // Check for spaces at start and end
                    let span = &line[start_pos..=i];
                    let content = &span[1..span.len() - 1];
                    
                    if content.starts_with(' ') || content.ends_with(' ') {
                        let trimmed = content.trim();
                        if !trimmed.is_empty() {
                            let fixed = format!("`{}`", trimmed);
                            issues.push((start_pos + 1, span.to_string(), fixed));
                        }
                    }
                }
            }
        }
        
        issues
    }
}

impl Rule for MD038NoSpaceInCode {
    fn name(&self) -> &'static str {
        "MD038"
    }

    fn description(&self) -> &'static str {
        "Spaces inside code span elements"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (i, line) in content.lines().enumerate() {
            if !self.is_in_code_block(content, i + 1) {
                for (column, original, fixed) in self.check_line(line) {
                    warnings.push(LintWarning {
                        message: format!("Spaces inside code span elements: '{}'", original),
                        line: i + 1,
                        column,
                        fix: Some(Fix {
                            line: i + 1,
                            column,
                            replacement: fixed,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::new();

        for i in 0..lines.len() {
            let mut line = lines[i].to_string();
            if !self.is_in_code_block(content, i + 1) {
                for (_, original, fixed) in self.check_line(lines[i]) {
                    line = line.replace(&original, &fixed);
                }
            }
            result.push_str(&line);
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }
} 