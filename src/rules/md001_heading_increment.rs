use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::heading_utils::HeadingUtils;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use crate::HeadingStyle;

/// Rule MD001: Heading levels should only increment by one level at a time
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
#[derive(Debug, Default)]
pub struct MD001HeadingIncrement;

impl Rule for MD001HeadingIncrement {
    fn name(&self) -> &'static str {
        "MD001"
    }

    fn description(&self) -> &'static str {
        "Heading levels should only increment by one level at a time"
    }

    fn check(&self, content: &str) -> LintResult {
        // Early return for empty content
        if content.is_empty() {
            return Ok(vec![]);
        }

        let structure = DocumentStructure::new(content);
        if structure.heading_lines.is_empty() {
            return Ok(vec![]);
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let mut prev_level = 0;
        let lines: Vec<&str> = content.lines().collect();

        // Process headings using pre-computed heading information
        for i in 0..structure.heading_lines.len() {
            let line_num = structure.heading_lines[i];
            let level = structure.heading_levels[i];

            // Check if this heading level is more than one level deeper than the previous
            if prev_level > 0 && level > prev_level + 1 {
                let adjusted_line_num = line_num - 1; // Convert 1-indexed to 0-indexed
                let indentation = if adjusted_line_num < lines.len() {
                    HeadingUtils::get_indentation(lines[adjusted_line_num])
                } else {
                    0
                };

                // Get the heading text
                let heading_text = if adjusted_line_num < lines.len() {
                    lines[adjusted_line_num]
                        .trim_start()
                        .trim_start_matches('#')
                        .trim()
                        .to_string()
                } else {
                    String::new()
                };

                // Determine heading style
                let style = if adjusted_line_num + 1 < lines.len()
                    && (lines[adjusted_line_num + 1].trim().starts_with('=')
                        || lines[adjusted_line_num + 1].trim().starts_with('-'))
                {
                    if lines[adjusted_line_num + 1].trim().starts_with('=') {
                        HeadingStyle::Setext1
                    } else {
                        HeadingStyle::Setext2
                    }
                } else {
                    HeadingStyle::Atx
                };

                // Create a fix with the correct heading level
                let fixed_level = prev_level + 1;
                let replacement =
                    HeadingUtils::convert_heading_style(&heading_text, fixed_level as u32, style);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: line_num,
                    column: indentation + 1,
                    message: format!("Heading level should be {} for this level", prev_level + 1),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line_num, indentation + 1),
                        replacement: format!("{}{}", " ".repeat(indentation), replacement),
                    }),
                });
            }

            prev_level = level;
        }

        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return for empty content or if no headings exist
        if content.is_empty() || structure.heading_lines.is_empty() {
            return Ok(vec![]);
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let mut prev_level = 0;
        let lines: Vec<&str> = content.lines().collect();

        // Process headings using pre-computed heading information
        for i in 0..structure.heading_lines.len() {
            let line_num = structure.heading_lines[i];
            let level = structure.heading_levels[i];

            // Check if this heading level is more than one level deeper than the previous
            if prev_level > 0 && level > prev_level + 1 {
                let adjusted_line_num = line_num - 1; // Convert 1-indexed to 0-indexed
                let indentation = if adjusted_line_num < lines.len() {
                    HeadingUtils::get_indentation(lines[adjusted_line_num])
                } else {
                    0
                };

                // Get the heading text
                let heading_text = if adjusted_line_num < lines.len() {
                    lines[adjusted_line_num]
                        .trim_start()
                        .trim_start_matches('#')
                        .trim()
                        .to_string()
                } else {
                    String::new()
                };

                // Determine heading style
                let style = if adjusted_line_num + 1 < lines.len()
                    && (lines[adjusted_line_num + 1].trim().starts_with('=')
                        || lines[adjusted_line_num + 1].trim().starts_with('-'))
                {
                    if lines[adjusted_line_num + 1].trim().starts_with('=') {
                        HeadingStyle::Setext1
                    } else {
                        HeadingStyle::Setext2
                    }
                } else {
                    HeadingStyle::Atx
                };

                // Create a fix with the correct heading level
                let fixed_level = prev_level + 1;
                let replacement =
                    HeadingUtils::convert_heading_style(&heading_text, fixed_level as u32, style);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: line_num,
                    column: indentation + 1,
                    message: format!("Heading level should be {} for this level", prev_level + 1),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line_num, indentation + 1),
                        replacement: format!("{}{}", " ".repeat(indentation), replacement),
                    }),
                });
            }

            prev_level = level;
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut fixed_lines = Vec::new();
        let mut prev_level = 0;
        let lines: Vec<&str> = content.lines().collect();
        let ends_with_newline = content.ends_with('\n');

        let structure = DocumentStructure::new(content);

        let mut i = 0;
        while i < lines.len() {
            // Check if this line is a heading (by 1-indexed line number)
            if let Some(idx) = structure.heading_lines.iter().position(|&ln| ln == i + 1) {
                let level = structure.heading_levels[idx];
                let region = structure.heading_regions[idx];
                let start = region.0 - 1; // 0-indexed
                let end = region.1 - 1; // 0-indexed
                let indentation = HeadingUtils::get_indentation(lines[start]);
                let is_setext = start != end;

                // Determine style
                let style = if is_setext {
                    if lines.get(end).map_or("", |l| l.trim()).starts_with('=') {
                        HeadingStyle::Setext1
                    } else {
                        HeadingStyle::Setext2
                    }
                } else {
                    HeadingStyle::Atx
                };

                // Check if we need to fix the heading level
                if level > prev_level + 1 {
                    let fixed_level = prev_level + 1;
                    let text = lines[start].trim_start_matches('#').trim();
                    let replacement =
                        HeadingUtils::convert_heading_style(text, fixed_level as u32, style);
                    fixed_lines.push(format!("{}{}", " ".repeat(indentation), replacement));
                    prev_level = fixed_level;
                } else {
                    // No fix needed, keep original
                    fixed_lines.extend(lines[start..=end].iter().map(|l| l.to_string()));
                    prev_level = level;
                }

                i = end + 1;
            } else {
                // Not a heading, keep as is
                fixed_lines.push(lines[i].to_string());
                i += 1;
            }
        }

        let mut result = fixed_lines.join("\n");
        if ends_with_newline {
            result.push('\n');
        }
        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::Heading
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty() || !content.contains('#')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl DocumentStructureExtensions for MD001HeadingIncrement {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // This rule is only relevant if there are at least two headings
        doc_structure.heading_lines.len() >= 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        let rule = MD001HeadingIncrement;

        // Test with valid headings
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());

        // Test with invalid headings
        let content = "# Heading 1\n### Heading 3\n#### Heading 4";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }
}
