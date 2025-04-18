//! Utilities for position/range conversions

use std::collections::HashSet;
use std::ops::Range;

#[derive(Debug)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    content: String,
    code_block_lines: Option<HashSet<usize>>,
}

impl LineIndex {
    pub fn new(content: String) -> Self {
        let mut line_starts = vec![0];
        let mut pos = 0;

        for c in content.chars() {
            pos += c.len_utf8();
            if c == '\n' {
                line_starts.push(pos);
            }
        }

        let mut index = Self {
            line_starts,
            content,
            code_block_lines: None,
        };

        // Pre-compute code block lines for better performance
        index.compute_code_block_lines();

        index
    }

    pub fn line_col_to_byte_range(&self, line: usize, column: usize) -> Range<usize> {
        let line = line.saturating_sub(1);
        let line_start = *self.line_starts.get(line).unwrap_or(&self.content.len());

        let current_line = self.content.lines().nth(line).unwrap_or("");
        let col = column.clamp(1, current_line.len() + 1);

        let start = line_start + col - 1;
        start..start
    }

    /// Get the global start byte offset for a given 1-based line number.
    pub fn get_line_start_byte(&self, line_num: usize) -> Option<usize> {
        if line_num == 0 {
            return None; // Lines are 1-based
        }
        // line_num is 1-based, line_starts index is 0-based
        self.line_starts.get(line_num - 1).cloned()
    }

    /// Check if the line at the given index is within a code block
    pub fn is_code_block(&self, line: usize) -> bool {
        if let Some(ref code_block_lines) = self.code_block_lines {
            code_block_lines.contains(&line)
        } else {
            // Fallback to a simpler check if pre-computation wasn't done
            self.is_code_fence(line)
        }
    }

    /// Check if the line is a code fence marker (``` or ~~~)
    pub fn is_code_fence(&self, line: usize) -> bool {
        self.content.lines().nth(line).map_or(false, |l| {
            let trimmed = l.trim();
            trimmed.starts_with("```") || trimmed.starts_with("~~~")
        })
    }

    /// Check if the line is a tilde code fence marker (~~~)
    pub fn is_tilde_code_block(&self, line: usize) -> bool {
        self.content
            .lines()
            .nth(line)
            .map_or(false, |l| l.trim().starts_with("~~~"))
    }

    /// Get a reference to the content
    pub fn get_content(&self) -> &str {
        &self.content
    }

    /// Pre-compute which lines are within code blocks for faster lookup
    fn compute_code_block_lines(&mut self) {
        let mut code_block_lines = HashSet::new();
        let lines: Vec<&str> = self.content.lines().collect();

        // Initialize block tracking
        let mut in_block = false;
        let mut active_fence_type = ' '; // '`' or '~'
        let mut block_indent = 0;
        let mut block_fence_length = 0;
        let mut in_markdown_block = false;
        let mut nested_fence_start = None;
        let mut nested_fence_end = None;

        // Process each line
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let indent = line.len() - trimmed.len();

            // 1. Detect indented code blocks (independent of fenced code blocks)
            if line.starts_with("    ") || line.starts_with("\t") {
                code_block_lines.insert(i);
                continue; // Skip further processing for indented code blocks
            }

            // 2. Handle fenced code blocks (backticks and tildes)
            if !in_block {
                // Check for opening fences
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    let char_type = if trimmed.starts_with("```") { '`' } else { '~' };
                    let count = trimmed.chars().take_while(|&c| c == char_type).count();
                    let info_string = if trimmed.len() > count {
                        trimmed[count..].trim()
                    } else {
                        ""
                    };

                    // Mark the start of a new code block
                    in_block = true;
                    active_fence_type = char_type;
                    block_indent = indent;
                    block_fence_length = count;
                    in_markdown_block = info_string == "markdown";
                    nested_fence_start = None;
                    nested_fence_end = None;

                    code_block_lines.insert(i);
                }
            } else {
                // We're inside a code block
                code_block_lines.insert(i);

                // Detection of nested fences in markdown blocks
                if in_markdown_block && nested_fence_start.is_none() && trimmed.starts_with("```") {
                    // Check if this looks like a nested fence opening (has content after the backticks)
                    let count = trimmed.chars().take_while(|&c| c == '`').count();
                    let remaining = if trimmed.len() > count {
                        trimmed[count..].trim()
                    } else {
                        ""
                    };

                    if !remaining.is_empty() {
                        nested_fence_start = Some(i);
                    }
                }

                // Check if we've found a nested fence end (only if we have a start)
                if in_markdown_block
                    && nested_fence_start.is_some()
                    && nested_fence_end.is_none()
                    && trimmed.starts_with("```")
                    && trimmed.trim_start_matches('`').trim().is_empty()
                {
                    nested_fence_end = Some(i);
                }

                // Check if this line matches the closing fence pattern for the outer block
                if trimmed.starts_with(&active_fence_type.to_string().repeat(3)) {
                    let count = trimmed
                        .chars()
                        .take_while(|&c| c == active_fence_type)
                        .count();
                    let remaining = if trimmed.len() > count {
                        trimmed[count..].trim()
                    } else {
                        ""
                    };

                    // A line is a closing fence if:
                    // 1. It uses the same fence character as the opening fence
                    // 2. It has at least as many fence characters as the opening fence
                    // 3. It has no content after the fence characters (except for whitespace)
                    // 4. Its indentation level is less than or equal to the opening fence
                    let is_valid_closing_fence = count >= block_fence_length
                        && remaining.is_empty()
                        && indent <= block_indent;

                    // For nested code blocks in markdown, the first backtick fence after the nested content
                    // should be recognized as the closing fence for the outer block
                    let is_nested_closing =
                        nested_fence_end.is_some() && i == nested_fence_end.unwrap();

                    // Skip nested closing fences
                    if is_valid_closing_fence && !is_nested_closing {
                        in_block = false;
                        in_markdown_block = false;
                    }
                }
            }
        }

        self.code_block_lines = Some(code_block_lines);
    }
}
