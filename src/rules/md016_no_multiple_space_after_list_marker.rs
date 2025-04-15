use crate::utils::element_cache::{get_element_cache, ListMarkerType};
use crate::utils::range_utils::LineIndex;
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};

#[derive(Debug, Default)]
pub struct MD016NoMultipleSpaceAfterListMarker {
    pub allow_multiple_spaces: bool,
}

impl MD016NoMultipleSpaceAfterListMarker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_allow_multiple_spaces(allow_multiple_spaces: bool) -> Self {
        Self {
            allow_multiple_spaces,
        }
    }
}

impl Rule for MD016NoMultipleSpaceAfterListMarker {
    fn name(&self) -> &'static str {
        "MD016"
    }

    fn description(&self) -> &'static str {
        "List markers should not be followed by multiple spaces"
    }

    fn check(&self, content: &str) -> LintResult {
        // Skip processing if allowing multiple spaces
        if self.allow_multiple_spaces {
            return Ok(Vec::new());
        }

        // Fast path - check if content has any list markers
        if !content.contains('*') && !content.contains('-') && !content.contains('+') && 
           !content.contains("1.") && !content.contains("2.") {
            return Ok(Vec::new());
        }
        
        let line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        
        // Get cached document elements - this provides efficient access to lists and code blocks
        let element_cache = get_element_cache(content);
        
        // Process each list item from the cache
        for list_item in element_cache.get_list_items() {
            // Check if this list item has multiple spaces after marker
            if list_item.spaces_after_marker > 1 {
                // Create a warning with fix
                let line_num = list_item.line_number;
                let message = match list_item.marker_type {
                    ListMarkerType::Asterisk | ListMarkerType::Plus | ListMarkerType::Minus => 
                        "Multiple spaces after unordered list marker".to_string(),
                    ListMarkerType::Ordered => 
                        "Multiple spaces after ordered list marker".to_string(),
                };
                
                // Generate the fixed line with exactly one space after marker
                let indentation = " ".repeat(list_item.indentation);
                let fixed_line = format!("{}{} {}", indentation, list_item.marker, list_item.content);
                
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    severity: Severity::Warning,
                    line: line_num,
                    column: list_item.indentation + 1,
                    message,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range(line_num, 1),
                        replacement: fixed_line,
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Skip processing if allowing multiple spaces
        if self.allow_multiple_spaces {
            return Ok(content.to_string());
        }
        
        // Fast path - check if content has any list markers
        if !content.contains('*') && !content.contains('-') && !content.contains('+') && 
           !content.contains("1.") && !content.contains("2.") {
            return Ok(content.to_string());
        }
        
        let element_cache = get_element_cache(content);
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::new();
        
        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            
            // Check if this line is a list item with multiple spaces
            if let Some(list_item) = element_cache.get_list_item(line_num) {
                if list_item.spaces_after_marker > 1 {
                    // Generate the fixed line
                    let indentation = " ".repeat(list_item.indentation);
                    let fixed_line = format!("{}{} {}", indentation, list_item.marker, list_item.content);
                    result.push_str(&fixed_line);
                } else {
                    // Use the original line
                    result.push_str(line);
                }
            } else {
                // Not a list item, keep as is
                result.push_str(line);
            }
            
            // Add newline except for last line
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }
        
        // Ensure trailing newline is preserved
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }
        
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_md016_check() {
        let rule = MD016NoMultipleSpaceAfterListMarker::default();
        
        // Valid test cases
        let content1 = "- Item with one space\n* Another item with one space\n+ A third item";
        let warnings1 = rule.check(content1).unwrap();
        assert_eq!(warnings1.len(), 0);
        
        // Invalid test cases
        let content2 = "-  Item with two spaces\n*   Another item with three spaces\n+    Four spaces";
        let warnings2 = rule.check(content2).unwrap();
        assert_eq!(warnings2.len(), 3);
        
        // Mixed case
        let content3 = "- Valid item\n-  Invalid item\n```\n-  Ignored in code block\n```";
        let warnings3 = rule.check(content3).unwrap();
        assert_eq!(warnings3.len(), 1);
        
        // Test with allow_multiple_spaces = true
        let rule_allowing_spaces = MD016NoMultipleSpaceAfterListMarker::with_allow_multiple_spaces(true);
        let warnings4 = rule_allowing_spaces.check(content2).unwrap();
        assert_eq!(warnings4.len(), 0);
    }
    
    #[test]
    fn test_md016_fix() {
        let rule = MD016NoMultipleSpaceAfterListMarker::default();
        
        // Fix test case
        let content = "-  Item with two spaces\n*   Another item with three spaces\n+    Four spaces";
        let expected = "- Item with two spaces\n* Another item with three spaces\n+ Four spaces";
        
        let fixed = rule.fix(content).unwrap();
        assert_eq!(fixed, expected);
        
        // Test with code blocks
        let content2 = "- Valid item\n-  Invalid item\n```\n-  Ignored in code block\n```";
        let expected2 = "- Valid item\n- Invalid item\n```\n-  Ignored in code block\n```";
        
        let fixed2 = rule.fix(content2).unwrap();
        assert_eq!(fixed2, expected2);
    }
}
