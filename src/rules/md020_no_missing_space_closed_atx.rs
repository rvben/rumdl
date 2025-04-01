use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref CLOSED_ATX_NO_SPACE_PATTERN: Regex =
        Regex::new(r"^(\s*)(#+)([^#\s].*?)([^#\s])(#+)\s*$").unwrap();
    static ref CLOSED_ATX_NO_SPACE_START_PATTERN: Regex =
        Regex::new(r"^(\s*)(#+)([^#\s].*?)\s(#+)\s*$").unwrap();
    static ref CLOSED_ATX_NO_SPACE_END_PATTERN: Regex =
        Regex::new(r"^(\s*)(#+)\s(.*?)([^#\s])(#+)\s*$").unwrap();
    
    // Matches code fence blocks
    static ref CODE_FENCE_PATTERN: Regex = 
        Regex::new(r"^(\s*)(`{3,}|~{3,})").unwrap();
}

#[derive(Debug, Default)]
pub struct MD020NoMissingSpaceClosedAtx;

impl MD020NoMissingSpaceClosedAtx {
    pub fn new() -> Self {
        Self
    }

    fn is_closed_atx_heading_without_space(&self, line: &str) -> bool {
        CLOSED_ATX_NO_SPACE_PATTERN.is_match(line)
            || CLOSED_ATX_NO_SPACE_START_PATTERN.is_match(line)
            || CLOSED_ATX_NO_SPACE_END_PATTERN.is_match(line)
    }

    fn fix_closed_atx_heading(&self, line: &str) -> String {
        if let Some(captures) = CLOSED_ATX_NO_SPACE_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let last_char = &captures[4];
            let closing_hashes = &captures[5];
            format!(
                "{}{} {}{} {}",
                indentation, opening_hashes, content, last_char, closing_hashes
            )
        } else if let Some(captures) = CLOSED_ATX_NO_SPACE_START_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let closing_hashes = &captures[4];
            format!(
                "{}{} {} {}",
                indentation, opening_hashes, content, closing_hashes
            )
        } else if let Some(captures) = CLOSED_ATX_NO_SPACE_END_PATTERN.captures(line) {
            let indentation = &captures[1];
            let opening_hashes = &captures[2];
            let content = &captures[3];
            let last_char = &captures[4];
            let closing_hashes = &captures[5];
            format!(
                "{}{} {}{} {}",
                indentation, opening_hashes, content, last_char, closing_hashes
            )
        } else {
            line.to_string()
        }
    }
    
    // Calculate the byte range for a specific line in the content
    fn get_line_byte_range(&self, content: &str, line_num: usize) -> std::ops::Range<usize> {
        let mut current_line = 1;
        let mut start_byte = 0;
        
        for (i, c) in content.char_indices() {
            if current_line == line_num && c == '\n' {
                return start_byte..i;
            } else if c == '\n' {
                current_line += 1;
                if current_line == line_num {
                    start_byte = i + 1;
                }
            }
        }
        
        // If we're looking for the last line and it doesn't end with a newline
        if current_line == line_num {
            return start_byte..content.len();
        }
        
        // Fallback if line not found (shouldn't happen)
        0..0
    }
}

impl Rule for MD020NoMissingSpaceClosedAtx {
    fn name(&self) -> &'static str {
        "MD020"
    }

    fn description(&self) -> &'static str {
        "No space inside hashes on closed ATX style heading"
    }

    fn check(&self, content: &str) -> LintResult {
        if content.is_empty() {
            return Ok(Vec::new());
        }
        
        let mut warnings = Vec::new();
        let mut in_code_block = false;
        
        for (i, line) in content.lines().enumerate() {
            let line_num = i + 1; // Convert to 1-indexed
            
            // Handle code blocks
            if CODE_FENCE_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                continue;
            }
            
            // Skip content inside code blocks
            if in_code_block {
                continue;
            }
            
            // Check if line matches closed ATX pattern without space
            if self.is_closed_atx_heading_without_space(line) {
                let captures = if let Some(c) = CLOSED_ATX_NO_SPACE_PATTERN.captures(line) {
                    c
                } else if let Some(c) = CLOSED_ATX_NO_SPACE_START_PATTERN.captures(line) {
                    c
                } else {
                    CLOSED_ATX_NO_SPACE_END_PATTERN.captures(line).unwrap()
                };
                
                let indentation = captures.get(1).unwrap();
                let opening_hashes = captures.get(2).unwrap();
                let line_range = self.get_line_byte_range(content, line_num);
                
                warnings.push(LintWarning {
                    message: format!(
                        "Missing space inside hashes on closed ATX style heading with {} hashes",
                        opening_hashes.as_str().len()
                    ),
                    line: line_num,
                    column: indentation.end() + 1,
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_range,
                        replacement: self.fix_closed_atx_heading(line),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        if content.is_empty() {
            return Ok(String::new());
        }
        
        let mut result = String::new();
        let mut in_code_block = false;

        for (i, line) in content.lines().enumerate() {
            // Handle code blocks
            if CODE_FENCE_PATTERN.is_match(line) {
                in_code_block = !in_code_block;
                result.push_str(line);
            } else if in_code_block {
                result.push_str(line);
            } else if self.is_closed_atx_heading_without_space(line) {
                result.push_str(&self.fix_closed_atx_heading(line));
            } else {
                result.push_str(line);
            }
            
            if i < content.lines().count() - 1 {
                result.push('\n');
            }
        }

        // Preserve trailing newline if original had it
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }
}
