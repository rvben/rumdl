use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Regex to capture list markers and the spaces *after* them
    // Allows ZERO or more spaces after marker now using \s*
    static ref LIST_REGEX: Regex = Regex::new(r"^(\s*)([-*+]|\d+\.)(\s*)").unwrap();
    // Regex used for fixing - ensures exactly the required number of spaces
    // Note: Captures slightly differently to handle replacement efficiently
    static ref LIST_FIX_REGEX: Regex = Regex::new(r"^(\s*)([-*+]|\d+\.)(\s*)").unwrap();
    static ref CODE_BLOCK_REGEX: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,})").unwrap();
}

#[derive(Debug)]
pub struct MD030ListMarkerSpace {
    ul_single: usize,
    ul_multi: usize,
    ol_single: usize,
    ol_multi: usize,
}

impl Default for MD030ListMarkerSpace {
    fn default() -> Self {
        Self {
            ul_single: 1,
            ul_multi: 1,
            ol_single: 1,
            ol_multi: 1,
        }
    }
}

impl MD030ListMarkerSpace {
    pub fn new(ul_single: usize, ul_multi: usize, ol_single: usize, ol_multi: usize) -> Self {
        Self {
            ul_single,
            ul_multi,
            ol_single,
            ol_multi,
        }
    }

    fn is_list_item(line: &str) -> Option<(ListType, String, usize)> {
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() {
            return None;
        }

        // Add check for horizontal rules before checking for list markers
        if trimmed_line.chars().all(|c| c == '-' || c == ' ') && trimmed_line.chars().filter(|&c| c == '-').count() >= 3 { return None; }
        if trimmed_line.chars().all(|c| c == '*' || c == ' ') && trimmed_line.chars().filter(|&c| c == '*').count() >= 3 { return None; }
        // Note: '_' HRs won't conflict with list markers anyway

        if let Some(cap) = LIST_REGEX.captures(line) {
            let marker = &cap[2];
            let spaces = cap[3].len(); // Group 3 now captures zero or more spaces
            let list_type = if marker.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                ListType::Ordered
            } else {
                ListType::Unordered
            };
            // Return the whole matched line part for column calculation later
            return Some((list_type, cap[0].to_string(), spaces)); 
        }

        None
    }

    fn is_multi_line_item(&self, lines: &[&str], current_idx: usize) -> bool {
        if current_idx >= lines.len() - 1 {
            return false;
        }

        let next_line = lines[current_idx + 1].trim();

        // Fast path
        if next_line.is_empty() {
            return false;
        }

        // Check if the next line is a list item or not
        if Self::is_list_item(next_line).is_some() {
            return false;
        }

        // Check if it's a continued list item (indented)
        let current_indentation = lines[current_idx]
            .chars()
            .take_while(|c| c.is_whitespace())
            .count();
        let next_indentation = lines[current_idx + 1]
            .chars()
            .take_while(|c| c.is_whitespace())
            .count();

        next_indentation > current_indentation
    }

    fn get_expected_spaces(&self, list_type: ListType, is_multi: bool) -> usize {
        match (list_type, is_multi) {
            (ListType::Unordered, false) => self.ul_single,
            (ListType::Unordered, true) => self.ul_multi,
            (ListType::Ordered, false) => self.ol_single,
            (ListType::Ordered, true) => self.ol_multi,
        }
    }

    fn fix_line(&self, line: &str, list_type: ListType, is_multi: bool) -> String {
        let expected_spaces = self.get_expected_spaces(list_type, is_multi);
        // Use the LIST_FIX_REGEX for replacement
        LIST_FIX_REGEX
            .replace(line, |caps: &regex::Captures| {
                // Reconstruct the start: indentation + marker + correct spaces
                format!("{}{}{}", 
                       &caps[1], 
                       &caps[2], 
                       " ".repeat(expected_spaces))
            })
            .to_string()
    }

    fn precompute_states(&self, lines: &[&str]) -> (Vec<bool>, Vec<bool>) {
        let mut is_list_line = vec![false; lines.len()];
        let mut multi_line = vec![false; lines.len()];
        let mut in_code_block = false;

        // First pass: mark code blocks
        for (i, &line) in lines.iter().enumerate() {
            if CODE_BLOCK_REGEX.is_match(line) {
                in_code_block = !in_code_block;
            }
            if !in_code_block && Self::is_list_item(line).is_some() {
                is_list_line[i] = true;
            }
        }

        // Second pass: compute multi-line states
        for i in 0..lines.len() {
            if is_list_line[i] {
                multi_line[i] = self.is_multi_line_item(lines, i);
            }
        }

        (is_list_line, multi_line)
    }
}

#[derive(Debug, Clone, Copy)]
enum ListType {
    Unordered,
    Ordered,
}

impl Rule for MD030ListMarkerSpace {
    fn name(&self) -> &'static str {
        "MD030"
    }

    fn description(&self) -> &'static str {
        "Spaces after list markers"
    }

    fn check(&self, content: &str) -> LintResult {
        // Early return for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Skip if no list markers
        if !content.contains('*')
            && !content.contains('-')
            && !content.contains('+')
            && !content.contains(|c: char| c.is_ascii_digit())
        {
            return Ok(Vec::new());
        }

        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        // Precompute list states
        let (is_list_line, multi_line) = self.precompute_states(&lines);

        for (i, &line) in lines.iter().enumerate() {
            if !is_list_line[i] { // Skip if not identified as a list item line by precompute
                continue;
            }

            // Re-check with updated regex to get space count (including 0)
            if let Some((list_type, _line_start_match, spaces)) = Self::is_list_item(line) {
                let expected_spaces = self.get_expected_spaces(list_type, multi_line[i]);
                
                // The check is now simply if the captured spaces count differs from expected
                if spaces != expected_spaces {
                    // Calculate column: indentation + marker length
                    let marker_part = LIST_REGEX.captures(line).unwrap(); // Re-capture for precise groups
                    let indentation_len = marker_part[1].len();
                    let marker_len = marker_part[2].len();
                    let col = indentation_len + marker_len + 1; // Column is *after* the marker

                    warnings.push(LintWarning {
                        rule_name: Some(self.name()),
                        line: i + 1,
                        column: col, 
                        message: format!(
                            "Expected {} space{} after list marker, found {}",
                            expected_spaces,
                            if expected_spaces == 1 { "" } else { "s" },
                            spaces
                        ),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            // Fix applies to the whole line for simplicity with regex replace
                            range: line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: self.fix_line(line, list_type, multi_line[i]),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Early return for empty content
        if content.is_empty() {
            return Ok(String::new());
        }

        // Skip if no list markers
        if !content.contains('*')
            && !content.contains('-')
            && !content.contains('+')
            && !content.contains(|c: char| c.is_ascii_digit())
        {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();

        // Precompute list states
        let (is_list_line, multi_line) = self.precompute_states(&lines);

        let mut result_lines = Vec::with_capacity(lines.len());

        for (i, &line) in lines.iter().enumerate() {
            if is_list_line[i] {
                // Check if it's a list item that needs fixing
                if let Some((list_type, _line_start_match, spaces)) = Self::is_list_item(line) {
                    let expected_spaces = self.get_expected_spaces(list_type, multi_line[i]);
                    if spaces != expected_spaces {
                        result_lines.push(self.fix_line(line, list_type, multi_line[i]));
                    } else {
                        result_lines.push(line.to_string()); // No fix needed
                    }
                } else {
                     result_lines.push(line.to_string()); // Not matched by regex, don't change
                }
            } else {
                result_lines.push(line.to_string()); // Not a list line
            }
        }

        let mut result = result_lines.join("\n");

        // Preserve trailing newline if original content had one
        if content.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty()
            || (!content.contains('*')
                && !content.contains('-')
                && !content.contains('+')
                && !content.contains(|c: char| c.is_ascii_digit()))
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}

impl DocumentStructureExtensions for MD030ListMarkerSpace {
    fn has_relevant_elements(&self, _content: &str, doc_structure: &DocumentStructure) -> bool {
        // Rule is only relevant if there are list items
        !doc_structure.list_lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_document_structure() {
        let rule = MD030ListMarkerSpace::default();

        // Test with correct spacing
        let content = "* Item 1\n* Item 2\n  * Nested item\n1. Ordered item";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Correctly spaced list markers should not generate warnings"
        );

        // Test with incorrect spacing
        let content = "*  Item 1 (too many spaces)\n* Item 2\n1.   Ordered item (too many spaces)";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(
            result.len(),
            2,
            "Should have warnings for both items with incorrect spacing"
        );

        // Test with multiline items
        let content = "* Item 1\n  continued on next line\n* Item 2";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(
            result.is_empty(),
            "Default spacing for single and multiline is 1"
        );

        // Test with custom spacing settings
        let custom_rule = MD030ListMarkerSpace::new(1, 2, 1, 2);
        let content = "* Item 1\n  continued on next line\n*  Item 2 with 2 spaces";
        let structure = DocumentStructure::new(content);
        let result = custom_rule
            .check_with_structure(content, &structure)
            .unwrap();
        assert_eq!(
            result.len(),
            2,
            "Should have warnings for both items (wrong spacing)"
        );
    }
}
