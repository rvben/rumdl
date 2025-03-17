use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;

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
        indent / 2 // Assuming 2 spaces per indent level
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
        content
            .lines()
            .map(|line| {
                if CODE_BLOCK_PATTERN.is_match(line) {
                    in_code_block = !in_code_block;
                }
                in_code_block
            })
            .collect()
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
        let line_index = LineIndex::new(content.to_string());

        // Early return for empty content
        if content.trim().is_empty() {
            return Ok(vec![]);
        }

        // Quick check if there are any numbered lists at all
        if !content.contains(|c: char| c.is_ascii_digit() && c != '0') || !content.contains(". ") {
            return Ok(vec![]);
        }

        let mut warnings = Vec::new();
        let mut level_indices = HashMap::new();

        // Pre-compute code block regions
        let in_code_block = Self::precompute_code_blocks(content);

        for (line_num, (line, is_in_code_block)) in
            content.lines().zip(in_code_block.iter()).enumerate()
        {
            // Skip lines in code blocks
            if *is_in_code_block {
                continue;
            }

            if let Some(number) = Self::get_list_number(line) {
                let indent_level = Self::get_indent_level(line);

                // Get or insert a new counter for this indent level
                let index = *level_indices.entry(indent_level).or_insert(0);

                let expected = self.get_expected_number(index);

                if self.style == "one" {
                    // For "one" style, all list items should have number 1
                    if number != 1 {
                        let indentation = line.len() - line.trim_start().len();
                        warnings.push(LintWarning {
                            line: line_num + 1,
                            column: indentation + 1,
                            message: "List item prefix should be 1 for style 'one'".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index
                                    .line_col_to_byte_range(line_num + 1, indentation + 1),
                                replacement: format!(
                                    "1{}",
                                    // Get everything after the number including the period
                                    &line[indentation + number.to_string().len()..]
                                ),
                            }),
                        });
                    }
                } else if self.style == "zero" {
                    // For "zero" style, all list items should have number 0
                    if number != 0 {
                        let indentation = line.len() - line.trim_start().len();
                        warnings.push(LintWarning {
                            line: line_num + 1,
                            column: indentation + 1,
                            message: "List item prefix should be 0 for style 'zero'".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index
                                    .line_col_to_byte_range(line_num + 1, indentation + 1),
                                replacement: format!(
                                    "0{}",
                                    &line[indentation + number.to_string().len()..]
                                ),
                            }),
                        });
                    }
                } else if self.style == "ordered" {
                    // For "ordered" style, list items should be sequential
                    if number != index + 1 {
                        let indentation = line.len() - line.trim_start().len();
                        warnings.push(LintWarning {
                            line: line_num + 1,
                            column: indentation + 1,
                            message: format!(
                                "List item prefix should be {} for style 'ordered'",
                                expected
                            ),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index
                                    .line_col_to_byte_range(line_num + 1, indentation + 1),
                                replacement: format!(
                                    "{}{}",
                                    expected,
                                    &line[indentation + number.to_string().len()..]
                                ),
                            }),
                        });
                    }
                }

                // Increment the counter for this indent level
                level_indices.insert(indent_level, index + 1);

                // Reset all deeper indent levels when we encounter a less indented item
                let deeper_levels: Vec<usize> = level_indices
                    .keys()
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
        let warnings = self.check(content)?;

        // Early return if content is empty or has no potential issues
        if warnings.is_empty() {
            return Ok(content.to_string());
        }

        let mut result = String::new();
        let mut level_indices = HashMap::new();
        let in_code_block = Self::precompute_code_blocks(content);

        for (line_num, (line, is_in_code_block)) in
            content.lines().zip(in_code_block.iter()).enumerate()
        {
            let mut fixed_line = line.to_string();

            // Don't fix lines in code blocks
            if !*is_in_code_block {
                if let Some(number) = Self::get_list_number(line) {
                    let indent_level = Self::get_indent_level(line);
                    let index = *level_indices.entry(indent_level).or_insert(0);
                    let expected = self.get_expected_number(index);

                    if (self.style == "one" && number != 1)
                        || (self.style == "zero" && number != 0)
                        || (self.style == "ordered" && number != index + 1)
                    {
                        let indentation = line.len() - line.trim_start().len();
                        fixed_line = format!(
                            "{}{}{}",
                            " ".repeat(indentation),
                            expected,
                            &line[indentation + number.to_string().len()..]
                        );
                    }

                    level_indices.insert(indent_level, index + 1);

                    // Reset deeper levels
                    let deeper_levels: Vec<usize> = level_indices
                        .keys()
                        .filter(|&k| *k > indent_level)
                        .cloned()
                        .collect();

                    for level in deeper_levels {
                        level_indices.remove(&level);
                    }
                }
            }

            result.push_str(&fixed_line);
            if line_num < content.lines().count() - 1 {
                result.push('\n');
            }
        }

        // Preserve trailing newline if present
        if content.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }
}
