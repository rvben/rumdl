use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use std::collections::HashMap;

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
        let re = Regex::new(r"^\s*(\d+)\.\s").unwrap();
        re.captures(line)
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
}

impl Rule for MD029OrderedListPrefix {
    fn name(&self) -> &'static str {
        "MD029"
    }

    fn description(&self) -> &'static str {
        "Ordered list item prefix should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let mut level_indices = HashMap::new();

        for (line_num, line) in content.lines().enumerate() {
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
            } else if line.trim().is_empty() {
                // Reset all indices on empty line
                level_indices.clear();
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut level_indices = HashMap::new();

        for line in content.lines() {
            if let Some(_) = Self::get_list_number(line) {
                let indent_level = Self::get_indent_level(line);
                let indentation = line.len() - line.trim_start().len();
                
                // Get or insert a new counter for this indent level
                let index = *level_indices.entry(indent_level).or_insert(0);
                let expected = self.get_expected_number(index);
                
                let fixed_line = format!("{}{}.{}", 
                    " ".repeat(indentation),
                    expected,
                    line.trim_start().split('.').skip(1).collect::<String>()
                );
                
                result.push_str(&fixed_line);
                
                // Increment the counter for this indent level
                level_indices.insert(indent_level, index + 1);
                
                // Reset all deeper indent levels
                let deeper_levels: Vec<usize> = level_indices.keys()
                    .filter(|&k| *k > indent_level)
                    .cloned()
                    .collect();
                
                for level in deeper_levels {
                    level_indices.remove(&level);
                }
            } else {
                result.push_str(line);
                if line.trim().is_empty() {
                    // Reset all indices on empty line
                    level_indices.clear();
                }
            }
            result.push('\n');
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 