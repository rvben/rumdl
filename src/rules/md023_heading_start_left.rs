/// Rule MD023: Headings must start at the left margin
///
/// See [docs/md023.md](../../docs/md023.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_single_line_range;

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

        let mut warnings = Vec::new();

        // Process all headings using cached heading information
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Skip invalid headings (e.g., `#NoSpace` which lacks required space after #)
                if !heading.is_valid {
                    continue;
                }

                // Skip hashtag-like patterns (e.g., #tag, #123, #29039) for ATX level 1
                // These are likely issue refs or social hashtags, not intended headings
                if heading.level == 1 && matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    // Get first "word" of heading text (up to space, comma, or closing paren)
                    let first_word: String = heading
                        .text
                        .trim()
                        .chars()
                        .take_while(|c| !c.is_whitespace() && *c != ',' && *c != ')')
                        .collect();
                    if let Some(first_char) = first_word.chars().next() {
                        // Skip if first word starts with lowercase or number
                        if first_char.is_lowercase() || first_char.is_numeric() {
                            continue;
                        }
                    }
                }

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
                            rule_name: Some(self.name().to_string()),
                            line: start_line_calc,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: Severity::Warning,
                            message: format!("Setext heading should not be indented by {indentation} spaces"),
                            fix: Some(Fix {
                                range: ctx.line_index.line_col_to_byte_range_with_length(
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
                                    rule_name: Some(self.name().to_string()),
                                    line: underline_start_line,
                                    column: underline_start_col,
                                    end_line: underline_end_line,
                                    end_column: underline_end_col,
                                    severity: Severity::Warning,
                                    message: "Setext heading underline should not be indented".to_string(),
                                    fix: Some(Fix {
                                        range: ctx.line_index.line_col_to_byte_range_with_length(
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
                            rule_name: Some(self.name().to_string()),
                            line: atx_start_line,
                            column: atx_start_col,
                            end_line: atx_end_line,
                            end_column: atx_end_col,
                            severity: Severity::Warning,
                            message: format!("Heading should not be indented by {indentation} spaces"),
                            fix: Some(Fix {
                                range: ctx.line_index.line_col_to_byte_range_with_length(
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
                // Skip invalid headings (e.g., `#NoSpace` which lacks required space after #)
                if !heading.is_valid {
                    fixed_lines.push(line_info.content(ctx.content).to_string());
                    continue;
                }

                let indentation = line_info.indent;
                let is_setext = matches!(
                    heading.style,
                    crate::lint_context::HeadingStyle::Setext1 | crate::lint_context::HeadingStyle::Setext2
                );

                if indentation > 0 {
                    // This heading needs to be fixed
                    if is_setext {
                        // For Setext headings, add the heading text without indentation
                        fixed_lines.push(line_info.content(ctx.content).trim().to_string());
                        // Then add the underline without indentation
                        if i + 1 < ctx.lines.len() {
                            fixed_lines.push(ctx.lines[i + 1].content(ctx.content).trim().to_string());
                            skip_next = true;
                        }
                    } else {
                        // For ATX headings, simply trim the indentation
                        fixed_lines.push(line_info.content(ctx.content).trim_start().to_string());
                    }
                } else {
                    // This heading is already at the beginning of the line
                    fixed_lines.push(line_info.content(ctx.content).to_string());
                    if is_setext && i + 1 < ctx.lines.len() {
                        fixed_lines.push(ctx.lines[i + 1].content(ctx.content).to_string());
                        skip_next = true;
                    }
                }
            } else {
                // Not a heading, copy as-is
                fixed_lines.push(line_info.content(ctx.content).to_string());
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
        // Fast path: check if document likely has headings
        if !ctx.likely_has_headings() {
            return true;
        }
        // Verify headings actually exist
        ctx.lines.iter().all(|line| line.heading.is_none())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with indented headings
        let content = "  # Heading 1\n ## Heading 2\n   ### Heading 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3); // Should flag all three indented headings
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
        assert_eq!(result[2].line, 3);

        // Test with setext headings
        let content = "Heading 1\n=========\n  Heading 2\n  ---------";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2); // Should flag the indented heading and underline
        assert_eq!(result[0].line, 3);
        assert_eq!(result[1].line, 4);
    }

    #[test]
    fn test_issue_refs_skipped_but_real_headings_caught() {
        let rule = MD023HeadingStartLeft;

        // Issue refs should NOT be flagged (starts with number)
        let content = "- fix: issue\n  #29039)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "#29039) should not be flagged as indented heading. Got: {result:?}"
        );

        // Hashtags should NOT be flagged (starts with lowercase)
        let content = "Some text\n  #hashtag";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "#hashtag should not be flagged as indented heading. Got: {result:?}"
        );

        // But uppercase single-# SHOULD be flagged (likely intended heading)
        let content = "Some text\n  #Summary";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "#Summary SHOULD be flagged as indented heading. Got: {result:?}"
        );

        // Multi-hash patterns SHOULD always be flagged
        let content = "Some text\n  ##introduction";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "##introduction SHOULD be flagged as indented heading. Got: {result:?}"
        );

        // Multi-hash with numbers SHOULD be flagged
        let content = "Some text\n  ##123";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "##123 SHOULD be flagged as indented heading. Got: {result:?}"
        );

        // Properly aligned headings should pass
        let content = "# Summary\n## Details";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Properly aligned headings should pass. Got: {result:?}"
        );
    }
}
