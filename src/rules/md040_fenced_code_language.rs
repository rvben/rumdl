use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

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
            } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    // Opening fence
                    let fence = if trimmed.starts_with("```") { "```" } else { "~~~" };
                    fence_char = Some(fence.to_string());
                    
                    // Check if language is specified
                    let after_fence = trimmed[fence.len()..].trim();
                    if after_fence.is_empty() {
                        let indent = line.len() - trimmed.len();
                        warnings.push(LintWarning {
                            message: "Fenced code block should specify a language".to_string(),
                            line: i + 1,
                            column: indent + 1,
                            fix: Some(Fix {
                                line: i + 1,
                                column: 1,
                                replacement: format!("{}{}{}", " ".repeat(indent), fence, "text"), // Use 'text' as default language
                            }),
                        });
                    }
                }
                in_code_block = true;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut in_code_block = false;
        let mut fence_char = None;

        for line in content.lines() {
            let trimmed = line.trim();
            
            if let Some(ref current_fence) = fence_char {
                if trimmed.starts_with(current_fence) {
                    result.push_str(line);
                    result.push('\n');
                    in_code_block = false;
                    fence_char = None;
                    continue;
                }
                result.push_str(line);
                result.push('\n');
            } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    let fence = if trimmed.starts_with("```") { "```" } else { "~~~" };
                    fence_char = Some(fence.to_string());
                    
                    // Add 'text' as default language for opening fence if no language specified
                    let after_fence = trimmed[fence.len()..].trim();
                    if after_fence.is_empty() {
                        let indent = line.len() - trimmed.len();
                        result.push_str(&format!("{}{}{}\n", " ".repeat(indent), fence, "text"));
                    } else {
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