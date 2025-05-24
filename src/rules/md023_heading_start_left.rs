/// Rule MD023: Headings must start at the left margin
///
/// See [docs/md023.md](../../docs/md023.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::markdown_elements::{ElementType, MarkdownElements};
use crate::utils::range_utils::LineIndex;

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
        let content = ctx.content;
        // Early return for empty content
        if content.is_empty() {
            return Ok(vec![]);
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Get all headings using the MarkdownElements utility
        let headings = MarkdownElements::detect_headings(content);

        for heading in headings {
            if heading.element_type != ElementType::Heading {
                continue;
            }

            // Get the line at this position
            let start_line = heading.start_line;
            if start_line >= lines.len() {
                continue; // Safety check
            }

            let line = lines[start_line];
            let indentation = line.len() - line.trim_start().len();

            // If the heading is indented, add a warning
            if indentation > 0 {
                // Determine if it's an ATX or Setext heading
                let is_setext = heading.end_line > heading.start_line;
                let level = if let Some(level_str) = &heading.metadata {
                    level_str.parse::<u32>().unwrap_or(1)
                } else {
                    1 // Default to level 1 if not specified
                };

                if is_setext {
                    // For Setext headings, we need to fix both the heading text and underline
                    let heading_text = lines[start_line].trim();
                    let underline_line = start_line + 1;

                    if underline_line < lines.len() {
                        let underline_text = lines[underline_line].trim();

                        // Add warning for the heading text line
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line + 1, // Convert to 1-indexed
                            column: 1,
                            severity: Severity::Warning,
                            message: format!(
                                "Setext heading should not be indented by {} spaces",
                                indentation
                            ),
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(start_line + 1, 1),
                                replacement: heading_text.to_string(),
                            }),
                        });

                        // Add warning for the underline - only if it's indented
                        let underline_indentation =
                            lines[underline_line].len() - lines[underline_line].trim_start().len();
                        if underline_indentation > 0 {
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                line: underline_line + 1, // Convert to 1-indexed
                                column: 1,
                                severity: Severity::Warning,
                                message: "Setext heading underline should not be indented"
                                    .to_string(),
                                fix: Some(Fix {
                                    range: line_index.line_col_to_byte_range(underline_line + 1, 1),
                                    replacement: underline_text.to_string(),
                                }),
                            });
                        }
                    }
                } else {
                    // For ATX headings, just fix the single line
                    let is_closed_atx = line.trim().ends_with('#');
                    let heading_content = if heading.text.trim().is_empty() {
                        String::new() // Empty heading
                    } else {
                        format!(" {}", heading.text.trim())
                    };

                    // Create a fixed version without indentation
                    let fixed_heading = if is_closed_atx {
                        if heading_content.trim().is_empty() {
                            format!(
                                "{} {}",
                                "#".repeat(level as usize),
                                "#".repeat(level as usize)
                            )
                        } else {
                            format!(
                                "{}{} {}",
                                "#".repeat(level as usize),
                                heading_content,
                                "#".repeat(level as usize)
                            )
                        }
                    } else {
                        format!("{}{}", "#".repeat(level as usize), heading_content)
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: start_line + 1, // Convert to 1-indexed
                        column: 1,
                        severity: Severity::Warning,
                        message: format!(
                            "Heading should not be indented by {} spaces",
                            indentation
                        ),
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(start_line + 1, 1),
                            replacement: fixed_heading,
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;
        let mut fixed_lines = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        // Get all headings using the MarkdownElements utility
        let headings = MarkdownElements::detect_headings(content);

        // Create a map of line number to heading
        let mut heading_map = std::collections::HashMap::new();
        for heading in headings {
            if heading.element_type == ElementType::Heading {
                heading_map.insert(heading.start_line, heading);
            }
        }

        while i < lines.len() {
            // Check if this line is part of a heading
            if let Some(heading) = heading_map.get(&i) {
                let indentation = lines[i].len() - lines[i].trim_start().len();
                let is_setext = heading.end_line > heading.start_line;

                if indentation > 0 {
                    // This heading needs to be fixed
                    if is_setext {
                        // For Setext headings, add the heading text without indentation
                        fixed_lines.push(lines[i].trim().to_string());
                        // Then add the underline without indentation
                        if i + 1 < lines.len() {
                            fixed_lines.push(lines[i + 1].trim().to_string());
                        }
                        i += 2; // Skip both heading and underline
                    } else {
                        // For ATX headings, determine if it's closed
                        let is_closed_atx = lines[i].trim().ends_with('#');

                        // Get the heading level
                        let level = if let Some(level_str) = &heading.metadata {
                            level_str.parse::<u32>().unwrap_or(1)
                        } else {
                            1 // Default to level 1 if not specified
                        };

                        // Get heading content, handling empty headings
                        let heading_content = if heading.text.trim().is_empty() {
                            String::new() // Empty heading
                        } else {
                            format!(" {}", heading.text.trim())
                        };

                        // Create a fixed version without indentation
                        let fixed_heading = if is_closed_atx {
                            if heading_content.trim().is_empty() {
                                format!(
                                    "{} {}",
                                    "#".repeat(level as usize),
                                    "#".repeat(level as usize)
                                )
                            } else {
                                format!(
                                    "{}{} {}",
                                    "#".repeat(level as usize),
                                    heading_content,
                                    "#".repeat(level as usize)
                                )
                            }
                        } else {
                            format!("{}{}", "#".repeat(level as usize), heading_content)
                        };

                        fixed_lines.push(fixed_heading);
                        i += 1;
                    }
                } else {
                    // This heading is already at the beginning of the line
                    fixed_lines.push(lines[i].to_string());
                    if is_setext && i + 1 < lines.len() {
                        fixed_lines.push(lines[i + 1].to_string());
                        i += 2; // Skip both heading and underline
                    } else {
                        i += 1;
                    }
                }
            } else {
                // Not a heading, copy as-is
                fixed_lines.push(lines[i].to_string());
                i += 1;
            }
        }

        let result = fixed_lines.join("\n");
        if content.ends_with('\n') {
            Ok(result + "\n")
        } else {
            Ok(result)
        }
    }

    /// Optimized check using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
        // Early return if no headings
        if structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(ctx.content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = ctx.content.lines().collect();

        // Process only heading lines using structure.heading_lines
        for &line_num in &structure.heading_lines {
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];
            let indentation = line.len() - line.trim_start().len();

            // If the heading is indented, add a warning
            if indentation > 0 {
                // Determine if it's an ATX or Setext heading
                let is_setext = line_idx + 1 < lines.len()
                    && (lines[line_idx + 1].trim().starts_with('=')
                        || lines[line_idx + 1].trim().starts_with('-'));

                // Find the heading level from the structure
                let level_idx = structure
                    .heading_lines
                    .iter()
                    .position(|&l| l == line_num)
                    .unwrap_or(0);
                let level = structure.heading_levels.get(level_idx).unwrap_or(&1);

                if is_setext {
                    // For Setext headings, we need to fix both the heading text and underline
                    let heading_text = line.trim();
                    let underline_line_idx = line_idx + 1;

                    if underline_line_idx < lines.len() {
                        let underline_text = lines[underline_line_idx].trim();

                        // Add warning for the heading text line
                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: line_num, // Already 1-indexed from structure
                            column: 1,
                            severity: Severity::Warning,
                            message: format!(
                                "Setext heading should not be indented by {} spaces",
                                indentation
                            ),
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range(line_num, 1),
                                replacement: heading_text.to_string(),
                            }),
                        });

                        // Add warning for the underline - only if it's indented
                        let underline_indentation = lines[underline_line_idx].len()
                            - lines[underline_line_idx].trim_start().len();
                        if underline_indentation > 0 {
                            warnings.push(LintWarning {
                                rule_name: Some(self.name()),
                                line: underline_line_idx + 1, // Convert to 1-indexed
                                column: 1,
                                severity: Severity::Warning,
                                message: "Setext heading underline should not be indented"
                                    .to_string(),
                                fix: Some(Fix {
                                    range: line_index
                                        .line_col_to_byte_range(underline_line_idx + 1, 1),
                                    replacement: underline_text.to_string(),
                                }),
                            });
                        }
                    }
                } else {
                    // For ATX headings, just fix the single line
                    let is_closed_atx = line.trim().ends_with('#');
                    let heading_text = line.trim();

                    // Extract the heading content by removing the hashes
                    let mut content_start = 0;
                    while content_start < heading_text.len()
                        && heading_text.chars().nth(content_start) == Some('#')
                    {
                        content_start += 1;
                    }

                    let heading_content = if content_start < heading_text.len() {
                        heading_text[content_start..].trim().to_string()
                    } else {
                        String::new() // Empty heading
                    };

                    // Create a fixed version without indentation
                    let fixed_heading = if is_closed_atx {
                        if heading_content.trim().is_empty() {
                            format!("{} {}", "#".repeat(*level), "#".repeat(*level))
                        } else {
                            format!(
                                "{} {} {}",
                                "#".repeat(*level),
                                heading_content.trim(),
                                "#".repeat(*level)
                            )
                        }
                    } else if heading_content.trim().is_empty() {
                        "#".repeat(*level).to_string()
                    } else {
                        format!("{} {}", "#".repeat(*level), heading_content.trim())
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: line_num, // Already 1-indexed from structure
                        column: 1,
                        severity: Severity::Warning,
                        message: format!(
                            "Heading should not be indented by {} spaces",
                            indentation
                        ),
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line_num, 1),
                            replacement: fixed_heading,
                        }),
                    });
                }
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
        Box::new(MD023HeadingStartLeft)
    }
}

impl DocumentStructureExtensions for MD023HeadingStartLeft {
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
    use crate::utils::document_structure::DocumentStructure;

    #[test]
    fn test_with_document_structure() {
        let rule = MD023HeadingStartLeft;

        // Test with properly aligned headings
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert!(result.is_empty());

        // Test with indented headings
        let content = "  # Heading 1\n ## Heading 2\n   ### Heading 3";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(result.len(), 3); // Should flag all three indented headings
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 2);
        assert_eq!(result[2].line, 3);

        // Test with setext headings
        let content = "Heading 1\n=========\n  Heading 2\n  ---------";
        let structure = DocumentStructure::new(content);
        let ctx = LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should flag the indented heading and underline
        assert_eq!(result[0].line, 3);
        assert_eq!(result[1].line, 4);
    }
}
