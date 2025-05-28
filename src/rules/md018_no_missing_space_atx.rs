/// Rule MD018: No missing space after ATX heading marker
///
/// See [docs/md018.md](../../docs/md018.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::calculate_single_line_range;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref ATX_NO_SPACE_PATTERN: Regex = Regex::new(r"(?m)^(#+)([^#\s].*)").unwrap();
}

#[derive(Clone)]
pub struct MD018NoMissingSpaceAtx;

impl Default for MD018NoMissingSpaceAtx {
    fn default() -> Self {
        Self::new()
    }
}

impl MD018NoMissingSpaceAtx {
    pub fn new() -> Self {
        Self
    }

    fn is_atx_heading_without_space(&self, line: &str) -> bool {
        ATX_NO_SPACE_PATTERN.is_match(line)
    }

    fn fix_atx_heading(&self, line: &str) -> String {
        let captures = ATX_NO_SPACE_PATTERN.captures(line).unwrap();

        let hashes = captures.get(1).unwrap();

        let content = &line[hashes.end()..];
        format!("{} {}", hashes.as_str(), content)
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

impl Rule for MD018NoMissingSpaceAtx {
    fn name(&self) -> &'static str {
        "MD018"
    }

    fn description(&self) -> &'static str {
        "No space after hash on ATX style heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early return for empty content or content without ATX headings
        if content.is_empty() || !content.contains('#') {
            return Ok(Vec::new());
        }

        // Fallback path: create structure manually (should rarely be used)
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Fast path: if no hash symbols, return unchanged
        if content.is_empty() || !content.contains('#') {
            return Ok(content.to_string());
        }

        // Use document structure to identify code blocks and heading lines
        let structure = DocumentStructure::new(content);

        // If no headings, return unchanged
        if structure.heading_lines.is_empty() {
            return Ok(content.to_string());
        }

        // Create a set of heading line numbers for fast lookup
        let heading_lines: std::collections::HashSet<usize> =
            structure.heading_lines.iter().cloned().collect();

        // Process line by line, only applying regex to heading lines
        let lines: Vec<&str> = content.lines().collect();
        let mut result_lines = Vec::with_capacity(lines.len());

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1; // Convert to 1-indexed

            // Only apply fix to heading lines that need it
            if heading_lines.contains(&line_num) && self.is_atx_heading_without_space(line) {
                result_lines.push(self.fix_atx_heading(line));
            } else {
                result_lines.push(line.to_string());
            }
        }

        Ok(result_lines.join("\n"))
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        _ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        // Early return if no headings
        if structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let content = _ctx.content;
        let lines: Vec<&str> = content.lines().collect();

        // Process only heading lines using structure.heading_lines
        for &line_num in &structure.heading_lines {
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];

            // Check if this is an ATX heading without space
            if self.is_atx_heading_without_space(line) {
                let captures = ATX_NO_SPACE_PATTERN.captures(line).unwrap();
                let hashes = captures.get(1).unwrap();
                let content_start = captures.get(2).unwrap();

                // Calculate precise range: highlight from end of hashes to start of content
                let hash_end_col = hashes.end() + 1; // 1-indexed
                let content_start_col = content_start.start() + 1; // 1-indexed
                let (start_line, start_col, end_line, end_col) = calculate_single_line_range(
                    line_num,
                    hash_end_col,
                    content_start_col - hash_end_col,
                );

                let line_range = self.get_line_byte_range(content, line_num);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!(
                        "No space after {} in ATX style heading",
                        "#".repeat(hashes.as_str().len())
                    ),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_range,
                        replacement: self.fix_atx_heading(line),
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
        Box::new(MD018NoMissingSpaceAtx::new())
    }
}

impl DocumentStructureExtensions for MD018NoMissingSpaceAtx {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        doc_structure: &DocumentStructure,
    ) -> bool {
        // This rule is only relevant if there are headings
        !doc_structure.heading_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_with_document_structure() {
        let rule = MD018NoMissingSpaceAtx;

        // Test with correct space
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let result = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert!(result.is_empty());

        // Test with missing space
        let content = "#Heading 1\n## Heading 2\n###Heading 3";
        let structure = DocumentStructure::new(content);
        let result = rule
            .check_with_structure(&LintContext::new(content), &structure)
            .unwrap();
        assert_eq!(result.len(), 2); // Should flag the two headings with missing spaces
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }
}
