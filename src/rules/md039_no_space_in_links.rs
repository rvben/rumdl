use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::DocumentStructure;
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;

/// Rule MD039: No space inside link text
///
/// See [docs/md039.md](../../docs/md039.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when link text has leading or trailing spaces which can cause
/// unexpected rendering in some Markdown parsers.
#[derive(Debug, Default, Clone)]
pub struct MD039NoSpaceInLinks;

impl MD039NoSpaceInLinks {
    pub fn new() -> Self {
        Self
    }

    /// Fast check to see if content has any potential links
    #[inline]
    fn has_links(&self, content: &str) -> bool {
        content.contains('[') && content.contains("](")
    }

    /// Check if the text has leading or trailing spaces, and return the fixed version if so
    #[inline]
    fn check_link_text(&self, text: &str) -> Option<String> {
        if text.starts_with(' ') || text.ends_with(' ') {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                Some(trimmed.to_string())
            } else {
                None
            }
        } else {
            None
        }
    }

    fn check_line(&self, line: &str) -> Vec<(usize, String, String)> {
        let mut issues = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '[' {
                let text_start_idx = i + 1;
                let mut text_end_idx = None;
                let mut link_start_idx = None;
                let mut link_end_idx = None;
                let mut bracket_depth = 1;
                let mut j = i + 1;

                // Find matching closing bracket
                while j < chars.len() {
                    match chars[j] {
                        '[' => bracket_depth += 1,
                        ']' => {
                            bracket_depth -= 1;
                            if bracket_depth == 0 {
                                text_end_idx = Some(j);
                                // Look for opening parenthesis
                                if j + 1 < chars.len() && chars[j + 1] == '(' {
                                    link_start_idx = Some(j + 2);
                                    // Find closing parenthesis
                                    let mut paren_depth = 1;
                                    let mut k = j + 2;
                                    while k < chars.len() {
                                        match chars[k] {
                                            '(' => paren_depth += 1,
                                            ')' => {
                                                paren_depth -= 1;
                                                if paren_depth == 0 {
                                                    link_end_idx = Some(k);
                                                    break;
                                                }
                                            }
                                            _ => {}
                                        }
                                        k += 1;
                                    }
                                }
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }

                // If we found a complete link pattern
                if let (Some(text_end_idx), Some(link_start_idx), Some(link_end_idx)) =
                    (text_end_idx, link_start_idx, link_end_idx)
                {
                    // Extract text and link using safe char-based operations
                    let text: String = chars[text_start_idx..text_end_idx].iter().collect();
                    let link: String = chars[link_start_idx..link_end_idx].iter().collect();

                    // Check for spaces at start or end of text
                    if text.starts_with(' ') || text.ends_with(' ') {
                        let trimmed_text = text.trim();
                        if !trimmed_text.is_empty() {
                            // Safely reconstruct the original text using char indices
                            let original: String = chars[i..=link_end_idx].iter().collect();
                            let fixed = format!("[{}]({})", trimmed_text, link);

                            // Calculate the byte position for the column
                            // This is the byte offset of the start of the link
                            let byte_position = chars[..i].iter().collect::<String>().len() + 1;

                            issues.push((byte_position, original, fixed));
                        }
                    }

                    i = link_end_idx + 1;
                    continue;
                }
            }
            i += 1;
        }

        issues
    }
}

impl Rule for MD039NoSpaceInLinks {
    fn name(&self) -> &'static str {
        "MD039"
    }

    fn description(&self) -> &'static str {
        "Spaces inside link text"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn should_skip(&self, content: &str) -> bool {
        // Skip empty content or content without links
        content.is_empty() || !self.has_links(content)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Fast path - skip if no links
        if structure.links.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let line_index = LineIndex::new(content.to_string());

        // Process all links from the document structure
        for link in &structure.links {
            if let Some(fixed_text) = self.check_link_text(&link.text) {
                // Calculate the position for fixing
                let start_col = link.start_col;
                let line_num = link.line;

                // Calculate the byte position for the start of the link
                let start_pos = line_index.line_col_to_byte_range(line_num, start_col).start;

                // Create fixed version of the entire link
                let original = format!("[{}]({})", link.text, link.url);
                let fixed = format!("[{}]({})", fixed_text, link.url);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: line_num,
                    column: start_col,
                    message: "Spaces inside link text should be removed".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: start_pos..start_pos + original.len(),
                        replacement: fixed,
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn check(&self, content: &str) -> LintResult {
        if self.should_skip(content) {
            return Ok(Vec::new());
        }

        // Get document structure for code block detection
        let doc_structure = DocumentStructure::new(content);
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        for (i, line) in content.lines().enumerate() {
            let line_num = i + 1;
            // Skip lines in code blocks using centralized detection
            if !doc_structure.is_in_code_block(line_num) {
                for (column, _original, fixed) in self.check_line(line) {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: line_num,
                        column,
                        message: "Spaces inside link text should be removed".to_string(),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line_num, column),
                            replacement: fixed,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if self.should_skip(content) {
            return Ok(content.to_string());
        }

        // Get document structure for code block detection
        let doc_structure = DocumentStructure::new(content);
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::with_capacity(content.len());

        for (i, line) in lines.iter().enumerate() {
            let mut line_str = line.to_string();
            let line_num = i + 1;

            // Skip lines in code blocks using centralized detection
            if !doc_structure.is_in_code_block(line_num) {
                for (_, original, fixed) in self.check_line(line) {
                    // Use a safe replacement method
                    line_str = line_str.replace(&original, &fixed);
                }
            }

            result.push_str(&line_str);
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD039NoSpaceInLinks::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_links() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link](url) and [another link](url) here";
        let result = rule.check(content).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_spaces_both_ends() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link ](url) and [ another link ](url) here";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "[link](url) and [another link](url) here");
    }

    #[test]
    fn test_space_at_start() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link](url) and [ another link](url) here";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "[link](url) and [another link](url) here");
    }

    #[test]
    fn test_space_at_end() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[link ](url) and [another link ](url) here";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "[link](url) and [another link](url) here");
    }

    #[test]
    fn test_link_in_code_block() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "```\n[ link ](url)\n```\n[ link ](url)";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "```\n[ link ](url)\n```\n[link](url)");
    }

    #[test]
    fn test_multiple_links() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link ](url) and [ another ](url) in one line";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "[link](url) and [another](url) in one line");
    }

    #[test]
    fn test_link_with_internal_spaces() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[this is link](url) and [ this is also link ](url)";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "[this is link](url) and [this is also link](url)");
    }

    #[test]
    fn test_link_with_punctuation() {
        let rule = MD039NoSpaceInLinks::new();
        let content = "[ link! ](url) and [ link? ](url) here";
        let result = rule.check(content).unwrap();
        assert_eq!(result.len(), 2);
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, "[link!](url) and [link?](url) here");
    }
}
