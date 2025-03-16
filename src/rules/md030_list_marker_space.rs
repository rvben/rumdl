use crate::utils::range_utils::LineIndex;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
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
        let _line_index = LineIndex::new(content.to_string());
        // Fast path
        if content.is_empty() {
            return Ok(Vec::new());
        }

        let lines: Vec<&str> = content.lines().collect();
        let (is_list_line, multi_line) = self.precompute_states(&lines);
        let mut warnings = Vec::new();

        for i in 0..lines.len() {
            if !is_list_line[i] {
                continue;
            }

            let line = lines[i];
            if let Some((list_type, marker, spaces)) = Self::is_list_item(line) {
                let is_multi = multi_line[i];
                let expected_spaces = self.get_expected_spaces(list_type, is_multi);

                if spaces != expected_spaces {
                    warnings.push(LintWarning {
                        line: i + 1,
                        column: marker.len() - spaces,
                        severity: Severity::Warning,
                        message: format!(
                            "Expected {} space{} after list marker, got {}",
                            expected_spaces,
                            if expected_spaces == 1 { "" } else { "s" },
                            spaces
                        ),
                        fix: Some(Fix {
                            range: _line_index.line_col_to_byte_range(i + 1, 1),
                            replacement: self.fix_line(line, list_type, is_multi),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let _line_index = LineIndex::new(content.to_string());
        // Fast path
        if content.is_empty() {
            return Ok(String::new());
        }

        let lines: Vec<&str> = content.lines().collect();
        let (is_list_line, multi_line) = self.precompute_states(&lines);
        let mut result = String::new();

        for i in 0..lines.len() {
            let line = lines[i];
            if is_list_line[i] {
                if let Some((list_type, _, _)) = Self::is_list_item(line) {
                    let is_multi = multi_line[i];
                    result.push_str(&self.fix_line(line, list_type, is_multi));
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

        Ok(result)
    }
}
