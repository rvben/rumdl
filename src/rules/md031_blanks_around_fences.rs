use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

#[derive(Debug, Default)]
pub struct MD031BlanksAroundFences;

impl MD031BlanksAroundFences {
    fn is_code_fence_line(line: &str) -> bool {
        line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")
    }

    fn is_empty_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    fn get_indentation(line: &str) -> String {
        let chars: Vec<char> = line.chars().collect();
        let indent_len = chars.iter()
            .take_while(|&&c| c.is_whitespace())
            .count();
        chars[..indent_len].iter().collect()
    }
}

impl Rule for MD031BlanksAroundFences {
    fn name(&self) -> &'static str {
        "MD031"
    }

    fn description(&self) -> &'static str {
        "Fenced code blocks should be surrounded by blank lines"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            if Self::is_code_fence_line(line) {
                // Check line before code fence
                if i > 0 && !Self::is_empty_line(lines[i - 1]) {
                    warnings.push(LintWarning {
                        message: "No blank line before fenced code block".to_string(),
                        line: i + 1,
                        column: 1,
                        fix: Some(Fix {
                            line: i + 1,
                            column: 1,
                            replacement: format!("\n{}", line),
                        }),
                    });
                }

                // Find the closing fence
                let mut j = i + 1;
                while j < lines.len() && !Self::is_code_fence_line(lines[j]) {
                    j += 1;
                }

                // Check line after code fence if we found a closing fence
                if j < lines.len() && j + 1 < lines.len() && !Self::is_empty_line(lines[j + 1]) {
                    warnings.push(LintWarning {
                        message: "No blank line after fenced code block".to_string(),
                        line: j + 2,
                        column: 1,
                        fix: Some(Fix {
                            line: j + 2,
                            column: 1,
                            replacement: format!("\n{}", lines[j + 1]),
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
        let mut i = 0;

        while i < lines.len() {
            if Self::is_code_fence_line(lines[i]) {
                // Add blank line before code fence if needed
                if i > 0 && !Self::is_empty_line(lines[i - 1]) {
                    result.push('\n');
                }

                // Add the code fence line
                result.push_str(lines[i]);
                result.push('\n');
                i += 1;

                // Copy the code block content
                while i < lines.len() && !Self::is_code_fence_line(lines[i]) {
                    result.push_str(lines[i]);
                    result.push('\n');
                    i += 1;
                }

                // Add the closing fence if found
                if i < lines.len() {
                    result.push_str(lines[i]);
                    result.push('\n');
                    
                    // Add blank line after code fence if needed
                    if i + 1 < lines.len() && !Self::is_empty_line(lines[i + 1]) {
                        result.push('\n');
                    }
                }
            } else {
                result.push_str(lines[i]);
                result.push('\n');
            }
            i += 1;
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 