use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Default, Clone)]
pub struct MD009NoTrailingSpaces;

impl Rule for MD009NoTrailingSpaces {
    fn name(&self) -> &'static str {
        "MD009"
    }

    fn description(&self) -> &'static str {
        "Trailing spaces are not allowed"
    }

    /// Static regex pattern for detecting trailing spaces
    /// Using once_cell::Lazy for better initialization
    fn check(&self, content: &str) -> LintResult {
        // Quick check: if the content doesn't contain any trailing spaces, return early
        if !content.contains(" \n") && !content.ends_with(" ") {
            return Ok(Vec::new());
        }

        static TRAILING_SPACE_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r" +$").unwrap()
        });

        let mut warnings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            // Quick check before applying regex
            if !line.ends_with(" ") {
                continue;
            }

            // Calculate trailing spaces
            let trailing_spaces = line.len() - line.trim_end().len();
            if trailing_spaces > 0 {
                let line_pos = content.lines().take(line_num).map(|l| l.len() + 1).sum::<usize>();
                let start_pos = line_pos + line.len() - trailing_spaces;
                
                warnings.push(LintWarning {
            rule_name: Some(self.name()),
                    message: format!("Found {} trailing space(s)", trailing_spaces),
                    line: line_num + 1,
                    column: line.len() - trailing_spaces + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: start_pos..(start_pos + trailing_spaces),
                        replacement: String::new(),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, _structure: &DocumentStructure) -> LintResult {
        self.check(content) // For this rule, document structure doesn't provide much benefit
    }

    /// Quick check if we should run this rule
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty() || (!content.contains(" \n") && !content.ends_with(" "))
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Quick check: if the content doesn't contain any trailing spaces, return early
        if !content.contains(" \n") && !content.ends_with(" ") {
            return Ok(content.to_string());
        }

        static TRAILING_SPACE_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r" +$").unwrap()
        });

        let mut result = String::with_capacity(content.len());
        let mut last_line = 0;

        for (line_num, line) in content.lines().enumerate() {
            if line_num > last_line {
                result.push('\n');
            }
            
            // Only run regex if needed
            if line.ends_with(" ") {
                result.push_str(line.trim_end());
            } else {
                result.push_str(line);
            }
            
            last_line = line_num;
        }

        // Preserve trailing newline if it exists
        if content.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Whitespace
    }
}

impl DocumentStructureExtensions for MD009NoTrailingSpaces {
    fn has_relevant_elements(&self, content: &str, _doc_structure: &DocumentStructure) -> bool {
        !content.is_empty() && (content.contains(" \n") || content.ends_with(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trailing_spaces() {
        let rule = MD009NoTrailingSpaces::default();
        
        // Test with no trailing spaces
        let content = "This is a test\nWith multiple lines\nBut no trailing spaces";
        let result = rule.check(content).unwrap();
        assert!(result.is_empty());
        
        // Test with trailing spaces
        let content = "This is a test \nWith multiple lines\nAnd trailing spaces ";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }
    
    #[test]
    fn test_fix_trailing_spaces() {
        let rule = MD009NoTrailingSpaces::default();
        
        // Test fixing trailing spaces
        let content = "This is a test \nWith multiple lines\nAnd trailing spaces ";
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "This is a test\nWith multiple lines\nAnd trailing spaces");
    }
}