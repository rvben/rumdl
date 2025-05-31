/// Rule MD021: No multiple spaces inside closed ATX heading
///
/// See [docs/md021.md](../../docs/md021.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{calculate_single_line_range, LineIndex};
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

        let line_index = LineIndex::new(ctx.content.to_string());
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
                let _indentation = captures.get(1).unwrap();
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

                // Calculate precise character range for the extra spaces
                let start_col;
                let length;
                let replacement;

                if start_spaces > 1 && end_spaces > 1 {
                    // Fix the extra spaces at the start (after opening hashes)
                    let opening_hashes = captures.get(2).unwrap();
                    start_col = opening_hashes.end() + 2; // After hash + first space
                    length = start_spaces - 1; // Extra spaces only
                    replacement = String::new(); // Remove extra spaces
                } else if start_spaces > 1 {
                    // Fix the extra spaces after opening hashes
                    let opening_hashes = captures.get(2).unwrap();
                    start_col = opening_hashes.end() + 2; // After hash + first space
                    length = start_spaces - 1; // Extra spaces only
                    replacement = String::new(); // Remove extra spaces
                } else {
                    // Fix the extra spaces before closing hashes
                    let content = captures.get(4).unwrap();
                    start_col = content.end() + 2; // After content + first space
                    length = end_spaces - 1; // Extra spaces only
                    replacement = String::new(); // Remove extra spaces
                };

                let (start_line, start_col_calc, end_line, end_col) =
                    calculate_single_line_range(line_num, start_col, length);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message,
                    line: start_line,
                    column: start_col_calc,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(start_line, start_col_calc),
                        replacement,
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
