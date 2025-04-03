use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

#[derive(Debug, Default)]
pub struct MD047FileEndNewline;

impl Rule for MD047FileEndNewline {
    fn name(&self) -> &'static str {
        "MD047"
    }

    fn description(&self) -> &'static str {
        "Files should end with a single newline character"
    }

    fn check(&self, content: &str) -> LintResult {
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Empty content is fine
        if content.is_empty() {
            return Ok(warnings);
        }

        // Check if file ends with newline
        let has_trailing_newline = content.ends_with('\n');
        
        // Check if file has multiple trailing newlines
        let has_multiple_newlines = content.ends_with("\n\n");
        
        // Only issue warning if there's no newline or more than one
        if !has_trailing_newline || has_multiple_newlines {
            let lines: Vec<&str> = content.lines().collect();
            let last_line = lines.len();
            let last_column = lines.last().map_or(1, |line| line.len() + 1);

            warnings.push(LintWarning {
            rule_name: Some(self.name()),
                message: String::from("File should end with a single newline character"),
                line: last_line,
                column: last_column,
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: line_index.line_col_to_byte_range(last_line, last_column),
                    replacement: if has_trailing_newline {
                        // If there are multiple newlines, fix by ensuring just one
                        let trimmed = content.trim_end();
                        if !trimmed.is_empty() {
                            trimmed.to_string() + "\n"
                        } else {
                            // Handle the case where content is just whitespace and newlines
                            String::new()
                        }
                    } else {
                        // If there's no newline, add one to the last line
                        String::from("\n")
                    },
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Empty content remains empty
        if content.is_empty() {
            return Ok(content.to_string());
        }

        // If the content doesn't end with a newline, add one
        if !content.ends_with('\n') {
            return Ok(content.to_string() + "\n");
        }

        // Handle multiple trailing newlines
        let mut result = content.to_string();
        
        // If there are multiple newlines, trim them down to just one
        if content.ends_with("\n\n") {
            // Preserve any whitespace at the end but only have one newline
            let content_without_trailing_newlines = content.trim_end_matches('\n');
            result = content_without_trailing_newlines.to_string() + "\n";
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_content() {
        let rule = MD047FileEndNewline::default();
        let content = "";
        let result = rule.check(content).unwrap();
        assert!(result.is_empty(), "Empty content should not trigger warnings");
    }

    #[test]
    fn test_single_newline() {
        let rule = MD047FileEndNewline::default();
        let content = "# Test\n";
        let result = rule.check(content).unwrap();
        assert!(result.is_empty(), "Content with single trailing newline should not trigger warnings");
    }

    #[test]
    fn test_missing_newline() {
        let rule = MD047FileEndNewline::default();
        let content = "# Test";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 1, "Content without trailing newline should trigger a warning");
        
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "# Test\n", "Fixed content should have a trailing newline");
    }

    #[test]
    fn test_multiple_newlines() {
        let rule = MD047FileEndNewline::default();
        let content = "# Test\n\n";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 1, "Content with multiple trailing newlines should trigger a warning");
        
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "# Test\n", "Fixed content should have exactly one trailing newline");
    }
    
    #[test]
    fn test_only_whitespace() {
        let rule = MD047FileEndNewline::default();
        let content = "  \n\n";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 1, "Content with only whitespace and multiple newlines should trigger a warning");
        
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "  \n", "Fixed content should have exactly one trailing newline");
    }
}
