use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD037SpacesAroundEmphasis;

impl MD037SpacesAroundEmphasis {
    fn is_in_code_block(&self, content: &str, line_num: usize) -> bool {
        let mut in_code_block = false;
        let mut in_inline_code = false;
        
        for (i, line) in content.lines().enumerate() {
            if i + 1 == line_num {
                // Count backticks in the current line up to this point
                let backticks = line.chars().filter(|&c| c == '`').count();
                in_inline_code = backticks % 2 == 1;
                break;
            }
            
            if line.trim().starts_with("```") || line.trim().starts_with("~~~") {
                in_code_block = !in_code_block;
            }
        }
        
        in_code_block || in_inline_code
    }

    fn find_emphasis_issues(&self, line: &str) -> Vec<(usize, String, String)> {
        let mut issues = Vec::new();
        
        // Match emphasis with spaces inside
        let patterns = [
            // Single asterisk with spaces inside
            (r"\*\s+([^\s*].*?[^\s*])\s*\*|\*\s*([^\s*].*?[^\s*])\s+\*", "*"),
            // Double asterisk with spaces inside
            (r"\*\*\s+([^\s*].*?[^\s*])\s*\*\*|\*\*\s*([^\s*].*?[^\s*])\s+\*\*", "**"),
            // Single underscore with spaces inside
            (r"_\s+([^\s_].*?[^\s_])\s*_|_\s*([^\s_].*?[^\s_])\s+_", "_"),
            // Double underscore with spaces inside
            (r"__\s+([^\s_].*?[^\s_])\s*__|__\s*([^\s_].*?[^\s_])\s+__", "__"),
        ];

        for (pattern, marker) in patterns {
            let re = Regex::new(pattern).unwrap();
            for cap in re.captures_iter(line) {
                let full_match = cap.get(0).unwrap();
                let start = full_match.start();
                let original = full_match.as_str().to_string();
                
                // Get the content from either the first or second capture group
                let content = cap.get(1).or_else(|| cap.get(2)).map(|m| m.as_str().trim());
                
                if let Some(content) = content {
                    let fixed = format!("{}{}{}", marker, content, marker);
                    if fixed != original {
                        issues.push((start, original, fixed));
                    }
                }
            }
        }

        issues
    }
}

impl Rule for MD037SpacesAroundEmphasis {
    fn name(&self) -> &'static str {
        "MD037"
    }

    fn description(&self) -> &'static str {
        "Spaces inside emphasis markers"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            if !self.is_in_code_block(content, line_num + 1) {
                for (col, original, fixed) in self.find_emphasis_issues(line) {
                    let marker_count = original.chars().take_while(|&c| c == '*' || c == '_').count();
                    let marker = &original[..marker_count];
                    warnings.push(LintWarning {
                        message: format!("Spaces inside {} emphasis markers", marker),
                        line: line_num + 1,
                        column: col + 1,
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: col + 1,
                            replacement: fixed,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let mut fixed_line = line.to_string();
            if !self.is_in_code_block(content, i + 1) {
                let mut issues = self.find_emphasis_issues(line);
                issues.reverse(); // Process from right to left to maintain correct indices
                for (col, original, fixed) in issues {
                    fixed_line.replace_range(col..col + original.len(), &fixed);
                }
            }
            result.push_str(&fixed_line);
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }
} 