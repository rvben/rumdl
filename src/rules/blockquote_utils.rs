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
        if let Some(captures) = BLOCKQUOTE_LINE.captures(line)
            && let Some(content) = captures.get(2)
        {
            return content.as_str().to_string();
        }

        String::new()
    }

    /// Extract the indentation of a blockquote line
    pub fn extract_indentation(line: &str) -> String {
        if let Some(captures) = BLOCKQUOTE_LINE.captures(line)
            && let Some(indent) = captures.get(1)
        {
            return indent.as_str().to_string();
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
                return format!("{indent}> {content}");
            }
        } else if Self::has_multiple_spaces_after_marker(line)
            && let Some(captures) = BLOCKQUOTE_MULTIPLE_SPACES.captures(line)
        {
            let indent = captures.get(1).map_or("", |m| m.as_str());
            let content = captures.get(3).map_or("", |m| m.as_str());
            return format!("{indent}> {content}");
        }

        line.to_string()
    }

    /// Fix nested blockquotes to ensure each level has exactly one space after the > marker
    pub fn fix_nested_blockquote_spacing(line: &str) -> String {
        if !Self::is_blockquote(line) {
            return line.to_string();
        }

        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];

        // Parse through the blockquote markers
        let mut remaining = trimmed;
        let mut markers = Vec::new();

        while remaining.starts_with('>') {
            markers.push('>');
            remaining = &remaining[1..];

            // Skip any spaces between markers
            remaining = remaining.trim_start();
        }

        // Build the result with proper spacing
        let mut result = indent.to_string();
        for (i, _) in markers.iter().enumerate() {
            if i > 0 {
                result.push(' ');
            }
            result.push('>');
        }

        // Add the content with a single space before it (if there's content)
        if !remaining.is_empty() {
            result.push(' ');
            result.push_str(remaining);
        }

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

                if Self::is_blockquote(prev_line) && Self::is_blockquote(next_line) && current_line.trim().is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_blockquote() {
        // Valid blockquotes
        assert!(BlockquoteUtils::is_blockquote("> Quote"));
        assert!(BlockquoteUtils::is_blockquote(">Quote"));
        assert!(BlockquoteUtils::is_blockquote("  > Indented quote"));
        assert!(BlockquoteUtils::is_blockquote(">> Nested quote"));
        assert!(BlockquoteUtils::is_blockquote(">"));
        assert!(BlockquoteUtils::is_blockquote("> "));

        // Not blockquotes
        assert!(!BlockquoteUtils::is_blockquote(""));
        assert!(!BlockquoteUtils::is_blockquote("Plain text"));
        assert!(!BlockquoteUtils::is_blockquote("a > b"));
        assert!(!BlockquoteUtils::is_blockquote("# > Not a quote"));
    }

    #[test]
    fn test_is_empty_blockquote() {
        // Empty blockquotes
        assert!(BlockquoteUtils::is_empty_blockquote(">"));
        assert!(BlockquoteUtils::is_empty_blockquote("> "));
        assert!(BlockquoteUtils::is_empty_blockquote(">   "));
        assert!(BlockquoteUtils::is_empty_blockquote(">>"));
        assert!(BlockquoteUtils::is_empty_blockquote("  >  "));

        // Not empty blockquotes
        assert!(!BlockquoteUtils::is_empty_blockquote("> Content"));
        assert!(!BlockquoteUtils::is_empty_blockquote(">Text"));
        assert!(!BlockquoteUtils::is_empty_blockquote(""));
        assert!(!BlockquoteUtils::is_empty_blockquote("Plain text"));
    }

    #[test]
    fn test_needs_md028_fix() {
        // Needs fixing (no space after >)
        assert!(BlockquoteUtils::needs_md028_fix(">"));
        assert!(BlockquoteUtils::needs_md028_fix(">>"));
        assert!(BlockquoteUtils::needs_md028_fix("  >"));

        // Does not need fixing
        assert!(!BlockquoteUtils::needs_md028_fix("> "));
        assert!(!BlockquoteUtils::needs_md028_fix("> Content"));
        assert!(!BlockquoteUtils::needs_md028_fix(""));
        assert!(!BlockquoteUtils::needs_md028_fix("Plain text"));
    }

    #[test]
    fn test_has_no_space_after_marker() {
        assert!(BlockquoteUtils::has_no_space_after_marker(">Content"));
        assert!(BlockquoteUtils::has_no_space_after_marker("  >Text"));

        assert!(!BlockquoteUtils::has_no_space_after_marker("> Content"));
        assert!(!BlockquoteUtils::has_no_space_after_marker(">  Content"));
        assert!(!BlockquoteUtils::has_no_space_after_marker(">"));
        assert!(!BlockquoteUtils::has_no_space_after_marker(""));
    }

    #[test]
    fn test_has_multiple_spaces_after_marker() {
        assert!(BlockquoteUtils::has_multiple_spaces_after_marker(">  Content"));
        assert!(BlockquoteUtils::has_multiple_spaces_after_marker(">   Text"));
        assert!(BlockquoteUtils::has_multiple_spaces_after_marker("  >    Quote"));

        assert!(!BlockquoteUtils::has_multiple_spaces_after_marker("> Content"));
        assert!(!BlockquoteUtils::has_multiple_spaces_after_marker(">Content"));
        assert!(!BlockquoteUtils::has_multiple_spaces_after_marker(">"));
        assert!(!BlockquoteUtils::has_multiple_spaces_after_marker(""));
    }

    #[test]
    fn test_is_nested_blockquote() {
        assert!(BlockquoteUtils::is_nested_blockquote(">> Nested"));
        assert!(BlockquoteUtils::is_nested_blockquote(">>> Triple nested"));
        assert!(BlockquoteUtils::is_nested_blockquote("> > Spaced nested"));
        assert!(BlockquoteUtils::is_nested_blockquote("  > >> Indented nested"));

        assert!(!BlockquoteUtils::is_nested_blockquote("> Single level"));
        assert!(!BlockquoteUtils::is_nested_blockquote(">Single"));
        assert!(!BlockquoteUtils::is_nested_blockquote(""));
        assert!(!BlockquoteUtils::is_nested_blockquote("Plain text"));
    }

    #[test]
    fn test_get_nesting_level() {
        assert_eq!(BlockquoteUtils::get_nesting_level(""), 0);
        assert_eq!(BlockquoteUtils::get_nesting_level("Plain text"), 0);
        assert_eq!(BlockquoteUtils::get_nesting_level("> Quote"), 1);
        assert_eq!(BlockquoteUtils::get_nesting_level(">> Nested"), 2);
        assert_eq!(BlockquoteUtils::get_nesting_level(">>> Triple"), 3);
        assert_eq!(BlockquoteUtils::get_nesting_level("  > Indented"), 1);
        assert_eq!(BlockquoteUtils::get_nesting_level("  >> Indented nested"), 2);
        assert_eq!(BlockquoteUtils::get_nesting_level(">>>> Four levels"), 4);
    }

    #[test]
    fn test_extract_content() {
        assert_eq!(BlockquoteUtils::extract_content("> Content"), "Content");
        assert_eq!(BlockquoteUtils::extract_content(">Content"), "Content");
        assert_eq!(BlockquoteUtils::extract_content(">  Content"), " Content");
        assert_eq!(BlockquoteUtils::extract_content("> "), "");
        assert_eq!(BlockquoteUtils::extract_content(">"), "");
        assert_eq!(
            BlockquoteUtils::extract_content("  > Indented content"),
            "Indented content"
        );
        assert_eq!(BlockquoteUtils::extract_content(""), "");
        assert_eq!(BlockquoteUtils::extract_content("Plain text"), "");
    }

    #[test]
    fn test_extract_indentation() {
        assert_eq!(BlockquoteUtils::extract_indentation("> Content"), "");
        assert_eq!(BlockquoteUtils::extract_indentation("  > Content"), "  ");
        assert_eq!(BlockquoteUtils::extract_indentation("    > Content"), "    ");
        assert_eq!(BlockquoteUtils::extract_indentation("\t> Content"), "\t");
        assert_eq!(BlockquoteUtils::extract_indentation(">Content"), "");
        assert_eq!(BlockquoteUtils::extract_indentation(""), "");
        assert_eq!(BlockquoteUtils::extract_indentation("Plain text"), "");
    }

    #[test]
    fn test_fix_blockquote_spacing() {
        // Fix missing space
        assert_eq!(BlockquoteUtils::fix_blockquote_spacing(">Content"), "> Content");
        assert_eq!(BlockquoteUtils::fix_blockquote_spacing("  >Text"), "  > Text");

        // Fix multiple spaces
        assert_eq!(BlockquoteUtils::fix_blockquote_spacing(">  Content"), "> Content");
        assert_eq!(BlockquoteUtils::fix_blockquote_spacing(">   Text"), "> Text");

        // Already correct
        assert_eq!(BlockquoteUtils::fix_blockquote_spacing("> Content"), "> Content");
        assert_eq!(BlockquoteUtils::fix_blockquote_spacing("  > Text"), "  > Text");

        // Not blockquotes
        assert_eq!(BlockquoteUtils::fix_blockquote_spacing(""), "");
        assert_eq!(BlockquoteUtils::fix_blockquote_spacing("Plain text"), "Plain text");
    }

    #[test]
    fn test_fix_nested_blockquote_spacing() {
        // Fix missing spaces between markers
        assert_eq!(
            BlockquoteUtils::fix_nested_blockquote_spacing(">>Content"),
            "> > Content"
        );
        assert_eq!(BlockquoteUtils::fix_nested_blockquote_spacing(">>>Text"), "> > > Text");

        // Fix inconsistent spacing
        assert_eq!(
            BlockquoteUtils::fix_nested_blockquote_spacing("> >Content"),
            "> > Content"
        );
        assert_eq!(
            BlockquoteUtils::fix_nested_blockquote_spacing(">  >Content"),
            "> > Content"
        );

        // Already correct
        assert_eq!(
            BlockquoteUtils::fix_nested_blockquote_spacing("> > Content"),
            "> > Content"
        );
        assert_eq!(
            BlockquoteUtils::fix_nested_blockquote_spacing("> > > Text"),
            "> > > Text"
        );

        // Single level
        assert_eq!(BlockquoteUtils::fix_nested_blockquote_spacing("> Content"), "> Content");
        assert_eq!(BlockquoteUtils::fix_nested_blockquote_spacing(">Content"), "> Content");

        // Empty blockquotes
        assert_eq!(BlockquoteUtils::fix_nested_blockquote_spacing(">"), ">");
        assert_eq!(BlockquoteUtils::fix_nested_blockquote_spacing(">>"), "> >");
        assert_eq!(BlockquoteUtils::fix_nested_blockquote_spacing(">>>"), "> > >");

        // With indentation
        assert_eq!(
            BlockquoteUtils::fix_nested_blockquote_spacing("  >>Content"),
            "  > > Content"
        );
        assert_eq!(
            BlockquoteUtils::fix_nested_blockquote_spacing("\t> > Content"),
            "\t> > Content"
        );

        // Not blockquotes
        assert_eq!(BlockquoteUtils::fix_nested_blockquote_spacing(""), "");
        assert_eq!(
            BlockquoteUtils::fix_nested_blockquote_spacing("Plain text"),
            "Plain text"
        );
    }

    #[test]
    fn test_has_blank_between_blockquotes() {
        let content1 = "> Quote 1\n> Quote 2";
        assert_eq!(
            BlockquoteUtils::has_blank_between_blockquotes(content1),
            Vec::<usize>::new()
        );

        let content2 = "> Quote 1\n>\n> Quote 2";
        assert_eq!(BlockquoteUtils::has_blank_between_blockquotes(content2), vec![2]);

        let content3 = "> Quote 1\n> \n> Quote 2";
        assert_eq!(BlockquoteUtils::has_blank_between_blockquotes(content3), vec![2]);

        let content4 = "> Line 1\n>\n>\n> Line 4";
        assert_eq!(BlockquoteUtils::has_blank_between_blockquotes(content4), vec![2, 3]);

        let content5 = "Plain text\n> Quote";
        assert_eq!(
            BlockquoteUtils::has_blank_between_blockquotes(content5),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn test_fix_blank_between_blockquotes() {
        let content1 = "> Quote 1\n> Quote 2";
        assert_eq!(
            BlockquoteUtils::fix_blank_between_blockquotes(content1),
            "> Quote 1\n> Quote 2"
        );

        let content2 = "> Quote 1\n\n> Quote 2";
        assert_eq!(
            BlockquoteUtils::fix_blank_between_blockquotes(content2),
            "> Quote 1\n> Quote 2"
        );

        // Multiple blank lines - the function keeps them all except when between blockquotes
        let content3 = "> Quote 1\n\n\n> Quote 2";
        assert_eq!(
            BlockquoteUtils::fix_blank_between_blockquotes(content3),
            "> Quote 1\n\n\n> Quote 2"
        );

        let content4 = "Text\n\n> Quote";
        assert_eq!(
            BlockquoteUtils::fix_blank_between_blockquotes(content4),
            "Text\n\n> Quote"
        );
    }

    #[test]
    fn test_get_blockquote_start_col() {
        assert_eq!(BlockquoteUtils::get_blockquote_start_col("> Content"), 1);
        assert_eq!(BlockquoteUtils::get_blockquote_start_col("  > Content"), 3);
        assert_eq!(BlockquoteUtils::get_blockquote_start_col("    > Content"), 5);
        assert_eq!(BlockquoteUtils::get_blockquote_start_col(">Content"), 1);
    }

    #[test]
    fn test_get_blockquote_content() {
        assert_eq!(BlockquoteUtils::get_blockquote_content("> Content"), "Content");
        assert_eq!(BlockquoteUtils::get_blockquote_content(">Content"), "Content");
        assert_eq!(BlockquoteUtils::get_blockquote_content("> "), "");
        assert_eq!(BlockquoteUtils::get_blockquote_content(""), "");
    }

    #[test]
    fn test_unicode_content() {
        assert!(BlockquoteUtils::is_blockquote("> 你好"));
        assert_eq!(BlockquoteUtils::extract_content("> émphasis"), "émphasis");
        assert_eq!(BlockquoteUtils::fix_blockquote_spacing(">🌟"), "> 🌟");
        assert_eq!(BlockquoteUtils::get_nesting_level(">> 日本語"), 2);
    }

    #[test]
    fn test_edge_cases() {
        // Empty string
        assert!(!BlockquoteUtils::is_blockquote(""));
        assert_eq!(BlockquoteUtils::extract_content(""), "");
        assert_eq!(BlockquoteUtils::get_nesting_level(""), 0);

        // Just ">" character in middle of line
        assert!(!BlockquoteUtils::is_blockquote("a > b"));

        // Tabs
        assert!(BlockquoteUtils::is_blockquote("\t> Tab indent"));
        assert_eq!(BlockquoteUtils::extract_indentation("\t> Content"), "\t");

        // Mixed indentation
        assert_eq!(BlockquoteUtils::fix_blockquote_spacing(" \t>Content"), " \t> Content");
    }
}
