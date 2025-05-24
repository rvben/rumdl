use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pattern to match blockquote lines
    static ref BLOCKQUOTE_LINE: Regex = Regex::new(r"^(\s*)>\s?(.*)$").unwrap();

    // Pattern to match empty blockquote lines (> with no space or content)
    static ref EMPTY_BLOCKQUOTE_LINE: Regex = Regex::new(r"^(\s*)>$").unwrap();

    // Pattern to match nested empty blockquote lines (>> with no space or content)
    static ref NESTED_EMPTY_BLOCKQUOTE_LINE: Regex = Regex::new(r"^(\s*)>+$").unwrap();

    // Pattern to match blockquote lines with no space after >
    static ref BLOCKQUOTE_NO_SPACE: Regex = Regex::new(r"^(\s*)>([^\s].*)$").unwrap();

    // Pattern to match blockquote lines with multiple spaces after >
    static ref BLOCKQUOTE_MULTIPLE_SPACES: Regex = Regex::new(r"^(\s*)>(\s{2,})(.*)$").unwrap();

    // Pattern to match nested blockquotes
    static ref NESTED_BLOCKQUOTE: Regex = Regex::new(r"^(\s*)>((?:\s*>)+)(\s*.*)$").unwrap();
}

/// Utility functions for detecting and handling blockquotes in Markdown documents
pub struct BlockquoteUtils;

impl BlockquoteUtils {
    /// Check if a line is a blockquote
    pub fn is_blockquote(line: &str) -> bool {
        BLOCKQUOTE_LINE.is_match(line)
    }

    /// Check if a line is an empty blockquote (> with no content)
    pub fn is_empty_blockquote(line: &str) -> bool {
        // Check for simple empty blockquote (> with no space)
        if EMPTY_BLOCKQUOTE_LINE.is_match(line) {
            return true;
        }

        // Check for nested empty blockquote (>> with no space)
        if NESTED_EMPTY_BLOCKQUOTE_LINE.is_match(line) {
            return true;
        }

        // Check if it's a blockquote with only whitespace content
        if BLOCKQUOTE_LINE.is_match(line) {
            let content = Self::extract_content(line);
            return content.trim().is_empty();
        }

        false
    }

    /// Check if an empty blockquote line needs fixing for MD028
    /// This is more restrictive than is_empty_blockquote - only flags lines that actually need fixing
    pub fn needs_md028_fix(line: &str) -> bool {
        // Only flag blockquotes that have NO space after the > marker
        // Lines with a single space ("> ") are already correct and don't need fixing
        if EMPTY_BLOCKQUOTE_LINE.is_match(line) {
            return true;
        }

        if NESTED_EMPTY_BLOCKQUOTE_LINE.is_match(line) {
            return true;
        }

        false
    }

    /// Check if a blockquote line has no space after the > marker
    pub fn has_no_space_after_marker(line: &str) -> bool {
        BLOCKQUOTE_NO_SPACE.is_match(line)
    }

    /// Check if a blockquote line has multiple spaces after the > marker
    pub fn has_multiple_spaces_after_marker(line: &str) -> bool {
        BLOCKQUOTE_MULTIPLE_SPACES.is_match(line)
    }

    /// Check if a line is a nested blockquote
    pub fn is_nested_blockquote(line: &str) -> bool {
        NESTED_BLOCKQUOTE.is_match(line)
    }

    /// Get the nesting level of a blockquote line
    pub fn get_nesting_level(line: &str) -> usize {
        if !Self::is_blockquote(line) {
            return 0;
        }

        // Count the number of '>' characters at the beginning of the line
        let trimmed = line.trim_start();
        let mut count = 0;

        for c in trimmed.chars() {
            if c == '>' {
                count += 1;
            } else {
                break;
            }
        }

        count
    }

    /// Extract the content of a blockquote line
    pub fn extract_content(line: &str) -> String {
        if let Some(captures) = BLOCKQUOTE_LINE.captures(line) {
            if let Some(content) = captures.get(2) {
                return content.as_str().to_string();
            }
        }

        String::new()
    }

    /// Extract the indentation of a blockquote line
    pub fn extract_indentation(line: &str) -> String {
        if let Some(captures) = BLOCKQUOTE_LINE.captures(line) {
            if let Some(indent) = captures.get(1) {
                return indent.as_str().to_string();
            }
        }

        String::new()
    }

    /// Fix a blockquote line to ensure it has exactly one space after the > marker
    pub fn fix_blockquote_spacing(line: &str) -> String {
        if !Self::is_blockquote(line) {
            return line.to_string();
        }

        if Self::has_no_space_after_marker(line) {
            if let Some(captures) = BLOCKQUOTE_NO_SPACE.captures(line) {
                let indent = captures.get(1).map_or("", |m| m.as_str());
                let content = captures.get(2).map_or("", |m| m.as_str());
                return format!("{}> {}", indent, content);
            }
        } else if Self::has_multiple_spaces_after_marker(line) {
            if let Some(captures) = BLOCKQUOTE_MULTIPLE_SPACES.captures(line) {
                let indent = captures.get(1).map_or("", |m| m.as_str());
                let content = captures.get(3).map_or("", |m| m.as_str());
                return format!("{}> {}", indent, content);
            }
        }

        line.to_string()
    }

    /// Fix nested blockquotes to ensure each level has exactly one space after the > marker
    pub fn fix_nested_blockquote_spacing(line: &str) -> String {
        if !Self::is_nested_blockquote(line) {
            return line.to_string();
        }

        let level = Self::get_nesting_level(line);
        let content = Self::extract_content(line);
        let indent = Self::extract_indentation(line);

        let mut result = indent;
        for _ in 0..level {
            result.push_str("> ");
        }
        result.push_str(&content);

        result
    }

    /// Check if there are blank lines between blockquotes
    pub fn has_blank_between_blockquotes(content: &str) -> Vec<usize> {
        let lines: Vec<&str> = content.lines().collect();
        let mut blank_line_numbers = Vec::new();

        for i in 1..lines.len() {
            let prev_line = lines[i - 1];
            let current_line = lines[i];

            if Self::is_blockquote(prev_line) && Self::is_blockquote(current_line) {
                // Check if the current blockquote line is empty
                if Self::is_empty_blockquote(current_line) {
                    blank_line_numbers.push(i + 1); // 1-indexed line number
                }
            }
        }

        blank_line_numbers
    }

    /// Fix blank lines between blockquotes by removing them
    pub fn fix_blank_between_blockquotes(content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::new();
        let mut skip_next = false;

        for i in 0..lines.len() {
            if skip_next {
                skip_next = false;
                continue;
            }

            let current_line = lines[i];

            if i > 0 && i < lines.len() - 1 {
                let prev_line = lines[i - 1];
                let next_line = lines[i + 1];

                if Self::is_blockquote(prev_line)
                    && Self::is_blockquote(next_line)
                    && current_line.trim().is_empty()
                {
                    // Skip this blank line between blockquotes
                    skip_next = false;
                    continue;
                }
            }

            result.push(current_line);
        }

        result.join("\n")
    }

    /// Get the starting column of the blockquote marker '>'
    pub fn get_blockquote_start_col(line: &str) -> usize {
        let indent_length = Self::extract_indentation(line).len();
        indent_length + 1 // 1-indexed column for the '>' character
    }

    /// Get the content after the blockquote marker
    pub fn get_blockquote_content(line: &str) -> String {
        Self::extract_content(line)
    }
}
