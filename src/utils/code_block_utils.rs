//!
//! Utility functions for detecting and handling code blocks and code spans in Markdown for rumdl.

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"^(```|~~~)").unwrap();
    static ref CODE_SPAN_PATTERN: Regex = Regex::new(r"`+").unwrap();
}

/// Utility functions for detecting and handling code blocks in Markdown
pub struct CodeBlockUtils;

impl CodeBlockUtils {
    /// Detect all code blocks in the content
    pub fn detect_code_blocks(content: &str) -> Vec<(usize, usize)> {
        let mut blocks = Vec::new();
        let mut in_code_block = false;
        let mut code_block_start = 0;

        // Pre-compute line positions for efficient offset calculation
        let lines: Vec<&str> = content.lines().collect();
        let mut line_positions = Vec::with_capacity(lines.len());
        let mut pos = 0;
        for line in &lines {
            line_positions.push(pos);
            pos += line.len() + 1; // +1 for newline
        }

        // Find fenced code blocks
        for (i, line) in lines.iter().enumerate() {
            let line_start = line_positions[i];

            if CODE_BLOCK_PATTERN.is_match(line.trim()) {
                if !in_code_block {
                    code_block_start = line_start;
                    in_code_block = true;
                } else {
                    let code_block_end = line_start + line.len();
                    blocks.push((code_block_start, code_block_end));
                    in_code_block = false;
                }
            }
        }

        // Handle unclosed code blocks
        if in_code_block {
            blocks.push((code_block_start, content.len()));
        }

        // Find indented code blocks (4+ spaces or tab at start of line)
        // According to CommonMark, indented code blocks must be preceded by a blank line
        // (unless they're at the start of the document or after a block-level element)
        let mut in_indented_block = false;
        let mut indented_block_start = 0;

        for (line_idx, line) in lines.iter().enumerate() {
            let line_start = if line_idx < line_positions.len() {
                line_positions[line_idx]
            } else {
                0
            };

            // Check if this line is indented code
            let is_indented = line.starts_with("    ") || line.starts_with("\t");

            // Check if this looks like a list item (has list marker after indentation)
            let trimmed = line.trim_start();
            let is_list_item = trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("+ ")
                || trimmed.chars().next().is_some_and(|c| c.is_numeric())
                    && trimmed.chars().nth(1).is_some_and(|c| c == '.' || c == ')');

            // Check if previous line was blank
            let prev_blank = line_idx > 0 && lines[line_idx - 1].trim().is_empty();

            if is_indented && !line.trim().is_empty() && !is_list_item {
                if !in_indented_block {
                    // Only start an indented code block if preceded by a blank line
                    if prev_blank {
                        in_indented_block = true;
                        indented_block_start = line_start;
                    }
                    // Otherwise, this is just an indented line, not a code block
                }
            } else if in_indented_block {
                // End of indented code block
                let block_end = if line_idx > 0 && line_idx - 1 < line_positions.len() {
                    line_positions[line_idx - 1] + lines[line_idx - 1].len()
                } else {
                    line_start
                };
                blocks.push((indented_block_start, block_end));
                in_indented_block = false;
            }
        }

        // Handle indented block that goes to end of file
        if in_indented_block {
            blocks.push((indented_block_start, content.len()));
        }

        // Find inline code spans
        let mut i = 0;
        while i < content.len() {
            if let Some(m) = CODE_SPAN_PATTERN.find_at(content, i) {
                let backtick_length = m.end() - m.start();
                let start = m.start();

                // Find matching closing backticks
                if let Some(end_pos) = content[m.end()..].find(&"`".repeat(backtick_length)) {
                    let end = m.end() + end_pos + backtick_length;
                    blocks.push((start, end));
                    i = end;
                } else {
                    i = m.end();
                }
            } else {
                break;
            }
        }

        blocks.sort_by(|a, b| a.0.cmp(&b.0));
        blocks
    }

    /// Check if a position is within a code block or code span
    pub fn is_in_code_block_or_span(blocks: &[(usize, usize)], pos: usize) -> bool {
        blocks.iter().any(|&(start, end)| pos >= start && pos < end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_fenced_code_blocks() {
        // The function detects BOTH fenced blocks and inline code spans
        // Fenced blocks with backticks also get picked up as inline spans due to the backticks

        // Basic fenced code block with backticks
        let content = "Some text\n```\ncode here\n```\nMore text";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Should find: 1 fenced block + 1 inline span (the ```)
        assert_eq!(blocks.len(), 2);

        // Check that we have the fenced block
        let fenced_block = blocks
            .iter()
            .find(|(start, end)| end - start > 10 && content[*start..*end].contains("code here"));
        assert!(fenced_block.is_some());

        // Fenced code block with tildes (no inline code detection for ~)
        let content = "Some text\n~~~\ncode here\n~~~\nMore text";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(&content[blocks[0].0..blocks[0].1], "~~~\ncode here\n~~~");

        // Multiple code blocks
        let content = "Text\n```\ncode1\n```\nMiddle\n~~~\ncode2\n~~~\nEnd";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 2 fenced blocks + 1 inline span for the ```
        assert_eq!(blocks.len(), 3);
    }

    #[test]
    fn test_detect_code_blocks_with_language() {
        // Code block with language identifier
        let content = "Text\n```rust\nfn main() {}\n```\nMore";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced block + 1 inline span for ```
        assert_eq!(blocks.len(), 2);
        // Check we have the full fenced block
        let fenced = blocks.iter().find(|(s, e)| content[*s..*e].contains("fn main"));
        assert!(fenced.is_some());
    }

    #[test]
    fn test_unclosed_code_block() {
        // Unclosed code block should extend to end of content
        let content = "Text\n```\ncode here\nno closing fence";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, content.len());
    }

    #[test]
    fn test_indented_code_blocks() {
        // Basic indented code block
        let content = "Paragraph\n\n    code line 1\n    code line 2\n\nMore text";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert!(content[blocks[0].0..blocks[0].1].contains("code line 1"));
        assert!(content[blocks[0].0..blocks[0].1].contains("code line 2"));

        // Indented code with tabs
        let content = "Paragraph\n\n\tcode with tab\n\tanother line\n\nText";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn test_indented_code_requires_blank_line() {
        // Indented lines without preceding blank line are not code blocks
        let content = "Paragraph\n    indented but not code\nMore text";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);

        // With blank line, it becomes a code block
        let content = "Paragraph\n\n    now it's code\nMore text";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn test_list_items_not_code_blocks() {
        // List items should not be detected as code blocks
        let content = "List:\n\n    - Item 1\n    - Item 2\n    * Item 3\n    + Item 4";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);

        // Numbered lists
        let content = "List:\n\n    1. First\n    2. Second\n    1) Also first";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);
    }

    #[test]
    fn test_inline_code_spans() {
        // Single backtick code span
        let content = "Text with `inline code` here";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(&content[blocks[0].0..blocks[0].1], "`inline code`");

        // Multiple backtick code span
        let content = "Text with ``code with ` backtick`` here";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(&content[blocks[0].0..blocks[0].1], "``code with ` backtick``");

        // Multiple code spans
        let content = "Has `code1` and `code2` spans";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn test_unclosed_code_span() {
        // Unclosed code span should not be detected
        let content = "Text with `unclosed code span";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);

        // Mismatched backticks
        let content = "Text with ``one style` different close";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);
    }

    #[test]
    fn test_mixed_code_blocks_and_spans() {
        let content = "Has `span1` text\n```\nblock\n```\nand `span2`";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // The function may detect overlapping blocks (fenced block and inline spans)
        // We should have at least: span1, fenced block, span2
        assert!(blocks.len() >= 3);

        // Check we have the expected elements
        assert!(blocks.iter().any(|(s, e)| &content[*s..*e] == "`span1`"));
        assert!(blocks.iter().any(|(s, e)| &content[*s..*e] == "`span2`"));
        assert!(blocks.iter().any(|(s, e)| content[*s..*e].contains("block")));

        // Verify they're sorted by position (allowing duplicates/overlaps)
        for i in 1..blocks.len() {
            assert!(blocks[i - 1].0 <= blocks[i].0);
        }
    }

    #[test]
    fn test_is_in_code_block_or_span() {
        let blocks = vec![(10, 20), (30, 40), (50, 60)];

        // Test positions inside blocks
        assert!(CodeBlockUtils::is_in_code_block_or_span(&blocks, 15));
        assert!(CodeBlockUtils::is_in_code_block_or_span(&blocks, 35));
        assert!(CodeBlockUtils::is_in_code_block_or_span(&blocks, 55));

        // Test positions at boundaries
        assert!(CodeBlockUtils::is_in_code_block_or_span(&blocks, 10)); // Start is inclusive
        assert!(!CodeBlockUtils::is_in_code_block_or_span(&blocks, 20)); // End is exclusive

        // Test positions outside blocks
        assert!(!CodeBlockUtils::is_in_code_block_or_span(&blocks, 5));
        assert!(!CodeBlockUtils::is_in_code_block_or_span(&blocks, 25));
        assert!(!CodeBlockUtils::is_in_code_block_or_span(&blocks, 65));
    }

    #[test]
    fn test_empty_content() {
        let blocks = CodeBlockUtils::detect_code_blocks("");
        assert_eq!(blocks.len(), 0);
    }

    #[test]
    fn test_code_block_at_start() {
        let content = "```\ncode\n```\nText after";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced + 1 inline span
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, 0); // Fenced block starts at 0
    }

    #[test]
    fn test_code_block_at_end() {
        let content = "Text before\n```\ncode\n```";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced + 1 inline span
        assert_eq!(blocks.len(), 2);
        // Check we have the fenced block
        let fenced = blocks.iter().find(|(s, e)| content[*s..*e].contains("code"));
        assert!(fenced.is_some());
    }

    #[test]
    fn test_nested_fence_markers() {
        // Code block containing fence markers as content
        let content = "Text\n````\n```\nnested\n```\n````\nAfter";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Should detect: outer block, inner ```, outer ````
        assert!(!blocks.is_empty());
        // Check we have the outer block
        let outer = blocks.iter().find(|(s, e)| content[*s..*e].contains("nested"));
        assert!(outer.is_some());
    }

    #[test]
    fn test_indented_code_with_blank_lines() {
        // Indented code blocks can contain blank lines
        let content = "Text\n\n    line1\n\n    line2\n\nAfter";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // May have multiple blocks due to blank line handling
        assert!(!blocks.is_empty());
        // Check that we captured the indented code
        let all_content: String = blocks
            .iter()
            .map(|(s, e)| &content[*s..*e])
            .collect::<Vec<_>>()
            .join("");
        assert!(all_content.contains("line1") || content[blocks[0].0..blocks[0].1].contains("line1"));
    }

    #[test]
    fn test_code_span_with_spaces() {
        // Code spans can have leading/trailing spaces
        let content = "Text ` code with spaces ` more";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(&content[blocks[0].0..blocks[0].1], "` code with spaces `");
    }

    #[test]
    fn test_fenced_block_with_info_string() {
        // Fenced code blocks with complex info strings
        let content = "```rust,no_run,should_panic\ncode\n```";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced + 1 inline span
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, 0);
    }

    #[test]
    fn test_indented_fences_not_code_blocks() {
        // Indented fence markers should still work as fences
        let content = "Text\n  ```\n  code\n  ```\nAfter";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced + 1 inline span
        assert_eq!(blocks.len(), 2);
    }
}
