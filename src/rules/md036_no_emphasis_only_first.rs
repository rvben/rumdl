use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    // Optimize regex patterns with compilation once at startup
    static ref RE_ASTERISK_SINGLE: Regex = Regex::new(r"^\s*\*([^*\n]+)\*\s*$").unwrap();
    static ref RE_UNDERSCORE_SINGLE: Regex = Regex::new(r"^\s*_([^_\n]+)_\s*$").unwrap();
    static ref RE_ASTERISK_DOUBLE: Regex = Regex::new(r"^\s*\*\*([^*\n]+)\*\*\s*$").unwrap();
    static ref RE_UNDERSCORE_DOUBLE: Regex = Regex::new(r"^\s*__([^_\n]+)__\s*$").unwrap();
    
    // Add code block detection patterns
    static ref FENCED_CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,})").unwrap();
}

#[derive(Debug, Default)]
pub struct MD036NoEmphasisOnlyFirst;

impl MD036NoEmphasisOnlyFirst {
    fn is_entire_line_emphasized(line: &str) -> Option<(usize, String)> {
        let line = line.trim();
        
        // Fast path for empty lines and lines that don't contain emphasis markers
        if line.is_empty() || (!line.contains('*') && !line.contains('_')) {
            return None;
        }
        
        // Quick check: lines must start and end with emphasis markers
        let first_char = line.chars().next().unwrap();
        let last_char = line.chars().last().unwrap();
        
        if (first_char != '*' && first_char != '_') || (last_char != '*' && last_char != '_') {
            return None;
        }
        
        // Now check specific patterns
        // Check for *emphasis* pattern (entire line)
        if let Some(caps) = RE_ASTERISK_SINGLE.captures(line) {
            return Some((1, caps.get(1).unwrap().as_str().trim().to_string()));
        }
        
        // Check for _emphasis_ pattern (entire line)
        if let Some(caps) = RE_UNDERSCORE_SINGLE.captures(line) {
            return Some((1, caps.get(1).unwrap().as_str().trim().to_string()));
        }
        
        // Check for **strong** pattern (entire line)
        if let Some(caps) = RE_ASTERISK_DOUBLE.captures(line) {
            return Some((2, caps.get(1).unwrap().as_str().trim().to_string()));
        }
        
        // Check for __strong__ pattern (entire line)
        if let Some(caps) = RE_UNDERSCORE_DOUBLE.captures(line) {
            return Some((2, caps.get(1).unwrap().as_str().trim().to_string()));
        }
        
        None
    }

    fn precompute_code_blocks(content: &str) -> Vec<bool> {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = false;
        let mut result = vec![false; lines.len()];
        
        for (i, line) in lines.iter().enumerate() {
            if line.trim().starts_with("```") || line.trim().starts_with("~~~") {
                in_code_block = !in_code_block;
            }
            result[i] = in_code_block;
        }
        
        result
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
        "Emphasis should not be used instead of a heading"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        
        // Fast path for empty content
        if content.is_empty() {
            return Ok(warnings);
        }
        
        // Pre-compute code block states for the entire document
        let code_block_states = Self::precompute_code_blocks(content);

        for (i, line) in content.lines().enumerate() {
            // Skip obvious non-matches quickly
            if line.trim().is_empty() || (!line.contains('*') && !line.contains('_')) {
                continue;
            }
            
            // Check if in code block using pre-computed state
            if code_block_states[i] {
                continue;
            }
            
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

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        // Fast path for empty content
        if content.is_empty() {
            return Ok(String::new());
        }
        
        let mut result = String::with_capacity(content.len());
        let lines: Vec<&str> = content.lines().collect();
        
        // Pre-compute code block states for the entire document
        let code_block_states = Self::precompute_code_blocks(content);

        for i in 0..lines.len() {
            // Fast path for lines that are in a code block or don't have emphasis markers
            let line = lines[i];
            if code_block_states[i] || (!line.contains('*') && !line.contains('_')) {
                result.push_str(line);
            } else if let Some((level, text)) = Self::is_entire_line_emphasized(line) {
                result.push_str(&Self::get_heading_for_emphasis(level, &text));
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