
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};

/// Rule MD047: Files should end with a single trailing newline character
pub struct MD047SingleTrailingNewline;

impl MD047SingleTrailingNewline {
    pub fn new() -> Self {
        Self
    }
}

impl Rule for MD047SingleTrailingNewline {
    fn name(&self) -> &'static str {
        "MD047"
    }

    fn description(&self) -> &'static str {
        "Files should end with a single trailing newline character"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let line_count = content.lines().count();

        if !content.ends_with('\n') || content.ends_with("\n\n") {
            warnings.push(LintWarning {
            rule_name: Some(self.name()),
                line: line_count,
                column: 1,
                message: "File should end with a single newline character".to_string(),
                fix: Some(Fix {
                    line: line_count,
                    column: 1,
                    replacement: if content.ends_with('\n') {
                        content.trim_end().to_string() + "\n"
                    } else {
                        content.to_string() + "\n"
                    },
                }),
            });
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.ends_with('\n') {
            Ok(content.trim_end().to_string() + "\n")
        } else {
            Ok(content.to_string() + "\n")
        }
    }
} 