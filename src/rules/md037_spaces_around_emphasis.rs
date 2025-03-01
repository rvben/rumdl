use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;

#[derive(Debug, Default)]
pub struct MD037SpacesAroundEmphasis;

lazy_static! {
    // Regex to identify code blocks
    static ref CODE_BLOCK_DELIMITER: Regex = Regex::new(r"^(```|~~~)").unwrap();
    
    // Regex for emphasis patterns with spaces inside
    static ref SINGLE_ASTERISK_WITH_SPACE: Regex = Regex::new(r"(?:^|\s|\(|\[|\{|>|_|\*)(\*)(\s+)([^\s\*][^\*\n]*)(\s+)(\*)(?:$|\s|\.|\,|\?|\!|\)|\]|\}|<|_|\*)").unwrap();
    
    static ref DOUBLE_ASTERISK_WITH_SPACE: Regex = Regex::new(r"(?:^|\s|\(|\[|\{|>|_|\*)(\*\*)(\s+)([^\s\*][^\*\n]*)(\s+)(\*\*)(?:$|\s|\.|\,|\?|\!|\)|\]|\}|<|_|\*)").unwrap();
    
    static ref SINGLE_UNDERSCORE_WITH_SPACE: Regex = Regex::new(r"(?:^|\s|\(|\[|\{|>|\*|_)(_)(\s+)([^\s_][^_\n]*)(\s+)(_)(?:$|\s|\.|\,|\?|\!|\)|\]|\}|<|\*|_)").unwrap();
    
    static ref DOUBLE_UNDERSCORE_WITH_SPACE: Regex = Regex::new(r"(?:^|\s|\(|\[|\{|>|\*|_)(__)(\s+)([^\s_][^_\n]*)(\s+)(__)(?:$|\s|\.|\,|\?|\!|\)|\]|\}|<|\*|_)").unwrap();
    
    // Regex for identifying list items to avoid modifying them
    static ref LIST_ITEM: Regex = Regex::new(r"^\s*([*+-]|\d+\.)\s+").unwrap();
}

impl Rule for MD037SpacesAroundEmphasis {
    fn name(&self) -> &'static str {
        "MD037"
    }

    fn description(&self) -> &'static str {
        "Spaces inside emphasis markers"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        
        for (line_num, line) in lines.iter().enumerate() {
            if CODE_BLOCK_DELIMITER.is_match(line.trim()) {
                in_code_block = !in_code_block;
                continue;
            }
            
            if in_code_block || LIST_ITEM.is_match(line) {
                continue;
            }
            
            // Check for spaces inside single asterisk emphasis
            for cap in SINGLE_ASTERISK_WITH_SPACE.captures_iter(line) {
                let start_pos = cap.get(0).unwrap().start();
                let content = &cap[3];
                let marker = &cap[1];
                
                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: start_pos + 1,
                    message: "Spaces inside * emphasis markers".to_string(),
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: start_pos + 1,
                        replacement: format!("{}{}{}", marker, content, marker),
                    }),
                });
            }
            
            // Check for spaces inside double asterisk emphasis
            for cap in DOUBLE_ASTERISK_WITH_SPACE.captures_iter(line) {
                let start_pos = cap.get(0).unwrap().start();
                let content = &cap[3];
                let marker = &cap[1];
                
                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: start_pos + 1,
                    message: "Spaces inside ** emphasis markers".to_string(),
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: start_pos + 1,
                        replacement: format!("{}{}{}", marker, content, marker),
                    }),
                });
            }
            
            // Check for spaces inside single underscore emphasis
            for cap in SINGLE_UNDERSCORE_WITH_SPACE.captures_iter(line) {
                let start_pos = cap.get(0).unwrap().start();
                let content = &cap[3];
                let marker = &cap[1];
                
                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: start_pos + 1,
                    message: "Spaces inside _ emphasis markers".to_string(),
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: start_pos + 1,
                        replacement: format!("{}{}{}", marker, content, marker),
                    }),
                });
            }
            
            // Check for spaces inside double underscore emphasis
            for cap in DOUBLE_UNDERSCORE_WITH_SPACE.captures_iter(line) {
                let start_pos = cap.get(0).unwrap().start();
                let content = &cap[3];
                let marker = &cap[1];
                
                warnings.push(LintWarning {
                    line: line_num + 1,
                    column: start_pos + 1,
                    message: "Spaces inside __ emphasis markers".to_string(),
                    fix: Some(Fix {
                        line: line_num + 1,
                        column: start_pos + 1,
                        replacement: format!("{}{}{}", marker, content, marker),
                    }),
                });
            }
        }
        
        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        
        for (line_num, line) in lines.iter().enumerate() {
            let mut modified_line = line.to_string();
            
            if CODE_BLOCK_DELIMITER.is_match(line.trim()) {
                in_code_block = !in_code_block;
                result.push_str(&modified_line);
                if line_num < lines.len() - 1 || content.ends_with('\n') {
                    result.push('\n');
                }
                continue;
            }
            
            if in_code_block || LIST_ITEM.is_match(line) {
                result.push_str(&modified_line);
                if line_num < lines.len() - 1 || content.ends_with('\n') {
                    result.push('\n');
                }
                continue;
            }
            
            // Fix single asterisk emphasis with spaces
            modified_line = SINGLE_ASTERISK_WITH_SPACE.replace_all(&modified_line, |caps: &regex::Captures| {
                format!("{}{}{}", &caps[1], &caps[3], &caps[5])
            }).to_string();
            
            // Fix double asterisk emphasis with spaces
            modified_line = DOUBLE_ASTERISK_WITH_SPACE.replace_all(&modified_line, |caps: &regex::Captures| {
                format!("{}{}{}", &caps[1], &caps[3], &caps[5])
            }).to_string();
            
            // Fix single underscore emphasis with spaces
            modified_line = SINGLE_UNDERSCORE_WITH_SPACE.replace_all(&modified_line, |caps: &regex::Captures| {
                format!("{}{}{}", &caps[1], &caps[3], &caps[5])
            }).to_string();
            
            // Fix double underscore emphasis with spaces
            modified_line = DOUBLE_UNDERSCORE_WITH_SPACE.replace_all(&modified_line, |caps: &regex::Captures| {
                format!("{}{}{}", &caps[1], &caps[3], &caps[5])
            }).to_string();
            
            result.push_str(&modified_line);
            if line_num < lines.len() - 1 || content.ends_with('\n') {
                result.push('\n');
            }
        }
        
        Ok(result)
    }
} 