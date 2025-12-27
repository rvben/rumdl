use crate::utils::element_cache::ElementCache;
use crate::utils::range_utils::LineIndex;
use regex::Regex;
use std::fmt;
use std::sync::LazyLock;

// Standard code block detection patterns
static FENCED_CODE_BLOCK_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)```(?:[^`\r\n]*)$").unwrap());
static FENCED_CODE_BLOCK_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)```\s*$").unwrap());
static ALTERNATE_FENCED_CODE_BLOCK_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)~~~(?:[^~\r\n]*)$").unwrap());
static ALTERNATE_FENCED_CODE_BLOCK_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)~~~\s*$").unwrap());
static LIST_ITEM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)([*+-]|\d+[.)])(\s*)(.*)$").unwrap());

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
        if line_num < lines.len() && Self::is_indented_code_block(lines[line_num]) {
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

    /// Check if a line is an indented code block (4+ columns of leading whitespace)
    pub fn is_indented_code_block(line: &str) -> bool {
        // Use proper tab expansion to calculate effective indentation
        ElementCache::calculate_indentation_width_default(line) >= 4
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
    /// use rumdl_lib::rules::code_block_utils::CodeBlockUtils;
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
        if FENCED_CODE_BLOCK_START.is_match(line) || ALTERNATE_FENCED_CODE_BLOCK_START.is_match(line) {
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
    /// use rumdl_lib::rules::code_block_utils::CodeBlockUtils;
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
                if ElementCache::calculate_indentation_width_default(line) >= 4 && !LIST_ITEM_RE.is_match(line) {
                    in_code_block[i] = true;
                }
            }
        }

        in_code_block
    }
}

// Cached regex patterns for better performance
static FENCED_CODE_BLOCK_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:```|~~~)").unwrap());

/// Tracks which lines are inside code blocks and their types
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CodeBlockState {
    None,
    Fenced,
    Indented,
}

/// Structure to hold pre-computed code block information
#[derive(Debug)]
pub struct CodeBlockInfo<'a> {
    /// Whether each line is in a code block, and which type
    pub block_states: Vec<CodeBlockState>,
    /// Positions of code spans in the text (start, end)
    pub code_spans: Vec<(usize, usize)>,
    /// The original content used to create this info
    content: &'a str,
    /// LineIndex for correct byte position calculations across all line ending types
    line_index: LineIndex<'a>,
}

impl<'a> CodeBlockInfo<'a> {
    /// Create a new CodeBlockInfo by analyzing the content
    pub fn new(content: &'a str) -> Self {
        let block_states = compute_code_blocks(content);
        let code_spans = compute_code_spans(content);
        let line_index = LineIndex::new(content);

        CodeBlockInfo {
            block_states,
            code_spans,
            content,
            line_index,
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
        // Calculate absolute position using LineIndex for correct handling of all line ending types
        let line_start = self
            .line_index
            .get_line_start_byte(line_index + 1)
            .unwrap_or(self.content.len());
        let position = line_start + column_index;

        // Check if position is in any code span
        for &(start, end) in &self.code_spans {
            if position >= start && position <= end {
                return true;
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
        } else if !line.trim().is_empty() {
            // Use proper tab expansion to check for indented code block
            if ElementCache::calculate_indentation_width_default(line) >= 4 {
                result.push(CodeBlockState::Indented);
            } else {
                result.push(CodeBlockState::None);
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_in_code_block() {
        let content = "Normal text
```rust
let x = 1;
```
More text";

        assert!(!CodeBlockUtils::is_in_code_block(content, 0));
        assert!(CodeBlockUtils::is_in_code_block(content, 1));
        assert!(CodeBlockUtils::is_in_code_block(content, 2));
        assert!(!CodeBlockUtils::is_in_code_block(content, 3)); // Closing ``` ends the block
        assert!(!CodeBlockUtils::is_in_code_block(content, 4));

        // Test with alternate fence
        let content2 = "Text\n~~~\ncode\n~~~\nEnd";
        assert!(!CodeBlockUtils::is_in_code_block(content2, 0));
        assert!(CodeBlockUtils::is_in_code_block(content2, 1));
        assert!(CodeBlockUtils::is_in_code_block(content2, 2));
        assert!(!CodeBlockUtils::is_in_code_block(content2, 3)); // Closing ~~~ ends the block
        assert!(!CodeBlockUtils::is_in_code_block(content2, 4));

        // Test indented code block
        let content3 = "Normal\n    indented code\nNormal";
        assert!(!CodeBlockUtils::is_in_code_block(content3, 0));
        assert!(CodeBlockUtils::is_in_code_block(content3, 1));
        assert!(!CodeBlockUtils::is_in_code_block(content3, 2));

        // Test out of bounds
        assert!(!CodeBlockUtils::is_in_code_block("test", 10));
    }

    #[test]
    fn test_is_code_block_delimiter() {
        assert!(CodeBlockUtils::is_code_block_delimiter("```"));
        assert!(CodeBlockUtils::is_code_block_delimiter("```rust"));
        assert!(CodeBlockUtils::is_code_block_delimiter("  ```"));
        assert!(CodeBlockUtils::is_code_block_delimiter("~~~"));
        assert!(CodeBlockUtils::is_code_block_delimiter("~~~python"));

        assert!(!CodeBlockUtils::is_code_block_delimiter("Normal text"));
        assert!(!CodeBlockUtils::is_code_block_delimiter("``"));
        assert!(!CodeBlockUtils::is_code_block_delimiter("~"));
        assert!(!CodeBlockUtils::is_code_block_delimiter(""));
    }

    #[test]
    fn test_is_code_block_start() {
        assert!(CodeBlockUtils::is_code_block_start("```"));
        assert!(CodeBlockUtils::is_code_block_start("```rust"));
        assert!(CodeBlockUtils::is_code_block_start("~~~"));
        assert!(CodeBlockUtils::is_code_block_start("~~~python"));
        assert!(CodeBlockUtils::is_code_block_start("  ```"));

        assert!(!CodeBlockUtils::is_code_block_start("Normal text"));
        assert!(!CodeBlockUtils::is_code_block_start(""));
    }

    #[test]
    fn test_is_code_block_end() {
        assert!(CodeBlockUtils::is_code_block_end("```"));
        assert!(CodeBlockUtils::is_code_block_end("~~~"));
        assert!(CodeBlockUtils::is_code_block_end("  ```"));
        assert!(CodeBlockUtils::is_code_block_end("```  "));

        // Language specifiers make it a start, not end
        assert!(!CodeBlockUtils::is_code_block_end("```rust"));
        assert!(!CodeBlockUtils::is_code_block_end("~~~python"));
        assert!(!CodeBlockUtils::is_code_block_end("Normal text"));
    }

    #[test]
    fn test_is_indented_code_block() {
        assert!(CodeBlockUtils::is_indented_code_block("    code"));
        assert!(CodeBlockUtils::is_indented_code_block("        more indented"));

        // Tab expansion per CommonMark: tabs expand to next tab stop (columns 4, 8, 12, ...)
        assert!(CodeBlockUtils::is_indented_code_block("\tcode")); // tab → column 4
        assert!(CodeBlockUtils::is_indented_code_block("\t\tcode")); // 2 tabs → column 8
        assert!(CodeBlockUtils::is_indented_code_block("  \tcode")); // 2 spaces + tab → column 4
        assert!(CodeBlockUtils::is_indented_code_block(" \tcode")); // 1 space + tab → column 4
        assert!(CodeBlockUtils::is_indented_code_block("   \tcode")); // 3 spaces + tab → column 4

        assert!(!CodeBlockUtils::is_indented_code_block("   code")); // Only 3 spaces
        assert!(!CodeBlockUtils::is_indented_code_block("normal text"));
        assert!(!CodeBlockUtils::is_indented_code_block(""));
    }

    #[test]
    fn test_get_language_specifier() {
        assert_eq!(
            CodeBlockUtils::get_language_specifier("```rust"),
            Some("rust".to_string())
        );
        assert_eq!(
            CodeBlockUtils::get_language_specifier("~~~python"),
            Some("python".to_string())
        );
        assert_eq!(
            CodeBlockUtils::get_language_specifier("```javascript"),
            Some("javascript".to_string())
        );
        assert_eq!(
            CodeBlockUtils::get_language_specifier("  ```rust"),
            Some("rust".to_string())
        );
        assert_eq!(
            CodeBlockUtils::get_language_specifier("```rust ignore"),
            Some("rust ignore".to_string())
        );

        assert_eq!(CodeBlockUtils::get_language_specifier("```"), None);
        assert_eq!(CodeBlockUtils::get_language_specifier("~~~"), None);
        assert_eq!(CodeBlockUtils::get_language_specifier("Normal text"), None);
        assert_eq!(CodeBlockUtils::get_language_specifier(""), None);
    }

    #[test]
    fn test_identify_code_block_lines() {
        let content = "Normal text
```rust
let x = 1;
```
More text";

        let result = CodeBlockUtils::identify_code_block_lines(content);
        assert_eq!(result, vec![false, true, true, true, false]);

        // Test with alternate fence
        let content2 = "Text\n~~~\ncode\n~~~\nEnd";
        let result2 = CodeBlockUtils::identify_code_block_lines(content2);
        assert_eq!(result2, vec![false, true, true, true, false]);

        // Test with indented code
        let content3 = "Normal\n    code\n    more code\nNormal";
        let result3 = CodeBlockUtils::identify_code_block_lines(content3);
        assert_eq!(result3, vec![false, true, true, false]);

        // Test with list items (should not be treated as code)
        let content4 = "List:\n    * Item 1\n    * Item 2";
        let result4 = CodeBlockUtils::identify_code_block_lines(content4);
        assert_eq!(result4, vec![false, false, false]);
    }

    #[test]
    fn test_code_block_state_enum() {
        assert_eq!(CodeBlockState::None, CodeBlockState::None);
        assert_eq!(CodeBlockState::Fenced, CodeBlockState::Fenced);
        assert_eq!(CodeBlockState::Indented, CodeBlockState::Indented);
        assert_ne!(CodeBlockState::None, CodeBlockState::Fenced);
    }

    #[test]
    fn test_code_block_info() {
        let content = "Normal\n```\ncode\n```\nText";
        let info = CodeBlockInfo::new(content);

        assert!(!info.is_in_code_block(0));
        assert!(info.is_in_code_block(1));
        assert!(info.is_in_code_block(2));
        assert!(info.is_in_code_block(3));
        assert!(!info.is_in_code_block(4));

        assert!(info.has_code_blocks());

        // Test out of bounds
        assert!(!info.is_in_code_block(100));
    }

    #[test]
    fn test_code_block_info_code_spans() {
        let content = "Text with `inline code` here";
        let info = CodeBlockInfo::new(content);

        assert!(info.has_code_spans());
        assert!(!info.has_code_blocks());

        // Test position inside code span
        assert!(info.is_in_code_span(0, 11)); // Start of `inline
        assert!(info.is_in_code_span(0, 15)); // Inside inline code
        assert!(!info.is_in_code_span(0, 5)); // Before code span
        assert!(!info.is_in_code_span(0, 25)); // After code span
    }

    #[test]
    fn test_compute_code_blocks() {
        let content = "Normal\n```\ncode\n```\n    indented";
        let states = compute_code_blocks(content);

        assert_eq!(states[0], CodeBlockState::None);
        assert_eq!(states[1], CodeBlockState::Fenced);
        assert_eq!(states[2], CodeBlockState::Fenced);
        assert_eq!(states[3], CodeBlockState::Fenced);
        assert_eq!(states[4], CodeBlockState::Indented);
    }

    #[test]
    fn test_compute_code_spans() {
        let content = "Text `code` and ``double`` backticks";
        let spans = compute_code_spans(content);

        assert_eq!(spans.len(), 2);
        // First span: `code`
        assert_eq!(&content[spans[0].0..spans[0].1], "`code`");
        // Second span: ``double``
        assert_eq!(&content[spans[1].0..spans[1].1], "``double``");

        // Test escaped backticks
        let content2 = r"Text \`not code\` but `real code`";
        let spans2 = compute_code_spans(content2);
        assert_eq!(spans2.len(), 1);
        assert!(content2[spans2[0].0..spans2[0].1].contains("real code"));
    }

    #[test]
    fn test_code_block_style() {
        assert_eq!(CodeBlockStyle::Fenced.to_string(), "fenced");
        assert_eq!(CodeBlockStyle::Indented.to_string(), "indented");
        assert_eq!(CodeBlockStyle::Consistent.to_string(), "consistent");

        assert_eq!(CodeBlockStyle::default(), CodeBlockStyle::Consistent);
    }

    #[test]
    fn test_nested_code_blocks() {
        // Nested code blocks don't exist in markdown, but test edge cases
        let content = "```\n```\ncode\n```\n```";
        let result = CodeBlockUtils::identify_code_block_lines(content);
        // First ``` starts a block, second ``` ends it, third starts new block
        assert_eq!(result, vec![true, true, false, true, true]);
    }

    #[test]
    fn test_unicode_content() {
        let content = "```rust\nlet 你好 = \"世界\";\n```";
        let result = CodeBlockUtils::identify_code_block_lines(content);
        assert_eq!(result, vec![true, true, true]);

        assert_eq!(CodeBlockUtils::get_language_specifier("```🦀"), Some("🦀".to_string()));
    }

    #[test]
    fn test_edge_cases() {
        // Empty content
        assert_eq!(CodeBlockUtils::identify_code_block_lines(""), Vec::<bool>::new());
        assert!(!CodeBlockUtils::is_in_code_block("", 0));

        // Just delimiters
        assert_eq!(CodeBlockUtils::identify_code_block_lines("```"), vec![true]);
        assert_eq!(CodeBlockUtils::identify_code_block_lines("~~~"), vec![true]);

        // Mixed fence types (should not close each other)
        let content = "```\ncode\n~~~\nmore\n```";
        let result = CodeBlockUtils::identify_code_block_lines(content);
        assert_eq!(result, vec![true, true, true, true, true]);
    }
}
