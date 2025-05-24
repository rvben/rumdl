use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use std::sync::{Arc, Mutex};

lazy_static! {
    // Efficient regex patterns
    static ref CODE_BLOCK_START_REGEX: Regex = Regex::new(r"^(\s*)(```|~~~)(.*)$").unwrap();
    static ref CODE_BLOCK_END_REGEX: Regex = Regex::new(r"^(\s*)(```|~~~)\s*$").unwrap();
    static ref INDENTED_CODE_BLOCK_REGEX: Regex = Regex::new(r"^(\s{4,})(.+)$").unwrap();

    // List detection patterns
    static ref UNORDERED_LIST_REGEX: FancyRegex = FancyRegex::new(r"^(?P<indent>[ \t]*)(?P<marker>[*+-])(?P<after>[ \t]*)(?P<content>.*)$").unwrap();
    static ref ORDERED_LIST_REGEX: FancyRegex = FancyRegex::new(r"^(?P<indent>[ \t]*)(?P<marker>\d+\.)(?P<after>[ \t]*)(?P<content>.*)$").unwrap();

    // Inline code span pattern
    static ref CODE_SPAN_REGEX: Regex = Regex::new(r"`+").unwrap();
}

/// Represents a range in the document with start and end positions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: usize,
    pub end: usize,
}

/// Represents the type of code block
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeBlockType {
    Fenced,
    Indented,
}

/// Represents a code block in the document
#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub range: Range,
    pub block_type: CodeBlockType,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Option<String>,
}

/// Represents the type of list marker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMarkerType {
    Asterisk,
    Plus,
    Minus,
    Ordered,
}

/// Represents a list item in the document
#[derive(Debug, Clone)]
pub struct ListItem {
    pub line_number: usize, // 1-indexed
    pub indentation: usize,
    pub indent_str: String, // Actual leading whitespace
    pub marker_type: ListMarkerType,
    pub marker: String,
    pub content: String,
    pub spaces_after_marker: usize,
    pub nesting_level: usize,
    pub parent_line_number: Option<usize>,
    pub blockquote_depth: usize, // Number of leading blockquote markers
    pub blockquote_prefix: String, // The actual prefix (e.g., "> > ")
}

/// Cache for Markdown document structural elements
/// This allows sharing computed data across multiple rule checks
#[derive(Debug, Default, Clone)]
pub struct ElementCache {
    // Document content and metadata
    content: Option<String>,
    line_count: usize,

    // Code blocks
    code_blocks: Vec<CodeBlock>,
    code_block_line_map: Vec<bool>, // Line index -> is in code block

    // Code spans (inline code)
    code_spans: Vec<Range>,

    // Lists
    list_items: Vec<ListItem>,
    list_line_map: Vec<bool>, // Line index -> is list item
}

impl ElementCache {
    /// Create a new cache from document content
    pub fn new(content: &str) -> Self {
        let mut cache = ElementCache {
            content: Some(content.to_string()),
            line_count: content.lines().count(),
            code_blocks: Vec::new(),
            code_block_line_map: Vec::new(),
            code_spans: Vec::new(),
            list_items: Vec::new(),
            list_line_map: Vec::new(),
        };

        // Initialize maps
        cache.code_block_line_map = vec![false; cache.line_count];
        cache.list_line_map = vec![false; cache.line_count];

        // Populate the cache
        cache.populate_code_blocks();
        cache.populate_code_spans();
        cache.populate_list_items();

        cache
    }

    /// Check if a line is within a code block
    pub fn is_in_code_block(&self, line_num: usize) -> bool {
        if line_num == 0 || line_num > self.code_block_line_map.len() {
            return false;
        }
        self.code_block_line_map[line_num - 1] // Convert 1-indexed to 0-indexed
    }

    /// Check if a position is within a code span
    pub fn is_in_code_span(&self, position: usize) -> bool {
        self.code_spans
            .iter()
            .any(|span| position >= span.start && position < span.end)
    }

    /// Check if a line is a list item
    pub fn is_list_item(&self, line_num: usize) -> bool {
        if line_num == 0 || line_num > self.list_line_map.len() {
            return false;
        }
        self.list_line_map[line_num - 1] // Convert 1-indexed to 0-indexed
    }

    /// Get list item at line
    pub fn get_list_item(&self, line_num: usize) -> Option<&ListItem> {
        self.list_items
            .iter()
            .find(|item| item.line_number == line_num)
    }

    /// Get all list items
    pub fn get_list_items(&self) -> &[ListItem] {
        &self.list_items
    }

    /// Get all code blocks
    pub fn get_code_blocks(&self) -> &[CodeBlock] {
        &self.code_blocks
    }

    /// Get all code spans
    pub fn get_code_spans(&self) -> &[Range] {
        &self.code_spans
    }

    /// Detect and populate code blocks
    fn populate_code_blocks(&mut self) {
        if let Some(content) = &self.content {
            let lines: Vec<&str> = content.lines().collect();
            let mut in_fenced_block = false;
            let mut fence_marker = String::new();
            let mut block_start_line = 0;
            let mut block_language = String::new();

            for (i, line) in lines.iter().enumerate() {
                if in_fenced_block {
                    // Already in a fenced code block, look for the end
                    self.code_block_line_map[i] = true;

                    if line.trim().starts_with(&fence_marker) {
                        // End of code block
                        let start_pos = lines[0..block_start_line].join("\n").len()
                            + if block_start_line > 0 { 1 } else { 0 };
                        let end_pos = lines[0..=i].join("\n").len();

                        self.code_blocks.push(CodeBlock {
                            range: Range {
                                start: start_pos,
                                end: end_pos,
                            },
                            block_type: CodeBlockType::Fenced,
                            start_line: block_start_line + 1, // 1-indexed
                            end_line: i + 1,                  // 1-indexed
                            language: if !block_language.is_empty() {
                                Some(block_language.clone())
                            } else {
                                None
                            },
                        });

                        in_fenced_block = false;
                        fence_marker.clear();
                        block_language.clear();
                    }
                } else if let Some(caps) = CODE_BLOCK_START_REGEX.captures(line) {
                    // Start of a new code block
                    fence_marker = caps.get(2).map_or("```", |m| m.as_str()).to_string();
                    in_fenced_block = true;
                    block_start_line = i;
                    block_language = caps.get(3).map_or("", |m| m.as_str().trim()).to_string();
                    self.code_block_line_map[i] = true;
                } else if INDENTED_CODE_BLOCK_REGEX.is_match(line) {
                    // Only mark as indented code block if not a list item
                    let is_unordered_list = UNORDERED_LIST_REGEX.is_match(line).unwrap_or(false);
                    let is_ordered_list = ORDERED_LIST_REGEX.is_match(line).unwrap_or(false);
                    if !is_unordered_list && !is_ordered_list {
                        // Indented code block
                        self.code_block_line_map[i] = true;
                        // For indented code blocks, we handle them as individual lines
                        // We don't track them as blocks with start/end because they can be
                        // interrupted by blank lines, etc.
                        let start_pos = lines[0..i].join("\n").len() + if i > 0 { 1 } else { 0 };
                        let end_pos = start_pos + line.len();
                        self.code_blocks.push(CodeBlock {
                            range: Range {
                                start: start_pos,
                                end: end_pos,
                            },
                            block_type: CodeBlockType::Indented,
                            start_line: i + 1, // 1-indexed
                            end_line: i + 1,   // 1-indexed
                            language: None,
                        });
                    }
                }
            }

            // Handle unclosed code block
            if in_fenced_block {
                let start_pos = lines[0..block_start_line].join("\n").len()
                    + if block_start_line > 0 { 1 } else { 0 };
                let end_pos = content.len();

                self.code_blocks.push(CodeBlock {
                    range: Range {
                        start: start_pos,
                        end: end_pos,
                    },
                    block_type: CodeBlockType::Fenced,
                    start_line: block_start_line + 1, // 1-indexed
                    end_line: lines.len(),            // 1-indexed
                    language: if !block_language.is_empty() {
                        Some(block_language)
                    } else {
                        None
                    },
                });
            }
        }
    }

    /// Detect and populate code spans
    fn populate_code_spans(&mut self) {
        if let Some(content) = &self.content {
            // Find inline code spans using regex for backticks
            let mut i = 0;
            while i < content.len() {
                if let Some(m) = CODE_SPAN_REGEX.find_at(content, i) {
                    let backtick_length = m.end() - m.start();
                    let start = m.start();

                    // Find matching closing backticks
                    if let Some(end_pos) = content[m.end()..].find(&"`".repeat(backtick_length)) {
                        let end = m.end() + end_pos + backtick_length;
                        self.code_spans.push(Range { start, end });
                        i = end;
                    } else {
                        i = m.end();
                    }
                } else {
                    break;
                }
            }
        }
    }

    /// Detect and populate list items
    fn populate_list_items(&mut self) {
        if let Some(content) = &self.content {
            let lines: Vec<&str> = content.lines().collect();
            let mut prev_items: Vec<(usize, usize, usize)> = Vec::new(); // (blockquote_depth, nesting_level, line_number)
            for (i, line) in lines.iter().enumerate() {
                // Reset stack on blank lines
                if line.trim().is_empty() {
                    prev_items.clear(); // Reset nesting after blank line
                    continue;
                }
                // Parse and strip blockquote prefix
                let (blockquote_depth, blockquote_prefix, rest) = Self::parse_blockquote_prefix(line);
                // Always call parse_list_item and always push if Some
                if let Some(item) = self.parse_list_item(rest, i + 1, &mut prev_items, blockquote_depth, blockquote_prefix.clone()) {
                    self.list_items.push(item);
                    self.list_line_map[i] = true;
                }
            }
        }
    }

    /// Parse and strip all leading blockquote markers, returning (depth, prefix, rest_of_line)
    fn parse_blockquote_prefix(line: &str) -> (usize, String, &str) {
        let mut rest = line;
        let mut prefix = String::new();
        let mut depth = 0;
        loop {
            let trimmed = rest.trim_start();
            if trimmed.starts_with('>') {
                // Find the '>' and a single optional space
                let after = &trimmed[1..];
                let mut chars = after.chars();
                let mut space_count = 0;
                if let Some(' ') = chars.next() {
                    space_count = 1;
                }
                let (spaces, after_marker) = after.split_at(space_count);
                prefix.push('>');
                prefix.push_str(spaces);
                rest = after_marker;
                depth += 1;
            } else {
                break;
            }
        }
        (depth, prefix, rest)
    }

    /// Calculate the nesting level for a list item, considering blockquote depth
    fn calculate_nesting_level(
        &self,
        indent: usize,
        blockquote_depth: usize,
        prev_items: &mut Vec<(usize, usize, usize)>,
    ) -> usize {
        // Only consider previous items with the same blockquote depth
        let mut nesting_level = 0;
        // _last_bq is always equal to blockquote_depth here due to the filter above
        if let Some(&(_last_bq, last_indent, last_level)) = prev_items.iter().rev().find(|(bq, _, _)| *bq == blockquote_depth) {
            if indent >= last_indent + 2 {
                nesting_level = last_level + 1;
            } else {
                // Walk back to find the most recent indent <= current indent with same blockquote depth
                for &(prev_bq, prev_indent, prev_level) in prev_items.iter().rev() {
                    if prev_bq == blockquote_depth && prev_indent <= indent {
                        nesting_level = prev_level;
                        break;
                    }
                }
            }
        }
        // Remove stack entries with indent >= current indent and same blockquote depth
        while let Some(&(prev_bq, prev_indent, _)) = prev_items.last() {
            if prev_bq != blockquote_depth || prev_indent < indent {
                break;
            }
            prev_items.pop();
        }
        prev_items.push((blockquote_depth, indent, nesting_level));
        nesting_level
    }

    /// Parse a line as a list item and determine its nesting level
    fn parse_list_item(
        &self,
        line: &str,
        line_num: usize,
        prev_items: &mut Vec<(usize, usize, usize)>,
        blockquote_depth: usize,
        blockquote_prefix: String,
    ) -> Option<ListItem> {
        match UNORDERED_LIST_REGEX.captures(line) {
            Ok(Some(captures)) => {
                let indent_str = captures
                    .name("indent")
                    .map_or("", |m| m.as_str())
                    .to_string();
                let indentation = indent_str.len();
                let marker = captures.name("marker").unwrap().as_str();
                let after = captures.name("after").map_or("", |m| m.as_str());
                let spaces = after.len();
                let raw_content = captures.name("content").map_or("", |m| m.as_str());
                let content = raw_content.trim_start().to_string();
                let marker_type = match marker {
                    "*" => ListMarkerType::Asterisk,
                    "+" => ListMarkerType::Plus,
                    "-" => ListMarkerType::Minus,
                    _ => unreachable!(),
                };
                let nesting_level = self.calculate_nesting_level(indentation, blockquote_depth, prev_items);
                // Find parent: most recent previous item with lower nesting_level and same blockquote depth
                let parent_line_number = prev_items
                    .iter()
                    .rev()
                    .find(|(bq, _, level)| *bq == blockquote_depth && *level < nesting_level)
                    .map(|(_, _, line_num)| *line_num);
                return Some(ListItem {
                    line_number: line_num,
                    indentation,
                    indent_str,
                    marker_type,
                    marker: marker.to_string(),
                    content,
                    spaces_after_marker: spaces,
                    nesting_level,
                    parent_line_number,
                    blockquote_depth,
                    blockquote_prefix,
                });
            }
            Ok(None) => {
                // No debug output
            }
            Err(_) => {}
        }
        match ORDERED_LIST_REGEX.captures(line) {
            Ok(Some(captures)) => {
                let indent_str = captures
                    .name("indent")
                    .map_or("", |m| m.as_str())
                    .to_string();
                let indentation = indent_str.len();
                let marker = captures.name("marker").unwrap().as_str();
                let spaces = captures.name("after").map_or(0, |m| m.as_str().len());
                let content = captures
                    .name("content")
                    .map_or("", |m| m.as_str())
                    .trim_start()
                    .to_string();
                let nesting_level = self.calculate_nesting_level(indentation, blockquote_depth, prev_items);
                // Find parent: most recent previous item with lower nesting_level and same blockquote depth
                let parent_line_number = prev_items
                    .iter()
                    .rev()
                    .find(|(bq, _, level)| *bq == blockquote_depth && *level < nesting_level)
                    .map(|(_, _, line_num)| *line_num);
                return Some(ListItem {
                    line_number: line_num,
                    indentation,
                    indent_str,
                    marker_type: ListMarkerType::Ordered,
                    marker: marker.to_string(),
                    content,
                    spaces_after_marker: spaces,
                    nesting_level,
                    parent_line_number,
                    blockquote_depth,
                    blockquote_prefix,
                });
            }
            Ok(None) => {}
            Err(_) => {}
        }
        None
    }
}

// Global cache for sharing across threads
lazy_static! {
    static ref ELEMENT_CACHE: Arc<Mutex<Option<ElementCache>>> = Arc::new(Mutex::new(None));
}

/// Get or create element cache for document content
pub fn get_element_cache(content: &str) -> ElementCache {
    // Try to get existing cache
    {
        let cache_guard = ELEMENT_CACHE.lock().unwrap();

        // If cache exists and content matches, return it
        if let Some(existing_cache) = &*cache_guard {
            if let Some(cached_content) = &existing_cache.content {
                if cached_content == content {
                    return existing_cache.clone(); // Keep existing cache
                }
            }
        }
    }

    // Content doesn't match, create new cache
    let new_cache = ElementCache::new(content);

    // Store in global cache
    {
        let mut cache_guard = ELEMENT_CACHE.lock().unwrap();
        *cache_guard = Some(new_cache.clone());
    }

    new_cache
}

/// Reset the element cache
pub fn reset_element_cache() {
    let mut cache_guard = ELEMENT_CACHE.lock().unwrap();
    *cache_guard = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_block_detection() {
        let content =
            "Regular text\n\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\n\nMore text";
        let cache = ElementCache::new(content);

        assert_eq!(cache.code_blocks.len(), 1);
        assert_eq!(cache.code_blocks[0].start_line, 3);
        assert_eq!(cache.code_blocks[0].end_line, 7);
        assert_eq!(cache.code_blocks[0].block_type, CodeBlockType::Fenced);
        assert_eq!(cache.code_blocks[0].language, Some("rust".to_string()));

        assert!(!cache.is_in_code_block(1));
        assert!(!cache.is_in_code_block(2));
        assert!(cache.is_in_code_block(3));
        assert!(cache.is_in_code_block(4));
        assert!(cache.is_in_code_block(5));
        assert!(cache.is_in_code_block(6));
        assert!(cache.is_in_code_block(7));
        assert!(!cache.is_in_code_block(8));
        assert!(!cache.is_in_code_block(9));
    }

    #[test]
    fn test_list_item_detection_simple() {
        let content = "# Heading\n\n- First item\n  - Nested item\n- Second item\n\n1. Ordered item\n   1. Nested ordered\n";
        let cache = ElementCache::new(content);
        assert_eq!(cache.list_items.len(), 5);
        // Check the first item
        assert_eq!(cache.list_items[0].line_number, 3);
        assert_eq!(cache.list_items[0].marker, "-");
        assert_eq!(cache.list_items[0].nesting_level, 0);
        // Check the nested item
        assert_eq!(cache.list_items[1].line_number, 4);
        assert_eq!(cache.list_items[1].marker, "-");
        assert_eq!(cache.list_items[1].nesting_level, 1);
        // Check the second list item
        assert_eq!(cache.list_items[2].line_number, 5);
        assert_eq!(cache.list_items[2].marker, "-");
        assert_eq!(cache.list_items[2].nesting_level, 0);
        // Check ordered list item
        assert_eq!(cache.list_items[3].line_number, 7);
        assert_eq!(cache.list_items[3].marker, "1.");
        assert_eq!(cache.list_items[3].nesting_level, 0);
        // Check nested ordered list item
        assert_eq!(cache.list_items[4].line_number, 8);
        assert_eq!(cache.list_items[4].marker, "1.");
        assert_eq!(cache.list_items[4].nesting_level, 1);
    }

    #[test]
    fn test_list_item_detection_complex() {
        let complex = "  * Level 1 item 1\n    - Level 2 item 1\n      + Level 3 item 1\n    - Level 2 item 2\n  * Level 1 item 2\n\n* Top\n  + Nested\n    - Deep\n      * Deeper\n        + Deepest\n";
        let cache = ElementCache::new(complex);
        // Should detect all 10 list items
        assert_eq!(cache.list_items.len(), 10);
        // Check markers and nesting levels
        assert_eq!(cache.list_items[0].marker, "*");
        assert_eq!(cache.list_items[0].nesting_level, 0);
        assert_eq!(cache.list_items[1].marker, "-");
        assert_eq!(cache.list_items[1].nesting_level, 1);
        assert_eq!(cache.list_items[2].marker, "+");
        assert_eq!(cache.list_items[2].nesting_level, 2);
        assert_eq!(cache.list_items[3].marker, "-");
        assert_eq!(cache.list_items[3].nesting_level, 1);
        assert_eq!(cache.list_items[4].marker, "*");
        assert_eq!(cache.list_items[4].nesting_level, 0);
        assert_eq!(cache.list_items[5].marker, "*");
        assert_eq!(cache.list_items[5].nesting_level, 0);
        assert_eq!(cache.list_items[6].marker, "+");
        assert_eq!(cache.list_items[6].nesting_level, 1);
        assert_eq!(cache.list_items[7].marker, "-");
        assert_eq!(cache.list_items[7].nesting_level, 2);
        assert_eq!(cache.list_items[8].marker, "*");
        assert_eq!(cache.list_items[8].nesting_level, 3);
        assert_eq!(cache.list_items[9].marker, "+");
        assert_eq!(cache.list_items[9].nesting_level, 4);
        let expected_nesting = vec![0, 1, 2, 1, 0, 0, 1, 2, 3, 4];
        let actual_nesting: Vec<_> = cache
            .list_items
            .iter()
            .map(|item| item.nesting_level)
            .collect();
        assert_eq!(
            actual_nesting, expected_nesting,
            "Nesting levels should match expected values"
        );
    }

    #[test]
    fn test_list_item_detection_edge() {
        let edge = "* Item 1\n\n    - Nested 1\n  + Nested 2\n\n* Item 2\n";
        let cache = ElementCache::new(edge);
        assert_eq!(cache.list_items.len(), 4);
        // Per CommonMark, after a blank line, indented list items are not nested unless they are part of a continued list structure.
        // All items should have nesting_level 0.
        for item in &cache.list_items {
            assert_eq!(item.nesting_level, 0);
        }
    }

    #[test]
    fn test_code_span_detection() {
        let content = "Here is some `inline code` and here are ``nested `code` spans``";
        let cache = ElementCache::new(content);

        // Should have two code spans
        assert_eq!(cache.code_spans.len(), 2);

        // Check spans
        let span1_content = &content[cache.code_spans[0].start..cache.code_spans[0].end];
        assert_eq!(span1_content, "`inline code`");

        let span2_content = &content[cache.code_spans[1].start..cache.code_spans[1].end];
        assert_eq!(span2_content, "``nested `code` spans``");
    }

    #[test]
    fn test_get_element_cache() {
        let content1 = "Test content";
        let content2 = "Different content";

        // First call should create a new cache
        let cache1 = get_element_cache(content1);

        // Second call with same content should return the same cache
        let cache2 = get_element_cache(content1);

        // Third call with different content should create new cache
        let cache3 = get_element_cache(content2);

        assert_eq!(cache1.content.as_ref().unwrap(), content1);
        assert_eq!(cache2.content.as_ref().unwrap(), content1);
        assert_eq!(cache3.content.as_ref().unwrap(), content2);
    }

    #[test]
    fn test_list_item_detection_deep_nesting_and_edge_cases() {
        // Deeply nested unordered lists, mixed markers, excessive indentation, tabs, and blank lines
        let content = "\
* Level 1
  - Level 2
    + Level 3
      * Level 4
        - Level 5
          + Level 6
* Sibling 1
    * Sibling 2
\n    - After blank line, not nested\n\n\t* Tab indented\n        * 8 spaces indented\n* After excessive indent\n";
        let cache = ElementCache::new(content);
        // Should detect all lines that start with a valid unordered list marker
        let _expected_markers = ["*", "-", "+", "*", "-", "+", "*", "*", "-", "*", "*", "*"];
        let _expected_indents = [0, 4, 8, 0, 4, 8, 0, 4, 8, 12, 16, 20];
        let expected_content = vec![
            "Level 1",
            "Level 2",
            "Level 3",
            "Level 4",
            "Level 5",
            "Level 6",
            "Sibling 1",
            "Sibling 2",
            "After blank line, not nested",
            "Tab indented",      // Content after marker
            "8 spaces indented", // Content after marker
            "After excessive indent",
        ];
        let actual_content: Vec<_> = cache
            .list_items
            .iter()
            .map(|item| item.content.clone())
            .collect();
        assert_eq!(
            actual_content, expected_content,
            "List item contents should match expected values"
        );
        let expected_nesting = vec![0, 1, 2, 3, 4, 5, 0, 1, 0, 0, 1, 0];
        let actual_nesting: Vec<_> = cache
            .list_items
            .iter()
            .map(|item| item.nesting_level)
            .collect();
        assert_eq!(
            actual_nesting, expected_nesting,
            "Nesting levels should match expected values"
        );
        // Check that tab-indented and 8-space-indented items are detected
        assert!(
            cache
                .list_items
                .iter()
                .any(|item| item.marker == "*" && item.indentation >= 1),
            "Tab or 8-space indented item not detected"
        );
        // Check that after blank lines, items are not nested
        let after_blank = cache
            .list_items
            .iter()
            .find(|item| item.content.contains("After blank line"));
        assert!(after_blank.is_some());
        assert_eq!(
            after_blank.unwrap().nesting_level,
            0,
            "Item after blank line should not be nested"
        );
    }
}
