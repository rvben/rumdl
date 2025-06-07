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
        
        for (i, line) in content_lines.iter().enumerate() {
            let byte_offset = line_offsets.get(i).copied().unwrap_or(0);
            let indent = line.len() - line.trim_start().len();
            let is_blank = line.trim().is_empty();
            let in_code_block = CodeBlockUtils::is_in_code_block_or_span(code_blocks, byte_offset);
            
            lines.push(LineInfo {
                content: line.to_string(),
                byte_offset,
                indent,
                is_blank,
                in_code_block,
            });
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
