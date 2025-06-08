use log::warn;
use markdown::{mdast::Node, to_mdast, ParseOptions};
use std::panic;
use crate::utils::code_block_utils::CodeBlockUtils;

/// Pre-computed information about a line
#[derive(Debug, Clone)]
pub struct LineInfo {
    /// The actual line content (without newline)
    pub content: String,
    /// Byte offset where this line starts in the document
    pub byte_offset: usize,
    /// Number of leading spaces/tabs
    pub indent: usize,
    /// Whether the line is blank (empty or only whitespace)
    pub is_blank: bool,
    /// Whether this line is inside a code block
    pub in_code_block: bool,
    /// List item information if this line starts a list item
    pub list_item: Option<ListItemInfo>,
    /// Heading information if this line is a heading
    pub heading: Option<HeadingInfo>,
}

/// Information about a list item
#[derive(Debug, Clone)]
pub struct ListItemInfo {
    /// The marker used (*, -, +, or number with . or ))
    pub marker: String,
    /// Whether it's ordered (true) or unordered (false)
    pub is_ordered: bool,
    /// The number for ordered lists
    pub number: Option<usize>,
    /// Column where the marker starts (0-based)
    pub marker_column: usize,
    /// Column where content after marker starts
    pub content_column: usize,
}

/// Heading style type
#[derive(Debug, Clone, PartialEq)]
pub enum HeadingStyle {
    /// ATX style heading (# Heading)
    ATX,
    /// Setext style heading with = underline
    Setext1,
    /// Setext style heading with - underline
    Setext2,
}

/// Information about a heading
#[derive(Debug, Clone)]
pub struct HeadingInfo {
    /// Heading level (1-6 for ATX, 1-2 for Setext)
    pub level: u8,
    /// Style of heading
    pub style: HeadingStyle,
    /// The heading marker (# characters or underline)
    pub marker: String,
    /// Column where the marker starts (0-based)
    pub marker_column: usize,
    /// Column where heading text starts
    pub content_column: usize,
    /// The heading text (without markers)
    pub text: String,
    /// Whether it has a closing sequence (for ATX)
    pub has_closing_sequence: bool,
    /// The closing sequence if present
    pub closing_sequence: String,
}

pub struct LintContext<'a> {
    pub content: &'a str,
    pub ast: Node, // The root of the AST
    pub line_offsets: Vec<usize>,
    pub code_blocks: Vec<(usize, usize)>, // Cached code block and code span ranges
    pub lines: Vec<LineInfo>, // Pre-computed line information
}

impl<'a> LintContext<'a> {
    pub fn new(content: &'a str) -> Self {
        // Check for problematic patterns that cause the markdown crate to panic
        if content_has_problematic_lists(content) {
            warn!("Detected problematic list patterns in LintContext, skipping AST parsing");
            let ast = Node::Root(markdown::mdast::Root {
                children: vec![],
                position: None,
            });

            let mut line_offsets = vec![0];
            for (i, c) in content.char_indices() {
                if c == '\n' {
                    line_offsets.push(i + 1);
                }
            }
            
            // Detect code blocks once and cache them
            let code_blocks = CodeBlockUtils::detect_code_blocks(content);
            
            // Pre-compute line information
            let lines = Self::compute_line_info(content, &line_offsets, &code_blocks);
            
            return Self {
                content,
                ast,
                line_offsets,
                code_blocks,
                lines,
            };
        }

        // Try to parse AST, but handle panics from the markdown crate
        let ast = match panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            to_mdast(content, &ParseOptions::gfm())
        })) {
            Ok(Ok(ast)) => {
                // Successfully parsed AST
                ast
            }
            Ok(Err(err)) => {
                // Parsing failed with an error
                warn!("Failed to parse markdown AST: {:?}", err);
                Node::Root(markdown::mdast::Root {
                    children: vec![],
                    position: None,
                })
            }
            Err(_) => {
                // Parsing panicked
                warn!("Markdown AST parsing panicked, falling back to empty AST");
                Node::Root(markdown::mdast::Root {
                    children: vec![],
                    position: None,
                })
            }
        };

        let mut line_offsets = vec![0];
        for (i, c) in content.char_indices() {
            if c == '\n' {
                line_offsets.push(i + 1);
            }
        }
        
        // Detect code blocks once and cache them
        let code_blocks = CodeBlockUtils::detect_code_blocks(content);
        
        // Pre-compute line information
        let lines = Self::compute_line_info(content, &line_offsets, &code_blocks);
        
        Self {
            content,
            ast,
            line_offsets,
            code_blocks,
            lines,
        }
    }

    /// Map a byte offset to (line, column)
    pub fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        match self.line_offsets.binary_search(&offset) {
            Ok(line) => (line + 1, 1),
            Err(line) => {
                let line_start = self
                    .line_offsets
                    .get(line.wrapping_sub(1))
                    .copied()
                    .unwrap_or(0);
                (line, offset - line_start + 1)
            }
        }
    }
    
    /// Check if a position is within a code block or code span
    pub fn is_in_code_block_or_span(&self, pos: usize) -> bool {
        CodeBlockUtils::is_in_code_block_or_span(&self.code_blocks, pos)
    }
    
    /// Get line information by line number (1-indexed)
    pub fn line_info(&self, line_num: usize) -> Option<&LineInfo> {
        if line_num > 0 {
            self.lines.get(line_num - 1)
        } else {
            None
        }
    }
    
    /// Get byte offset for a line number (1-indexed)
    pub fn line_to_byte_offset(&self, line_num: usize) -> Option<usize> {
        self.line_info(line_num).map(|info| info.byte_offset)
    }
    
    /// Pre-compute line information
    fn compute_line_info(content: &str, line_offsets: &[usize], code_blocks: &[(usize, usize)]) -> Vec<LineInfo> {
        let mut lines = Vec::new();
        let content_lines: Vec<&str> = content.lines().collect();
        
        // Regex for list detection - allow any whitespace including no space (to catch malformed lists)
        let unordered_regex = regex::Regex::new(r"^(\s*)([-*+])([ \t]*)(.*)").unwrap();
        let ordered_regex = regex::Regex::new(r"^(\s*)(\d+)([.)])([ \t]*)(.*)").unwrap();
        
        // Regex for heading detection
        let atx_heading_regex = regex::Regex::new(r"^(\s*)(#{1,6})(\s*)(.*)$").unwrap();
        let setext_underline_regex = regex::Regex::new(r"^(\s*)(=+|-+)\s*$").unwrap();
        
        for (i, line) in content_lines.iter().enumerate() {
            let byte_offset = line_offsets.get(i).copied().unwrap_or(0);
            let indent = line.len() - line.trim_start().len();
            let is_blank = line.trim().is_empty();
            // Check if this line is inside a code block (not inline code span)
            // We only want to check for fenced/indented code blocks, not inline code
            let in_code_block = code_blocks.iter().any(|&(start, end)| {
                // Only consider ranges that span multiple lines (code blocks)
                // Inline code spans are typically on a single line
                let block_content = &content[start..end];
                let is_multiline = block_content.contains('\n');
                let is_fenced = block_content.starts_with("```") || block_content.starts_with("~~~");
                let is_indented = !is_fenced && block_content.lines().all(|l| l.starts_with("    ") || l.starts_with("\t") || l.trim().is_empty());
                
                byte_offset >= start && byte_offset < end && (is_multiline || is_fenced || is_indented)
            });
            
            // Detect list items
            let list_item = if !in_code_block && !is_blank {
                if let Some(caps) = unordered_regex.captures(line) {
                    let leading_spaces = caps.get(1).map_or("", |m| m.as_str());
                    let marker = caps.get(2).map_or("", |m| m.as_str());
                    let spacing = caps.get(3).map_or("", |m| m.as_str());
                    let content = caps.get(4).map_or("", |m| m.as_str());
                    let marker_column = leading_spaces.len();
                    let content_column = marker_column + marker.len() + spacing.len();
                    
                    // Check if this is likely emphasis or not a list item
                    if spacing.is_empty() {
                        // No space after marker - check if it's likely emphasis or just text
                        if marker == "*" && content.ends_with('*') && !content[..content.len()-1].contains('*') {
                            // Likely emphasis like *text*
                            None
                        } else if marker == "*" && content.starts_with('*') {
                            // Likely bold emphasis like **text** or horizontal rule like ***
                            None
                        } else if (marker == "*" || marker == "-") && content.chars().all(|c| c == marker.chars().next().unwrap()) && content.len() >= 2 {
                            // Likely horizontal rule like *** or ---
                            None
                        } else if content.len() > 0 && content.chars().next().unwrap().is_alphabetic() {
                            // Single word starting with marker, likely not intended as list
                            // This matches markdownlint behavior
                            None
                        } else {
                            // Other cases with no space - treat as malformed list item
                            Some(ListItemInfo {
                                marker: marker.to_string(),
                                is_ordered: false,
                                number: None,
                                marker_column,
                                content_column,
                            })
                        }
                    } else {
                        Some(ListItemInfo {
                            marker: marker.to_string(),
                            is_ordered: false,
                            number: None,
                            marker_column,
                            content_column,
                        })
                    }
                } else if let Some(caps) = ordered_regex.captures(line) {
                    let leading_spaces = caps.get(1).map_or("", |m| m.as_str());
                    let number_str = caps.get(2).map_or("", |m| m.as_str());
                    let delimiter = caps.get(3).map_or("", |m| m.as_str());
                    let spacing = caps.get(4).map_or("", |m| m.as_str());
                    let content = caps.get(5).map_or("", |m| m.as_str());
                    let marker = format!("{}{}", number_str, delimiter);
                    let marker_column = leading_spaces.len();
                    let content_column = marker_column + marker.len() + spacing.len();
                    
                    // Check if this is likely not a list item
                    if spacing.is_empty() && content.len() > 0 && content.chars().next().unwrap().is_alphabetic() {
                        // No space after marker and starts with alphabetic, likely not intended as list
                        // This matches markdownlint behavior
                        None
                    } else {
                        Some(ListItemInfo {
                            marker,
                            is_ordered: true,
                            number: number_str.parse().ok(),
                            marker_column,
                            content_column,
                        })
                    }
                } else {
                    None
                }
            } else {
                None
            };
            
            lines.push(LineInfo {
                content: line.to_string(),
                byte_offset,
                indent,
                is_blank,
                in_code_block,
                list_item,
                heading: None, // Will be populated in second pass for Setext headings
            });
        }
        
        // Detect front matter boundaries
        let mut in_front_matter = false;
        let mut front_matter_end = 0;
        if content_lines.first().map(|l| l.trim()) == Some("---") {
            in_front_matter = true;
            for (idx, line) in content_lines.iter().enumerate().skip(1) {
                if line.trim() == "---" {
                    front_matter_end = idx;
                    break;
                }
            }
        }
        
        // Second pass: detect headings (including Setext which needs look-ahead)
        for i in 0..content_lines.len() {
            if lines[i].in_code_block || lines[i].is_blank {
                continue;
            }
            
            // Skip lines in front matter
            if in_front_matter && i <= front_matter_end {
                continue;
            }
            
            let line = content_lines[i];
            
            // Check for ATX headings
            if let Some(caps) = atx_heading_regex.captures(line) {
                let leading_spaces = caps.get(1).map_or("", |m| m.as_str());
                let hashes = caps.get(2).map_or("", |m| m.as_str());
                let spaces_after = caps.get(3).map_or("", |m| m.as_str());
                let rest = caps.get(4).map_or("", |m| m.as_str());
                
                let level = hashes.len() as u8;
                let marker_column = leading_spaces.len();
                
                // Check for closing sequence
                let (text, has_closing, closing_seq) = {
                    // Find the start of a potential closing sequence
                    let trimmed_rest = rest.trim_end();
                    if let Some(last_hash_pos) = trimmed_rest.rfind('#') {
                        // Look for the start of the hash sequence
                        let mut start_of_hashes = last_hash_pos;
                        while start_of_hashes > 0 && trimmed_rest.chars().nth(start_of_hashes - 1) == Some('#') {
                            start_of_hashes -= 1;
                        }
                        
                        // Check if this is a valid closing sequence (all hashes to end of line)
                        let potential_closing = &trimmed_rest[start_of_hashes..];
                        let is_all_hashes = potential_closing.chars().all(|c| c == '#');
                        
                        if is_all_hashes {
                            // This is a closing sequence, regardless of spacing
                            let closing_hashes = potential_closing.to_string();
                            let text_part = rest[..start_of_hashes].trim_end();
                            (text_part.to_string(), true, closing_hashes)
                        } else {
                            (rest.to_string(), false, String::new())
                        }
                    } else {
                        (rest.to_string(), false, String::new())
                    }
                };
                
                let content_column = marker_column + hashes.len() + spaces_after.len();
                
                lines[i].heading = Some(HeadingInfo {
                    level,
                    style: HeadingStyle::ATX,
                    marker: hashes.to_string(),
                    marker_column,
                    content_column,
                    text: text.trim().to_string(),
                    has_closing_sequence: has_closing,
                    closing_sequence: closing_seq,
                });
            }
            // Check for Setext headings (need to look at next line)
            else if i + 1 < content_lines.len() {
                let next_line = content_lines[i + 1];
                if !lines[i + 1].in_code_block && setext_underline_regex.is_match(next_line) {
                    // Skip if next line is front matter delimiter
                    if in_front_matter && i + 1 <= front_matter_end {
                        continue;
                    }
                    
                    let underline = next_line.trim();
                    let level = if underline.starts_with('=') { 1 } else { 2 };
                    let style = if level == 1 { HeadingStyle::Setext1 } else { HeadingStyle::Setext2 };
                    
                    lines[i].heading = Some(HeadingInfo {
                        level,
                        style,
                        marker: underline.to_string(),
                        marker_column: next_line.len() - next_line.trim_start().len(),
                        content_column: lines[i].indent,
                        text: line.trim().to_string(),
                        has_closing_sequence: false,
                        closing_sequence: String::new(),
                    });
                }
            }
        }
        
        lines
    }
}

/// Check if content contains patterns that cause the markdown crate to panic
fn content_has_problematic_lists(content: &str) -> bool {
    let lines: Vec<&str> = content.lines().collect();

    // Look for mixed list markers in consecutive lines (which causes the panic)
    for window in lines.windows(3) {
        if window.len() >= 2 {
            let line1 = window[0].trim_start();
            let line2 = window[1].trim_start();

            // Check if both lines are list items with different markers
            let is_list1 =
                line1.starts_with("* ") || line1.starts_with("+ ") || line1.starts_with("- ");
            let is_list2 =
                line2.starts_with("* ") || line2.starts_with("+ ") || line2.starts_with("- ");

            if is_list1 && is_list2 {
                let marker1 = line1.chars().next().unwrap_or(' ');
                let marker2 = line2.chars().next().unwrap_or(' ');

                // If different markers, this could cause a panic
                if marker1 != marker2 {
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown::mdast::{Heading, Node};

    #[test]
    fn test_empty_content() {
        let ctx = LintContext::new("");
        assert_eq!(ctx.content, "");
        // Should be a Root node with no children
        match &ctx.ast {
            Node::Root(root) => assert!(root.children.is_empty()),
            _ => panic!("AST root is not Root node"),
        }
        assert_eq!(ctx.line_offsets, vec![0]);
        assert_eq!(ctx.offset_to_line_col(0), (1, 1));
        assert_eq!(ctx.lines.len(), 0);
    }

    #[test]
    fn test_single_line() {
        let ctx = LintContext::new("# Hello");
        assert_eq!(ctx.content, "# Hello");
        // Should parse a heading
        match &ctx.ast {
            Node::Root(root) => {
                assert_eq!(root.children.len(), 1);
                match &root.children[0] {
                    Node::Heading(Heading { depth, .. }) => assert_eq!(*depth, 1),
                    _ => panic!("First child is not a Heading"),
                }
            }
            _ => panic!("AST root is not Root node"),
        }
        assert_eq!(ctx.line_offsets, vec![0]);
        assert_eq!(ctx.offset_to_line_col(0), (1, 1));
        assert_eq!(ctx.offset_to_line_col(3), (1, 4));
    }

    #[test]
    fn test_multi_line() {
        let content = "# Title\n\nSecond line\nThird line";
        let ctx = LintContext::new(content);
        assert_eq!(ctx.line_offsets, vec![0, 8, 9, 21]);
        // Test offset to line/col
        assert_eq!(ctx.offset_to_line_col(0), (1, 1)); // start
        assert_eq!(ctx.offset_to_line_col(8), (2, 1)); // start of blank line
        assert_eq!(ctx.offset_to_line_col(9), (3, 1)); // start of 'Second line'
        assert_eq!(ctx.offset_to_line_col(15), (3, 7)); // middle of 'Second line'
        assert_eq!(ctx.offset_to_line_col(21), (4, 1)); // start of 'Third line'
    }

    #[test]
    fn test_line_info() {
        let content = "# Title\n    indented\n\ncode:\n```rust\nfn main() {}\n```";
        let ctx = LintContext::new(content);
        
        // Test line info
        assert_eq!(ctx.lines.len(), 7);
        
        // Line 1: "# Title"
        let line1 = &ctx.lines[0];
        assert_eq!(line1.content, "# Title");
        assert_eq!(line1.byte_offset, 0);
        assert_eq!(line1.indent, 0);
        assert!(!line1.is_blank);
        assert!(!line1.in_code_block);
        assert!(line1.list_item.is_none());
        
        // Line 2: "    indented"
        let line2 = &ctx.lines[1];
        assert_eq!(line2.content, "    indented");
        assert_eq!(line2.byte_offset, 8);
        assert_eq!(line2.indent, 4);
        assert!(!line2.is_blank);
        
        // Line 3: "" (blank)
        let line3 = &ctx.lines[2];
        assert_eq!(line3.content, "");
        assert!(line3.is_blank);
        
        // Test helper methods
        assert_eq!(ctx.line_to_byte_offset(1), Some(0));
        assert_eq!(ctx.line_to_byte_offset(2), Some(8));
        assert_eq!(ctx.line_info(1).map(|l| l.indent), Some(0));
        assert_eq!(ctx.line_info(2).map(|l| l.indent), Some(4));
    }

    #[test]
    fn test_list_item_detection() {
        let content = "- Unordered item\n  * Nested item\n1. Ordered item\n   2) Nested ordered\n\nNot a list";
        let ctx = LintContext::new(content);
        
        // Line 1: "- Unordered item"
        let line1 = &ctx.lines[0];
        assert!(line1.list_item.is_some());
        let list1 = line1.list_item.as_ref().unwrap();
        assert_eq!(list1.marker, "-");
        assert!(!list1.is_ordered);
        assert_eq!(list1.marker_column, 0);
        assert_eq!(list1.content_column, 2);
        
        // Line 2: "  * Nested item"
        let line2 = &ctx.lines[1];
        assert!(line2.list_item.is_some());
        let list2 = line2.list_item.as_ref().unwrap();
        assert_eq!(list2.marker, "*");
        assert_eq!(list2.marker_column, 2);
        
        // Line 3: "1. Ordered item"
        let line3 = &ctx.lines[2];
        assert!(line3.list_item.is_some());
        let list3 = line3.list_item.as_ref().unwrap();
        assert_eq!(list3.marker, "1.");
        assert!(list3.is_ordered);
        assert_eq!(list3.number, Some(1));
        
        // Line 6: "Not a list"
        let line6 = &ctx.lines[5];
        assert!(line6.list_item.is_none());
    }

    #[test]
    fn test_offset_to_line_col_edge_cases() {
        let content = "a\nb\nc";
        let ctx = LintContext::new(content);
        // line_offsets: [0, 2, 4]
        assert_eq!(ctx.offset_to_line_col(0), (1, 1)); // 'a'
        assert_eq!(ctx.offset_to_line_col(1), (1, 2)); // after 'a'
        assert_eq!(ctx.offset_to_line_col(2), (2, 1)); // 'b'
        assert_eq!(ctx.offset_to_line_col(3), (2, 2)); // after 'b'
        assert_eq!(ctx.offset_to_line_col(4), (3, 1)); // 'c'
        assert_eq!(ctx.offset_to_line_col(5), (3, 2)); // after 'c'
    }
}
