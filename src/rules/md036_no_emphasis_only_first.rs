use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug, Default)]
pub struct MD036NoEmphasisOnlyFirst;

impl MD036NoEmphasisOnlyFirst {
    fn is_entire_line_emphasized(line: &str) -> Option<(usize, String)> {
        let line = line.trim();
        
        // Skip if line is empty
        if line.is_empty() {
            return None;
        }
        
        // Check for *emphasis* pattern (entire line)
        let re_asterisk_single = Regex::new(r"^\*([^*\n]+)\*$").unwrap();
        if let Some(caps) = re_asterisk_single.captures(line) {
            return Some((1, caps.get(1).unwrap().as_str().trim().to_string()));
        }
        
        // Check for _emphasis_ pattern (entire line)
        let re_underscore_single = Regex::new(r"^_([^_\n]+)_$").unwrap();
        if let Some(caps) = re_underscore_single.captures(line) {
            return Some((1, caps.get(1).unwrap().as_str().trim().to_string()));
        }
        
        // Check for **strong** pattern (entire line)
        let re_asterisk_double = Regex::new(r"^\*\*([^*\n]+)\*\*$").unwrap();
        if let Some(caps) = re_asterisk_double.captures(line) {
            return Some((2, caps.get(1).unwrap().as_str().trim().to_string()));
        }
        
        // Check for __strong__ pattern (entire line)
        let re_underscore_double = Regex::new(r"^__([^_\n]+)__$").unwrap();
        if let Some(caps) = re_underscore_double.captures(line) {
            return Some((2, caps.get(1).unwrap().as_str().trim().to_string()));
        }
        
        None
    }

    fn is_in_code_block(&self, content: &str, line_num: usize) -> bool {
        let mut in_code_block = false;
        for (i, line) in content.lines().enumerate() {
            if line.trim().starts_with("```") || line.trim().starts_with("~~~") {
                in_code_block = !in_code_block;
            }
            if i + 1 == line_num {
                break;
            }
        }
        in_code_block
    }

    fn get_heading_for_emphasis(level: usize, text: &str) -> String {
        let prefix = "#".repeat(level);
        format!("{} {}", prefix, text)
    }
}

impl Rule for MD036NoEmphasisOnlyFirst {
    fn name(&self) -> &'static str {
        "MD036"
    }

    fn description(&self) -> &'static str {
        "No emphasis used instead of headings"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();

        for (i, line) in content.lines().enumerate() {
            if !self.is_in_code_block(content, i + 1) {
                if let Some((level, text)) = Self::is_entire_line_emphasized(line) {
                    warnings.push(LintWarning {
                        message: "Emphasis should not be used instead of a heading".to_string(),
                        line: i + 1,
                        column: 1,
                        fix: Some(Fix {
                            line: i + 1,
                            column: 1,
                            replacement: Self::get_heading_for_emphasis(level, &text),
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

        for i in 0..lines.len() {
            if !self.is_in_code_block(content, i + 1) {
                if let Some((level, text)) = Self::is_entire_line_emphasized(lines[i]) {
                    result.push_str(&Self::get_heading_for_emphasis(level, &text));
                } else {
                    result.push_str(lines[i]);
                }
            } else {
                result.push_str(lines[i]);
            }
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }

        Ok(result)
    }
} 