use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

#[derive(Debug, Default)]
pub struct MD031BlanksAroundFences;

impl MD031BlanksAroundFences {
    fn is_code_fence_line(line: &str) -> bool {
        line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")
    }

    fn is_empty_line(line: &str) -> bool {
        line.trim().is_empty()
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
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        let mut i = 0;

        while i < lines.len() {
            if Self::is_code_fence_line(lines[i]) {
                // Check for blank line before fence
                if i > 0 && !Self::is_empty_line(lines[i - 1]) {
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: "No blank line before fenced code block".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: format!("\n{}", lines[i]),
                        }),
                    });
                }

                // Find closing fence
                let _opening_fence = i;
                i += 1;
                while i < lines.len() && !Self::is_code_fence_line(lines[i]) {
                    i += 1;
                }

                // If we found a closing fence
                if i < lines.len() {
                    // Check for blank line after fence
                    if i + 1 < lines.len() && !Self::is_empty_line(lines[i + 1]) {
                        warnings.push(LintWarning {
                            line: i + 1,
                            column: 1,
                            message: "No blank line after fenced code block".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: _line_index
                                    .line_col_to_byte_range(i + 1, lines[i].len() + 1),
                                replacement: format!("{}\n", lines[i]),
                            }),
                        });
                    }
                }
            }
            i += 1;
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let lines: Vec<&str> = content.lines().collect();

        let mut result = Vec::new();

        let mut i = 0;

        while i < lines.len() {
            if Self::is_code_fence_line(lines[i]) {
                // Add blank line before fence if needed
                if i > 0 && !Self::is_empty_line(lines[i - 1]) {
                    result.push(String::new());
                }

                // Add opening fence
                result.push(lines[i].to_string());

                // Find and add content within code block
                let mut j = i + 1;
                while j < lines.len() && !Self::is_code_fence_line(lines[j]) {
                    result.push(lines[j].to_string());
                    j += 1;
                }

                // Add closing fence if found
                if j < lines.len() {
                    result.push(lines[j].to_string());

                    // Add blank line after fence if needed
                    if j + 1 < lines.len() && !Self::is_empty_line(lines[j + 1]) {
                        result.push(String::new());
                    }

                    i = j;
                } else {
                    i = j;
                }
            } else {
                result.push(lines[i].to_string());
            }
            i += 1;
        }

        Ok(result.join("\n"))
    }
}
