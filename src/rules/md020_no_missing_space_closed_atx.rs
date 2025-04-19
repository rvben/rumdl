use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref CLOSED_ATX_NO_SPACE_PATTERN: Regex =
        Regex::new(r"^(\s*)(#+)([^#\s].*?)([^#\s])(#+)\s*$").unwrap();
    static ref CLOSED_ATX_NO_SPACE_START_PATTERN: Regex =
        Regex::new(r"^(\s*)(#+)([^#\s].*?)\s(#+)\s*$").unwrap();
    static ref CLOSED_ATX_NO_SPACE_END_PATTERN: Regex =
        Regex::new(r"^(\s*)(#+)\s(.*?)([^#\s])(#+)\s*$").unwrap();

    // Matches code fence blocks
    static ref CODE_FENCE_PATTERN: Regex =
        Regex::new(r"^(\s*)(`{3,}|~{3,})").unwrap();
}

#[derive(Debug, Default)]
pub struct MD020NoMissingSpaceClosedAtx;

impl MD020NoMissingSpaceClosedAtx {
    pub fn new() -> Self {
        Self
    }

    fn is_closed_atx_heading_without_space(&self, line: &str) -> bool {
        CLOSED_ATX_NO_SPACE_PATTERN.is_match(line)
            || CLOSED_ATX_NO_SPACE_START_PATTERN.is_match(line)
            || CLOSED_ATX_NO_SPACE_END_PATTERN.is_match(line)
    }

    fn fix_closed_atx_heading(&self, line: &str) -> String {
        if let Some(captures) = CLOSED_ATX_NO_SPACE_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let last_char = &captures[4];
            let closing_hashes = &captures[5];
            format!(
                "{}{} {}{} {}",
                indentation, opening_hashes, content, last_char, closing_hashes
            )
        } else if let Some(captures) = CLOSED_ATX_NO_SPACE_START_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let closing_hashes = &captures[4];
            format!(
                "{}{} {} {}",
                indentation, opening_hashes, content, closing_hashes
            )
        } else if let Some(captures) = CLOSED_ATX_NO_SPACE_END_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let last_char = &captures[4];
            let closing_hashes = &captures[5];
            format!(
                "{}{} {}{} {}",
                indentation, opening_hashes, content, last_char, closing_hashes
            )
        } else {
            line.to_string()
        }
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

impl Rule for MD020NoMissingSpaceClosedAtx {
    fn name(&self) -> &'static str {
        "MD020"
    }

    fn description(&self) -> &'static str {
        "No space inside hashes on closed ATX style heading"
    }

    fn check(&self, content: &str) -> LintResult {
        let structure = DocumentStructure::new(content);
        self.check_with_structure(content, &structure)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.is_empty() {
            return Ok(String::new());
        }
        let structure = DocumentStructure::new(content);
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            // Only process heading lines
            let is_heading_line = structure.heading_lines.iter().any(|&ln| ln == i + 1);
            if is_heading_line && self.is_closed_atx_heading_without_space(line) {
                result.push_str(&self.fix_closed_atx_heading(line));
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
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if no headings
        if structure.heading_lines.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Process only heading lines using structure.heading_lines
        for &line_num in &structure.heading_lines {
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];

            // Check if line matches closed ATX pattern without space
            if self.is_closed_atx_heading_without_space(line) {
                let captures = if let Some(c) = CLOSED_ATX_NO_SPACE_PATTERN.captures(line) {
                    c
                } else if let Some(c) = CLOSED_ATX_NO_SPACE_START_PATTERN.captures(line) {
                    c
                } else {
                    CLOSED_ATX_NO_SPACE_END_PATTERN
                        .captures(line)
                        .unwrap_or_else(|| {
                            // This shouldn't happen given the is_closed_atx_heading_without_space check,
                            // but we'll handle it gracefully anyway
                            CLOSED_ATX_NO_SPACE_PATTERN.captures(" # # ").unwrap()
                        })
                };

                let indentation = captures.get(1).unwrap();
                let opening_hashes = captures.get(2).unwrap();
                let line_range = self.get_line_byte_range(content, line_num);

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    message: format!(
                        "Missing space inside hashes on closed ATX style heading with {} hashes",
                        opening_hashes.as_str().len()
                    ),
                    line: line_num,
                    column: indentation.end() + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_range,
                        replacement: self.fix_closed_atx_heading(line),
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
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty() || !content.contains('#')
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl DocumentStructureExtensions for MD020NoMissingSpaceClosedAtx {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // This rule is only relevant if there are headings
        !doc_structure.heading_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        let rule = MD020NoMissingSpaceClosedAtx;

        // Test with correct spacing
        let content = "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());

        // Test with missing spaces
        let content = "# Heading 1#\n## Heading 2 ##\n### Heading 3###";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should flag the two headings with missing spaces
        assert_eq!(result[0].line, 1);
        assert_eq!(result[1].line, 3);
    }
}
