use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::heading_utils::HeadingStyle;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use lazy_static::lazy_static;
use regex::Regex;
use std::ops::Range;

lazy_static! {
    static ref HEADING_PATTERN: Regex = Regex::new(r"^(\s*)(#{1,6})\s+(.+?)(?:\s+#*)?$").unwrap();
    static ref SETEXT_HEADING_1: Regex = Regex::new(r"^(\s*)=+\s*$").unwrap();
    static ref SETEXT_HEADING_2: Regex = Regex::new(r"^(\s*)-+\s*$").unwrap();
    static ref FRONT_MATTER: Regex = Regex::new(r"(?m)^---\s*$").unwrap();
}

/// Rule MD002: First heading should be a top-level heading
///
/// This rule enforces that the first heading in a document is a top-level heading (typically h1),
/// which establishes the main topic or title of the document.
///
/// ## Purpose
///
/// - **Document Structure**: Ensures proper document hierarchy with a single top-level heading
/// - **Accessibility**: Improves screen reader navigation by providing a clear document title
/// - **SEO**: Helps search engines identify the primary topic of the document
/// - **Readability**: Provides users with a clear understanding of the document's main subject
///
/// ## Configuration Options
///
/// The rule supports customizing the required level for the first heading:
///
/// ```yaml
/// MD002:
///   level: 1  # The heading level required for the first heading (default: 1)
/// ```
///
/// Setting `level: 2` would require the first heading to be an h2 instead of h1.
///
/// ## Examples
///
/// ### Correct (with default configuration)
///
/// ```markdown
/// # Document Title
///
/// ## Section 1
///
/// Content here...
///
/// ## Section 2
///
/// More content...
/// ```
///
/// ### Incorrect (with default configuration)
///
/// ```markdown
/// ## Introduction
///
/// Content here...
///
/// # Main Title
///
/// More content...
/// ```
///
/// ## Behavior
///
/// This rule:
/// - Ignores front matter (YAML metadata at the beginning of the document)
/// - Works with both ATX (`#`) and Setext (underlined) heading styles
/// - Only examines the first heading it encounters
/// - Does not apply to documents with no headings
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Changes the level of the first heading to match the configured level
/// - Preserves the original heading style (ATX, closed ATX, or Setext)
/// - Maintains indentation and other formatting
///
/// ## Rationale
///
/// Having a single top-level heading establishes the document's primary topic and creates
/// a logical structure. This follows semantic HTML principles where each page should have
/// a single `<h1>` element that defines its main subject.
///
#[derive(Debug)]
pub struct MD002FirstHeadingH1 {
    level: u32,
}

impl Default for MD002FirstHeadingH1 {
    fn default() -> Self {
        Self { level: 1 }
    }
}

impl MD002FirstHeadingH1 {
    pub fn new(level: u32) -> Self {
        Self { level }
    }

    fn skip_front_matter(&self, content: &str) -> usize {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() || !lines[0].trim_end().eq("---") {
            return 0;
        }

        for (i, line) in lines.iter().enumerate().skip(1) {
            if line.trim_end().eq("---") {
                return i + 1;
            }
        }
        0
    }

    fn parse_heading(
        &self,
        content: &str,
        line_number: usize,
    ) -> Option<(String, String, u32, HeadingStyle)> {
        let lines: Vec<&str> = content.lines().collect();
        if line_number == 0 || line_number > lines.len() {
            return None;
        }

        let line = lines[line_number - 1];

        // Skip if line is within a code block
        if self.is_in_code_block(content, line_number) {
            return None;
        }

        // Check for ATX style headings
        if let Some(captures) = HEADING_PATTERN.captures(line) {
            let indent = captures.get(1).map_or("", |m| m.as_str());
            let level = captures.get(2).map_or(0, |m| m.as_str().len()) as u32;
            let text = captures.get(3).map_or("", |m| m.as_str());
            let style = if line.trim_end().ends_with('#') {
                HeadingStyle::AtxClosed
            } else {
                HeadingStyle::Atx
            };
            return Some((indent.to_string(), text.to_string(), level, style));
        }

        // Check for Setext style headings
        if line_number < lines.len() {
            let next_line = lines[line_number];
            if !next_line.trim().is_empty() {
                if let Some(captures) = SETEXT_HEADING_1.captures(next_line) {
                    let indent = captures.get(1).map_or("", |m| m.as_str());
                    return Some((
                        indent.to_string(),
                        line.trim().to_string(),
                        1,
                        HeadingStyle::Setext1,
                    ));
                } else if let Some(captures) = SETEXT_HEADING_2.captures(next_line) {
                    let indent = captures.get(1).map_or("", |m| m.as_str());
                    return Some((
                        indent.to_string(),
                        line.trim().to_string(),
                        2,
                        HeadingStyle::Setext2,
                    ));
                }
            }
        }

        None
    }

    fn is_in_code_block(&self, content: &str, line_number: usize) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        let mut fence_char = None;

        for (i, line) in lines.iter().enumerate() {
            if i >= line_number {
                break;
            }

            let trimmed = line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if !in_code_block {
                    fence_char = Some(&trimmed[..3]);
                    in_code_block = true;
                } else if Some(&trimmed[..3]) == fence_char {
                    in_code_block = false;
                }
            }
        }

        in_code_block
    }
}

impl Rule for MD002FirstHeadingH1 {
    fn name(&self) -> &'static str {
        "MD002"
    }

    fn description(&self) -> &'static str {
        "First heading should be top level"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut result = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let start_line = self.skip_front_matter(content);

        for (i, _) in lines.iter().enumerate().skip(start_line) {
            if let Some((_, _, level, _)) = self.parse_heading(content, i + 1) {
                if level != self.level {
                    result.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: i + 1,
                        column: 1,
                        message: format!(
                            "First heading should be level {}, found level {}",
                            self.level, level
                        ),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: Range {
                                start: i + 1,
                                end: i + 1,
                            },
                            replacement: format!(
                                "{}{}",
                                "#".repeat(self.level as usize),
                                " ".repeat(i + 1 - start_line)
                            ),
                        }),
                    });
                }
                break;
            }
        }

        Ok(result)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if no headings
        if structure.heading_lines.is_empty() {
            return Ok(vec![]);
        }

        let mut result = Vec::new();

        // Get the first heading
        let first_heading_line = structure.heading_lines[0];
        let first_heading_level = structure.heading_levels[0];

        // Check if the level matches the required level
        if first_heading_level as u32 != self.level {
            // Get the line from the content
            let line_idx = first_heading_line - 1; // Convert 1-indexed to 0-indexed

            let lines: Vec<&str> = content.lines().collect();
            let line = if line_idx < lines.len() {
                lines[line_idx]
            } else {
                return Ok(vec![]); // Error condition, shouldn't happen
            };

            // Determine heading style
            let _style = if line_idx + 1 < lines.len()
                && (lines[line_idx + 1].trim().starts_with('=')
                    || lines[line_idx + 1].trim().starts_with('-'))
            {
                if lines[line_idx + 1].trim().starts_with('=') {
                    HeadingStyle::Setext1
                } else {
                    HeadingStyle::Setext2
                }
            } else if line.trim_end().ends_with('#') {
                HeadingStyle::AtxClosed
            } else {
                HeadingStyle::Atx
            };

            result.push(LintWarning {
                rule_name: Some(self.name()),
                line: first_heading_line,
                column: 1,
                message: format!(
                    "First heading should be level {}, found level {}",
                    self.level, first_heading_level
                ),
                severity: Severity::Warning,
                fix: Some(Fix {
                    range: Range {
                        start: first_heading_line,
                        end: first_heading_line,
                    },
                    replacement: format!(
                        "{}{}",
                        "#".repeat(self.level as usize),
                        " ".repeat(first_heading_line - 1)
                    ),
                }),
            });
        }

        Ok(result)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut _i = 0;
        let lines: Vec<&str> = content.lines().collect();
        let mut first_heading_fixed = false;
        let start_line = self.skip_front_matter(content);

        // Copy front matter if present
        for line in lines.iter().take(start_line) {
            result.push_str(line);
            result.push('\n');
        }
        _i = start_line;

        while _i < lines.len() {
            if !first_heading_fixed {
                if let Some((indent, text, level, style)) = self.parse_heading(content, _i + 1) {
                    if level != self.level {
                        let fixed = match style {
                            HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                                format!("{}{} {}", indent, "#".repeat(self.level as usize), text)
                            }
                            HeadingStyle::AtxClosed => {
                                format!(
                                    "{}{} {} {}",
                                    indent,
                                    "#".repeat(self.level as usize),
                                    text,
                                    "#".repeat(self.level as usize)
                                )
                            }
                            _ => {
                                format!("{}{} {}", indent, "#".repeat(self.level as usize), text)
                            }
                        };
                        result.push_str(&fixed);
                        if style == HeadingStyle::Setext1 || style == HeadingStyle::Setext2 {
                            _i += 1; // Skip the underline line
                        }
                    } else {
                        result.push_str(lines[_i]);
                    }
                    first_heading_fixed = true;
                } else {
                    result.push_str(lines[_i]);
                }
            } else {
                result.push_str(lines[_i]);
            }

            // Add newline if not at the end of the file
            if _i < lines.len() - 1 {
                result.push('\n');
            }
            _i += 1;
        }

        // Preserve final newline if present in original
        if content.ends_with('\n') {
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
        content.is_empty()
            || (!content.contains('#') && !content.contains('=') && !content.contains('-'))
    }
}

impl DocumentStructureExtensions for MD002FirstHeadingH1 {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // Rule is only relevant if there are headings
        !doc_structure.heading_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        let rule = MD002FirstHeadingH1::default();

        // Test with correct heading level
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());

        // Test with incorrect heading level
        let content = "## Heading 2\n### Heading 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
    }
}
