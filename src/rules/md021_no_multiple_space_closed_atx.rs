/// Rule MD021: No multiple spaces inside closed ATX heading
///
/// See [docs/md021.md](../../docs/md021.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Matches closed ATX headings with spaces between hashes and content,
    // including indented ones
    static ref CLOSED_ATX_MULTIPLE_SPACE_PATTERN: Regex =
        Regex::new(r"^(\s*)(#+)(\s+)(.*?)(\s+)(#+)\s*$").unwrap();

    // Matches code fence blocks
    static ref CODE_FENCE_PATTERN: Regex =
        Regex::new(r"^(`{3,}|~{3,})").unwrap();
}

#[derive(Clone)]
pub struct MD021NoMultipleSpaceClosedAtx;

impl Default for MD021NoMultipleSpaceClosedAtx {
    fn default() -> Self {
        Self::new()
    }
}

impl MD021NoMultipleSpaceClosedAtx {
    pub fn new() -> Self {
        Self
    }

    fn is_closed_atx_heading_with_multiple_spaces(&self, line: &str) -> bool {
        if let Some(captures) = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line) {
            let start_spaces = captures.get(3).unwrap().as_str().len();
            let end_spaces = captures.get(5).unwrap().as_str().len();
            start_spaces > 1 || end_spaces > 1
        } else {
            false
        }
    }

    fn fix_closed_atx_heading(&self, line: &str) -> String {
        if let Some(captures) = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[4];
            let closing_hashes = &captures[6];
            format!(
                "{}{} {} {}",
                indentation,
                opening_hashes,
                content.trim(),
                closing_hashes
            )
        } else {
            line.to_string()
        }
    }

    fn count_spaces(&self, line: &str) -> (usize, usize) {
        if let Some(captures) = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line) {
            let start_spaces = captures.get(3).unwrap().as_str().len();
            let end_spaces = captures.get(5).unwrap().as_str().len();
            (start_spaces, end_spaces)
        } else {
            (0, 0)
        }
    }

    // Calculate the byte range for a specific line in the content
    fn get_line_byte_range(&self, content: &str, line_num: usize) -> std::ops::Range<usize> {
        let mut current_line = 1;
        let mut start_byte = 0;

        for (i, c) in content.char_indices() {
            if current_line == line_num && c == '\n' {
                return start_byte..i;
            } else if c == '\n' {
                current_line += 1;
                if current_line == line_num {
                    start_byte = i + 1;
                }
            }
        }

        // If we're looking for the last line and it doesn't end with a newline
        if current_line == line_num {
            return start_byte..content.len();
        }

        // Fallback if line not found (shouldn't happen)
        0..0
    }
}

impl Rule for MD021NoMultipleSpaceClosedAtx {
    fn name(&self) -> &'static str {
        "MD021"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces inside hashes on closed ATX style heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        if content.is_empty() {
            return Ok(String::new());
        }
        let structure = DocumentStructure::new(content);
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            // Only process heading lines
            let is_heading_line = structure.heading_lines.iter().any(|&ln| ln == i + 1);
            if is_heading_line && self.is_closed_atx_heading_with_multiple_spaces(line) {
                result.push_str(&self.fix_closed_atx_heading(line));
            } else {
                result.push_str(line);
            }
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }
        // Preserve trailing newline if original had it
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> LintResult {
        // Early return if no headings
        if _doc_structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let lines: Vec<&str> = ctx.content.lines().collect();

        // Process only heading lines using structure.heading_lines
        for &line_num in &_doc_structure.heading_lines {
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];

            // Check if line matches closed ATX pattern with multiple spaces
            if self.is_closed_atx_heading_with_multiple_spaces(line) {
                let captures = CLOSED_ATX_MULTIPLE_SPACE_PATTERN.captures(line).unwrap();
                let indentation = captures.get(1).unwrap();
                let opening_hashes = captures.get(2).unwrap();
                let (start_spaces, end_spaces) = self.count_spaces(line);

                let message = if start_spaces > 1 && end_spaces > 1 {
                    format!(
                        "Multiple spaces ({} at start, {} at end) inside hashes on closed ATX style heading with {} hashes",
                        start_spaces,
                        end_spaces,
                        opening_hashes.as_str().len()
                    )
                } else if start_spaces > 1 {
                    format!(
                        "Multiple spaces ({}) after opening hashes on closed ATX style heading with {} hashes",
                        start_spaces,
                        opening_hashes.as_str().len()
                    )
                } else {
                    format!(
                        "Multiple spaces ({}) before closing hashes on closed ATX style heading with {} hashes",
                        end_spaces,
                        opening_hashes.as_str().len()
                    )
                };

                let line_range = self.get_line_byte_range(ctx.content, line_num);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message,
                    line: line_num,
                    column: indentation.end() + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_range,
                        replacement: self.fix_closed_atx_heading(line),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        let content = ctx.content;
        content.is_empty() || !content.contains('#')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_maybe_document_structure(&self) -> Option<&dyn crate::rule::MaybeDocumentStructure> {
        Some(self)
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD021NoMultipleSpaceClosedAtx::new())
    }
}

impl DocumentStructureExtensions for MD021NoMultipleSpaceClosedAtx {
    fn has_relevant_elements(
        &self,
        ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
        let content = ctx.content;
        !content.is_empty() && content.contains('#')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_with_document_structure() {
        let rule = MD021NoMultipleSpaceClosedAtx;

        // Test with correct spacing
        let content = "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###";
        let structure = DocumentStructure::new(content);
        let result = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(result.is_empty());

        // Test with multiple spaces
        let content = "#  Heading 1 #\n## Heading 2 ##\n### Heading 3  ###";
        let structure = DocumentStructure::new(content);
        let result = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert_eq!(result.len(), 2); // Should flag the two headings with multiple spaces
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }
}
