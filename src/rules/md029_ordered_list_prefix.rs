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

#[derive(Debug, Clone)]
struct ListSequence {
    index: usize,
    indent_level: usize,
    last_seen_line: usize,
}

impl MD029OrderedListPrefix {
    pub fn new(style: &str) -> Self {
        Self {
            style: style.to_string(),
        }
    }

    fn get_list_number(line: &str) -> Option<usize> {
        LIST_NUMBER_PATTERN
            .captures(line)
            .and_then(|cap| cap[1].parse().ok())
    }

    fn get_indent_level(line: &str) -> usize {
        let indent = line.len() - line.trim_start().len();
        indent / 2 // Assuming 2 spaces per indent level
    }

    fn get_expected_number(&self, sequence: &ListSequence) -> usize {
        match self.style.as_str() {
            "ordered" => {
                if sequence.indent_level > 0 {
                    // For nested lists, always start at 1
                    sequence.index + 1
                } else {
                    // For root level lists, maintain sequence
                    sequence.index + 1
                }
            }
            "one" => 1,
            "zero" => 0,
            _ => sequence.index + 1,
        }
    }

    fn is_blank_line(line: &str) -> bool {
        line.trim().is_empty()
    }

    // Helper function to find or create a sequence for a given indentation level
    fn find_or_create_sequence<'a>(
        sequences: &'a mut HashMap<usize, ListSequence>,
        indent_level: usize,
        line_num: usize,
    ) -> &'a mut ListSequence {
        sequences.entry(indent_level).or_insert(ListSequence {
            index: 0,
            indent_level,
            last_seen_line: line_num,
        })
    }

    // Helper function to check if a number matches the expected value
    fn is_number_valid(&self, number: usize, expected: usize) -> bool {
        match self.style.as_str() {
            "one" => number == 1,
            "zero" => number == 0,
            _ => number == expected,
        }
    }

    // Helper function to reset sequences for deeper levels
    fn reset_deeper_sequences(sequences: &mut HashMap<usize, ListSequence>, current_level: usize, line_num: usize) {
        // Only reset sequences at deeper levels that haven't been seen recently
        sequences.retain(|&level, sequence| {
            level <= current_level || line_num - sequence.last_seen_line <= 2
        });
    }

    // Helper function to check if a line is part of a code block
    fn is_code_block_marker(line: &str) -> bool {
        line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")
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
        let mut sequences: HashMap<usize, ListSequence> = HashMap::new();
        let mut in_code_block = false;

        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            // Handle code block transitions
            if Self::is_code_block_marker(line) {
                in_code_block = !in_code_block;
                continue;
            }

            // Skip lines in code blocks
            if in_code_block {
                continue;
            }

            if let Some(number) = Self::get_list_number(line) {
                let indent_level = Self::get_indent_level(line);

                // Reset sequences for deeper levels when going back to a shallower level
                Self::reset_deeper_sequences(&mut sequences, indent_level, line_num);

                // Find or create sequence for this level
                let sequence = Self::find_or_create_sequence(&mut sequences, indent_level, line_num);
                let expected = self.get_expected_number(sequence);

                if !self.is_number_valid(number, expected) {
                    let indentation = line.len() - line.trim_start().len();
                    let message = match self.style.as_str() {
                        "one" => "List item prefix should be 1 for style 'one'".to_string(),
                        "zero" => "List item prefix should be 0 for style 'zero'".to_string(),
                        _ => format!("List item prefix should be {}", expected),
                    };

                    warnings.push(LintWarning {
            rule_name: Some(self.name()),
                        line: line_num + 1,
                        column: indentation + 1,
                        message,
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: line_index.line_col_to_byte_range(line_num + 1, indentation + 1),
                            replacement: format!(
                                "{}{}",
                                if self.style == "one" { 1 } 
                                else if self.style == "zero" { 0 }
                                else { expected },
                                &line[indentation + number.to_string().len()..]
                            ),
                        }),
                    });
                }

                sequence.index += 1;
                sequence.last_seen_line = line_num;
            } else if !Self::is_blank_line(line) && !Self::is_code_block_marker(line) {
                // Non-list, non-blank, non-code-block line resets all sequences
                sequences.clear();
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
        let mut sequences: HashMap<usize, ListSequence> = HashMap::new();
        let mut in_code_block = false;

        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            let mut fixed_line = line.to_string();

            // Handle code block transitions
            if Self::is_code_block_marker(line) {
                in_code_block = !in_code_block;
                result.push_str(&fixed_line);
                if line_num < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // Skip lines in code blocks
            if in_code_block {
                result.push_str(&fixed_line);
                if line_num < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            if let Some(number) = Self::get_list_number(line) {
                let indent_level = Self::get_indent_level(line);

                // Reset sequences for deeper levels when going back to a shallower level
                Self::reset_deeper_sequences(&mut sequences, indent_level, line_num);

                // Find or create sequence for this level
                let sequence = Self::find_or_create_sequence(&mut sequences, indent_level, line_num);
                let expected = self.get_expected_number(sequence);
                let expected_num = match self.style.as_str() {
                    "one" => 1,
                    "zero" => 0,
                    _ => expected,
                };

                if !self.is_number_valid(number, expected) {
                    let indentation = line.len() - line.trim_start().len();
                    fixed_line = format!(
                        "{}{}{}",
                        " ".repeat(indentation),
                        expected_num,
                        &line[indentation + number.to_string().len()..]
                    );
                }

                sequence.index += 1;
                sequence.last_seen_line = line_num;
            } else if !Self::is_blank_line(line) && !Self::is_code_block_marker(line) {
                // Non-list, non-blank, non-code-block line resets all sequences
                sequences.clear();
            }

            result.push_str(&fixed_line);
            if line_num < lines.len() - 1 {
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
