use lazy_static::lazy_static;
use regex::Regex;
use std::cell::RefCell;

lazy_static! {
    // Efficient regex patterns
    static ref CODE_BLOCK_START_REGEX: Regex = Regex::new(r"^(\s*)(```|~~~)(.*)$").unwrap();
    static ref CODE_BLOCK_END_REGEX: Regex = Regex::new(r"^(\s*)(```|~~~)\s*$").unwrap();
    static ref INDENTED_CODE_BLOCK_REGEX: Regex = Regex::new(r"^(\s{4,})(.+)$").unwrap();
    
    // List detection patterns
    static ref UNORDERED_LIST_REGEX: Regex = Regex::new(r"^(\s*)([*+-])(\s+)").unwrap();
    static ref ORDERED_LIST_REGEX: Regex = Regex::new(r"^(\s*)(\d+\.)(\s+)").unwrap();
    
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
    pub marker_type: ListMarkerType,
    pub marker: String,
    pub content: String,
    pub spaces_after_marker: usize,
    pub nesting_level: usize,
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
        self.code_spans.iter().any(|span| position >= span.start && position < span.end)
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
        self.list_items.iter().find(|item| item.line_number == line_num)
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
                        let start_pos = lines[0..block_start_line].join("\n").len() + if block_start_line > 0 { 1 } else { 0 };
                        let end_pos = lines[0..=i].join("\n").len();
                        
                        self.code_blocks.push(CodeBlock {
                            range: Range { start: start_pos, end: end_pos },
                            block_type: CodeBlockType::Fenced,
                            start_line: block_start_line + 1, // 1-indexed
                            end_line: i + 1, // 1-indexed
                            language: if !block_language.is_empty() { Some(block_language.clone()) } else { None },
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
                    // Indented code block
                    self.code_block_line_map[i] = true;
                    
                    // For indented code blocks, we handle them as individual lines
                    // We don't track them as blocks with start/end because they can be
                    // interrupted by blank lines, etc.
                    let start_pos = lines[0..i].join("\n").len() + if i > 0 { 1 } else { 0 };
                    let end_pos = start_pos + line.len();
                    
                    self.code_blocks.push(CodeBlock {
                        range: Range { start: start_pos, end: end_pos },
                        block_type: CodeBlockType::Indented,
                        start_line: i + 1, // 1-indexed
                        end_line: i + 1, // 1-indexed
                        language: None,
                    });
                }
            }
            
            // Handle unclosed code block
            if in_fenced_block {
                let start_pos = lines[0..block_start_line].join("\n").len() + if block_start_line > 0 { 1 } else { 0 };
                let end_pos = content.len();
                
                self.code_blocks.push(CodeBlock {
                    range: Range { start: start_pos, end: end_pos },
                    block_type: CodeBlockType::Fenced,
                    start_line: block_start_line + 1, // 1-indexed
                    end_line: lines.len(), // 1-indexed
                    language: if !block_language.is_empty() { Some(block_language) } else { None },
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
            let mut list_levels: Vec<(usize, usize)> = Vec::new(); // (indent, level)
            
            for (i, line) in lines.iter().enumerate() {
                // Skip lines in code blocks
                if self.code_block_line_map[i] {
                    continue;
                }
                
                // Check if it's a list item
                if let Some(item) = self.parse_list_item(line, i + 1, &mut list_levels) {
                    self.list_items.push(item);
                    self.list_line_map[i] = true;
                }
            }
        }
    }
    
    /// Parse a line as a list item and determine its nesting level
    fn parse_list_item(&self, line: &str, line_num: usize, list_levels: &mut Vec<(usize, usize)>) -> Option<ListItem> {
        // Try to match unordered list pattern
        if let Some(captures) = UNORDERED_LIST_REGEX.captures(line) {
            let indentation = captures.get(1).map_or(0, |m| m.as_str().len());
            let marker = captures.get(2).unwrap().as_str();
            let spaces = captures.get(3).map_or(0, |m| m.as_str().len());
            let content_start = indentation + marker.len() + spaces;
            let content = if content_start < line.len() {
                line[content_start..].to_string()
            } else {
                String::new()
            };
            
            // Determine marker type
            let marker_type = match marker {
                "*" => ListMarkerType::Asterisk,
                "+" => ListMarkerType::Plus,
                "-" => ListMarkerType::Minus,
                _ => unreachable!(), // Regex ensures this
            };
            
            // Calculate nesting level
            let nesting_level = self.calculate_nesting_level(indentation, list_levels);
            
            return Some(ListItem {
                line_number: line_num,
                indentation,
                marker_type,
                marker: marker.to_string(),
                content,
                spaces_after_marker: spaces,
                nesting_level,
            });
        }
        
        // Try to match ordered list pattern
        if let Some(captures) = ORDERED_LIST_REGEX.captures(line) {
            let indentation = captures.get(1).map_or(0, |m| m.as_str().len());
            let marker = captures.get(2).unwrap().as_str();
            let spaces = captures.get(3).map_or(0, |m| m.as_str().len());
            let content_start = indentation + marker.len() + spaces;
            let content = if content_start < line.len() {
                line[content_start..].to_string()
            } else {
                String::new()
            };
            
            // Calculate nesting level
            let nesting_level = self.calculate_nesting_level(indentation, list_levels);
            
            return Some(ListItem {
                line_number: line_num,
                indentation,
                marker_type: ListMarkerType::Ordered,
                marker: marker.to_string(),
                content,
                spaces_after_marker: spaces,
                nesting_level,
            });
        }
        
        None
    }
    
    /// Calculate the nesting level for a list item
    fn calculate_nesting_level(&self, indent: usize, list_levels: &mut Vec<(usize, usize)>) -> usize {
        // Determine the nesting level based on indentation
        if indent == 0 {
            // Top level item
            list_levels.clear();
            list_levels.push((0, 0));
            0
        } else {
            // Find the appropriate nesting level based on indentation
            let mut level = 0;
            
            for &(prev_indent, prev_level) in list_levels.iter().rev() {
                if indent > prev_indent {
                    level = prev_level + 1;
                    break;
                } else if indent == prev_indent {
                    level = prev_level;
                    break;
                }
            }
            
            // Update the list level tracking
            list_levels.push((indent, level));
            level
        }
    }
}

// Thread-local cache for sharing across rules
thread_local! {
    static ELEMENT_CACHE: RefCell<Option<ElementCache>> = RefCell::new(None);
}

/// Get or create element cache for document content
pub fn get_element_cache(content: &str) -> ElementCache {
    // Try to get existing cache
    let mut needs_new_cache = false;
    
    ELEMENT_CACHE.with(|cache| {
        let cache_ref = cache.borrow_mut();
        
        // If cache exists and content matches, return it
        if let Some(existing_cache) = &*cache_ref {
            if let Some(cached_content) = &existing_cache.content {
                if cached_content == content {
                    return;  // Keep existing cache
                }
            }
        }
        
        // Content doesn't match, need new cache
        needs_new_cache = true;
    });
    
    if needs_new_cache {
        // Create new cache
        let new_cache = ElementCache::new(content);
        
        // Store in thread-local
        ELEMENT_CACHE.with(|cache| {
            *cache.borrow_mut() = Some(new_cache);
        });
    }
    
    // Return clone of cache
    ELEMENT_CACHE.with(|cache| {
        cache.borrow().clone().unwrap_or_else(|| ElementCache::new(content))
    })
}

/// Reset the element cache
pub fn reset_element_cache() {
    ELEMENT_CACHE.with(|cache| {
        *cache.borrow_mut() = None;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_code_block_detection() {
        let content = "Regular text\n\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\n\nMore text";
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
    fn test_list_item_detection() {
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
} 