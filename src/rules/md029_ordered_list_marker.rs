
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

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
        let re = Regex::new(r"^(\s*)\d+\.\s").unwrap();
        re.find(line).map(|m| (m.start(), m.end()))
    }

    fn get_list_number(line: &str) -> Option<usize> {
        let re = Regex::new(r"^\s*(\d+)\.\s").unwrap();
        re.captures(line)
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
        let re = Regex::new(r"^(\s*)\d+(\.\s.*)$").unwrap();
        re.replace(line, format!("${{1}}{}{}", expected_num, "$2"))
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
}

impl MD029OrderedListMarker {
    fn check_list_section(&self, items: &[(usize, String)], warnings: &mut Vec<LintWarning>) {
        for (idx, (line_num, line)) in items.iter().enumerate() {
            if let Some(actual_num) = Self::get_list_number(line) {
                let expected_num = self.get_expected_number(idx);
                if actual_num != expected_num {
                    warnings.push(LintWarning {
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