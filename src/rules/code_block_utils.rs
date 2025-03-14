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
    
    /// Get the language specifier from a code block start line
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
    
    /// Identify which lines in the content are in code blocks
    /// Returns a vector of booleans, where each element indicates if the corresponding line is in a code block
    pub fn identify_code_block_lines(content: &str) -> Vec<bool> {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_code_block = vec![false; lines.len()];
        
        let mut in_fenced_code = false;
        let mut in_alternate_fenced = false;
        
        for (i, line) in lines.iter().enumerate() {
            // Quick check for code fence markers with literal prefixes
            let trimmed = line.trim_start();
            
            if trimmed.starts_with("```") {
                if FENCED_CODE_BLOCK_START.is_match(line) {
                    in_fenced_code = !in_fenced_code;
                    in_code_block[i] = true; // Mark the delimiter line as part of the code block
                } else if in_fenced_code && FENCED_CODE_BLOCK_END.is_match(line) {
                    in_fenced_code = false;
                    in_code_block[i] = true; // Mark the delimiter line as part of the code block
                }
            } else if trimmed.starts_with("~~~") {
                if ALTERNATE_FENCED_CODE_BLOCK_START.is_match(line) {
                    in_alternate_fenced = !in_alternate_fenced;
                    in_code_block[i] = true; // Mark the delimiter line as part of the code block
                } else if in_alternate_fenced && ALTERNATE_FENCED_CODE_BLOCK_END.is_match(line) {
                    in_alternate_fenced = false;
                    in_code_block[i] = true; // Mark the delimiter line as part of the code block
                }
            }
            
            // If we're in a code fence, mark the line
            if in_fenced_code || in_alternate_fenced {
                in_code_block[i] = true;
            } else if !in_code_block[i] {
                // Check for indented code blocks only if not already marked
                in_code_block[i] = line.starts_with("    ") || INDENTED_CODE_BLOCK.is_match(line);
            }
        }
        
        in_code_block
    }
}

// Cached regex patterns for better performance
lazy_static! {
    static ref FENCED_CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(?:```|~~~)").unwrap();
    static ref INDENTED_CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(?: {4}|\t)").unwrap();
    static ref BACKTICK_PATTERN: Regex = Regex::new(r"(`+)").unwrap();
}

/// Tracks which lines are inside code blocks and their types
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CodeBlockState {
    None,
    Fenced,
    Indented,
}

/// Structure to hold pre-computed code block information
#[derive(Debug)]
pub struct CodeBlockInfo {
    /// Whether each line is in a code block, and which type
    pub block_states: Vec<CodeBlockState>,
    /// Positions of code spans in the text (start, end)
    pub code_spans: Vec<(usize, usize)>,
    /// The original content used to create this info
    content: String,
}

impl CodeBlockInfo {
    /// Create a new CodeBlockInfo by analyzing the content
    pub fn new(content: &str) -> Self {
        let block_states = compute_code_blocks(content);
        let code_spans = compute_code_spans(content);
        
        CodeBlockInfo {
            block_states,
            code_spans,
            content: content.to_string(),
        }
    }
    
    /// Check if a line is inside a code block
    pub fn is_in_code_block(&self, line_index: usize) -> bool {
        if line_index < self.block_states.len() {
            self.block_states[line_index] != CodeBlockState::None
        } else {
            false
        }
    }
    
    /// Check if a position is inside a code span
    pub fn is_in_code_span(&self, line_index: usize, column_index: usize) -> bool {
        // Calculate absolute position (this assumes content is ASCII-only)
        let mut position = 0;
        let content_lines: Vec<&str> = self.content.lines().collect();
        
        for i in 0..line_index {
            if i < content_lines.len() {
                position += content_lines[i].len() + 1; // +1 for newline
            }
        }
        
        if line_index < content_lines.len() {
            // Add column position
            let line = content_lines[line_index];
            if column_index < line.len() {
                position += column_index;
                
                // Check if position is in any code span
                for &(start, end) in &self.code_spans {
                    if position >= start && position <= end {
                        return true;
                    }
                }
            }
        }
        
        false
    }
    
    /// Quick check if content contains any code blocks
    pub fn has_code_blocks(&self) -> bool {
        self.block_states.iter().any(|state| *state != CodeBlockState::None)
    }
    
    /// Quick check if content contains any code spans
    pub fn has_code_spans(&self) -> bool {
        !self.code_spans.is_empty()
    }
}

/// Compute which lines are in code blocks and what type
pub fn compute_code_blocks(content: &str) -> Vec<CodeBlockState> {
    let mut in_fenced_block = false;
    let mut result = Vec::new();
    let mut fence_marker = "";
    
    for line in content.lines() {
        if in_fenced_block {
            if line.trim().starts_with(fence_marker) {
                in_fenced_block = false;
                result.push(CodeBlockState::Fenced); // The closing fence is still part of the block
            } else {
                result.push(CodeBlockState::Fenced);
            }
        } else if FENCED_CODE_BLOCK_PATTERN.is_match(line) {
            in_fenced_block = true;
            fence_marker = if line.trim().starts_with("```") { "```" } else { "~~~" };
            result.push(CodeBlockState::Fenced); // The opening fence is part of the block
        } else if INDENTED_CODE_BLOCK_PATTERN.is_match(line) && !line.trim().is_empty() {
            result.push(CodeBlockState::Indented);
        } else {
            result.push(CodeBlockState::None);
        }
    }
    
    result
}

/// Compute positions of code spans in the text
pub fn compute_code_spans(content: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut position = 0;
    
    // First try to use regex to find backticks
    for cap in BACKTICK_PATTERN.captures_iter(content) {
        let backticks = &cap[1];
        let start_pos = content[..position + cap.get(1).unwrap().start()].len();
        
        // Look for matching closing backticks
        if let Some(end_pos) = find_closing_backtick(content, start_pos + backticks.len(), backticks.len()) {
            spans.push((start_pos, end_pos));
        }
        
        position += cap.get(0).unwrap().end();
    }
    
    // If regex fails (e.g., in complex cases), fall back to manual parsing
    if spans.is_empty() {
        let mut in_code = false;
        let mut backtick_count = 0;
        let mut start = 0;
        
        for (i, c) in content.chars().enumerate() {
            if c == '`' {
                if !in_code {
                    in_code = true;
                    backtick_count = 1;
                    start = i;
                } else if backtick_count == 1 {
                    in_code = false;
                    spans.push((start, i));
                }
            } else if in_code && backtick_count == 1 {
                // We're in a single-backtick code span
            }
        }
    }
    
    spans
}

/// Find closing backtick sequence of the same length
fn find_closing_backtick(content: &str, start_pos: usize, length: usize) -> Option<usize> {
    let backtick_sequence = "`".repeat(length);
    let substring = &content[start_pos..];
    
    if let Some(pos) = substring.find(&backtick_sequence) {
        Some(start_pos + pos + length - 1)
    } else {
        None
    }
} 