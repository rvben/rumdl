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

/// Calculate end position for a single-line range
pub fn calculate_single_line_range(
    line: usize,
    start_col: usize,
    length: usize,
) -> (usize, usize, usize, usize) {
    (line, start_col, line, start_col + length)
}

/// Calculate range for entire line
pub fn calculate_line_range(line: usize, line_content: &str) -> (usize, usize, usize, usize) {
    let trimmed_len = line_content.trim_end().len();
    (line, 1, line, trimmed_len + 1)
}

/// Calculate range from regex match on a line
pub fn calculate_match_range(
    line: usize,
    line_content: &str,
    match_start: usize,
    match_len: usize,
) -> (usize, usize, usize, usize) {
    // Convert byte positions to character positions
    let char_start = line_content[..match_start].chars().count() + 1; // 1-indexed
    let char_len = line_content[match_start..match_start + match_len]
        .chars()
        .count();
    (line, char_start, line, char_start + char_len)
}

/// Calculate range for trailing content (like trailing spaces)
pub fn calculate_trailing_range(
    line: usize,
    line_content: &str,
    content_end: usize,
) -> (usize, usize, usize, usize) {
    let char_content_end = line_content[..content_end].chars().count() + 1; // 1-indexed
    let line_char_len = line_content.chars().count() + 1;
    (line, char_content_end, line, line_char_len)
}

/// Calculate range for a heading (entire line)
pub fn calculate_heading_range(line: usize, line_content: &str) -> (usize, usize, usize, usize) {
    calculate_line_range(line, line_content)
}

/// Calculate range for emphasis markers and content
pub fn calculate_emphasis_range(
    line: usize,
    line_content: &str,
    start_pos: usize,
    end_pos: usize,
) -> (usize, usize, usize, usize) {
    let char_start = line_content[..start_pos].chars().count() + 1; // 1-indexed
    let char_end = line_content[..end_pos].chars().count() + 1; // 1-indexed
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
pub fn calculate_excess_range(
    line: usize,
    line_content: &str,
    limit: usize,
) -> (usize, usize, usize, usize) {
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
        let (start_line, start_col, end_line, end_col) =
            calculate_match_range(1, content, tag_start, tag_len);
        assert_eq!(start_line, 1);
        assert_eq!(start_col, 6); // 1-indexed
        assert_eq!(end_line, 1);
        assert_eq!(end_col, 11); // 6 + 5
    }

    #[test]
    fn test_trailing_range() {
        let content = "Text content   "; // 3 trailing spaces
        let content_end = 12; // End of "Text content"
        let (start_line, start_col, end_line, end_col) =
            calculate_trailing_range(1, content, content_end);
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
}
