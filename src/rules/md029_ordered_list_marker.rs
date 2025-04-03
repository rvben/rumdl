use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity, RuleCategory};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref ORDERED_LIST_ITEM_REGEX: Regex = Regex::new(r"^(\s*)\d+\.\s").unwrap();
    static ref LIST_NUMBER_REGEX: Regex = Regex::new(r"^\s*(\d+)\.\s").unwrap();
    static ref FIX_LINE_REGEX: Regex = Regex::new(r"^(\s*)\d+(\.\s.*)$").unwrap();
}

#[derive(Debug)]
pub struct MD029OrderedListMarker {
    style: ListStyle,
}

#[derive(Debug, PartialEq)]
pub enum ListStyle {
    OneOne,     // All ones (1. 1. 1.)
    Ordered,    // Sequential (1. 2. 3.)
    Ordered0,   // Zero-based (0. 1. 2.)
}

impl Default for MD029OrderedListMarker {
    fn default() -> Self {
        Self {
            style: ListStyle::Ordered,
        }
    }
}

impl MD029OrderedListMarker {
    pub fn new(style: ListStyle) -> Self {
        Self { style }
    }

    fn is_ordered_list_item(line: &str) -> Option<(usize, usize)> {
        ORDERED_LIST_ITEM_REGEX.find(line).map(|m| (m.start(), m.end()))
    }

    fn get_list_number(line: &str) -> Option<usize> {
        LIST_NUMBER_REGEX
            .captures(line)
            .and_then(|cap| cap[1].parse::<usize>().ok())
    }

    fn get_expected_number(&self, index: usize) -> usize {
        match self.style {
            ListStyle::OneOne => 1,
            ListStyle::Ordered => index + 1,
            ListStyle::Ordered0 => index,
        }
    }

    fn fix_line(&self, line: &str, expected_num: usize) -> String {
        FIX_LINE_REGEX
            .replace(line, format!("${{1}}{}{}", expected_num, "$2"))
            .to_string()
    }
}

impl Rule for MD029OrderedListMarker {
    fn name(&self) -> &'static str {
        "MD029"
    }

    fn description(&self) -> &'static str {
        "Ordered list marker value"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut list_items = Vec::new();
        let mut in_code_block = false;

        // First pass: collect list items
        for (line_num, line) in content.lines().enumerate() {
            if line.trim().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }

            if in_code_block {
                continue;
            }

            if let Some(_) = Self::is_ordered_list_item(line) {
                list_items.push((line_num, line.to_string()));
            } else if !line.trim().is_empty() {
                // Non-empty, non-list line breaks the list
                if !list_items.is_empty() {
                    self.check_list_section(&list_items, &mut warnings);
                    list_items.clear();
                }
            }
        }

        // Check last section if it exists
        if !list_items.is_empty() {
            self.check_list_section(&list_items, &mut warnings);
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut list_items = Vec::new();
        let mut in_code_block = false;
        let mut current_section_start = 0;

        let lines: Vec<&str> = content.lines().collect();

        for (i, &line) in lines.iter().enumerate() {
            if line.trim().starts_with("```") {
                in_code_block = !in_code_block;
                if !list_items.is_empty() {
                    self.fix_list_section(&mut result, &lines[current_section_start..i], &list_items);
                    list_items.clear();
                }
                result.push_str(line);
                result.push('\n');
                current_section_start = i + 1;
                continue;
            }

            if in_code_block {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if let Some(_) = Self::is_ordered_list_item(line) {
                list_items.push((i, line.to_string()));
            } else if !line.trim().is_empty() {
                if !list_items.is_empty() {
                    self.fix_list_section(&mut result, &lines[current_section_start..i], &list_items);
                    list_items.clear();
                }
                result.push_str(line);
                result.push('\n');
                current_section_start = i + 1;
            } else {
                if list_items.is_empty() {
                    result.push_str(line);
                    result.push('\n');
                    current_section_start = i + 1;
                }
            }
        }

        // Fix last section if it exists
        if !list_items.is_empty() {
            self.fix_list_section(&mut result, &lines[current_section_start..], &list_items);
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    /// Optimized check using document structure
    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        // Early return if no lists
        if structure.list_lines.is_empty() {
            return Ok(Vec::new());
        }
        
        // Quick check if there are no ordered lists
        if !content.contains('1') || (!content.contains("1.") && !content.contains("2.") && !content.contains("0.")) {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let mut list_items = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Create a set of list line indices for faster lookup
        let mut list_line_set = std::collections::HashSet::new();
        for &line_num in &structure.list_lines {
            list_line_set.insert(line_num); // Keep as 1-indexed for easier comparison
        }

        // Group ordered list items into sections
        let mut current_list_start = 0;
        let mut in_list = false;
        
        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = line_idx + 1; // Convert to 1-indexed
            
            // Skip lines in code blocks
            if structure.is_in_code_block(line_num) {
                // If we were in a list, check it before continuing
                if in_list && !list_items.is_empty() {
                    self.check_list_section(&list_items, &mut warnings);
                    list_items.clear();
                    in_list = false;
                }
                continue;
            }
            
            if list_line_set.contains(&line_num) {
                if let Some(_) = Self::is_ordered_list_item(line) {
                    // If this is the first item of a new list, record the list start
                    if !in_list {
                        current_list_start = line_idx;
                        in_list = true;
                    }
                    
                    list_items.push((line_idx, line.to_string()));
                }
            } else if !line.trim().is_empty() {
                // Non-empty, non-list line breaks the list
                if in_list && !list_items.is_empty() {
                    self.check_list_section(&list_items, &mut warnings);
                    list_items.clear();
                    in_list = false;
                }
            }
        }

        // Check last section if it exists
        if !list_items.is_empty() {
            self.check_list_section(&list_items, &mut warnings);
        }

        Ok(warnings)
    }
    
    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }
    
    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty() || 
        !content.contains('1') || 
        (!content.contains("1.") && !content.contains("2.") && !content.contains("0."))
    }
}

impl DocumentStructureExtensions for MD029OrderedListMarker {
    fn has_relevant_elements(&self, content: &str, doc_structure: &DocumentStructure) -> bool {
        // This rule is only relevant if there are list items AND they might be ordered lists
        !doc_structure.list_lines.is_empty() && 
        (content.contains("1.") || content.contains("2.") || content.contains("0."))
    }
}

impl MD029OrderedListMarker {
    fn check_list_section(&self, items: &[(usize, String)], warnings: &mut Vec<LintWarning>) {
        for (idx, (line_num, line)) in items.iter().enumerate() {
            if let Some(actual_num) = Self::get_list_number(line) {
                let expected_num = self.get_expected_number(idx);
                if actual_num != expected_num {
                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        message: format!(
                            "Ordered list item number {} does not match style (expected {})",
                            actual_num, expected_num
                        ),
                        line: line_num + 1,
                        column: line.find(char::is_numeric).unwrap_or(0) + 1,
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: self.fix_line(line, expected_num),
                        }),
                    });
                }
            }
        }
    }

    fn fix_list_section(&self, result: &mut String, lines: &[&str], items: &[(usize, String)]) {
        let mut current_line = 0;

        for (idx, (line_num, _)) in items.iter().enumerate() {
            // Add any non-list lines before this item
            while current_line < *line_num {
                result.push_str(lines[current_line]);
                result.push('\n');
                current_line += 1;
            }

            // Add the fixed list item
            let expected_num = self.get_expected_number(idx);
            result.push_str(&self.fix_line(lines[*line_num], expected_num));
            result.push('\n');
            current_line = line_num + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_with_document_structure() {
        // Test with default style (ordered)
        let rule = MD029OrderedListMarker::default();
        
        // Test with correctly ordered list
        let content = "1. First item\n2. Second item\n3. Third item";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());
        
        // Test with incorrectly ordered list
        let content = "1. First item\n3. Third item\n5. Fifth item";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should have warnings for items 3 and 5
        
        // Test with one-one style
        let rule = MD029OrderedListMarker::new(ListStyle::OneOne);
        let content = "1. First item\n2. Second item\n3. Third item";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert_eq!(result.len(), 2); // Should have warnings for items 2 and 3
        
        // Test with ordered0 style
        let rule = MD029OrderedListMarker::new(ListStyle::Ordered0);
        let content = "0. First item\n1. Second item\n2. Third item";
        let structure = DocumentStructure::new(content);
        let result = rule.check_with_structure(content, &structure).unwrap();
        assert!(result.is_empty());
    }
} 
            
            if list_line_set.contains(&line_num) {
                if let Some(_) = Self::is_ordered_list_item(line) {
                    // If this is the first item of a new list, record the list start
                    if !in_list {
                        current_list_start = line_idx;
                        in_list = true;
                    }
                    
                    list_items.push((line_idx, line.to_string()));
                }
            } else if !line.trim().is_empty() {
                // Non-empty, non-list line breaks the list
                if in_list && !list_items.is_empty() {
                    self.check_list_section(&list_items, &mut warnings);
                    list_items.clear();
                    in_list = false;
                }
            }
        }

        // Check last section if it exists
        if !list_items.is_empty() {
            self.check_list_section(&list_items, &mut warnings);
        }

        Ok(warnings)
    }
    
    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }
    
    /// Check if this rule should be skipped
    fn should_skip(&self, content: &str) -> bool {
        content.is_empty() || 
        !content.contains('1') || 
        (!content.contains("1.") && !content.contains("2.") && !content.contains("0."))
    }
}

impl DocumentStructureExtensions for MD029OrderedListMarker {
    fn has_relevant_elements(&self, content: &str, doc_structure: &DocumentStructure) -> bool {
        // This rule is only relevant if there are list items AND they might be ordered lists
        !doc_structure.list_lines.is_empty() && 
        (content.contains("1.") || content.contains("2.") || content.contains("0."))
    }
}

impl MD029OrderedListMarker {
    fn check_list_section(&self, items: &[(usize, String)], warnings: &mut Vec<LintWarning>) {
        for (idx, (line_num, line)) in items.iter().enumerate() {
            if let Some(actual_num) = Self::get_list_number(line) {
                let expected_num = self.get_expected_number(idx);
                if actual_num != expected_num {
                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        message: format!(
                            "Ordered list item number {} does not match style (expected {})",
                            actual_num, expected_num
                        ),
                        line: line_num + 1,
                        column: line.find(char::is_numeric).unwrap_or(0) + 1,
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: 1,
                            replacement: self.fix_line(line, expected_num),
                        }),
                    });
                }
            }
        }
    }

    fn fix_list_section(&self, result: &mut String, lines: &[&str], items: &[(usize, String)]) {
        let mut current_line = 0;

        for (idx, (line_num, _)) in items.iter().enumerate() {
            // Add any non-list lines before this item
            while current_line < *line_num {
                result.push_str(lines[current_line]);
                result.push('\n');
                current_line += 1;
            }

            // Add the fixed list item
            let expected_num = self.get_expected_number(idx);
            result.push_str(&self.fix_line(lines[*line_num], expected_num));
            result.push('\n');
            current_line = line_num + 1;
        }
    }