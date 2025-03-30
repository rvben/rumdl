use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::heading_utils::HeadingUtils;
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
        let _line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let mut prev_level = 0;

        let lines: Vec<&str> = content.lines().collect();

        for (line_num, _) in lines.iter().enumerate() {
            if let Some(heading) = HeadingUtils::parse_heading(content, line_num + 1) {
                if prev_level > 0 && heading.level > prev_level + 1 {
                    let indentation = HeadingUtils::get_indentation(lines[line_num]);
                    let mut fixed_heading = heading.clone();
                    fixed_heading.level = prev_level + 1;
                    let replacement = HeadingUtils::convert_heading_style(&heading.text, fixed_heading.level, heading.style);
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: indentation + 1,
                        severity: Severity::Warning,
                        message: format!(
                            "Heading level should be {} for this level",
                            prev_level + 1
                        ),
                        fix: Some(Fix {
                            range: _line_index
                                .line_col_to_byte_range(line_num + 1, indentation + 1),
                            replacement: format!("{}{}", " ".repeat(indentation), replacement),
                        }),
                    });
                }
                prev_level = heading.level;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut fixed_lines = Vec::new();
        let mut prev_level = 0;
        let mut i = 0;
        let lines: Vec<&str> = content.lines().collect();
        let ends_with_newline = content.ends_with('\n');

        while i < lines.len() {
            if let Some(heading) = HeadingUtils::parse_heading(content, i + 1) {
                let indentation = HeadingUtils::get_indentation(lines[i]);
                let mut fixed_heading = heading.clone();
                
                // Only increment if the level is greater than the previous level
                if heading.level > prev_level + 1 {
                    fixed_heading.level = prev_level + 1;
                    let replacement = HeadingUtils::convert_heading_style(&heading.text, fixed_heading.level, heading.style);
                    fixed_lines.push(format!("{}{}", " ".repeat(indentation), replacement));
                } else {
                    fixed_lines.push(lines[i].to_string());
                }
                
                prev_level = if heading.level > prev_level + 1 {
                    prev_level + 1
                } else {
                    heading.level
                };

                if matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2) {
                    if i + 1 < lines.len() {
                        fixed_lines.push(lines[i + 1].to_string());
                    }
                    i += 2; // Skip the underline line
                } else {
                    i += 1;
                }
            } else {
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
}
