/// Rule MD019: No multiple spaces after ATX heading marker
///
/// See [docs/md019.md](../../docs/md019.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::{LineIndex, calculate_single_line_range};

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

    /// Count spaces after the ATX marker
    fn count_spaces_after_marker(&self, line: &str, marker_len: usize) -> usize {
        let after_marker = &line[marker_len..];
        after_marker.chars().take_while(|c| *c == ' ' || *c == '\t').count()
    }
}

impl Rule for MD019NoMultipleSpaceAtx {
    fn name(&self) -> &'static str {
        "MD019"
    }

    fn description(&self) -> &'static str {
        "Multiple spaces after hash in heading"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Create LineIndex once outside the loop
        let line_index = LineIndex::new(ctx.content.to_string());

        // Check all ATX headings from cached info
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Only check ATX headings
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    let line = &line_info.content;
                    let trimmed = line.trim_start();
                    let marker_pos = line_info.indent + heading.marker.len();

                    // Count spaces after marker
                    if trimmed.len() > heading.marker.len() {
                        let space_count = self.count_spaces_after_marker(trimmed, heading.marker.len());

                        if space_count > 1 {
                            // Calculate range for the extra spaces
                            let (start_line, start_col, end_line, end_col) = calculate_single_line_range(
                                line_num + 1,   // Convert to 1-indexed
                                marker_pos + 1, // Start after marker (1-indexed)
                                space_count,    // Length of all spaces (not just extra)
                            );

                            // Calculate byte range for just the extra spaces
                            let line_start_byte = line_index.get_line_start_byte(line_num + 1).unwrap_or(0);

                            // We need to work with the original line, not trimmed
                            let original_line = &line_info.content;
                            let marker_byte_pos = line_start_byte + line_info.indent + heading.marker.len();

                            // Get the actual byte length of the spaces/tabs after the marker
                            let after_marker_start = line_info.indent + heading.marker.len();
                            let after_marker = &original_line[after_marker_start..];
                            let space_bytes = after_marker
                                .as_bytes()
                                .iter()
                                .take_while(|&&b| b == b' ' || b == b'\t')
                                .count();

                            let extra_spaces_start = marker_byte_pos;
                            let extra_spaces_end = marker_byte_pos + space_bytes;

                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                message: format!(
                                    "Multiple spaces ({}) after {} in heading",
                                    space_count,
                                    "#".repeat(heading.level as usize)
                                ),
                                line: start_line,
                                column: start_col,
                                end_line,
                                end_column: end_col,
                                severity: Severity::Warning,
                                fix: Some(Fix {
                                    range: extra_spaces_start..extra_spaces_end,
                                    replacement: " ".to_string(), // Replace extra spaces with single space
                                }),
                            });
                        }
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut lines = Vec::new();

        for line_info in ctx.lines.iter() {
            let mut fixed = false;

            if let Some(heading) = &line_info.heading {
                // Fix ATX headings with multiple spaces
                if matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    let line = &line_info.content;
                    let trimmed = line.trim_start();

                    if trimmed.len() > heading.marker.len() {
                        let space_count = self.count_spaces_after_marker(trimmed, heading.marker.len());

                        if space_count > 1 {
                            // Normalize to single space
                            lines.push(format!(
                                "{}{} {}",
                                " ".repeat(line_info.indent),
                                heading.marker,
                                trimmed[heading.marker.len()..].trim_start()
                            ));
                            fixed = true;
                        }
                    }
                }
            }

            if !fixed {
                lines.push(line_info.content.clone());
            }
        }

        // Reconstruct content preserving line endings
        let mut result = lines.join("\n");
        if ctx.content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
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
        None
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD019NoMultipleSpaceAtx::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        let rule = MD019NoMultipleSpaceAtx::new();

        // Test with heading that has multiple spaces
        let content = "#  Multiple Spaces\n\nRegular content\n\n##   More Spaces";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should flag both headings
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 5);

        // Test with proper headings
        let content = "# Single Space\n\n## Also correct";
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Properly formatted headings should not generate warnings"
        );
    }
}
