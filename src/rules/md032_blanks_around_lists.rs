use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rules::code_block_utils::CodeBlockUtils;
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::utils::document_structure::document_structure_from_str;
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref LIST_ITEM_REGEX: Regex = Regex::new(r"^(\s*)([-*+]|\d+\.)\s").unwrap();
    static ref LIST_CONTENT_REGEX: Regex = Regex::new(r"^\s{2,}").unwrap();
}

/// Rule MD032: Lists should be surrounded by blank lines
///
/// This rule enforces that lists are surrounded by blank lines, which improves document
/// readability and ensures consistent rendering across different Markdown processors.
///
/// ## Purpose
///
/// - **Readability**: Blank lines create visual separation between lists and surrounding content
/// - **Parsing**: Many Markdown parsers require blank lines around lists for proper rendering
/// - **Consistency**: Ensures uniform document structure and appearance
/// - **Compatibility**: Improves compatibility across different Markdown implementations
///
/// ## Examples
///
/// ### Correct
///
/// ```markdown
/// This is a paragraph of text.
///
/// - Item 1
/// - Item 2
/// - Item 3
///
/// This is another paragraph.
/// ```
///
/// ### Incorrect
///
/// ```markdown
/// This is a paragraph of text.
/// - Item 1
/// - Item 2
/// - Item 3
/// This is another paragraph.
/// ```
///
/// ## Behavior Details
///
/// This rule checks for the following:
///
/// - **List Start**: There should be a blank line before the first item in a list
///   (unless the list is at the beginning of the document or after front matter)
/// - **List End**: There should be a blank line after the last item in a list
///   (unless the list is at the end of the document)
/// - **Nested Lists**: Properly handles nested lists and list continuations
/// - **List Types**: Works with ordered lists, unordered lists, and all valid list markers (-, *, +)
///
/// ## Special Cases
///
/// This rule handles several special cases:
///
/// - **Front Matter**: YAML front matter is detected and skipped
/// - **Code Blocks**: Lists inside code blocks are ignored
/// - **List Content**: Indented content belonging to list items is properly recognized as part of the list
/// - **Document Boundaries**: Lists at the beginning or end of the document have adjusted requirements
///
/// ## Fix Behavior
///
/// When applying automatic fixes, this rule:
/// - Adds a blank line before the first list item when needed
/// - Adds a blank line after the last list item when needed
/// - Preserves document structure and existing content
///
/// ## Performance Optimizations
///
/// The rule includes several optimizations:
/// - Fast path checks before applying more expensive regex operations
/// - Efficient list item detection
/// - Pre-computation of code block lines to avoid redundant processing
#[derive(Debug, Default)]
pub struct MD032BlanksAroundLists;

impl MD032BlanksAroundLists {
    #[inline]
    fn is_list_item(line: &str) -> bool {
        // Early return for empty lines
        if line.is_empty() || line.trim().is_empty() {
            return false;
        }

        // Fast literal check before regex
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return false;
        }

        // Quick character check before using regex
        let first_char = trimmed.chars().next().unwrap();
        if !matches!(first_char, '-' | '*' | '+' | '0'..='9') {
            return false;
        }

        // For likely list items, verify with regex
        if first_char.is_ascii_digit() {
            // Additional check for ordered lists to avoid regex when possible
            if let Some(idx) = trimmed.find('.') {
                // Verify it's a number followed by a period and a space
                if idx > 0
                    && idx < trimmed.len() - 1
                    && trimmed.as_bytes().get(idx + 1) == Some(&b' ')
                {
                    return true;
                }
            }
            // Fall back to regex for complex cases
            return LIST_ITEM_REGEX.is_match(line);
        } else if (first_char == '-' || first_char == '*' || first_char == '+')
            && trimmed.len() > 1
            && trimmed.as_bytes()[1] == b' '
        {
            // Fast path for simple unordered list items
            return true;
        }

        // Use regex as fallback for complex cases
        LIST_ITEM_REGEX.is_match(line)
    }

    #[inline]
    fn is_empty_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    #[inline]
    fn is_list_content(line: &str) -> bool {
        // Early return for empty lines
        if line.is_empty() || line.trim().is_empty() {
            return false;
        }

        // Fast path - check if line starts with at least 2 spaces
        if let Some(first_non_space) = line.as_bytes().iter().position(|&b| b != b' ') {
            return first_non_space >= 2;
        }

        false
    }
}

impl Rule for MD032BlanksAroundLists {
    fn name(&self) -> &'static str {
        "MD032"
    }

    fn description(&self) -> &'static str {
        "Lists should be surrounded by blank lines"
    }

    fn check(&self, content: &str) -> LintResult {
        let structure = document_structure_from_str(content);
        self.check_with_structure(content, &structure)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Early returns for common cases
        if content.is_empty() {
            return Ok(String::new());
        }

        // Apply front matter fixes first if needed
        let content = FrontMatterUtils::fix_malformed_front_matter(content);

        let lines: Vec<&str> = content.lines().collect();
        // Pre-allocate with extra capacity for added blank lines
        let mut result = Vec::with_capacity(lines.len() + 10);

        // Fast path check - if no list markers are present, return content as is
        if !content.contains([
            '-', '*', '+', '1', '2', '3', '4', '5', '6', '7', '8', '9', '0',
        ]) {
            return Ok(content.to_string());
        }

        // Pre-compute which lines are in code blocks or front matter - only when needed
        let mut excluded_lines = vec![false; lines.len()];
        let has_code_blocks =
            content.contains("```") || content.contains("~~~") || content.contains("    ");
        let has_front_matter = content.starts_with("---\n") || content.starts_with("---\r\n");

        if has_code_blocks || has_front_matter {
            for (i, _) in lines.iter().enumerate() {
                excluded_lines[i] = (has_front_matter
                    && FrontMatterUtils::is_in_front_matter(&content, i))
                    || (has_code_blocks && CodeBlockUtils::is_in_code_block(&content, i));
            }
        }

        let mut in_list = false;
        let mut _list_start_index = 0;
        let mut _list_end_index = 0;

        for (i, line) in lines.iter().enumerate() {
            // If this line is in front matter or code block, keep it as is
            if excluded_lines[i] {
                result.push(*line);
                continue;
            }

            if Self::is_list_item(line) {
                if !in_list {
                    // Starting a new list
                    // Add blank line before list if needed (unless it's the start of the document)
                    if i > 0
                        && !Self::is_empty_line(lines[i - 1])
                        && !excluded_lines[i - 1]
                        && !result.is_empty()
                    {
                        result.push("");
                    }
                    in_list = true;
                    _list_start_index = i;
                }
                result.push(*line);
            } else if Self::is_list_content(line) {
                // List content, just add it
                result.push(*line);
            } else if Self::is_empty_line(line) {
                // Empty line
                result.push(*line);
                in_list = false;
            } else {
                // Regular content
                if in_list {
                    // End of list, add blank line if needed
                    result.push("");
                    in_list = false;
                }
                result.push(*line);
            }
        }

        Ok(result.join("\n"))
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early returns for common cases
        if self.should_skip(content) {
            return Ok(Vec::new());
        }

        // Use document structure to check only for lists
        if !self.has_relevant_elements(content, structure) {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Pre-compute empty lines
        let mut is_empty_vec = vec![false; lines.len()];
        for (i, line) in lines.iter().enumerate() {
            is_empty_vec[i] = Self::is_empty_line(line);
        }

        // Pre-compute list content lines (indented content belonging to list items)
        let mut is_list_content_vec = vec![false; lines.len()];
        for (i, line) in lines.iter().enumerate() {
            if !is_empty_vec[i] && !Self::is_list_item(line) && Self::is_list_content(line) {
                is_list_content_vec[i] = true;
            }
        }

        // Get list boundaries from document structure
        let list_starts = structure.get_list_start_indices();
        let list_ends = structure.get_list_end_indices();

        // Process each list
        for i in 0..list_starts.len().min(list_ends.len()) {
            let start_idx = list_starts[i];
            let mut end_idx = list_ends[i];

            // Extend the list end to include indented content
            for j in end_idx..lines.len() {
                if is_list_content_vec[j] {
                    end_idx = j;
                } else if !is_empty_vec[j] {
                    break;
                }
            }

            // Check for blank line before list (unless at document start)
            if start_idx > 0
                && !is_empty_vec[start_idx - 1]
                && !structure.is_in_code_block(start_idx)
            {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: start_idx + 1,
                    column: 1,
                    message: "List should be preceded by a blank line".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(start_idx + 1, 1),
                        replacement: format!("\n{}", lines[start_idx]),
                    }),
                });
            }

            // Check for blank line after list (unless at document end)
            if end_idx < lines.len() - 1
                && !is_empty_vec[end_idx + 1]
                && !structure.is_in_code_block(end_idx + 1)
            {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: end_idx + 2,
                    column: 1,
                    message: "List should be followed by a blank line".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(end_idx + 2, 1),
                        replacement: format!("\n{}", lines[end_idx + 1]),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty()
            || !content.contains([
                '-', '*', '+', '1', '2', '3', '4', '5', '6', '7', '8', '9', '0',
            ])
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}

impl DocumentStructureExtensions for MD032BlanksAroundLists {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::document_structure::document_structure_from_str;

    #[test]
    fn test_with_document_structure() {
        let rule = MD032BlanksAroundLists;

        // Test case 1: List without blank lines
        let content = "Paragraph\n- Item 1\n- Item 2\nAnother paragraph";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(warnings.len(), 2); // Should warn about missing blank lines before and after

        // Test case 2: Lists with proper blank lines
        let content = "Paragraph\n\n- Item 1\n- Item 2\n\nAnother paragraph";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(warnings.len(), 0); // No warnings

        // Test case 3: List at beginning of document
        let content = "- Item 1\n- Item 2\n\nParagraph";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(warnings.len(), 0); // No warnings for missing blank line before

        // Test case 4: List at end of document
        let content = "Paragraph\n\n- Item 1\n- Item 2";
        let structure = document_structure_from_str(content);
        let warnings = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(warnings.len(), 0); // No warnings for missing blank line after
    }
}
