//! Utilities for position/range conversions

use crate::utils::element_cache::ElementCache;
use std::collections::HashSet;
use std::ops::Range;

/// Find the nearest valid UTF-8 character boundary at or before the given byte index.
/// This is critical for safely slicing strings that may contain multi-byte UTF-8 characters.
///
/// # Safety
/// Returns a byte index that is guaranteed to be a valid character boundary,
/// or the string length if the index is beyond the string.
fn find_char_boundary(s: &str, byte_idx: usize) -> usize {
    if byte_idx >= s.len() {
        return s.len();
    }

    // If the index is already at a character boundary, return it
    if s.is_char_boundary(byte_idx) {
        return byte_idx;
    }

    // Find the nearest character boundary by scanning backwards
    // This is safe because we know byte_idx < s.len()
    let mut pos = byte_idx;
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// Convert a byte index to a character count (1-indexed).
/// This safely handles multi-byte UTF-8 characters by finding the nearest character boundary.
fn byte_to_char_count(s: &str, byte_idx: usize) -> usize {
    let safe_byte_idx = find_char_boundary(s, byte_idx);
    s[..safe_byte_idx].chars().count() + 1 // 1-indexed
}

#[derive(Debug)]
pub struct LineIndex<'a> {
    line_starts: Vec<usize>,
    content: &'a str,
    code_block_lines: Option<HashSet<usize>>,
}

impl<'a> LineIndex<'a> {
    pub fn new(content: &'a str) -> Self {
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
        // Column is 1-indexed character position, not byte position
        let char_col = column.saturating_sub(1);
        let char_count = current_line.chars().count();
        let safe_char_col = char_col.min(char_count);

        // Convert character position to byte position
        let byte_offset = current_line
            .char_indices()
            .nth(safe_char_col)
            .map(|(idx, _)| idx)
            .unwrap_or(current_line.len());

        let start = line_start + byte_offset;
        start..start
    }

    /// Calculate a proper byte range for replacing text with a specific length
    /// This is the correct function to use for LSP fixes
    ///
    /// # Safety
    /// This function correctly handles multi-byte UTF-8 characters by converting
    /// character positions (columns) to byte positions.
    pub fn line_col_to_byte_range_with_length(&self, line: usize, column: usize, length: usize) -> Range<usize> {
        let line = line.saturating_sub(1);
        let line_start = *self.line_starts.get(line).unwrap_or(&self.content.len());
        let line_end = self.line_starts.get(line + 1).copied().unwrap_or(self.content.len());
        let mut current_line = &self.content[line_start..line_end];
        if let Some(stripped) = current_line.strip_suffix('\n') {
            current_line = stripped.strip_suffix('\r').unwrap_or(stripped);
        }
        if current_line.is_ascii() {
            let line_len = current_line.len();
            let start_byte = column.saturating_sub(1).min(line_len);
            let end_byte = start_byte.saturating_add(length).min(line_len);
            let start = line_start + start_byte;
            let end = line_start + end_byte;
            return start..end;
        }
        // Column is 1-indexed character position, not byte position
        let char_col = column.saturating_sub(1);
        let char_count = current_line.chars().count();
        let safe_char_col = char_col.min(char_count);

        // Convert character positions to byte positions
        let mut char_indices = current_line.char_indices();
        let start_byte = char_indices
            .nth(safe_char_col)
            .map(|(idx, _)| idx)
            .unwrap_or(current_line.len());

        // Calculate end position (start + length in characters)
        let end_char_col = (safe_char_col + length).min(char_count);
        let end_byte = current_line
            .char_indices()
            .nth(end_char_col)
            .map(|(idx, _)| idx)
            .unwrap_or(current_line.len());

        let start = line_start + start_byte;
        let end = line_start + end_byte;
        start..end
    }

    /// Calculate byte range for entire line replacement (including newline)
    /// This is ideal for rules that need to replace complete lines
    pub fn whole_line_range(&self, line: usize) -> Range<usize> {
        let line_idx = line.saturating_sub(1);
        let start = *self.line_starts.get(line_idx).unwrap_or(&self.content.len());
        let end = self
            .line_starts
            .get(line_idx + 1)
            .copied()
            .unwrap_or(self.content.len());
        start..end
    }

    /// Calculate byte range spanning multiple lines (from start_line to end_line inclusive)
    /// Both lines are 1-indexed. This is useful for replacing entire blocks like tables.
    pub fn multi_line_range(&self, start_line: usize, end_line: usize) -> Range<usize> {
        let start_idx = start_line.saturating_sub(1);
        let end_idx = end_line.saturating_sub(1);

        let start = *self.line_starts.get(start_idx).unwrap_or(&self.content.len());
        let end = self.line_starts.get(end_idx + 1).copied().unwrap_or(self.content.len());
        start..end
    }

    /// Calculate byte range for text within a line (excluding newline)
    /// Useful for replacing specific parts of a line
    ///
    /// # Safety
    /// This function correctly handles multi-byte UTF-8 characters by converting
    /// character positions (columns) to byte positions.
    pub fn line_text_range(&self, line: usize, start_col: usize, end_col: usize) -> Range<usize> {
        let line_idx = line.saturating_sub(1);
        let line_start = *self.line_starts.get(line_idx).unwrap_or(&self.content.len());

        // Get the actual line content to ensure we don't exceed bounds
        let current_line = self.content.lines().nth(line_idx).unwrap_or("");
        let char_count = current_line.chars().count();

        // Convert character positions to byte positions
        let start_char_col = start_col.saturating_sub(1).min(char_count);
        let end_char_col = end_col.saturating_sub(1).min(char_count);

        let mut char_indices = current_line.char_indices();
        let start_byte = char_indices
            .nth(start_char_col)
            .map(|(idx, _)| idx)
            .unwrap_or(current_line.len());

        let end_byte = current_line
            .char_indices()
            .nth(end_char_col)
            .map(|(idx, _)| idx)
            .unwrap_or(current_line.len());

        let start = line_start + start_byte;
        let end = line_start + end_byte.max(start_byte);
        start..end
    }

    /// Calculate byte range from start of line to end of line content (excluding newline)
    /// Useful for replacing line content while preserving line structure
    pub fn line_content_range(&self, line: usize) -> Range<usize> {
        let line_idx = line.saturating_sub(1);
        let line_start = *self.line_starts.get(line_idx).unwrap_or(&self.content.len());

        let current_line = self.content.lines().nth(line_idx).unwrap_or("");
        let line_end = line_start + current_line.len();
        line_start..line_end
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
        self.content.lines().nth(line).is_some_and(|l| {
            let trimmed = l.trim();
            trimmed.starts_with("```") || trimmed.starts_with("~~~")
        })
    }

    /// Check if the line is a tilde code fence marker (~~~)
    pub fn is_tilde_code_block(&self, line: usize) -> bool {
        self.content
            .lines()
            .nth(line)
            .is_some_and(|l| l.trim().starts_with("~~~"))
    }

    /// Get a reference to the content
    pub fn get_content(&self) -> &str {
        self.content
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

            // 1. Detect indented code blocks (4+ columns accounting for tab expansion)
            if ElementCache::calculate_indentation_width_default(line) >= 4 {
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
                    let count = trimmed.chars().take_while(|&c| c == active_fence_type).count();
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
                    let is_valid_closing_fence =
                        count >= block_fence_length && remaining.is_empty() && indent <= block_indent;

                    // For nested code blocks in markdown, the first backtick fence after the nested content
                    // should be recognized as the closing fence for the outer block
                    let is_nested_closing = nested_fence_end.is_some() && i == nested_fence_end.unwrap();

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

/// Calculate end position for a single-line range
pub fn calculate_single_line_range(line: usize, start_col: usize, length: usize) -> (usize, usize, usize, usize) {
    (line, start_col, line, start_col + length)
}

/// Calculate range for entire line
pub fn calculate_line_range(line: usize, line_content: &str) -> (usize, usize, usize, usize) {
    let trimmed_len = line_content.trim_end().len();
    (line, 1, line, trimmed_len + 1)
}

/// Calculate range from regex match on a line
///
/// # Safety
/// This function safely handles multi-byte UTF-8 characters by ensuring all
/// string slicing operations occur at valid character boundaries.
pub fn calculate_match_range(
    line: usize,
    line_content: &str,
    match_start: usize,
    match_len: usize,
) -> (usize, usize, usize, usize) {
    // Bounds check to prevent panic
    let line_len = line_content.len();
    if match_start > line_len {
        // If match_start is beyond line bounds, return a safe range at end of line
        let char_count = line_content.chars().count();
        return (line, char_count + 1, line, char_count + 1);
    }

    // Find safe character boundaries for the match range
    let safe_match_start = find_char_boundary(line_content, match_start);
    let safe_match_end_byte = find_char_boundary(line_content, (match_start + match_len).min(line_len));

    // Convert byte positions to character positions safely
    let char_start = byte_to_char_count(line_content, safe_match_start);
    let char_len = if safe_match_end_byte > safe_match_start {
        // Count characters in the safe range
        line_content[safe_match_start..safe_match_end_byte].chars().count()
    } else {
        0
    };
    (line, char_start, line, char_start + char_len)
}

/// Calculate range for trailing content (like trailing spaces)
///
/// # Safety
/// This function safely handles multi-byte UTF-8 characters by ensuring all
/// string slicing operations occur at valid character boundaries.
pub fn calculate_trailing_range(line: usize, line_content: &str, content_end: usize) -> (usize, usize, usize, usize) {
    // Find safe character boundary for content_end
    let safe_content_end = find_char_boundary(line_content, content_end);
    let char_content_end = byte_to_char_count(line_content, safe_content_end);
    let line_char_len = line_content.chars().count() + 1;
    (line, char_content_end, line, line_char_len)
}

/// Calculate range for a heading (entire line)
pub fn calculate_heading_range(line: usize, line_content: &str) -> (usize, usize, usize, usize) {
    calculate_line_range(line, line_content)
}

/// Calculate range for emphasis markers and content
///
/// # Safety
/// This function safely handles multi-byte UTF-8 characters by ensuring all
/// string slicing operations occur at valid character boundaries.
pub fn calculate_emphasis_range(
    line: usize,
    line_content: &str,
    start_pos: usize,
    end_pos: usize,
) -> (usize, usize, usize, usize) {
    // Find safe character boundaries for start and end positions
    let safe_start_pos = find_char_boundary(line_content, start_pos);
    let safe_end_pos = find_char_boundary(line_content, end_pos);
    let char_start = byte_to_char_count(line_content, safe_start_pos);
    let char_end = byte_to_char_count(line_content, safe_end_pos);
    (line, char_start, line, char_end)
}

/// Calculate range for HTML tags
pub fn calculate_html_tag_range(
    line: usize,
    line_content: &str,
    tag_start: usize,
    tag_len: usize,
) -> (usize, usize, usize, usize) {
    calculate_match_range(line, line_content, tag_start, tag_len)
}

/// Calculate range for URLs
pub fn calculate_url_range(
    line: usize,
    line_content: &str,
    url_start: usize,
    url_len: usize,
) -> (usize, usize, usize, usize) {
    calculate_match_range(line, line_content, url_start, url_len)
}

/// Calculate range for list markers
pub fn calculate_list_marker_range(
    line: usize,
    line_content: &str,
    marker_start: usize,
    marker_len: usize,
) -> (usize, usize, usize, usize) {
    calculate_match_range(line, line_content, marker_start, marker_len)
}

/// Calculate range that exceeds a limit (like line length)
pub fn calculate_excess_range(line: usize, line_content: &str, limit: usize) -> (usize, usize, usize, usize) {
    let char_limit = std::cmp::min(limit, line_content.chars().count());
    let line_char_len = line_content.chars().count() + 1;
    (line, char_limit + 1, line, line_char_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_line_range() {
        let (start_line, start_col, end_line, end_col) = calculate_single_line_range(5, 10, 3);
        assert_eq!(start_line, 5);
        assert_eq!(start_col, 10);
        assert_eq!(end_line, 5);
        assert_eq!(end_col, 13);
    }

    #[test]
    fn test_line_range() {
        let content = "# This is a heading  ";
        let (start_line, start_col, end_line, end_col) = calculate_line_range(1, content);
        assert_eq!(start_line, 1);
        assert_eq!(start_col, 1);
        assert_eq!(end_line, 1);
        assert_eq!(end_col, 20); // Trimmed length + 1
    }

    #[test]
    fn test_match_range() {
        let content = "Text <div>content</div> more";
        let tag_start = 5; // Position of '<'
        let tag_len = 5; // Length of "<div>"
        let (start_line, start_col, end_line, end_col) = calculate_match_range(1, content, tag_start, tag_len);
        assert_eq!(start_line, 1);
        assert_eq!(start_col, 6); // 1-indexed
        assert_eq!(end_line, 1);
        assert_eq!(end_col, 11); // 6 + 5
    }

    #[test]
    fn test_trailing_range() {
        let content = "Text content   "; // 3 trailing spaces
        let content_end = 12; // End of "Text content"
        let (start_line, start_col, end_line, end_col) = calculate_trailing_range(1, content, content_end);
        assert_eq!(start_line, 1);
        assert_eq!(start_col, 13); // content_end + 1 (1-indexed)
        assert_eq!(end_line, 1);
        assert_eq!(end_col, 16); // Total length + 1
    }

    #[test]
    fn test_excess_range() {
        let content = "This line is too long for the limit";
        let limit = 20;
        let (start_line, start_col, end_line, end_col) = calculate_excess_range(1, content, limit);
        assert_eq!(start_line, 1);
        assert_eq!(start_col, 21); // limit + 1
        assert_eq!(end_line, 1);
        assert_eq!(end_col, 36); // Total length + 1 (35 chars + 1 = 36)
    }

    #[test]
    fn test_whole_line_range() {
        let content = "Line 1\nLine 2\nLine 3";
        let line_index = LineIndex::new(content);

        // Test first line (includes newline)
        let range = line_index.whole_line_range(1);
        assert_eq!(range, 0..7); // "Line 1\n"

        // Test middle line
        let range = line_index.whole_line_range(2);
        assert_eq!(range, 7..14); // "Line 2\n"

        // Test last line (no newline)
        let range = line_index.whole_line_range(3);
        assert_eq!(range, 14..20); // "Line 3"
    }

    #[test]
    fn test_line_content_range() {
        let content = "Line 1\nLine 2\nLine 3";
        let line_index = LineIndex::new(content);

        // Test first line content (excludes newline)
        let range = line_index.line_content_range(1);
        assert_eq!(range, 0..6); // "Line 1"

        // Test middle line content
        let range = line_index.line_content_range(2);
        assert_eq!(range, 7..13); // "Line 2"

        // Test last line content
        let range = line_index.line_content_range(3);
        assert_eq!(range, 14..20); // "Line 3"
    }

    #[test]
    fn test_line_text_range() {
        let content = "Hello world\nAnother line";
        let line_index = LineIndex::new(content);

        // Test partial text in first line
        let range = line_index.line_text_range(1, 1, 5); // "Hell"
        assert_eq!(range, 0..4);

        // Test partial text in second line
        let range = line_index.line_text_range(2, 1, 7); // "Another"
        assert_eq!(range, 12..18);

        // Test bounds checking
        let range = line_index.line_text_range(1, 1, 100); // Should clamp to line end
        assert_eq!(range, 0..11); // "Hello world"
    }

    #[test]
    fn test_calculate_match_range_bounds_checking() {
        // Test case 1: match_start beyond line bounds
        let line_content = "] not a link [";
        let (line, start_col, end_line, end_col) = calculate_match_range(121, line_content, 57, 10);
        assert_eq!(line, 121);
        assert_eq!(start_col, 15); // line length + 1
        assert_eq!(end_line, 121);
        assert_eq!(end_col, 15); // same as start when out of bounds

        // Test case 2: match extends beyond line end
        let line_content = "short";
        let (line, start_col, end_line, end_col) = calculate_match_range(1, line_content, 2, 10);
        assert_eq!(line, 1);
        assert_eq!(start_col, 3); // position 2 + 1
        assert_eq!(end_line, 1);
        assert_eq!(end_col, 6); // clamped to line length + 1

        // Test case 3: normal case within bounds
        let line_content = "normal text here";
        let (line, start_col, end_line, end_col) = calculate_match_range(5, line_content, 7, 4);
        assert_eq!(line, 5);
        assert_eq!(start_col, 8); // position 7 + 1
        assert_eq!(end_line, 5);
        assert_eq!(end_col, 12); // position 7 + 4 + 1

        // Test case 4: zero length match
        let line_content = "test line";
        let (line, start_col, end_line, end_col) = calculate_match_range(10, line_content, 5, 0);
        assert_eq!(line, 10);
        assert_eq!(start_col, 6); // position 5 + 1
        assert_eq!(end_line, 10);
        assert_eq!(end_col, 6); // same as start for zero length
    }

    // ============================================================================
    // UTF-8 Multi-byte Character Tests (Issue #154)
    // ============================================================================

    #[test]
    fn test_issue_154_korean_character_boundary() {
        // Exact reproduction of issue #154: Korean character '후' (3 bytes: 18..21)
        // The error was: "byte index 19 is not a char boundary; it is inside '후'"
        let line_content = "- 2023 년 초 이후 주가 상승        +1,000% (10 배 상승)  ";

        // Test match at byte 19 (middle of '후' character)
        // This should not panic and should find the nearest character boundary
        let (line, start_col, end_line, end_col) = calculate_match_range(1, line_content, 19, 1);

        // Should successfully calculate without panicking
        assert!(start_col > 0);
        assert_eq!(line, 1);
        assert_eq!(end_line, 1);
        assert!(end_col >= start_col);
    }

    #[test]
    fn test_calculate_match_range_korean() {
        // Korean text: "안녕하세요" (Hello in Korean)
        // Each character is 3 bytes
        let line_content = "안녕하세요";
        // Match at byte 3 (start of second character)
        let (line, start_col, end_line, end_col) = calculate_match_range(1, line_content, 3, 3);
        assert_eq!(line, 1);
        assert_eq!(start_col, 2); // Second character (1-indexed)
        assert_eq!(end_line, 1);
        assert_eq!(end_col, 3); // End of second character

        // Match at byte 4 (middle of second character - should round down)
        let (line, start_col, end_line, _end_col) = calculate_match_range(1, line_content, 4, 3);
        assert_eq!(line, 1);
        assert_eq!(start_col, 2); // Should round to start of character
        assert_eq!(end_line, 1);
    }

    #[test]
    fn test_calculate_match_range_chinese() {
        // Chinese text: "你好世界" (Hello World)
        // Each character is 3 bytes
        let line_content = "你好世界";
        // Match at byte 6 (start of third character)
        let (line, start_col, end_line, end_col) = calculate_match_range(1, line_content, 6, 3);
        assert_eq!(line, 1);
        assert_eq!(start_col, 3); // Third character (1-indexed)
        assert_eq!(end_line, 1);
        assert_eq!(end_col, 4); // End of third character
    }

    #[test]
    fn test_calculate_match_range_japanese() {
        // Japanese text: "こんにちは" (Hello)
        // Each character is 3 bytes
        let line_content = "こんにちは";
        // Match at byte 9 (start of fourth character)
        let (line, start_col, end_line, end_col) = calculate_match_range(1, line_content, 9, 3);
        assert_eq!(line, 1);
        assert_eq!(start_col, 4); // Fourth character (1-indexed)
        assert_eq!(end_line, 1);
        assert_eq!(end_col, 5); // End of fourth character
    }

    #[test]
    fn test_calculate_match_range_mixed_unicode() {
        // Mixed ASCII and CJK: "Hello 世界"
        // "Hello " = 6 bytes (H, e, l, l, o, space)
        // "世" = bytes 6-8 (3 bytes), character 7
        // "界" = bytes 9-11 (3 bytes), character 8
        let line_content = "Hello 世界";

        // Match at byte 5 (space character)
        let (line, start_col, end_line, end_col) = calculate_match_range(1, line_content, 5, 1);
        assert_eq!(line, 1);
        assert_eq!(start_col, 6); // Space character (1-indexed: H=1, e=2, l=3, l=4, o=5, space=6)
        assert_eq!(end_line, 1);
        assert_eq!(end_col, 7); // After space

        // Match at byte 6 (start of first Chinese character "世")
        let (line, start_col, end_line, end_col) = calculate_match_range(1, line_content, 6, 3);
        assert_eq!(line, 1);
        assert_eq!(start_col, 7); // First Chinese character (1-indexed)
        assert_eq!(end_line, 1);
        assert_eq!(end_col, 8); // End of first Chinese character
    }

    #[test]
    fn test_calculate_trailing_range_korean() {
        // Korean text with trailing spaces
        let line_content = "안녕하세요   ";
        // content_end at byte 15 (middle of last character + spaces)
        let (line, start_col, end_line, end_col) = calculate_trailing_range(1, line_content, 15);
        assert_eq!(line, 1);
        assert!(start_col > 0);
        assert_eq!(end_line, 1);
        assert!(end_col > start_col);
    }

    #[test]
    fn test_calculate_emphasis_range_chinese() {
        // Chinese text with emphasis markers
        let line_content = "这是**重要**的";
        // start_pos and end_pos at byte boundaries within Chinese characters
        let (line, start_col, end_line, end_col) = calculate_emphasis_range(1, line_content, 6, 12);
        assert_eq!(line, 1);
        assert!(start_col > 0);
        assert_eq!(end_line, 1);
        assert!(end_col > start_col);
    }

    #[test]
    fn test_line_col_to_byte_range_korean() {
        // Test that column positions (character positions) are correctly converted to byte positions
        let content = "안녕하세요\nWorld";
        let line_index = LineIndex::new(content);

        // Column 1 (first character)
        let range = line_index.line_col_to_byte_range(1, 1);
        assert_eq!(range, 0..0);

        // Column 2 (second character)
        let range = line_index.line_col_to_byte_range(1, 2);
        assert_eq!(range, 3..3); // 3 bytes for first character

        // Column 3 (third character)
        let range = line_index.line_col_to_byte_range(1, 3);
        assert_eq!(range, 6..6); // 6 bytes for first two characters
    }

    #[test]
    fn test_line_col_to_byte_range_with_length_chinese() {
        // Test byte range calculation with length for Chinese characters
        let content = "你好世界\nTest";
        let line_index = LineIndex::new(content);

        // Column 1, length 2 (first two Chinese characters)
        let range = line_index.line_col_to_byte_range_with_length(1, 1, 2);
        assert_eq!(range, 0..6); // 6 bytes for two 3-byte characters

        // Column 2, length 1 (second Chinese character)
        let range = line_index.line_col_to_byte_range_with_length(1, 2, 1);
        assert_eq!(range, 3..6); // Bytes 3-6 for second character
    }

    #[test]
    fn test_line_text_range_japanese() {
        // Test text range calculation for Japanese characters
        let content = "こんにちは\nHello";
        let line_index = LineIndex::new(content);

        // Columns 2-4 (second to fourth Japanese characters)
        let range = line_index.line_text_range(1, 2, 4);
        assert_eq!(range, 3..9); // Bytes 3-9 for three 3-byte characters
    }

    #[test]
    fn test_find_char_boundary_edge_cases() {
        // Test the helper function directly
        let s = "안녕";

        // Byte 0 (start) - should be valid
        assert_eq!(find_char_boundary(s, 0), 0);

        // Byte 1 (middle of first character) - should round down to 0
        assert_eq!(find_char_boundary(s, 1), 0);

        // Byte 2 (middle of first character) - should round down to 0
        assert_eq!(find_char_boundary(s, 2), 0);

        // Byte 3 (start of second character) - should be valid
        assert_eq!(find_char_boundary(s, 3), 3);

        // Byte 4 (middle of second character) - should round down to 3
        assert_eq!(find_char_boundary(s, 4), 3);

        // Byte beyond string length - should return string length
        assert_eq!(find_char_boundary(s, 100), s.len());
    }

    #[test]
    fn test_byte_to_char_count_unicode() {
        // Test character counting with multi-byte characters
        let s = "안녕하세요";

        // Byte 0 (start) - 1 character
        assert_eq!(byte_to_char_count(s, 0), 1);

        // Byte 3 (start of second character) - 2 characters
        assert_eq!(byte_to_char_count(s, 3), 2);

        // Byte 6 (start of third character) - 3 characters
        assert_eq!(byte_to_char_count(s, 6), 3);

        // Byte 9 (start of fourth character) - 4 characters
        assert_eq!(byte_to_char_count(s, 9), 4);

        // Byte 12 (start of fifth character) - 5 characters
        assert_eq!(byte_to_char_count(s, 12), 5);

        // Byte 15 (end) - 6 characters (5 + 1 for 1-indexed)
        assert_eq!(byte_to_char_count(s, 15), 6);
    }

    #[test]
    fn test_all_range_functions_with_emoji() {
        // Test with emoji (4-byte UTF-8 characters)
        let line_content = "Hello 🎉 World 🌍";

        // calculate_match_range
        let (line, start_col, end_line, end_col) = calculate_match_range(1, line_content, 6, 4);
        assert_eq!(line, 1);
        assert!(start_col > 0);
        assert_eq!(end_line, 1);
        assert!(end_col > start_col);

        // calculate_trailing_range
        let (line, start_col, end_line, end_col) = calculate_trailing_range(1, line_content, 12);
        assert_eq!(line, 1);
        assert!(start_col > 0);
        assert_eq!(end_line, 1);
        assert!(end_col > start_col);

        // calculate_emphasis_range
        let (line, start_col, end_line, end_col) = calculate_emphasis_range(1, line_content, 0, 5);
        assert_eq!(line, 1);
        assert_eq!(start_col, 1);
        assert_eq!(end_line, 1);
        assert!(end_col > start_col);
    }
}
