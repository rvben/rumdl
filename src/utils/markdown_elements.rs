use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

/// Types of Markdown elements that can be detected
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ElementType {
    CodeBlock,
    CodeSpan,
    Heading,
    List,
    FrontMatter,
}

/// Quality status of an element
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ElementQuality {
    Valid,
    Malformed,
}

/// Represents a detected element in a Markdown document
#[derive(Debug, Clone)]
pub struct MarkdownElement {
    pub element_type: ElementType,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub metadata: Option<String>, // For code blocks: language, for headings: level, etc.
    pub quality: ElementQuality,  // Whether the element is well-formed or malformed
}

lazy_static! {
    // Code block patterns
    static ref CODE_BLOCK_START: Regex = Regex::new(r"^(\s*)(```|~~~)(.*)$").unwrap();
    static ref CODE_BLOCK_END: Regex = Regex::new(r"^(\s*)(```|~~~)\s*$").unwrap();
    static ref CODE_SPAN_PATTERN: Regex = Regex::new(r"`+").unwrap();

    // Heading patterns
    static ref ATX_HEADING: Regex = Regex::new(r"^(\s*)(#{1,6})(\s*)([^#\n]*?)(?:\s+(#{1,6}))?\s*$").unwrap();
    static ref ATX_HEADING_NO_SPACE: Regex = Regex::new(r"^(\s*)(#{1,6})([^#\s][^#\n]*?)(?:\s+(#{1,6}))?\s*$").unwrap();
    static ref SETEXT_HEADING_1: Regex = Regex::new(r"^(\s*)(=+)(\s*)$").unwrap();
    static ref SETEXT_HEADING_2: Regex = Regex::new(r"^(\s*)(-+)(\s*)$").unwrap();

    // List patterns
    static ref UNORDERED_LIST: Regex = Regex::new(r"^(\s*)([*+-])(\s+)").unwrap();
    static ref ORDERED_LIST: Regex = Regex::new(r"^(\s*)(\d+\.)(\s+)").unwrap();

    // Malformed list patterns
    static ref MALFORMED_UNORDERED_LIST: Regex = Regex::new(r"^(\s*)([*+-])([^\s])").unwrap();
    static ref MALFORMED_ORDERED_LIST: Regex = Regex::new(r"^(\s*)(\d+\.)([^\s])").unwrap();
    static ref MALFORMED_ORDERED_LIST_WRONG_MARKER: Regex = Regex::new(r"^(\s*)(\d+[)\]])(\s*)").unwrap();

    // Empty list patterns (just marker without content)
    static ref EMPTY_UNORDERED_LIST: Regex = Regex::new(r"^(\s*)([*+-])\s*$").unwrap();

    // Front matter pattern
    static ref FRONT_MATTER_DELIMITER: Regex = Regex::new(r"^---\s*$").unwrap();
}

/// Utility struct for working with Markdown elements
pub struct MarkdownElements;

impl MarkdownElements {
    /// Detect all code blocks in the content
    pub fn detect_code_blocks(content: &str) -> Vec<MarkdownElement> {
        let mut blocks = Vec::new();
        let mut in_code_block = false;
        let mut block_start = 0;
        let mut language = String::new();
        let mut fence_type = String::new();

        for (i, line) in content.lines().enumerate() {
            if let Some(captures) = CODE_BLOCK_START.captures(line) {
                if !in_code_block {
                    block_start = i;
                    in_code_block = true;
                    fence_type = captures.get(2).unwrap().as_str().to_string();
                    language = captures.get(3).map_or("", |m| m.as_str()).trim().to_string();
                } else if line.trim().starts_with(&fence_type) {
                    // End of code block
                    blocks.push(MarkdownElement {
                        element_type: ElementType::CodeBlock,
                        start_line: block_start,
                        end_line: i,
                        text: content
                            .lines()
                            .skip(block_start)
                            .take(i - block_start + 1)
                            .collect::<Vec<&str>>()
                            .join("\n"),
                        metadata: Some(language.clone()),
                        quality: ElementQuality::Valid,
                    });
                    in_code_block = false;
                    language = String::new();
                }
            }
        }

        // Handle unclosed code blocks
        if in_code_block {
            let line_count = content.lines().count();
            blocks.push(MarkdownElement {
                element_type: ElementType::CodeBlock,
                start_line: block_start,
                end_line: line_count - 1,
                text: content.lines().skip(block_start).collect::<Vec<&str>>().join("\n"),
                metadata: Some(language),
                quality: ElementQuality::Malformed, // Unclosed code block is malformed
            });
        }

        blocks
    }

    /// Detect all code block line indices in the content
    pub fn detect_code_block_lines(content: &str) -> HashSet<usize> {
        let code_blocks = Self::detect_code_blocks(content);
        let mut lines = HashSet::new();

        for block in code_blocks {
            for i in block.start_line..=block.end_line {
                lines.insert(i);
            }
        }

        lines
    }

    /// Check if position in a line is within a code span
    pub fn is_in_code_span(line: &str, position: usize) -> bool {
        let mut in_code_span = false;
        let mut code_start = 0;

        for (pos, c) in line.char_indices() {
            if c == '`' {
                if !in_code_span {
                    in_code_span = true;
                    code_start = pos;
                } else {
                    // Found end of code span, check if position is within
                    if position >= code_start && position <= pos {
                        return true;
                    }
                    in_code_span = false;
                }
            }

            // Early return optimization
            if pos > position && !in_code_span {
                return false;
            }
        }

        // Check if position is in an unclosed code span
        in_code_span && position >= code_start
    }

    /// Detect all headings in the content
    pub fn detect_headings(content: &str) -> Vec<MarkdownElement> {
        let mut headings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let code_block_lines = Self::detect_code_block_lines(content);

        // Get frontmatter to skip those lines
        let frontmatter_lines = if let Some(frontmatter) = Self::detect_front_matter(content) {
            (frontmatter.start_line..=frontmatter.end_line).collect::<HashSet<usize>>()
        } else {
            HashSet::new()
        };

        // Process each line
        for (i, line) in lines.iter().enumerate() {
            // Skip lines in code blocks or frontmatter
            if code_block_lines.contains(&i) || frontmatter_lines.contains(&i) {
                continue;
            }

            // Check for ATX style heading with proper space
            if let Some(captures) = ATX_HEADING.captures(line) {
                let hashes = captures.get(2).unwrap().as_str();
                let level = hashes.len().to_string();
                let text = captures.get(4).map_or("", |m| m.as_str()).trim().to_string();
                let spaces_after_hash = captures.get(3).map_or("", |m| m.as_str()).len();

                // Determine if heading is well-formed
                // Special cases for empty headings: # and ###### are valid, others need space
                let quality = if spaces_after_hash > 0 || (text.is_empty() && (hashes.len() == 1 || hashes.len() == 6))
                {
                    ElementQuality::Valid
                } else {
                    ElementQuality::Malformed
                };

                headings.push(MarkdownElement {
                    element_type: ElementType::Heading,
                    start_line: i,
                    end_line: i,
                    text,
                    metadata: Some(level),
                    quality,
                });

                continue;
            }

            // Check for ATX style heading without space after #
            if let Some(captures) = ATX_HEADING_NO_SPACE.captures(line) {
                let hashes = captures.get(2).unwrap().as_str();
                let level = hashes.len().to_string();
                let text = captures.get(3).map_or("", |m| m.as_str()).trim().to_string();

                headings.push(MarkdownElement {
                    element_type: ElementType::Heading,
                    start_line: i,
                    end_line: i,
                    text,
                    metadata: Some(level),
                    quality: ElementQuality::Malformed, // No space after # makes it malformed
                });

                continue;
            }

            // Check for Setext style heading (requires looking at next line)
            if i + 1 < lines.len() {
                let next_line = lines[i + 1];

                if SETEXT_HEADING_1.is_match(next_line) {
                    headings.push(MarkdownElement {
                        element_type: ElementType::Heading,
                        start_line: i,
                        end_line: i + 1,
                        text: line.trim().to_string(),
                        metadata: Some("1".to_string()), // Level 1 setext heading
                        quality: ElementQuality::Valid,
                    });

                    continue;
                }

                if SETEXT_HEADING_2.is_match(next_line) {
                    headings.push(MarkdownElement {
                        element_type: ElementType::Heading,
                        start_line: i,
                        end_line: i + 1,
                        text: line.trim().to_string(),
                        metadata: Some("2".to_string()), // Level 2 setext heading
                        quality: ElementQuality::Valid,
                    });

                    continue;
                }
            }
        }

        headings
    }

    /// Get heading level (1-6) for a heading element
    pub fn get_heading_level(element: &MarkdownElement) -> Option<u32> {
        if element.element_type != ElementType::Heading {
            return None;
        }

        element.metadata.as_ref().and_then(|level| level.parse::<u32>().ok())
    }

    /// Detect all list items in the content
    pub fn detect_lists(content: &str) -> Vec<MarkdownElement> {
        let mut lists = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let code_block_lines = Self::detect_code_block_lines(content);

        // Get frontmatter to skip those lines
        let frontmatter_lines = if let Some(frontmatter) = Self::detect_front_matter(content) {
            (frontmatter.start_line..=frontmatter.end_line).collect::<HashSet<usize>>()
        } else {
            HashSet::new()
        };

        // Pattern to match horizontal rule or front matter markers
        lazy_static! {
            static ref HORIZONTAL_RULE: Regex = Regex::new(r"^(\s*)(-{3,}|\*{3,}|_{3,})(\s*)$").unwrap();
        }

        for (i, line) in lines.iter().enumerate() {
            // Skip lines in code blocks or frontmatter
            if code_block_lines.contains(&i) || frontmatter_lines.contains(&i) {
                continue;
            }

            // Skip lines that are horizontal rules or front matter markers
            if HORIZONTAL_RULE.is_match(line) {
                continue;
            }

            // Check for well-formed unordered list items
            if let Some(_captures) = UNORDERED_LIST.captures(line) {
                let marker = if line.trim_start().starts_with('*') {
                    "asterisk"
                } else if line.trim_start().starts_with('+') {
                    "plus"
                } else {
                    "minus"
                };

                lists.push(MarkdownElement {
                    element_type: ElementType::List,
                    start_line: i,
                    end_line: i,
                    text: line.trim().to_string(),
                    metadata: Some(marker.to_string()),
                    quality: ElementQuality::Valid,
                });

                continue;
            }

            // Check for empty unordered list items (just marker)
            if let Some(_captures) = EMPTY_UNORDERED_LIST.captures(line) {
                // Exclude horizontal rules and front matter markers
                if line.trim() == "---" || line.trim() == "***" || line.trim() == "___" {
                    continue;
                }

                let marker = if line.trim_start().starts_with('*') {
                    "asterisk"
                } else if line.trim_start().starts_with('+') {
                    "plus"
                } else {
                    "minus"
                };

                lists.push(MarkdownElement {
                    element_type: ElementType::List,
                    start_line: i,
                    end_line: i,
                    text: String::new(), // Empty list item
                    metadata: Some(marker.to_string()),
                    quality: ElementQuality::Valid,
                });

                continue;
            }

            // Check for malformed unordered list (no space after marker)
            if let Some(_captures) = MALFORMED_UNORDERED_LIST.captures(line) {
                // Exclude horizontal rules and front matter markers which might match this pattern
                if line.trim() == "---" || line.trim() == "***" || line.trim() == "___" {
                    continue;
                }

                let marker = if line.trim_start().starts_with('*') {
                    "asterisk:no_space"
                } else if line.trim_start().starts_with('+') {
                    "plus:no_space"
                } else {
                    "minus:no_space"
                };

                lists.push(MarkdownElement {
                    element_type: ElementType::List,
                    start_line: i,
                    end_line: i,
                    text: line.trim().to_string(),
                    metadata: Some(marker.to_string()),
                    quality: ElementQuality::Malformed,
                });

                continue;
            }

            // Check for well-formed ordered list items
            if let Some(_captures) = ORDERED_LIST.captures(line) {
                lists.push(MarkdownElement {
                    element_type: ElementType::List,
                    start_line: i,
                    end_line: i,
                    text: line.trim().to_string(),
                    metadata: Some("ordered".to_string()),
                    quality: ElementQuality::Valid,
                });

                continue;
            }

            // Check for malformed ordered list (no space after marker)
            if let Some(_captures) = MALFORMED_ORDERED_LIST.captures(line) {
                lists.push(MarkdownElement {
                    element_type: ElementType::List,
                    start_line: i,
                    end_line: i,
                    text: line.trim().to_string(),
                    metadata: Some("ordered:no_space".to_string()),
                    quality: ElementQuality::Malformed,
                });

                continue;
            }

            // Check for malformed ordered list (wrong marker type)
            if let Some(_captures) = MALFORMED_ORDERED_LIST_WRONG_MARKER.captures(line) {
                lists.push(MarkdownElement {
                    element_type: ElementType::List,
                    start_line: i,
                    end_line: i,
                    text: line.trim().to_string(),
                    metadata: Some("ordered:wrong_marker".to_string()),
                    quality: ElementQuality::Malformed,
                });
            }
        }

        lists
    }

    /// Detect front matter in content
    pub fn detect_front_matter(content: &str) -> Option<MarkdownElement> {
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() || !FRONT_MATTER_DELIMITER.is_match(lines[0]) {
            return None;
        }

        // Look for closing delimiter
        for (i, line) in lines.iter().enumerate().skip(1) {
            if FRONT_MATTER_DELIMITER.is_match(line) {
                return Some(MarkdownElement {
                    element_type: ElementType::FrontMatter,
                    start_line: 0,
                    end_line: i,
                    text: lines[0..=i].join("\n"),
                    metadata: None,
                    quality: ElementQuality::Valid,
                });
            }
        }

        // Front matter without closing delimiter is malformed
        None
    }

    /// Convert heading text to a valid ID for fragment links
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

    /// Check if a line is in a code block
    pub fn is_line_in_code_block(content: &str, line_number: usize) -> bool {
        let code_block_lines = Self::detect_code_block_lines(content);
        code_block_lines.contains(&line_number)
    }

    /// Get all line indices in a given Markdown element
    pub fn get_element_line_indices(element: &MarkdownElement) -> Vec<usize> {
        (element.start_line..=element.end_line).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_code_blocks() {
        let content = "# Heading\n```js\nlet x = 1;\n```\nText";
        let blocks = MarkdownElements::detect_code_blocks(content);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].element_type, ElementType::CodeBlock);
        assert_eq!(blocks[0].start_line, 1);
        assert_eq!(blocks[0].end_line, 3);
        assert_eq!(blocks[0].metadata, Some("js".to_string()));
    }

    #[test]
    fn test_is_in_code_span() {
        let line = "Text with `code` and more";
        assert!(!MarkdownElements::is_in_code_span(line, 0));
        assert!(MarkdownElements::is_in_code_span(line, 11));
        assert!(!MarkdownElements::is_in_code_span(line, 20));
    }

    #[test]
    fn test_detect_headings() {
        let content = "# Heading 1\n## Heading 2\nText\nHeading 3\n===";
        let headings = MarkdownElements::detect_headings(content);

        assert_eq!(headings.len(), 3);
        assert_eq!(MarkdownElements::get_heading_level(&headings[0]), Some(1));
        assert_eq!(MarkdownElements::get_heading_level(&headings[1]), Some(2));
        assert_eq!(MarkdownElements::get_heading_level(&headings[2]), Some(1));
    }

    #[test]
    fn test_detect_lists() {
        let content = "- Item 1\n* Item 2\n+ Item 3\n1. Item 4";
        let lists = MarkdownElements::detect_lists(content);

        assert_eq!(lists.len(), 4);
        assert_eq!(lists[0].metadata, Some("minus".to_string()));
        assert_eq!(lists[1].metadata, Some("asterisk".to_string()));
        assert_eq!(lists[2].metadata, Some("plus".to_string()));
        assert_eq!(lists[3].metadata, Some("ordered".to_string()));
    }

    #[test]
    fn test_detect_front_matter() {
        let content = "---\ntitle: Test\n---\n# Content";
        let front_matter = MarkdownElements::detect_front_matter(content);

        assert!(front_matter.is_some());
        assert_eq!(front_matter.unwrap().end_line, 2);
    }

    #[test]
    fn test_heading_to_fragment() {
        assert_eq!(MarkdownElements::heading_to_fragment("Hello World!"), "hello-world");
        assert_eq!(
            MarkdownElements::heading_to_fragment("Complex: (Header) 123"),
            "complex-header-123"
        );
    }

    #[test]
    fn test_is_line_in_code_block() {
        let content = "Text\n```\nCode\n```\nMore text";
        assert!(!MarkdownElements::is_line_in_code_block(content, 0));
        assert!(MarkdownElements::is_line_in_code_block(content, 1));
        assert!(MarkdownElements::is_line_in_code_block(content, 2));
        assert!(MarkdownElements::is_line_in_code_block(content, 3));
        assert!(!MarkdownElements::is_line_in_code_block(content, 4));
    }
}
