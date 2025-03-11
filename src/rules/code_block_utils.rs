use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    // Standard code block detection patterns
    static ref FENCED_CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)```(?:[^`\r\n]*)$").unwrap();
    static ref FENCED_CODE_BLOCK_END: Regex = Regex::new(r"^(\s*)```\s*$").unwrap();
    static ref ALTERNATE_FENCED_CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)~~~(?:[^~\r\n]*)$").unwrap();
    static ref ALTERNATE_FENCED_CODE_BLOCK_END: Regex = Regex::new(r"^(\s*)~~~\s*$").unwrap();
    static ref INDENTED_CODE_BLOCK: Regex = Regex::new(r"^(\s{4,})").unwrap();
}

/// Utility functions for detecting and handling code blocks in Markdown documents
pub struct CodeBlockUtils;

impl CodeBlockUtils {
    /// Check if a line is inside a code block
    pub fn is_in_code_block(content: &str, line_num: usize) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        if line_num >= lines.len() {
            return false;
        }
        
        let mut in_fenced_code = false;
        let mut in_alternate_fenced = false;
        
        for (i, line) in lines.iter().enumerate() {
            if i > line_num {
                break;
            }
            
            if FENCED_CODE_BLOCK_START.is_match(line) {
                in_fenced_code = !in_fenced_code;
            } else if FENCED_CODE_BLOCK_END.is_match(line) && in_fenced_code {
                in_fenced_code = false;
            } else if ALTERNATE_FENCED_CODE_BLOCK_START.is_match(line) {
                in_alternate_fenced = !in_alternate_fenced;
            } else if ALTERNATE_FENCED_CODE_BLOCK_END.is_match(line) && in_alternate_fenced {
                in_alternate_fenced = false;
            }
        }
        
        // Check if the current line is indented as code block
        if line_num < lines.len() && INDENTED_CODE_BLOCK.is_match(lines[line_num]) {
            return true;
        }
        
        // Return true if we're in any type of code block
        in_fenced_code || in_alternate_fenced
    }
    
    /// Check if a line is a code block delimiter (start or end)
    pub fn is_code_block_delimiter(line: &str) -> bool {
        FENCED_CODE_BLOCK_START.is_match(line) || 
        FENCED_CODE_BLOCK_END.is_match(line) || 
        ALTERNATE_FENCED_CODE_BLOCK_START.is_match(line) || 
        ALTERNATE_FENCED_CODE_BLOCK_END.is_match(line)
    }
    
    /// Check if a line is the start of a code block
    pub fn is_code_block_start(line: &str) -> bool {
        FENCED_CODE_BLOCK_START.is_match(line) || ALTERNATE_FENCED_CODE_BLOCK_START.is_match(line)
    }
    
    /// Check if a line is the end of a code block
    pub fn is_code_block_end(line: &str) -> bool {
        FENCED_CODE_BLOCK_END.is_match(line) || ALTERNATE_FENCED_CODE_BLOCK_END.is_match(line)
    }
    
    /// Check if a line is an indented code block
    pub fn is_indented_code_block(line: &str) -> bool {
        INDENTED_CODE_BLOCK.is_match(line)
    }
    
    /// Extract the language specifier from a fenced code block start
    pub fn get_language_specifier(line: &str) -> Option<String> {
        if FENCED_CODE_BLOCK_START.is_match(line) {
            let trimmed = line.trim_start();
            let after_fence = &trimmed[3..].trim_start();
            if !after_fence.is_empty() {
                return Some(after_fence.to_string());
            }
        } else if ALTERNATE_FENCED_CODE_BLOCK_START.is_match(line) {
            let trimmed = line.trim_start();
            let after_fence = &trimmed[3..].trim_start();
            if !after_fence.is_empty() {
                return Some(after_fence.to_string());
            }
        }
        None
    }
} 