use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    // Standard list detection patterns
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
        UNORDERED_LIST_PATTERN.is_match(line) || ORDERED_LIST_PATTERN.is_match(line)
    }
    
    /// Check if a line is an unordered list item
    pub fn is_unordered_list_item(line: &str) -> bool {
        UNORDERED_LIST_PATTERN.is_match(line)
    }
    
    /// Check if a line is an ordered list item
    pub fn is_ordered_list_item(line: &str) -> bool {
        ORDERED_LIST_PATTERN.is_match(line)
    }
    
    /// Check if a line is a list item without proper spacing
    pub fn is_list_item_without_space(line: &str) -> bool {
        UNORDERED_LIST_NO_SPACE_PATTERN.is_match(line) || ORDERED_LIST_NO_SPACE_PATTERN.is_match(line)
    }
    
    /// Check if a line is a list item with multiple spaces
    pub fn is_list_item_with_multiple_spaces(line: &str) -> bool {
        UNORDERED_LIST_MULTIPLE_SPACE_PATTERN.is_match(line) || ORDERED_LIST_MULTIPLE_SPACE_PATTERN.is_match(line)
    }
    
    /// Parse a line into a ListItem struct if it's a valid list item
    pub fn parse_list_item(line: &str) -> Option<ListItem> {
        // Try to match unordered list pattern
        if let Some(caps) = UNORDERED_LIST_PATTERN.captures(line) {
            let indentation = caps[1].len();
            let marker = caps[2].to_string();
            let spaces = caps[3].len();
            let content_start = indentation + marker.len() + spaces;
            let content = if content_start < line.len() {
                line[content_start..].to_string()
            } else {
                String::new()
            };
            
            let marker_type = match marker.as_str() {
                "*" => ListMarkerType::Asterisk,
                "+" => ListMarkerType::Plus,
                "-" => ListMarkerType::Minus,
                _ => unreachable!(),
            };
            
            return Some(ListItem {
                indentation,
                marker_type,
                marker,
                content,
                spaces_after_marker: spaces,
            });
        }
        
        // Try to match ordered list pattern
        if let Some(caps) = ORDERED_LIST_PATTERN.captures(line) {
            let indentation = caps[1].len();
            let marker = caps[2].to_string();
            let spaces = caps[3].len();
            let content_start = indentation + marker.len() + spaces;
            let content = if content_start < line.len() {
                line[content_start..].to_string()
            } else {
                String::new()
            };
            
            return Some(ListItem {
                indentation,
                marker_type: ListMarkerType::Ordered,
                marker,
                content,
                spaces_after_marker: spaces,
            });
        }
        
        None
    }
    
    /// Check if a line is a list continuation (indented content belonging to a list item)
    pub fn is_list_continuation(line: &str, prev_list_item: &ListItem) -> bool {
        if line.trim().is_empty() {
            return true; // Empty lines are considered part of the list
        }
        
        let indentation = line.len() - line.trim_start().len();
        let required_indent = prev_list_item.indentation + prev_list_item.marker.len() + prev_list_item.spaces_after_marker;
        
        indentation >= required_indent && !Self::is_list_item(line)
    }
    
    /// Fix a list item without proper spacing
    pub fn fix_list_item_without_space(line: &str) -> String {
        if let Some(caps) = UNORDERED_LIST_NO_SPACE_PATTERN.captures(line) {
            let indentation = &caps[1];
            let marker = &caps[2];
            let first_char = &caps[3];
            
            let content_start_pos = indentation.len() + marker.len() + 1;
            let rest_of_content = if content_start_pos < line.len() {
                &line[content_start_pos..]
            } else {
                ""
            };
            
            format!("{}{} {}{}", indentation, marker, first_char, rest_of_content)
        } else if let Some(caps) = ORDERED_LIST_NO_SPACE_PATTERN.captures(line) {
            let indentation = &caps[1];
            let marker = &caps[2];
            let first_char = &caps[3];
            
            let content_start_pos = indentation.len() + marker.len() + 1;
            let rest_of_content = if content_start_pos < line.len() {
                &line[content_start_pos..]
            } else {
                ""
            };
            
            format!("{}{} {}{}", indentation, marker, first_char, rest_of_content)
        } else {
            line.to_string()
        }
    }
    
    /// Fix a list item with multiple spaces
    pub fn fix_list_item_with_multiple_spaces(line: &str) -> String {
        if let Some(caps) = UNORDERED_LIST_MULTIPLE_SPACE_PATTERN.captures(line) {
            let indentation = &caps[1];
            let marker = &caps[2];
            let content = line[indentation.len() + marker.len()..].trim_start();
            
            format!("{}{} {}", indentation, marker, content)
        } else if let Some(caps) = ORDERED_LIST_MULTIPLE_SPACE_PATTERN.captures(line) {
            let indentation = &caps[1];
            let marker = &caps[2];
            let content = line[indentation.len() + marker.len()..].trim_start();
            
            format!("{}{} {}", indentation, marker, content)
        } else {
            line.to_string()
        }
    }
} 