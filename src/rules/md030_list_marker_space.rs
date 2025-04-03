use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity, RuleCategory};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Optimize regex patterns with compilation once at startup
    static ref LIST_REGEX: Regex = Regex::new(r"^(\s*)([-*+]|\d+\.)(\s+)").unwrap();
    static ref LIST_FIX_REGEX: Regex = Regex::new(r"^(\s*)([-*+]|\d+\.)(\s+)").unwrap();
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
        // Fast path
        if line.trim().is_empty() {
            return None;
        }

        if let Some(cap) = LIST_REGEX.captures(line) {
            let marker = &cap[2];
            let spaces = cap[3].len();
            let list_type = if marker.chars().next().unwrap().is_ascii_digit() {
                ListType::Ordered
            } else {
                ListType::Unordered
            };
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
        LIST_FIX_REGEX
            .replace(line, format!("$1$2{}", " ".repeat(expected_spaces)))
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
        if !content.contains('*') && !content.contains('-') && !content.contains('+') &&
           !content.contains(|c: char| c.is_ascii_digit()) {
            return Ok(Vec::new());
        }
        
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        
        // Precompute list states
        let (is_list_line, multi_line) = self.precompute_states(&lines);

        for (i, &line) in lines.iter().enumerate() {
            // Skip non-list lines
            if !is_list_line[i] {
                continue;
            }

            if let Some((list_type, line_start, spaces)) = Self::is_list_item(line) {
                let expected_spaces = self.get_expected_spaces(list_type, multi_line[i]);
                if spaces != expected_spaces {
                    let col = line_start.len() - spaces;
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
        if !content.contains('*') && !content.contains('-') && !content.contains('+') &&
           !content.contains(|c: char| c.is_ascii_digit()) {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();
        
        // Precompute list states
        let (is_list_line, multi_line) = self.precompute_states(&lines);

        let mut result = String::with_capacity(content.len() + 100);

        for (i, &line) in lines.iter().enumerate() {
            if is_list_line[i] {
                if let Some((list_type, _, _)) = Self::is_list_item(line) {
                    result.push_str(&self.fix_line(line, list_type, multi_line[i]));
                } else {
                    result.push_str(line);
                }
            } else {
                result.push_str(line);
            }

            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        // Preserve trailing newline
        if content.ends_with('\n') && !result.ends_with('\n') {
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
        content.is_empty() ||
            (!content.contains('*') && !content.contains('-') && !content.contains('+') &&
             !content.contains(|c: char| c.is_ascii_digit()))
    }
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
        assert!(result.is_empty(), "Correctly spaced list markers should not generate warnings");
        
        // Test with incorrect spacing
        let content = "*  Item 1 (too many spaces)\n* Item 2\n1.   Ordered item (too many spaces)";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2, "Should have warnings for both items with incorrect spacing");
        
        // Test with multiline items
        let content = "* Item 1\n  continued on next line\n* Item 2";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty(), "Default spacing for single and multiline is 1");
        
        // Test with custom spacing settings
        let custom_rule = MD030ListMarkerSpace::new(1, 2, 1, 2);
        let content = "* Item 1\n  continued on next line\n*  Item 2 with 2 spaces";
        let structure = DocumentStructure::new(content);
        let result = custom_rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2, "Should have warnings for both items (wrong spacing)");
    }
}
