use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashMap;
use lazy_static::lazy_static;

// Cache regex patterns for better performance
lazy_static! {
    static ref LIST_NUMBER_PATTERN: Regex = Regex::new(r"^\s*(\d+)\.\s").unwrap();
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^```|^~~~").unwrap();
}

#[derive(Debug)]
pub struct MD029OrderedListPrefix {
    pub style: String,
}

impl Default for MD029OrderedListPrefix {
    fn default() -> Self {
        Self {
            style: "ordered".to_string(),
        }
    }
}

impl MD029OrderedListPrefix {
    pub fn new(style: &str) -> Self {
        Self {
            style: style.to_string(),
        }
    }

    fn get_list_number(line: &str) -> Option<usize> {
        // Use cached regex pattern
        LIST_NUMBER_PATTERN
            .captures(line)
            .and_then(|cap| cap[1].parse().ok())
    }

    fn get_indent_level(line: &str) -> usize {
        let indent = line.len() - line.trim_start().len();
        indent / 2  // Assuming 2 spaces per indent level
    }

    fn should_be_ordered(&self, current: usize, index: usize) -> bool {
        match self.style.as_str() {
            "ordered" => current != index + 1,
            "one" => current != 1,
            "zero" => current != 0,
            _ => false,
        }
    }

    fn get_expected_number(&self, index: usize) -> usize {
        match self.style.as_str() {
            "ordered" => index + 1,
            "one" => 1,
            "zero" => 0,
            _ => index + 1,
        }
    }

    // Pre-compute which lines are in code blocks
    fn precompute_code_blocks(content: &str) -> Vec<bool> {
        let mut in_code_block = false;
        content.lines().map(|line| {
            if CODE_BLOCK_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
            }
            in_code_block
        }).collect()
    }
}

impl Rule for MD029OrderedListPrefix {
    fn name(&self) -> &'static str {
        "MD029"
    }

    fn description(&self) -> &'static str {
        "Ordered list item prefix should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        // Early return for empty content
        if content.trim().is_empty() {
            return Ok(vec![]);
        }

        // Quick check if there are any numbered lists at all
        if !content.contains(|c: char| c.is_digit(10) && c != '0') || !content.contains(". ") {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();
        let mut level_indices = HashMap::new();
        
        // Pre-compute code block regions
        let in_code_block = Self::precompute_code_blocks(content);
        
        for (line_num, (line, is_in_code_block)) in content.lines().zip(in_code_block.iter()).enumerate() {
            // Skip lines in code blocks
            if *is_in_code_block {
                continue;
            }
            
            if let Some(number) = Self::get_list_number(line) {
                let indent_level = Self::get_indent_level(line);
                
                // Get or insert a new counter for this indent level
                let index = *level_indices.entry(indent_level).or_insert(0);
                
                if self.should_be_ordered(number, index) {
                    let indentation = line.len() - line.trim_start().len();
                    let expected = self.get_expected_number(index);
                    warnings.push(LintWarning {
                        line: line_num + 1,
                        column: indentation + 1,
                        message: format!("List item prefix should be {} for style '{}'", expected, self.style),
                        fix: Some(Fix {
                            line: line_num + 1,
                            column: indentation + 1,
                            replacement: format!("{}{}.{}", 
                                " ".repeat(indentation),
                                expected,
                                line.trim_start().split('.').skip(1).collect::<String>()
                            ),
                        }),
                    });
                }
                
                // Increment the counter for this indent level
                level_indices.insert(indent_level, index + 1);
                
                // Reset all deeper indent levels when we encounter a less indented item
                let deeper_levels: Vec<usize> = level_indices.keys()
                    .filter(|&k| *k > indent_level)
                    .cloned()
                    .collect();
                
                for level in deeper_levels {
                    level_indices.remove(&level);
                }
            }
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Early return if content is empty or has no potential numbered lists
        if content.trim().is_empty() || 
           (!content.contains(|c: char| c.is_digit(10) && c != '0') || !content.contains(". ")) {
            return Ok(content.to_string());
        }

        let mut result = String::new();
        let mut level_indices = HashMap::new();
        let in_code_block = Self::precompute_code_blocks(content);
        
        for (line, is_in_code_block) in content.lines().zip(in_code_block.iter()) {
            let mut fixed_line = line.to_string();
            
            // Don't fix lines in code blocks
            if !*is_in_code_block {
                if let Some(number) = Self::get_list_number(line) {
                    let indent_level = Self::get_indent_level(line);
                    let index = *level_indices.entry(indent_level).or_insert(0);
                    
                    if self.should_be_ordered(number, index) {
                        let indentation = line.len() - line.trim_start().len();
                        let expected = self.get_expected_number(index);
                        fixed_line = format!("{}{}.{}", 
                            " ".repeat(indentation),
                            expected,
                            line.trim_start().split('.').skip(1).collect::<String>()
                        );
                    }
                    
                    level_indices.insert(indent_level, index + 1);
                    
                    // Reset deeper levels
                    let deeper_levels: Vec<usize> = level_indices.keys()
                        .filter(|&k| *k > indent_level)
                        .cloned()
                        .collect();
                    
                    for level in deeper_levels {
                        level_indices.remove(&level);
                    }
                }
            }
            
            result.push_str(&fixed_line);
            result.push('\n');
        }
        
        // Remove trailing newline if original content didn't have it
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }
        
        Ok(result)
    }
} 