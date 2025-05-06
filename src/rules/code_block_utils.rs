use lazy_static::lazy_static;
use regex::Regex;
use std::fmt;

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
        FENCED_CODE_BLOCK_START.is_match(line)
            || FENCED_CODE_BLOCK_END.is_match(line)
            || ALTERNATE_FENCED_CODE_BLOCK_START.is_match(line)
            || ALTERNATE_FENCED_CODE_BLOCK_END.is_match(line)
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

    /// Extracts the language specifier from a fenced code block start line
    ///
    /// This function parses the line that starts a fenced code block (using either ``` or ~~~)
    /// and extracts the language specifier that follows the fence markers.
    ///
    /// # Parameters
    /// * `line` - The line of text that potentially contains a code block start with language specifier
    ///
    /// # Returns
    /// * `Some(String)` - The language specifier if found
    /// * `None` - If the line is not a code block start or has no language specifier
    ///
    /// # Examples
    /// ```
    /// use rumdl::rules::code_block_utils::CodeBlockUtils;
    ///
    /// let specifier = CodeBlockUtils::get_language_specifier("```rust");
    /// assert_eq!(specifier, Some("rust".to_string()));
    ///
    /// let specifier = CodeBlockUtils::get_language_specifier("~~~python");
    /// assert_eq!(specifier, Some("python".to_string()));
    ///
    /// let specifier = CodeBlockUtils::get_language_specifier("```");
    /// assert_eq!(specifier, None);
    /// ```
    pub fn get_language_specifier(line: &str) -> Option<String> {
        if FENCED_CODE_BLOCK_START.is_match(line)
            || ALTERNATE_FENCED_CODE_BLOCK_START.is_match(line)
        {
            let trimmed = line.trim_start();
            let after_fence = &trimmed[3..].trim_start();
            if !after_fence.is_empty() {
                return Some(after_fence.to_string());
            }
        }
        None
    }

    /// Identify which lines in the content are in code blocks
    ///
    /// This function analyzes Markdown content and determines which lines are part of code blocks,
    /// including both fenced code blocks (``` or ~~~) and indented code blocks.
    ///
    /// # Algorithm
    /// - Iterates through each line of content
    /// - Tracks state for fenced code blocks (toggled by fence delimiters)
    /// - Detects indented code blocks (4 spaces or 1 tab)
    /// - Handles nested code blocks appropriately
    ///
    /// # Parameters
    /// * `content` - The full Markdown content to analyze
    ///
    /// # Returns
    /// A vector of boolean values with the same length as the number of lines in the input content.
    /// Each element indicates whether the corresponding line is inside a code block:
    /// * `true` - The line is inside a code block
    /// * `false` - The line is not inside a code block
    ///
    /// # Examples
    /// ```
    /// use rumdl::rules::code_block_utils::CodeBlockUtils;
    ///
    /// let content = "Some text\n```rust\nlet x = 1;\n```\nMore text";
    /// let in_code_block = CodeBlockUtils::identify_code_block_lines(content);
    /// assert_eq!(in_code_block, vec![false, true, true, true, false]);
    /// ```
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
                // Do not mark as code block if the line is a list item
                lazy_static! {
                    static ref LIST_ITEM_RE: Regex =
                        Regex::new(r"^(\s*)([*+-]|\d+[.)])(\s*)(.*)$").unwrap();
                }
                if (line.starts_with("    ") || INDENTED_CODE_BLOCK.is_match(line))
                    && !LIST_ITEM_RE.is_match(line)
                {
                    in_code_block[i] = true;
                }
            }
        }

        in_code_block
    }
}

// Cached regex patterns for better performance
lazy_static! {
    static ref FENCED_CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(?:```|~~~)").unwrap();
    static ref INDENTED_CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s{4,})").unwrap();
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
        self.block_states
            .iter()
            .any(|state| *state != CodeBlockState::None)
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
            fence_marker = if line.trim().starts_with("```") {
                "```"
            } else {
                "~~~"
            };
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

    // Simplify by using a safer character-based approach
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Skip escaped backticks
        if i > 0 && chars[i] == '`' && chars[i - 1] == '\\' {
            i += 1;
            continue;
        }

        // Look for backtick sequences
        if chars[i] == '`' {
            let mut backtick_count = 1;
            let start_idx = i;

            // Count consecutive backticks
            i += 1;
            while i < chars.len() && chars[i] == '`' {
                backtick_count += 1;
                i += 1;
            }

            // Skip this if it looks like a code block delimiter
            // This prevents confusion between code spans and code blocks
            if is_likely_code_block_delimiter(&chars, start_idx) {
                continue;
            }

            // Skip over content until we find a matching sequence of backticks
            let mut j = i;
            let mut found_closing = false;

            while j < chars.len() {
                // Skip escaped backticks in the search too
                if j > 0 && chars[j] == '`' && chars[j - 1] == '\\' {
                    j += 1;
                    continue;
                }

                if chars[j] == '`' {
                    let mut closing_count = 1;
                    let potential_end = j;

                    // Count consecutive backticks
                    j += 1;
                    while j < chars.len() && chars[j] == '`' {
                        closing_count += 1;
                        j += 1;
                    }

                    // If we found a matching sequence, record the span
                    if closing_count == backtick_count {
                        // Convert from character indices to byte indices
                        let start_byte = chars[..start_idx].iter().map(|c| c.len_utf8()).sum();
                        let end_byte = chars[..potential_end + closing_count]
                            .iter()
                            .map(|c| c.len_utf8())
                            .sum();

                        spans.push((start_byte, end_byte));
                        i = j; // Resume search after this span
                        found_closing = true;
                        break;
                    }
                }

                j += 1;
            }

            if !found_closing {
                // If we didn't find a matching sequence, continue from where we left off
                continue;
            }
        } else {
            i += 1;
        }
    }

    spans
}

// Helper function to determine if a backtick sequence is likely a code block delimiter
fn is_likely_code_block_delimiter(chars: &[char], start_idx: usize) -> bool {
    let mut count = 0;
    let mut i = start_idx;

    // Count the backticks
    while i < chars.len() && chars[i] == '`' {
        count += 1;
        i += 1;
    }

    if count < 3 {
        // Not enough backticks for a code block
        return false;
    }

    // Check if this is at the start of a line or after only whitespace
    let mut j = start_idx;
    if j > 0 {
        j -= 1;
        // Go back to the beginning of the line
        while j > 0 && chars[j] != '\n' {
            if !chars[j].is_whitespace() {
                // Non-whitespace character before the backticks on the same line
                return false;
            }
            j -= 1;
        }
    }

    true
}

/// The style for code blocks (MD046)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum CodeBlockStyle {
    /// Consistent with the first code block style found
    #[default]
    Consistent,
    /// Indented code blocks (4 spaces)
    Indented,
    /// Fenced code blocks (``` or ~~~)
    Fenced,
}

impl fmt::Display for CodeBlockStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeBlockStyle::Fenced => write!(f, "fenced"),
            CodeBlockStyle::Indented => write!(f, "indented"),
            CodeBlockStyle::Consistent => write!(f, "consistent"),
        }
    }
}
