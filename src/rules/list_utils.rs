use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    // Optimized list detection patterns with anchors and non-capturing groups
    static ref UNORDERED_LIST_PATTERN: Regex = Regex::new(r"^(\s*)([*+-])(\s+)").unwrap();
    static ref ORDERED_LIST_PATTERN: Regex = Regex::new(r"^(\s*)(\d+\.)(\s+)").unwrap();
    
    // Patterns for lists without proper spacing
    static ref UNORDERED_LIST_NO_SPACE_PATTERN: Regex = Regex::new(r"^(\s*)([*+-])([^\s])").unwrap();
    static ref ORDERED_LIST_NO_SPACE_PATTERN: Regex = Regex::new(r"^(\s*)(\d+\.)([^\s])").unwrap();
    
    // Patterns for lists with multiple spaces
    static ref UNORDERED_LIST_MULTIPLE_SPACE_PATTERN: Regex = Regex::new(r"^(\s*)([*+-])(\s{2,})").unwrap();
    static ref ORDERED_LIST_MULTIPLE_SPACE_PATTERN: Regex = Regex::new(r"^(\s*)(\d+\.)(\s{2,})").unwrap();
}

/// Enum representing different types of list markers
#[derive(Debug, Clone, PartialEq)]
pub enum ListMarkerType {
    Asterisk,
    Plus,
    Minus,
    Ordered,
}

/// Struct representing a list item
#[derive(Debug, Clone)]
pub struct ListItem {
    pub indentation: usize,
    pub marker_type: ListMarkerType,
    pub marker: String,
    pub content: String,
    pub spaces_after_marker: usize,
}

/// Utility functions for detecting and handling lists in Markdown documents
pub struct ListUtils;

impl ListUtils {
    /// Check if a line is a list item
    pub fn is_list_item(line: &str) -> bool {
        // Fast path for common cases
        if line.is_empty() {
            return false;
        }
        
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return false;
        }
        
        // Quick literal check for common list markers
        let first_char = trimmed.chars().next().unwrap();
        match first_char {
            '*' | '+' | '-' => {
                if trimmed.len() > 1 {
                    let second_char = trimmed.chars().nth(1).unwrap();
                    return second_char.is_whitespace();
                }
                return false;
            },
            '0'..='9' => {
                // Check for ordered list pattern using a literal search first
                let dot_pos = trimmed.find('.');
                if let Some(pos) = dot_pos {
                    if pos > 0 && pos < trimmed.len() - 1 {
                        let after_dot = &trimmed[pos+1..];
                        return after_dot.starts_with(' ');
                    }
                }
                return false;
            },
            _ => return false
        }
    }
    
    /// Check if a line is an unordered list item
    pub fn is_unordered_list_item(line: &str) -> bool {
        // Fast path for common cases
        if line.is_empty() {
            return false;
        }
        
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return false;
        }
        
        // Quick literal check for unordered list markers
        let first_char = trimmed.chars().next().unwrap();
        if first_char == '*' || first_char == '+' || first_char == '-' {
            if trimmed.len() > 1 {
                let second_char = trimmed.chars().nth(1).unwrap();
                return second_char.is_whitespace();
            }
        }
        
        false
    }
    
    /// Check if a line is an ordered list item
    pub fn is_ordered_list_item(line: &str) -> bool {
        // Fast path for common cases
        if line.is_empty() {
            return false;
        }
        
        let trimmed = line.trim_start();
        if trimmed.is_empty() || !trimmed.chars().next().unwrap().is_ascii_digit() {
            return false;
        }
        
        // Check for ordered list pattern using a literal search
        let dot_pos = trimmed.find('.');
        if let Some(pos) = dot_pos {
            if pos > 0 && pos < trimmed.len() - 1 {
                let after_dot = &trimmed[pos+1..];
                return after_dot.starts_with(' ');
            }
        }
        
        false
    }
    
    /// Check if a line is a list item without proper spacing after the marker
    pub fn is_list_item_without_space(line: &str) -> bool {
        UNORDERED_LIST_NO_SPACE_PATTERN.is_match(line) || ORDERED_LIST_NO_SPACE_PATTERN.is_match(line)
    }
    
    /// Check if a line is a list item with multiple spaces after the marker
    pub fn is_list_item_with_multiple_spaces(line: &str) -> bool {
        UNORDERED_LIST_MULTIPLE_SPACE_PATTERN.is_match(line) || ORDERED_LIST_MULTIPLE_SPACE_PATTERN.is_match(line)
    }
    
    /// Parse a line as a list item
    pub fn parse_list_item(line: &str) -> Option<ListItem> {
        // First try to match unordered list pattern
        if let Some(captures) = UNORDERED_LIST_PATTERN.captures(line) {
            let indentation = captures.get(1).map_or(0, |m| m.as_str().len());
            let marker = captures.get(2).unwrap().as_str();
            let spaces = captures.get(3).map_or(0, |m| m.as_str().len());
            let content_start = indentation + marker.len() + spaces;
            let content = if content_start < line.len() {
                line[content_start..].to_string()
            } else {
                String::new()
            };
            
            let marker_type = match marker {
                "*" => ListMarkerType::Asterisk,
                "+" => ListMarkerType::Plus,
                "-" => ListMarkerType::Minus,
                _ => unreachable!(), // Regex ensures this
            };
            
            return Some(ListItem {
                indentation,
                marker_type,
                marker: marker.to_string(),
                content,
                spaces_after_marker: spaces,
            });
        }
        
        // Then try to match ordered list pattern
        if let Some(captures) = ORDERED_LIST_PATTERN.captures(line) {
            let indentation = captures.get(1).map_or(0, |m| m.as_str().len());
            let marker = captures.get(2).unwrap().as_str();
            let spaces = captures.get(3).map_or(0, |m| m.as_str().len());
            let content_start = indentation + marker.len() + spaces;
            let content = if content_start < line.len() {
                line[content_start..].to_string()
            } else {
                String::new()
            };
            
            return Some(ListItem {
                indentation,
                marker_type: ListMarkerType::Ordered,
                marker: marker.to_string(),
                content,
                spaces_after_marker: spaces,
            });
        }
        
        None
    }
    
    /// Check if a line is a continuation of a list item
    pub fn is_list_continuation(line: &str, prev_list_item: &ListItem) -> bool {
        if line.trim().is_empty() {
            return false;
        }
        
        // Quick check for indentation level
        let indentation = line.chars().take_while(|c| c.is_whitespace()).count();
        
        // Continuation should be indented at least as much as the content of the previous item
        let min_indent = prev_list_item.indentation + prev_list_item.marker.len() + prev_list_item.spaces_after_marker;
        indentation >= min_indent && !Self::is_list_item(line)
    }
    
    /// Fix a list item without space after the marker
    pub fn fix_list_item_without_space(line: &str) -> String {
        if let Some(captures) = UNORDERED_LIST_NO_SPACE_PATTERN.captures(line) {
            let leading_space = captures.get(1).map_or("", |m| m.as_str());
            let marker = captures.get(2).map_or("", |m| m.as_str());
            let content = captures.get(3).map_or("", |m| m.as_str());
            
            // Insert a space after the marker
            return format!("{}{} {}", leading_space, marker, content);
        }
        
        if let Some(captures) = ORDERED_LIST_NO_SPACE_PATTERN.captures(line) {
            let leading_space = captures.get(1).map_or("", |m| m.as_str());
            let marker = captures.get(2).map_or("", |m| m.as_str());
            let content = captures.get(3).map_or("", |m| m.as_str());
            
            // Insert a space after the marker
            return format!("{}{} {}", leading_space, marker, content);
        }
        
        // Return the original line if no pattern matched
        line.to_string()
    }
    
    /// Fix a list item with multiple spaces after the marker
    pub fn fix_list_item_with_multiple_spaces(line: &str) -> String {
        if let Some(captures) = UNORDERED_LIST_MULTIPLE_SPACE_PATTERN.captures(line) {
            let leading_space = captures.get(1).map_or("", |m| m.as_str());
            let marker = captures.get(2).map_or("", |m| m.as_str());
            let spaces = captures.get(3).map_or("", |m| m.as_str());
            
            // Get content after multiple spaces
            let start_pos = leading_space.len() + marker.len() + spaces.len();
            let content = if start_pos < line.len() {
                &line[start_pos..]
            } else {
                ""
            };
            
            // Replace multiple spaces with a single space
            return format!("{}{} {}", leading_space, marker, content);
        }
        
        if let Some(captures) = ORDERED_LIST_MULTIPLE_SPACE_PATTERN.captures(line) {
            let leading_space = captures.get(1).map_or("", |m| m.as_str());
            let marker = captures.get(2).map_or("", |m| m.as_str());
            let spaces = captures.get(3).map_or("", |m| m.as_str());
            
            // Get content after multiple spaces
            let start_pos = leading_space.len() + marker.len() + spaces.len();
            let content = if start_pos < line.len() {
                &line[start_pos..]
            } else {
                ""
            };
            
            // Replace multiple spaces with a single space
            return format!("{}{} {}", leading_space, marker, content);
        }
        
        // Return the original line if no pattern matched
        line.to_string()
    }
} 