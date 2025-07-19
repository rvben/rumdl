use crate::HeadingStyle;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::heading_utils::HeadingUtils;
use crate::utils::range_utils::{LineIndex, calculate_heading_range};

/// Rule MD001: Heading levels should only increment by one level at a time
///
/// See [docs/md001.md](../../docs/md001.md) for full documentation, configuration, and examples.
///
/// This rule enforces a fundamental principle of document structure: heading levels
/// should increase by exactly one level at a time to maintain a proper document hierarchy.
///
/// ## Purpose
///
/// Proper heading structure creates a logical document outline and improves:
/// - Readability for humans
/// - Accessibility for screen readers
/// - Navigation in rendered documents
/// - Automatic generation of tables of contents
///
/// ## Examples
///
/// ### Correct Heading Structure
/// ```markdown
/// # Heading 1
/// ## Heading 2
/// ### Heading 3
/// ## Another Heading 2
/// ```
///
/// ### Incorrect Heading Structure
/// ```markdown
/// # Heading 1
/// ### Heading 3 (skips level 2)
/// #### Heading 4
/// ```
///
/// ## Behavior
///
/// This rule:
/// - Tracks the heading level throughout the document
/// - Validates that each new heading is at most one level deeper than the previous heading
/// - Allows heading levels to decrease by any amount (e.g., going from ### to #)
/// - Works with both ATX (`#`) and Setext (underlined) heading styles
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Changes the level of non-compliant headings to be one level deeper than the previous heading
/// - Preserves the original heading style (ATX or Setext)
/// - Maintains indentation and other formatting
///
/// ## Rationale
///
/// Skipping heading levels (e.g., from `h1` to `h3`) can confuse readers and screen readers
/// by creating gaps in the document structure. Consistent heading increments create a proper
/// hierarchical outline essential for well-structured documents.
///
#[derive(Debug, Default, Clone)]
pub struct MD001HeadingIncrement;

impl Rule for MD001HeadingIncrement {
    fn name(&self) -> &'static str {
        "MD001"
    }

    fn description(&self) -> &'static str {
        "Heading levels should only increment by one level at a time"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();
        let mut prev_level: Option<usize> = None;

        // Process headings using cached heading information
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                let level = heading.level as usize;

                // Check if this heading level is more than one level deeper than the previous
                if let Some(prev) = prev_level {
                    if level > prev + 1 {
                        let indentation = line_info.indent;
                        let heading_text = &heading.text;

                        // Map heading style
                        let style = match heading.style {
                            crate::lint_context::HeadingStyle::ATX => HeadingStyle::Atx,
                            crate::lint_context::HeadingStyle::Setext1 => HeadingStyle::Setext1,
                            crate::lint_context::HeadingStyle::Setext2 => HeadingStyle::Setext2,
                        };

                        // Create a fix with the correct heading level
                        let fixed_level = prev + 1;
                        let replacement = HeadingUtils::convert_heading_style(heading_text, fixed_level as u32, style);

                        // Calculate precise range: highlight the entire heading
                        let line_content = &line_info.content;
                        let (start_line, start_col, end_line, end_col) =
                            calculate_heading_range(line_num + 1, line_content);

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: format!("Expected heading level {}, but found heading level {}", prev + 1, level),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_content_range(line_num + 1),
                                replacement: format!("{}{}", " ".repeat(indentation), replacement),
                            }),
                        });
                    }
                }

                prev_level = Some(level);
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let mut fixed_lines = Vec::new();
        let mut prev_level: Option<usize> = None;

        for line_info in ctx.lines.iter() {
            if let Some(heading) = &line_info.heading {
                let level = heading.level as usize;
                let mut fixed_level = level;

                // Check if this heading needs fixing
                if let Some(prev) = prev_level {
                    if level > prev + 1 {
                        fixed_level = prev + 1;
                    }
                }

                // Map heading style - when fixing, we may need to change Setext style based on level
                let style = match heading.style {
                    crate::lint_context::HeadingStyle::ATX => HeadingStyle::Atx,
                    crate::lint_context::HeadingStyle::Setext1 => {
                        if fixed_level == 1 {
                            HeadingStyle::Setext1
                        } else {
                            HeadingStyle::Setext2
                        }
                    }
                    crate::lint_context::HeadingStyle::Setext2 => {
                        if fixed_level == 1 {
                            HeadingStyle::Setext1
                        } else {
                            HeadingStyle::Setext2
                        }
                    }
                };

                let replacement = HeadingUtils::convert_heading_style(&heading.text, fixed_level as u32, style);
                fixed_lines.push(format!("{}{}", " ".repeat(line_info.indent), replacement));

                prev_level = Some(fixed_level);
            } else {
                fixed_lines.push(line_info.content.clone());
            }
        }

        let mut result = fixed_lines.join("\n");
        if ctx.content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no headings
        ctx.content.is_empty() || !ctx.lines.iter().any(|line| line.heading.is_some())
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
        Box::new(MD001HeadingIncrement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_basic_functionality() {
        let rule = MD001HeadingIncrement;

        // Test with valid headings
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with invalid headings
        let content = "# Heading 1\n### Heading 3\n#### Heading 4";
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }
}
