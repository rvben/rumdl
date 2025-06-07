//!
//! Utility functions for detecting and handling code blocks and code spans in Markdown for rumdl.

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(```|~~~)").unwrap();
    static ref CODE_SPAN_PATTERN: Regex = Regex::new(r"`+").unwrap();
}

/// Utility functions for detecting and handling code blocks in Markdown
pub struct CodeBlockUtils;

impl CodeBlockUtils {
    /// Detect all code blocks in the content
    pub fn detect_code_blocks(content: &str) -> Vec<(usize, usize)> {
        let mut blocks = Vec::new();
        let mut in_code_block = false;
        let mut code_block_start = 0;

        // Pre-compute line positions for efficient offset calculation
        let lines: Vec<&str> = content.lines().collect();
        let mut line_positions = Vec::with_capacity(lines.len());
        let mut pos = 0;
        for line in &lines {
            line_positions.push(pos);
            pos += line.len() + 1; // +1 for newline
        }

        // Find fenced code blocks
        for (i, line) in lines.iter().enumerate() {
            let line_start = line_positions[i];

            if CODE_BLOCK_PATTERN.is_match(line.trim()) {
                if !in_code_block {
                    code_block_start = line_start;
                    in_code_block = true;
                } else {
                    let code_block_end = line_start + line.len();
                    blocks.push((code_block_start, code_block_end));
                    in_code_block = false;
                }
            }
        }

        // Handle unclosed code blocks
        if in_code_block {
            blocks.push((code_block_start, content.len()));
        }

        // Find indented code blocks (4+ spaces or tab at start of line)
        // According to CommonMark, indented code blocks must be preceded by a blank line
        // (unless they're at the start of the document or after a block-level element)
        let mut in_indented_block = false;
        let mut indented_block_start = 0;
        
        for (line_idx, line) in lines.iter().enumerate() {
            let line_start = if line_idx < line_positions.len() {
                line_positions[line_idx]
            } else {
                0
            };
            
            // Check if this line is indented code
            let is_indented = line.starts_with("    ") || line.starts_with("\t");
            
            // Check if this looks like a list item (has list marker after indentation)
            let trimmed = line.trim_start();
            let is_list_item = trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") ||
                               trimmed.chars().next().map_or(false, |c| c.is_numeric()) && 
                               trimmed.chars().nth(1).map_or(false, |c| c == '.' || c == ')');
            
            // Check if previous line was blank 
            let prev_blank = line_idx > 0 && lines[line_idx - 1].trim().is_empty();
            
            if is_indented && !line.trim().is_empty() && !is_list_item {
                if !in_indented_block {
                    // Only start an indented code block if preceded by a blank line
                    if prev_blank {
                        in_indented_block = true;
                        indented_block_start = line_start;
                    }
                    // Otherwise, this is just an indented line, not a code block
                }
            } else if in_indented_block {
                // End of indented code block
                let block_end = if line_idx > 0 && line_idx - 1 < line_positions.len() {
                    line_positions[line_idx - 1] + lines[line_idx - 1].len()
                } else {
                    line_start
                };
                blocks.push((indented_block_start, block_end));
                in_indented_block = false;
            }
        }
        
        // Handle indented block that goes to end of file
        if in_indented_block {
            blocks.push((indented_block_start, content.len()));
        }

        // Find inline code spans
        let mut i = 0;
        while i < content.len() {
            if let Some(m) = CODE_SPAN_PATTERN.find_at(content, i) {
                let backtick_length = m.end() - m.start();
                let start = m.start();

                // Find matching closing backticks
                if let Some(end_pos) = content[m.end()..].find(&"`".repeat(backtick_length)) {
                    let end = m.end() + end_pos + backtick_length;
                    blocks.push((start, end));
                    i = end;
                } else {
                    i = m.end();
                }
            } else {
                break;
            }
        }

        blocks.sort_by(|a, b| a.0.cmp(&b.0));
        blocks
    }

    /// Check if a position is within a code block or code span
    pub fn is_in_code_block_or_span(blocks: &[(usize, usize)], pos: usize) -> bool {
        blocks.iter().any(|&(start, end)| pos >= start && pos < end)
    }
}
