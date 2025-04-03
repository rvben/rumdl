use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Updated regex to handle blockquote markers at the beginning of lines
    // This matches: optional blockquote markers (>), whitespace, list marker, space, and content
    static ref LIST_ITEM_RE: Regex = Regex::new(r"^((?:\s*>\s*)*\s*)([-*+])\s+(.*)$").unwrap();
    static ref CODE_BLOCK_MARKER: Regex = Regex::new(r"^(```|~~~)").unwrap();

    // Regex for finding the first list marker in content
    static ref FIRST_LIST_MARKER_RE: Regex = Regex::new(r"(?m)^(\s*)([*+-])(\s+[^*+\-\s]|\s*$)").unwrap();
}

/// Style mode for list markers
#[derive(Debug, Clone, PartialEq)]
pub enum StyleMode {
    /// Enforce a specific marker style
    Specific(String),
    /// Enforce consistency based on the first marker found
    Consistent,
}

/// Rule for checking unordered list style
/// This rule enforces a specific marker character (* or - or +) for unordered lists
#[derive(Debug)]
pub struct MD008ULStyle {
    pub style_mode: StyleMode,
}

impl Default for MD008ULStyle {
    fn default() -> Self {
        Self {
            style_mode: StyleMode::Consistent,
        }
    }
}

impl MD008ULStyle {
    pub fn new(style: char) -> Self {
        Self {
            style_mode: StyleMode::Specific(style.to_string()),
        }
    }

    /// Create a new instance with specific style mode
    pub fn with_mode(style: char, style_mode: StyleMode) -> Self {
        match style_mode {
            StyleMode::Specific(_) => Self {
                style_mode: StyleMode::Specific(style.to_string()),
            },
            StyleMode::Consistent => Self {
                style_mode: StyleMode::Consistent,
            },
        }
    }

    /// Parse a list item line, returning (indentation, marker, content_length)
    fn parse_list_item(line: &str) -> Option<(usize, char, usize)> {
        LIST_ITEM_RE.captures(line).map(|caps| {
            let whitespace = caps.get(1).map_or("", |m| m.as_str());
            let marker = caps
                .get(2)
                .map_or("", |m| m.as_str())
                .chars()
                .next()
                .unwrap();
            let content = caps.get(3).map_or("", |m| m.as_str());

            (whitespace.len(), marker, content.len())
        })
    }

    /// Determine if a line is in a code block
    fn is_in_code_block(content: &str, line_idx: usize) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;

        for (i, line) in lines.iter().enumerate() {
            if i > line_idx {
                break;
            }

            if CODE_BLOCK_MARKER.is_match(line.trim_start()) {
                in_code_block = !in_code_block;
            }

            if i == line_idx {
                return in_code_block;
            }
        }

        false
    }

    /// Check if content contains any list items (for fast skipping)
    #[inline]
    fn contains_potential_list_items(content: &str) -> bool {
        content.contains('*') || content.contains('-') || content.contains('+')
    }

    /// Precompute code blocks for faster checking
    fn precompute_code_blocks(content: &str) -> Vec<bool> {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        let mut code_blocks = vec![false; lines.len()];

        for (i, line) in lines.iter().enumerate() {
            if CODE_BLOCK_MARKER.is_match(line.trim_start()) {
                in_code_block = !in_code_block;
            }
            code_blocks[i] = in_code_block;
        }

        code_blocks
    }

    /// Helper method to find the first list marker in content
    fn find_first_list_marker(content: &str) -> Option<String> {
        if let Some(captures) = FIRST_LIST_MARKER_RE.captures(content) {
            if let Some(marker) = captures.get(2) {
                return Some(marker.as_str().to_string());
            }
        }

        None
    }

    /// Get the style from StyleMode for checking list items
    fn get_style_from_mode(&self, content: &str) -> String {
        match &self.style_mode {
            StyleMode::Specific(style) => style.clone(),
            StyleMode::Consistent => {
                // Find the first list marker to determine style
                Self::find_first_list_marker(content).unwrap_or_else(|| "*".to_string())
            }
        }
    }
}

impl Rule for MD008ULStyle {
    fn name(&self) -> &'static str {
        "MD008"
    }

    fn description(&self) -> &'static str {
        "Unordered list style"
    }

    fn check(&self, content: &str) -> LintResult {
        // Fast path - if content is empty or doesn't contain any list marker characters, return empty result
        if content.is_empty() || !Self::contains_potential_list_items(content) {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        // Precompute code blocks
        let code_blocks = Self::precompute_code_blocks(content);

        let lines: Vec<&str> = content.lines().collect();

        // Get the target style based on mode
        let expected_style = self.get_style_from_mode(content);

        let mut in_blockquote = false;

        for (i, line) in lines.iter().enumerate() {
            // Skip code blocks
            if code_blocks.get(i).unwrap_or(&false) == &true {
                continue;
            }

            let trimmed = line.trim_start();
            // Track blockquote state
            if trimmed.starts_with('>') {
                in_blockquote = true;
            } else if !trimmed.is_empty() {
                in_blockquote = false;
            }

            if let Some((indent, marker, _content_len)) = Self::parse_list_item(line) {
                // Skip if in blockquote since those are handled separately
                if in_blockquote {
                    continue;
                }

                if marker.to_string() != expected_style {
                    let trimmed_line = line.trim_start();
                    // For regular list items, just use indentation
                    let line_start = " ".repeat(indent);

                    // Find the list marker position and content after it
                    let list_marker_pos = line.find(marker).unwrap_or(0);
                    let content_after_marker = if list_marker_pos + 1 < line.len() {
                        &line[list_marker_pos + 1..]
                    } else {
                        ""
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: i + 1,
                        column: indent + 1,
                        message: format!(
                            "Unordered list item marker '{}' should be '{}'",
                            marker, expected_style
                        ),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: format!(
                                "{}{}{}",
                                line_start, expected_style, content_after_marker
                            ),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Fast path - if content is empty or no list items, return empty result
        if content.is_empty() || structure.list_lines.is_empty() {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        // Get the target style based on mode
        let expected_style = self.get_style_from_mode(content);

        let mut in_blockquote = false;

        for &line_num in &structure.list_lines {
            let line_idx = line_num - 1; // Convert 1-indexed to 0-indexed

            // Skip if out of bounds
            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];

            // Skip code blocks
            if structure.is_in_code_block(line_num) {
                continue;
            }

            let trimmed = line.trim_start();
            // Track blockquote state
            if trimmed.starts_with('>') {
                in_blockquote = true;
            } else if !trimmed.is_empty() {
                in_blockquote = false;
            }

            if let Some((indent, marker, _content_len)) = Self::parse_list_item(line) {
                // Skip if in blockquote since those are handled separately
                if in_blockquote {
                    continue;
                }

                if marker.to_string() != expected_style {
                    let trimmed_line = line.trim_start();
                    // For regular list items, just use indentation
                    let line_start = " ".repeat(indent);

                    // Find the list marker position and content after it
                    let list_marker_pos = line.find(marker).unwrap_or(0);
                    let content_after_marker = if list_marker_pos + 1 < line.len() {
                        &line[list_marker_pos + 1..]
                    } else {
                        ""
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: line_num,
                        column: indent + 1,
                        message: format!(
                            "Unordered list item marker '{}' should be '{}'",
                            marker, expected_style
                        ),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line_num, 1),
                            replacement: format!(
                                "{}{}{}",
                                line_start, expected_style, content_after_marker
                            ),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Fast path - if content is empty or doesn't contain any list marker characters, return content as-is
        if content.is_empty() || !Self::contains_potential_list_items(content) {
            return Ok(content.to_string());
        }

        let warnings = self.check(content)?;

        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        // Create a map of line numbers with fixes
        let mut line_fixes: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();
        for warning in &warnings {
            if let Some(fix) = &warning.fix {
                line_fixes.insert(warning.line, fix.replacement.clone());
            }
        }

        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            if let Some(fixed_line) = line_fixes.get(&line_num) {
                result.push_str(fixed_line);
            } else {
                result.push_str(line);
            }

            // Add newline for all lines except the last one
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Preserve ALL trailing newlines
        // Count trailing newlines in the original content
        let trailing_newlines_count = content.chars().rev().take_while(|&c| c == '\n').count();

        // Ensure result has the same number of trailing newlines
        let result_trailing_newlines = result.chars().rev().take_while(|&c| c == '\n').count();

        // Add any missing newlines
        if trailing_newlines_count > result_trailing_newlines {
            result.push_str(&"\n".repeat(trailing_newlines_count - result_trailing_newlines));
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if we should skip this rule based on content
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty() || !Self::contains_potential_list_items(content)
    }
}

impl DocumentStructureExtensions for MD008ULStyle {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        // Test with consistent mode (default)
        let rule = MD008ULStyle::default();

        // Test with valid style
        let content = "* Item 1\n* Item 2\n* Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for correct style (*)"
        );

        // Test with different marker but consistent
        let content = "- Item 1\n- Item 2\n- Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for consistent - style"
        );

        // Test with specific style
        let rule = MD008ULStyle::with_mode('*', StyleMode::Specific("*".to_string()));
        let content = "- Item 1\n- Item 2\n- Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(
            result.len(),
            3,
            "Expected warnings for - style with * rule in specific mode"
        );

        // Test with mixed styles
        let rule = MD008ULStyle::default(); // Consistent mode
        let content = "- Item 1\n* Item 2\n+ Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(
            result.len(),
            2,
            "Expected warnings for * and + markers when - is first"
        );

        // Test with blockquote
        let rule = MD008ULStyle::default(); // Consistent mode
        let content = "> * Item 1\n> * Item 2\n> - Item 3";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Expected no warnings for blockquote content"
        );
    }

    #[test]
    fn test_trailing_newlines_preservation() {
        let rule = MD008ULStyle::default();

        // Test with multiple trailing newlines
        let content = "* Item 1\n* Item 2\n- Item 3\n\n\n";
        let result = rule.fix(content).unwrap();
        assert_eq!(
            result, "* Item 1\n* Item 2\n* Item 3\n\n\n",
            "Should preserve all trailing newlines"
        );
    }

    #[test]
    fn test_blockquote_handling() {
        let rule = MD008ULStyle::default();

        // Test with blockquote content
        let content = "> * Item 1\n> * Item 2\n> - Item 3";
        let result = rule.check(content).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Expected no warnings for list markers in blockquotes"
        );

        // Mixed blockquote and regular list items
        let content = "> * Item 1\n* Item 2\n> - Item 3";
        let result = rule.check(content).unwrap();
        assert_eq!(
            result.len(),
            0,
            "Expected no warnings for mixed blockquote and list items"
        );
    }
}
