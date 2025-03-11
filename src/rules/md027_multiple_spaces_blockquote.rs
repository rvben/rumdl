use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use crate::rules::blockquote_utils::BlockquoteUtils;

#[derive(Debug, Default)]
pub struct MD027MultipleSpacesBlockquote;

impl Rule for MD027MultipleSpacesBlockquote {
    fn name(&self) -> &'static str {
        "MD027"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after blockquote symbol"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        for (i, &line) in lines.iter().enumerate() {
            if BlockquoteUtils::is_blockquote(line) && BlockquoteUtils::has_multiple_spaces_after_marker(line) {
                warnings.push(LintWarning {
                    message: "Multiple spaces after blockquote symbol".to_string(),
                    line: i + 1,
                    column: 1,
                    fix: Some(Fix {
                        line: i + 1,
                        column: 1,
                        replacement: BlockquoteUtils::fix_blockquote_spacing(line),
                    }),
                });
            }
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::with_capacity(lines.len());
        
        for line in lines {
            if BlockquoteUtils::is_blockquote(line) && BlockquoteUtils::has_multiple_spaces_after_marker(line) {
                result.push(BlockquoteUtils::fix_blockquote_spacing(line));
            } else {
                result.push(line.to_string());
            }
        }
        
        // Preserve trailing newline if original content had one
        Ok(result.join("\n") + if content.ends_with('\n') { "\n" } else { "" })
    }
} 