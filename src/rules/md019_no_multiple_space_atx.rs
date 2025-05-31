/// Rule MD019: No multiple spaces after ATX heading marker
///
/// See [docs/md019.md](../../docs/md019.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::{calculate_single_line_range, LineIndex};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref ATX_MULTIPLE_SPACE_PATTERN: Regex = Regex::new(r"^(#+)\s{2,}").unwrap();
}

#[derive(Clone)]
pub struct MD019NoMultipleSpaceAtx;

impl Default for MD019NoMultipleSpaceAtx {
    fn default() -> Self {
        Self::new()
    }
}

impl MD019NoMultipleSpaceAtx {
    pub fn new() -> Self {
        Self
    }

    fn is_atx_heading_with_multiple_spaces(&self, line: &str) -> bool {
        ATX_MULTIPLE_SPACE_PATTERN.is_match(line)
    }

    fn fix_atx_heading(&self, line: &str) -> String {
        let captures = ATX_MULTIPLE_SPACE_PATTERN.captures(line).unwrap();

        let hashes = captures.get(1).unwrap();

        let content = line[hashes.end()..].trim_start();
        format!("{} {}", hashes.as_str(), content)
    }

    fn count_spaces_after_hashes(&self, line: &str) -> usize {
        let captures = ATX_MULTIPLE_SPACE_PATTERN.captures(line).unwrap();

        let hashes = captures.get(1).unwrap();
        line[hashes.end()..]
            .chars()
            .take_while(|c| c.is_whitespace())
            .count()
    }
}

impl Rule for MD019NoMultipleSpaceAtx {
    fn name(&self) -> &'static str {
        "MD019"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after hash on ATX style heading"
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
        if content.is_empty() {
            return Ok(String::new());
        }
        let structure = DocumentStructure::new(content);
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            // Only process heading lines
            let is_heading_line = structure.heading_lines.iter().any(|&ln| ln == i + 1);
            if is_heading_line && self.is_atx_heading_with_multiple_spaces(line) {
                result.push_str(&self.fix_atx_heading(line));
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
        _ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        // Early return if no headings
        if structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(_ctx.content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = _ctx.content.lines().collect();

        // Process only heading lines using structure.heading_lines
        for &line_num in &structure.heading_lines {
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];

            // Check if this is an ATX heading with multiple spaces
            if self.is_atx_heading_with_multiple_spaces(line) {
                let captures = ATX_MULTIPLE_SPACE_PATTERN.captures(line).unwrap();
                let hashes = captures.get(1).unwrap();
                let spaces = self.count_spaces_after_hashes(line);

                // Calculate precise range: highlight the extra spaces (all spaces after the first one)
                let hash_end_col = hashes.end() + 1; // 1-indexed, position after hashes
                let spaces_start_col = hash_end_col; // Start of all spaces after hashes
                let spaces_len = spaces; // Length of all spaces

                let (start_line, start_col, end_line, end_col) =
                    calculate_single_line_range(line_num, spaces_start_col, spaces_len);

                // Generate the correct replacement text (just one space)
                let correct_spacing = " ";

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!(
                        "Multiple spaces ({}) after {} in ATX style heading",
                        spaces,
                        "#".repeat(hashes.as_str().len())
                    ),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line_num, start_col),
                        replacement: correct_spacing.to_string(),
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
        ctx.content.is_empty() || !ctx.content.contains('#')
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
        Box::new(MD019NoMultipleSpaceAtx::new())
    }
}

impl DocumentStructureExtensions for MD019NoMultipleSpaceAtx {
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
    use crate::utils::document_structure::document_structure_from_str;

    #[test]
    fn test_with_document_structure() {
        let rule = MD019NoMultipleSpaceAtx::new();

        // Test with heading that has multiple spaces
        let content = "#  Multiple Spaces\n\nRegular content\n\n##   More Spaces";
        let structure = document_structure_from_str(content);
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should flag both headings
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 5);

        // Test with proper headings
        let content = "# Single Space\n\n## Also correct";
        let structure = document_structure_from_str(content);
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Properly formatted headings should not generate warnings"
        );
    }
}
