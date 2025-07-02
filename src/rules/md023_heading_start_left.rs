/// Rule MD023: Headings must start at the left margin
///
/// See [docs/md023.md](../../docs/md023.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::{LineIndex, calculate_single_line_range};

#[derive(Clone)]
pub struct MD023HeadingStartLeft;

impl Rule for MD023HeadingStartLeft {
    fn name(&self) -> &'static str {
        "MD023"
    }

    fn description(&self) -> &'static str {
        "Headings must start at the beginning of the line"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Early return for empty content
        if ctx.lines.is_empty() {
            return Ok(vec![]);
        }

        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();

        // Process all headings using cached heading information
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                let indentation = line_info.indent;

                // If the heading is indented, add a warning
                if indentation > 0 {
                    let is_setext = matches!(
                        heading.style,
                        crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
                    );

                    if is_setext {
                        // For Setext headings, we need to fix both the heading text and underline
                        let underline_line = line_num + 1;

                        // Calculate precise character range for the indentation
                        let (start_line_calc, start_col, end_line, end_col) = calculate_single_line_range(
                            line_num + 1, // Convert to 1-indexed
                            1,
                            indentation,
                        );

                        // Add warning for the heading text line
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line_calc,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            message: format!("Setext heading should not be indented by {indentation} spaces"),
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range_with_length(
                                    line_num + 1,
                                    start_col,
                                    indentation,
                                ),
                                replacement: String::new(), // Remove the indentation
                            }),
                        });

                        // Add warning for the underline - only if it's indented
                        if underline_line < ctx.lines.len() {
                            let underline_indentation = ctx.lines[underline_line].indent;
                            if underline_indentation > 0 {
                                // Calculate precise character range for the underline indentation
                                let (underline_start_line, underline_start_col, underline_end_line, underline_end_col) =
                                    calculate_single_line_range(
                                        underline_line + 1, // Convert to 1-indexed
                                        1,
                                        underline_indentation,
                                    );

                                warnings.push(LintWarning {
                                    rule_name: Some(self.name()),
                                    line: underline_start_line,
                                    column: underline_start_col,
                                    end_line: underline_end_line,
                                    end_column: underline_end_col,
                                    severity: Severity::Warning,
                                    message: "Setext heading underline should not be indented".to_string(),
                                    fix: Some(Fix {
                                        range: line_index.line_col_to_byte_range_with_length(
                                            underline_line + 1,
                                            underline_start_col,
                                            underline_indentation,
                                        ),
                                        replacement: String::new(), // Remove the indentation
                                    }),
                                });
                            }
                        }
                    } else {
                        // For ATX headings, just fix the single line

                        // Calculate precise character range for the indentation
                        let (atx_start_line, atx_start_col, atx_end_line, atx_end_col) = calculate_single_line_range(
                            line_num + 1, // Convert to 1-indexed
                            1,
                            indentation,
                        );

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: atx_start_line,
                            column: atx_start_col,
                            end_line: atx_end_line,
                            end_column: atx_end_col,
                            severity: Severity::Warning,
                            message: format!("Heading should not be indented by {indentation} spaces"),
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range_with_length(
                                    line_num + 1,
                                    atx_start_col,
                                    indentation,
                                ),
                                replacement: String::new(), // Remove the indentation
                            }),
                        });
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut fixed_lines = Vec::new();
        let mut skip_next = false;

        for (i, line_info) in ctx.lines.iter().enumerate() {
            if skip_next {
                skip_next = false;
                continue;
            }

            // Check if this line is a heading
            if let Some(heading) = &line_info.heading {
                let indentation = line_info.indent;
                let is_setext = matches!(
                    heading.style,
                    crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
                );

                if indentation > 0 {
                    // This heading needs to be fixed
                    if is_setext {
                        // For Setext headings, add the heading text without indentation
                        fixed_lines.push(line_info.content.trim().to_string());
                        // Then add the underline without indentation
                        if i + 1 < ctx.lines.len() {
                            fixed_lines.push(ctx.lines[i + 1].content.trim().to_string());
                            skip_next = true;
                        }
                    } else {
                        // For ATX headings, simply trim the indentation
                        fixed_lines.push(line_info.content.trim_start().to_string());
                    }
                } else {
                    // This heading is already at the beginning of the line
                    fixed_lines.push(line_info.content.clone());
                    if is_setext && i + 1 < ctx.lines.len() {
                        fixed_lines.push(ctx.lines[i + 1].content.clone());
                        skip_next = true;
                    }
                }
            } else {
                // Not a heading, copy as-is
                fixed_lines.push(line_info.content.clone());
            }
        }

        let result = fixed_lines.join("\n");
        if ctx.content.ends_with('\n') {
            Ok(result + "\n")
        } else {
            Ok(result)
        }
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.lines.iter().all(|line| line.heading.is_none())
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
        Box::new(MD023HeadingStartLeft)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    #[test]
    fn test_basic_functionality() {
        let rule = MD023HeadingStartLeft;

        // Test with properly aligned headings
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with indented headings
        let content = "  # Heading 1\n ## Heading 2\n   ### Heading 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3); // Should flag all three indented headings
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
        assert_eq!(result[2].line, 3);

        // Test with setext headings
        let content = "Heading 1\n=========\n  Heading 2\n  ---------";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should flag the indented heading and underline
        assert_eq!(result[0].line, 3);
        assert_eq!(result[1].line, 4);
    }
}
