use lazy_static::lazy_static;
use regex::Regex;
use std::fmt;
use std::str::FromStr;

lazy_static! {
    // Optimized regex patterns with more efficient non-capturing groups
    static ref ATX_PATTERN: Regex = Regex::new(r"^(\s*)(#{1,6})(\s*)([^#\n]*?)(?:\s+(#{1,6}))?\s*$").unwrap();
    static ref SETEXT_HEADING_1: Regex = Regex::new(r"^(\s*)(=+)(\s*)$").unwrap();
    static ref SETEXT_HEADING_2: Regex = Regex::new(r"^(\s*)(-+)(\s*)$").unwrap();
    static ref FENCED_CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,}).*$").unwrap();
    static ref FENCED_CODE_BLOCK_END: Regex = Regex::new(r"^(\s*)(`{3,}|~{3,})\s*$").unwrap();
    static ref FRONT_MATTER_DELIMITER: Regex = Regex::new(r"^---\s*$").unwrap();
    static ref INDENTED_CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(\s{4,})").unwrap();

    // Single line emphasis patterns
    static ref SINGLE_LINE_ASTERISK_EMPHASIS: Regex = Regex::new(r"^\s*\*([^*\n]+)\*\s*$").unwrap();
    static ref SINGLE_LINE_UNDERSCORE_EMPHASIS: Regex = Regex::new(r"^\s*_([^_\n]+)_\s*$").unwrap();
    static ref SINGLE_LINE_DOUBLE_ASTERISK_EMPHASIS: Regex = Regex::new(r"^\s*\*\*([^*\n]+)\*\*\s*$").unwrap();
    static ref SINGLE_LINE_DOUBLE_UNDERSCORE_EMPHASIS: Regex = Regex::new(r"^\s*__([^_\n]+)__\s*$").unwrap();
}

/// Represents different styles of Markdown headings
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum HeadingStyle {
    Atx,       // # Heading
    AtxClosed, // # Heading #
    Setext1,   // Heading
    // =======
    Setext2, // Heading
    // -------
    Consistent, // For maintaining consistency with the first found header style
}

impl fmt::Display for HeadingStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            HeadingStyle::Atx => "atx",
            HeadingStyle::AtxClosed => "atx_closed",
            HeadingStyle::Setext1 => "setext1",
            HeadingStyle::Setext2 => "setext2",
            HeadingStyle::Consistent => "consistent",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for HeadingStyle {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "atx" => Ok(HeadingStyle::Atx),
            "atx_closed" => Ok(HeadingStyle::AtxClosed),
            "setext1" | "setext" => Ok(HeadingStyle::Setext1),
            "setext2" => Ok(HeadingStyle::Setext2),
            "consistent" => Ok(HeadingStyle::Consistent),
            _ => Err(()),
        }
    }
}

/// Represents a heading in a Markdown document
#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub text: String,
    pub level: u32,
    pub style: HeadingStyle,
    pub line_number: usize,
    pub original_text: String,
    pub indentation: String,
}

/// Utility functions for working with Markdown headings
pub struct HeadingUtils;

impl HeadingUtils {
    /// Check if a line is an ATX heading (starts with #)
    pub fn is_atx_heading(line: &str) -> bool {
        ATX_PATTERN.is_match(line)
    }

    /// Check if a line is inside a code block
    pub fn is_in_code_block(content: &str, line_number: usize) -> bool {
        let mut in_code_block = false;
        let mut fence_char = None;
        let mut line_count = 0;

        for line in content.lines() {
            line_count += 1;
            if line_count > line_number {
                break;
            }

            let trimmed = line.trim();
            if trimmed.len() >= 3 {
                let first_chars: Vec<char> = trimmed.chars().take(3).collect();
                if first_chars.iter().all(|&c| c == '`' || c == '~') {
                    if let Some(current_fence) = fence_char {
                        if first_chars[0] == current_fence
                            && first_chars.iter().all(|&c| c == current_fence)
                        {
                            in_code_block = false;
                            fence_char = None;
                        }
                    } else {
                        in_code_block = true;
                        fence_char = Some(first_chars[0]);
                    }
                }
            }
        }

        in_code_block
    }

    /// Parse a line into a Heading struct if it's a valid heading
    pub fn parse_heading(content: &str, line_num: usize) -> Option<Heading> {
        let lines: Vec<&str> = content.lines().collect();
        if line_num == 0 || line_num > lines.len() {
            return None;
        }

        let line = lines[line_num - 1];

        // Skip if line is within a code block
        if Self::is_in_code_block(content, line_num) {
            return None;
        }

        // Check for ATX style headings
        if let Some(captures) = ATX_PATTERN.captures(line) {
            let indentation = captures.get(1).map_or("", |m| m.as_str()).to_string();
            let opening_hashes = captures.get(2).map_or("", |m| m.as_str());
            let level = opening_hashes.len() as u32;
            let text = captures.get(4).map_or("", |m| m.as_str()).to_string();

            let style = if let Some(closing) = captures.get(5) {
                let closing_hashes = closing.as_str();
                if closing_hashes.len() == opening_hashes.len() {
                    HeadingStyle::AtxClosed
                } else {
                    HeadingStyle::Atx
                }
            } else {
                HeadingStyle::Atx
            };

            let heading = Heading {
                text: text.clone(),
                level,
                style,
                line_number: line_num,
                original_text: line.to_string(),
                indentation: indentation.clone(),
            };
            return Some(heading);
        }

        // Check for Setext style headings
        if line_num < lines.len() {
            let next_line = lines[line_num];
            let line_indentation = line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();

            // Skip empty lines - don't consider them as potential Setext headings
            if line.trim().is_empty() {
                return None;
            }

            // Skip list items - they shouldn't be considered as potential Setext headings
            if line.trim_start().starts_with('-')
                || line.trim_start().starts_with('*')
                || line.trim_start().starts_with('+')
                || line.trim_start().starts_with("1.")
            {
                return None;
            }

            // Skip front matter delimiters or lines within front matter
            if line.trim() == "---" || Self::is_in_front_matter(content, line_num - 1) {
                return None;
            }

            if let Some(captures) = SETEXT_HEADING_1.captures(next_line) {
                let underline_indent = captures.get(1).map_or("", |m| m.as_str());
                if underline_indent == line_indentation {
                    let heading = Heading {
                        text: line[line_indentation.len()..].to_string(),
                        level: 1,
                        style: HeadingStyle::Setext1,
                        line_number: line_num,
                        original_text: format!("{}\n{}", line, next_line),
                        indentation: line_indentation.clone(),
                    };
                    return Some(heading);
                }
            } else if let Some(captures) = SETEXT_HEADING_2.captures(next_line) {
                let underline_indent = captures.get(1).map_or("", |m| m.as_str());
                if underline_indent == line_indentation {
                    let heading = Heading {
                        text: line[line_indentation.len()..].to_string(),
                        level: 2,
                        style: HeadingStyle::Setext2,
                        line_number: line_num,
                        original_text: format!("{}\n{}", line, next_line),
                        indentation: line_indentation.clone(),
                    };
                    return Some(heading);
                }
            }
        }

        None
    }

    /// Get the indentation level of a line
    pub fn get_indentation(line: &str) -> usize {
        line.len() - line.trim_start().len()
    }

    /// Convert a heading to a different style
    pub fn convert_heading_style(text_content: &str, level: u32, style: HeadingStyle) -> String {
        if text_content.trim().is_empty() {
            return String::new();
        }

        // Validate heading level
        let level = level.clamp(1, 6);
        let indentation = text_content
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();
        let text_content = text_content.trim();

        match style {
            HeadingStyle::Atx => {
                format!(
                    "{}{} {}",
                    indentation,
                    "#".repeat(level as usize),
                    text_content
                )
            }
            HeadingStyle::AtxClosed => {
                format!(
                    "{}{} {} {}",
                    indentation,
                    "#".repeat(level as usize),
                    text_content,
                    "#".repeat(level as usize)
                )
            }
            HeadingStyle::Setext1 | HeadingStyle::Setext2 => {
                if level > 2 {
                    // Fall back to ATX style for levels > 2
                    format!(
                        "{}{} {}",
                        indentation,
                        "#".repeat(level as usize),
                        text_content
                    )
                } else {
                    let underline_char = if level == 1 || style == HeadingStyle::Setext1 {
                        '='
                    } else {
                        '-'
                    };
                    let visible_length = text_content.chars().count();
                    let underline_length = visible_length.max(3); // Ensure at least 3 underline chars
                    format!(
                        "{}{}\n{}{}",
                        indentation,
                        text_content,
                        indentation,
                        underline_char.to_string().repeat(underline_length)
                    )
                }
            }
            HeadingStyle::Consistent => {
                // For Consistent style, default to ATX as it's the most commonly used
                format!(
                    "{}{} {}",
                    indentation,
                    "#".repeat(level as usize),
                    text_content
                )
            }
        }
    }

    /// Get the text content of a heading line
    pub fn get_heading_text(line: &str) -> Option<String> {
        ATX_PATTERN.captures(line).map(|captures| {
            captures
                .get(4)
                .map_or("", |m| m.as_str())
                .trim()
                .to_string()
        })
    }

    /// Detect emphasis-only lines
    pub fn is_emphasis_only_line(line: &str) -> bool {
        let trimmed = line.trim();
        SINGLE_LINE_ASTERISK_EMPHASIS.is_match(trimmed)
            || SINGLE_LINE_UNDERSCORE_EMPHASIS.is_match(trimmed)
            || SINGLE_LINE_DOUBLE_ASTERISK_EMPHASIS.is_match(trimmed)
            || SINGLE_LINE_DOUBLE_UNDERSCORE_EMPHASIS.is_match(trimmed)
    }

    /// Extract text from an emphasis-only line
    pub fn extract_emphasis_text(line: &str) -> Option<(String, u32)> {
        let trimmed = line.trim();

        if let Some(caps) = SINGLE_LINE_ASTERISK_EMPHASIS.captures(trimmed) {
            return Some((caps.get(1).unwrap().as_str().trim().to_string(), 1));
        }

        if let Some(caps) = SINGLE_LINE_UNDERSCORE_EMPHASIS.captures(trimmed) {
            return Some((caps.get(1).unwrap().as_str().trim().to_string(), 1));
        }

        if let Some(caps) = SINGLE_LINE_DOUBLE_ASTERISK_EMPHASIS.captures(trimmed) {
            return Some((caps.get(1).unwrap().as_str().trim().to_string(), 2));
        }

        if let Some(caps) = SINGLE_LINE_DOUBLE_UNDERSCORE_EMPHASIS.captures(trimmed) {
            return Some((caps.get(1).unwrap().as_str().trim().to_string(), 2));
        }

        None
    }

    /// Convert emphasis to heading
    pub fn convert_emphasis_to_heading(line: &str) -> Option<String> {
        // Preserve the original indentation
        let indentation = line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();
        // Preserve trailing spaces at the end of the line
        let trailing = if line.ends_with(" ") {
            line.chars()
                .rev()
                .take_while(|c| c.is_whitespace())
                .collect::<String>()
        } else {
            String::new()
        };

        if let Some((text, level)) = Self::extract_emphasis_text(line) {
            // Preserve the original indentation and trailing spaces
            Some(format!(
                "{}{} {}{}",
                indentation,
                "#".repeat(level as usize),
                text,
                trailing
            ))
        } else {
            None
        }
    }

    /// Convert a heading text to a valid ID for fragment links
    pub fn heading_to_fragment(text: &str) -> String {
        // Remove any HTML tags
        let text_no_html = regex::Regex::new(r"<[^>]*>").unwrap().replace_all(text, "");

        // Convert to lowercase and trim
        let text_lower = text_no_html.trim().to_lowercase();

        // Replace spaces and punctuation with hyphens
        let text_with_hyphens = text_lower
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>();

        // Replace multiple consecutive hyphens with a single hyphen
        let text_clean = text_with_hyphens
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        // Remove leading and trailing hyphens
        text_clean.trim_matches('-').to_string()
    }

    /// Check if a line is in front matter
    pub fn is_in_front_matter(content: &str, line_number: usize) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() || line_number >= lines.len() {
            return false;
        }

        // Check if the document starts with front matter
        if !lines[0].trim_start().eq("---") {
            return false;
        }

        let mut in_front_matter = true;
        let mut found_closing = false;

        // Skip the first line (opening delimiter)
        for (i, line) in lines.iter().enumerate().skip(1) {
            if i > line_number {
                break;
            }

            if line.trim_start().eq("---") {
                found_closing = true;
                in_front_matter = i > line_number;
                break;
            }
        }

        in_front_matter && !found_closing
    }
}

/// Checks if a line is a heading
#[inline]
pub fn is_heading(line: &str) -> bool {
    // Fast path checks first
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    if trimmed.starts_with('#') {
        // Check for ATX heading
        ATX_PATTERN.is_match(line)
    } else {
        // We can't tell for setext headings without looking at the next line
        false
    }
}

/// Checks if a line is a setext heading marker
#[inline]
pub fn is_setext_heading_marker(line: &str) -> bool {
    SETEXT_HEADING_1.is_match(line) || SETEXT_HEADING_2.is_match(line)
}

/// Checks if a line is a setext heading by examining its next line
#[inline]
pub fn is_setext_heading(lines: &[&str], index: usize) -> bool {
    if index >= lines.len() - 1 {
        return false;
    }

    let current_line = lines[index];
    let next_line = lines[index + 1];

    // Skip if current line is empty
    if current_line.trim().is_empty() {
        return false;
    }

    // Check if next line is a setext heading marker with same indentation
    let current_indentation = current_line
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();

    if let Some(captures) = SETEXT_HEADING_1.captures(next_line) {
        let underline_indent = captures.get(1).map_or("", |m| m.as_str());
        return underline_indent == current_indentation;
    }

    if let Some(captures) = SETEXT_HEADING_2.captures(next_line) {
        let underline_indent = captures.get(1).map_or("", |m| m.as_str());
        return underline_indent == current_indentation;
    }

    false
}

/// Get the heading level for a line
#[inline]
pub fn get_heading_level(lines: &[&str], index: usize) -> u32 {
    if index >= lines.len() {
        return 0;
    }

    let line = lines[index];

    // Check for ATX style heading
    if let Some(captures) = ATX_PATTERN.captures(line) {
        let hashes = captures.get(2).map_or("", |m| m.as_str());
        return hashes.len() as u32;
    }

    // Check for setext style heading
    if index < lines.len() - 1 {
        let next_line = lines[index + 1];

        if SETEXT_HEADING_1.is_match(next_line) {
            return 1;
        }

        if SETEXT_HEADING_2.is_match(next_line) {
            return 2;
        }
    }

    0
}

/// Extract the text content from a heading
#[inline]
pub fn extract_heading_text(lines: &[&str], index: usize) -> String {
    if index >= lines.len() {
        return String::new();
    }

    let line = lines[index];

    // Extract from ATX heading
    if let Some(captures) = ATX_PATTERN.captures(line) {
        return captures
            .get(4)
            .map_or("", |m| m.as_str())
            .trim()
            .to_string();
    }

    // Extract from setext heading
    if index < lines.len() - 1 {
        let next_line = lines[index + 1];
        let line_indentation = line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();

        if let Some(captures) = SETEXT_HEADING_1.captures(next_line) {
            let underline_indent = captures.get(1).map_or("", |m| m.as_str());
            if underline_indent == line_indentation {
                return line[line_indentation.len()..].trim().to_string();
            }
        }

        if let Some(captures) = SETEXT_HEADING_2.captures(next_line) {
            let underline_indent = captures.get(1).map_or("", |m| m.as_str());
            if underline_indent == line_indentation {
                return line[line_indentation.len()..].trim().to_string();
            }
        }
    }

    line.trim().to_string()
}

/// Get the indentation of a heading
#[inline]
pub fn get_heading_indentation(lines: &[&str], index: usize) -> usize {
    if index >= lines.len() {
        return 0;
    }

    let line = lines[index];
    line.len() - line.trim_start().len()
}

/// Check if a line is a code block delimiter
#[inline]
pub fn is_code_block_delimiter(line: &str) -> bool {
    FENCED_CODE_BLOCK_START.is_match(line) || FENCED_CODE_BLOCK_END.is_match(line)
}

/// Check if a line is a front matter delimiter
#[inline]
pub fn is_front_matter_delimiter(line: &str) -> bool {
    FRONT_MATTER_DELIMITER.is_match(line)
}

/// Remove trailing hashes from a heading
#[inline]
pub fn remove_trailing_hashes(text: &str) -> String {
    let trimmed = text.trim_end();
    let mut result = trimmed.to_string();

    if let Some(last_hash_index) = trimmed.rfind('#') {
        if trimmed[last_hash_index..]
            .chars()
            .all(|c| c == '#' || c.is_whitespace())
        {
            result = trimmed[..trimmed.rfind('#').unwrap()]
                .trim_end()
                .to_string();
        }
    }

    result
}

/// Normalize a heading to the specified level
#[inline]
pub fn normalize_heading(line: &str, level: u32) -> String {
    let indentation = line
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();
    let trimmed = line.trim_start();

    if trimmed.starts_with('#') {
        if let Some(text) = HeadingUtils::get_heading_text(line) {
            format!("{}{} {}", indentation, "#".repeat(level as usize), text)
        } else {
            line.to_string()
        }
    } else {
        format!("{}{} {}", indentation, "#".repeat(level as usize), trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atx_heading_parsing() {
        let content = "# Heading 1\n## Heading 2\n### Heading 3";
        assert!(HeadingUtils::parse_heading(content, 1).is_some());
        assert_eq!(HeadingUtils::parse_heading(content, 1).unwrap().level, 1);
        assert_eq!(HeadingUtils::parse_heading(content, 2).unwrap().level, 2);
        assert_eq!(HeadingUtils::parse_heading(content, 3).unwrap().level, 3);
    }

    #[test]
    fn test_setext_heading_parsing() {
        let content = "Heading 1\n=========\nHeading 2\n---------";
        assert!(HeadingUtils::parse_heading(content, 1).is_some());
        assert_eq!(HeadingUtils::parse_heading(content, 1).unwrap().level, 1);
        assert_eq!(HeadingUtils::parse_heading(content, 3).unwrap().level, 2);
    }

    #[test]
    fn test_heading_style_conversion() {
        assert_eq!(
            HeadingUtils::convert_heading_style("Heading 1", 1, HeadingStyle::Atx),
            "# Heading 1"
        );
        assert_eq!(
            HeadingUtils::convert_heading_style("Heading 2", 2, HeadingStyle::AtxClosed),
            "## Heading 2 ##"
        );
        assert_eq!(
            HeadingUtils::convert_heading_style("Heading 1", 1, HeadingStyle::Setext1),
            "Heading 1\n========="
        );
        assert_eq!(
            HeadingUtils::convert_heading_style("Heading 2", 2, HeadingStyle::Setext2),
            "Heading 2\n---------"
        );
    }

    #[test]
    fn test_code_block_detection() {
        let content = "# Heading\n```\n# Not a heading\n```\n# Another heading";
        assert!(!HeadingUtils::is_in_code_block(content, 0));
        assert!(HeadingUtils::is_in_code_block(content, 2));
        assert!(!HeadingUtils::is_in_code_block(content, 4));
    }

    #[test]
    fn test_empty_line_with_dashes() {
        // Test that an empty line followed by dashes is not considered a heading
        let content = "\n---";

        // Empty line is at index 0, dashes at index 1
        assert_eq!(
            HeadingUtils::parse_heading(content, 1),
            None,
            "Empty line followed by dashes should not be detected as a heading"
        );

        // Also test with a regular horizontal rule
        let content2 = "Some content\n\n---\nMore content";
        assert_eq!(
            HeadingUtils::parse_heading(content2, 2),
            None,
            "Empty line followed by horizontal rule should not be detected as a heading"
        );
    }
}
