use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

#[derive(Debug, Default)]
pub struct MD040FencedCodeLanguage;

impl Rule for MD040FencedCodeLanguage {
    fn name(&self) -> &'static str {
        "MD040"
    }

    fn description(&self) -> &'static str {
        "Fenced code blocks should have a language specified"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());

        let mut warnings = Vec::new();

        let mut in_code_block = false;

        let mut fence_char = None;

        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            if let Some(ref current_fence) = fence_char {
                if trimmed.starts_with(current_fence) {
                    in_code_block = false;
                    fence_char = None;
                }
            } else if !in_code_block && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
                // Opening fence
                let fence = if trimmed.starts_with("```") {
                    "```"
                } else {
                    "~~~"
                };
                fence_char = Some(fence.to_string());

                // Check if language is specified
                let after_fence = trimmed[fence.len()..].trim();
                if after_fence.is_empty() {
                    let _indent = line.len() - line.trim_start().len();
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: 1,
                        message: "Fenced code blocks should have a language specified".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: if line.starts_with("```") {
                                "```text".to_string()
                            } else {
                                "~~~text".to_string()
                            },
                        }),
                    });
                }
                in_code_block = true;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());

        let mut result = String::new();

        let mut in_code_block = false;

        let mut fence_char = None;

        let lines: Vec<&str> = content.lines().collect();
        for line in lines.iter() {
            let trimmed = line.trim();

            if let Some(ref current_fence) = fence_char {
                if trimmed.starts_with(current_fence) {
                    // This is a closing fence - use no indentation
                    result.push_str(&format!("{}\n", current_fence));
                    in_code_block = false;
                    fence_char = None;
                    continue;
                }

                // This is content inside a code block - keep original indentation
                result.push_str(line);
                result.push('\n');
            } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    let fence = if trimmed.starts_with("```") {
                        "```"
                    } else {
                        "~~~"
                    };
                    fence_char = Some(fence.to_string());

                    // Add 'text' as default language for opening fence if no language specified
                    let after_fence = trimmed[fence.len()..].trim();
                    if after_fence.is_empty() {
                        // Use no indentation for the opening fence with language
                        result.push_str(&format!("{}text\n", fence));
                    } else {
                        // Keep original indentation for fences that already have a language
                        result.push_str(line);
                        result.push('\n');
                    }
                } else {
                    result.push_str(line);
                    result.push('\n');
                }
                in_code_block = true;
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
}
