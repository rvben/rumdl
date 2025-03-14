use crate::rule::{LintError, LintResult, LintWarning, Rule};
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::rules::code_block_utils::CodeBlockUtils;
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref LIST_ITEM_REGEX: Regex = Regex::new(r"^(\s*)([-*+]|\d+\.)\s").unwrap();
    static ref LIST_CONTENT_REGEX: Regex = Regex::new(r"^\s{2,}").unwrap();
}

#[derive(Debug, Default)]
pub struct MD032BlanksAroundLists;

impl MD032BlanksAroundLists {
    fn is_list_item(line: &str) -> bool {
        // Fast path for empty lines
        if line.trim().is_empty() {
            return false;
        }
        
        // Quick literal check before regex
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return false;
        }
        
        let first_char = trimmed.chars().next().unwrap();
        if first_char == '-' || first_char == '*' || first_char == '+' || first_char.is_ascii_digit() {
            // Use regex for complete validation
            return LIST_ITEM_REGEX.is_match(line);
        }
        
        false
    }

    fn is_empty_line(line: &str) -> bool {
        line.trim().is_empty()
    }
    
    fn is_list_content(line: &str) -> bool {
        if Self::is_empty_line(line) {
            return false;
        }
        
        // Fast path - check if line starts with at least 2 spaces
        if let Some(idx) = line.find(|c: char| !c.is_whitespace()) {
            if idx >= 2 {
                return true;
            }
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
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Pre-compute which lines are in code blocks or front matter
        let mut excluded_lines = vec![false; lines.len()];
        for i in 0..lines.len() {
            excluded_lines[i] = FrontMatterUtils::is_in_front_matter(content, i) || 
                               CodeBlockUtils::is_in_code_block(content, i);
        }
        
        // Pre-compute list items and content lines
        let mut is_list_item_vec = vec![false; lines.len()];
        let mut is_list_content_vec = vec![false; lines.len()];
        let mut is_empty_vec = vec![false; lines.len()];
        
        for i in 0..lines.len() {
            if excluded_lines[i] {
                continue;
            }
            
            let line = lines[i];
            is_list_item_vec[i] = Self::is_list_item(line);
            is_list_content_vec[i] = Self::is_list_content(line);
            is_empty_vec[i] = Self::is_empty_line(line);
        }
        
        let mut in_list = false;
        let mut _list_start_index = 0;
        let mut _list_end_index = 0;

        // First pass: Find list boundaries and check for blank lines around lists
        for i in 0..lines.len() {
            if excluded_lines[i] {
                continue;
            }
            
            let is_list_item = is_list_item_vec[i];
            let is_list_content = is_list_content_vec[i];
            let is_empty = is_empty_vec[i];

            if is_list_item {
                if !in_list {
                    // Starting a new list
                    in_list = true;
                    _list_start_index = i;

                    // Check if there's no blank line before the list (unless it's at the start of the document)
                    if i > 0 && !is_empty_vec[i - 1] && !excluded_lines[i - 1] {
                        warnings.push(LintWarning {
                            message: "List should be preceded by a blank line".to_string(),
                            line: i + 1,
                            column: 1,
                            fix: None,
                        });
                    }
                }
                _list_end_index = i;
            } else if is_list_content && in_list {
                // This is content belonging to a list item
                _list_end_index = i;
            } else if !is_empty {
                // Regular content line
                if in_list {
                    // Just finished a list, check if there's no blank line after
                    warnings.push(LintWarning {
                        message: "List should be followed by a blank line".to_string(),
                        line: i + 1,
                        column: 1,
                        fix: None,
                    });
                    in_list = false;
                }
            } else if is_empty {
                // Empty line
                in_list = false;
            }
        }

        // No need to check for list at the end of document - it doesn't need a blank line after

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.is_empty() {
            return Ok(String::new());
        }
        
        // Apply front matter fixes first if needed
        let content = FrontMatterUtils::fix_malformed_front_matter(content);
        
        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::with_capacity(lines.len() + 10);  // Add some extra capacity for blank lines
        
        // Pre-compute which lines are in code blocks or front matter
        let mut excluded_lines = vec![false; lines.len()];
        for i in 0..lines.len() {
            excluded_lines[i] = FrontMatterUtils::is_in_front_matter(&content, i) || 
                               CodeBlockUtils::is_in_code_block(&content, i);
        }
        
        let mut in_list = false;
        let mut _list_start_index = 0;
        let mut _list_end_index = 0;

        for (i, line) in lines.iter().enumerate() {
            // If this line is in front matter or code block, keep it as is
            if excluded_lines[i] {
                result.push(line.to_string());
                continue;
            }
            
            if Self::is_list_item(line) {
                if !in_list {
                    // Starting a new list
                    // Add blank line before list if needed (unless it's the start of the document)
                    if i > 0 && !Self::is_empty_line(lines[i - 1]) && 
                       !excluded_lines[i - 1] && !result.is_empty() {
                        result.push("".to_string());
                    }
                    in_list = true;
                    _list_start_index = i;
                }
                result.push(line.to_string());
            } else if Self::is_list_content(line) {
                // List content, just add it
                result.push(line.to_string());
            } else if Self::is_empty_line(line) {
                // Empty line
                result.push(line.to_string());
                in_list = false;
            } else {
                // Regular content
                if in_list {
                    // End of list, add blank line if needed
                    result.push("".to_string());
                    in_list = false;
                }
                result.push(line.to_string());
            }
        }
        
        Ok(result.join("\n"))
    }
} 