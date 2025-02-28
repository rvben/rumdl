use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

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
        let ul_re = Regex::new(r"^(\s*)[-*+](\s+)").unwrap();
        let ol_re = Regex::new(r"^(\s*)\d+\.(\s+)").unwrap();

        if let Some(cap) = ul_re.captures(line) {
            Some((
                ListType::Unordered,
                cap[0].to_string(),
                cap[2].len(),
            ))
        } else if let Some(cap) = ol_re.captures(line) {
            Some((
                ListType::Ordered,
                cap[0].to_string(),
                cap[2].len(),
            ))
        } else {
            None
        }
    }

    fn is_multi_line_item(&self, lines: &[&str], current_idx: usize) -> bool {
        if current_idx + 1 >= lines.len() {
            return false;
        }

        let next_line = lines[current_idx + 1].trim();
        !next_line.is_empty() && !next_line.starts_with('-') && !next_line.starts_with('*') && 
        !next_line.starts_with('+') && !next_line.chars().next().map_or(false, |c| c.is_ascii_digit())
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
        match list_type {
            ListType::Unordered => {
                let re = Regex::new(r"^(\s*[-*+])(\s+)").unwrap();
                re.replace(line, format!("$1{}", " ".repeat(expected_spaces)))
                    .to_string()
            }
            ListType::Ordered => {
                let re = Regex::new(r"^(\s*\d+\.)(\s+)").unwrap();
                re.replace(line, format!("$1{}", " ".repeat(expected_spaces)))
                    .to_string()
            }
        }
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
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;

        for (i, &line) in lines.iter().enumerate() {
            if line.trim().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }

            if in_code_block {
                continue;
            }

            if let Some((list_type, _, spaces)) = Self::is_list_item(line) {
                let is_multi = self.is_multi_line_item(&lines, i);
                let expected_spaces = self.get_expected_spaces(list_type, is_multi);

                if spaces != expected_spaces {
                    warnings.push(LintWarning {
                        message: format!(
                            "Expected {} space{} after list marker (found {})",
                            expected_spaces,
                            if expected_spaces == 1 { "" } else { "s" },
                            spaces
                        ),
                        line: i + 1,
                        column: line.find(char::is_whitespace).unwrap_or(0) + 1,
                        fix: Some(Fix {
                            line: i + 1,
                            column: 1,
                            replacement: self.fix_line(line, list_type, is_multi),
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;

        for (i, &line) in lines.iter().enumerate() {
            if line.trim().starts_with("```") {
                in_code_block = !in_code_block;
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if in_code_block {
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if let Some((list_type, _, _)) = Self::is_list_item(line) {
                let is_multi = self.is_multi_line_item(&lines, i);
                result.push_str(&self.fix_line(line, list_type, is_multi));
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 