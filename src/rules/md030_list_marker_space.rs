use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;

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
        // Fast path for non-list lines
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
        if current_idx + 1 >= lines.len() {
            return false;
        }

        let next_line = lines[current_idx + 1].trim();
        
        // Fast path for empty lines
        if next_line.is_empty() {
            return false;
        }
        
        // Check if next line is a new list item
        if let Some((_, _, _)) = Self::is_list_item(lines[current_idx + 1]) {
            return false;
        }
        
        true
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
        LIST_FIX_REGEX.replace(line, format!("$1$2{}", " ".repeat(expected_spaces)))
            .to_string()
    }

    fn precompute_states(&self, lines: &[&str]) -> (Vec<bool>, Vec<Option<bool>>) {
        let mut code_block_state = vec![false; lines.len()];
        let mut multi_line_state = vec![None; lines.len()];
        let mut in_code_block = false;

        // First pass: mark code blocks
        for (i, &line) in lines.iter().enumerate() {
            if CODE_BLOCK_REGEX.is_match(line) {
                in_code_block = !in_code_block;
            }
            code_block_state[i] = in_code_block;
        }

        // Second pass: compute multi-line states
        for i in 0..lines.len() {
            if !code_block_state[i] {
                multi_line_state[i] = Some(self.is_multi_line_item(lines, i));
            }
        }

        (code_block_state, multi_line_state)
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
        // Fast path for empty content
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        // Pre-compute states
        let (code_block_state, multi_line_state) = self.precompute_states(&lines);
        
        // Check list items
        for (i, &line) in lines.iter().enumerate() {
            if code_block_state[i] {
                continue;
            }

            if let Some((list_type, _, spaces)) = Self::is_list_item(line) {
                let is_multi = multi_line_state[i].unwrap_or_else(|| self.is_multi_line_item(&lines, i));
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
        // Fast path for empty content
        if content.is_empty() {
            return Ok(String::new());
        }
        
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::with_capacity(content.len());
        
        // Pre-compute states
        let (code_block_state, multi_line_state) = self.precompute_states(&lines);

        for (i, &line) in lines.iter().enumerate() {
            if code_block_state[i] {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            // Fast path for non-list items
            if line.trim().is_empty() || !line.contains('-') && !line.contains('*') && 
               !line.contains('+') && !line.contains('.') {
                result.push_str(line);
                if i < lines.len() - 1 {
                    result.push('\n');
                }
                continue;
            }

            if let Some((list_type, _, _)) = Self::is_list_item(line) {
                let is_multi = multi_line_state[i].unwrap_or_else(|| self.is_multi_line_item(&lines, i));
                result.push_str(&self.fix_line(line, list_type, is_multi));
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