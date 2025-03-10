use crate::rule::{LintError, LintResult, LintWarning, Rule};
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::rules::heading_utils::HeadingUtils;
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD032BlanksAroundLists;

impl MD032BlanksAroundLists {
    fn is_list_item(line: &str) -> bool {
        let list_re = Regex::new(r"^(\s*)([-*+]|\d+\.)\s").unwrap();
        list_re.is_match(line)
    }

    fn is_empty_line(line: &str) -> bool {
        line.trim().is_empty()
    }
    
    fn is_list_content(line: &str) -> bool {
        let content_re = Regex::new(r"^\s{2,}").unwrap();
        content_re.is_match(line) && !Self::is_empty_line(line)
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
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_list = false;
        let mut _list_start_index = 0;
        let mut list_end_index = 0;

        // First pass: Find list boundaries and check for blank lines around lists
        for i in 0..lines.len() {
            // Skip processing if line is in front matter or code block
            if FrontMatterUtils::is_in_front_matter(content, i) || HeadingUtils::is_in_code_block(content, i) {
                continue;
            }
            
            let line = lines[i];
            let is_list_item = Self::is_list_item(line);
            let is_list_content = Self::is_list_content(line);
            let is_empty = Self::is_empty_line(line);

            if is_list_item {
                if !in_list {
                    // Starting a new list
                    in_list = true;
                    _list_start_index = i;

                    // Check if there's no blank line before the list (unless it's at the start of the document)
                    if i > 0 && !Self::is_empty_line(lines[i - 1]) && 
                       !FrontMatterUtils::is_in_front_matter(content, i - 1) {
                        warnings.push(LintWarning {
                            message: "List should be preceded by a blank line".to_string(),
                            line: i + 1,
                            column: 1,
                            fix: None,
                        });
                    }
                }
                list_end_index = i;
            } else if is_list_content && in_list {
                // This is content belonging to a list item
                list_end_index = i;
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

        // Check for list at the end of document
        if in_list && list_end_index == lines.len() - 1 {
            // The list ends at the end of the document
            // We don't need a blank line after the list if it's at the end of the document
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Apply front matter fixes first if needed
        let content = FrontMatterUtils::fix_malformed_front_matter(content);
        
        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::new();
        let mut in_list = false;
        
        for (i, line) in lines.iter().enumerate() {
            // If this line is in front matter or code block, keep it as is
            if FrontMatterUtils::is_in_front_matter(&content, i) || HeadingUtils::is_in_code_block(&content, i) {
                result.push(line.to_string());
                continue;
            }
            
            if Self::is_list_item(line) {
                if !in_list {
                    // Starting a new list
                    // Add blank line before list if needed (unless it's the start of the document)
                    if i > 0 && !Self::is_empty_line(lines[i - 1]) && 
                       !FrontMatterUtils::is_in_front_matter(&content, i - 1) && 
                       !result.is_empty() {
                        result.push("".to_string());
                    }
                    in_list = true;
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