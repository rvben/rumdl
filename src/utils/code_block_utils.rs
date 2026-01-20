//!
//! Utility functions for detecting and handling code blocks and code spans in Markdown for rumdl.
//!
//! Code block detection is delegated to pulldown-cmark, which correctly implements the
//! CommonMark specification. This handles edge cases like:
//! - Backtick fences with backticks in the info string (invalid per spec)
//! - Nested fences (longer fence contains shorter fence as content)
//! - Mixed fence types (tilde fence contains backticks as content)
//! - Indented code blocks with proper list context handling

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Type alias for code block and span ranges: (code_blocks, code_spans)
pub type CodeRanges = (Vec<(usize, usize)>, Vec<(usize, usize)>);

/// Classification of code blocks relative to list contexts
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeBlockContext {
    /// Code block that separates lists (root-level, with blank lines)
    Standalone,
    /// Code block that continues a list (properly indented)
    Indented,
    /// Code block adjacent to list content (edge case, defaults to non-breaking)
    Adjacent,
}

/// Utility functions for detecting and handling code blocks in Markdown
pub struct CodeBlockUtils;

impl CodeBlockUtils {
    /// Detect all code blocks in the content (NOT including inline code spans)
    ///
    /// Uses pulldown-cmark for spec-compliant CommonMark parsing. This correctly handles:
    /// - Fenced code blocks (``` and ~~~)
    /// - Indented code blocks (4 spaces or tab)
    /// - Code blocks inside lists, blockquotes, and other containers
    /// - Edge cases like backticks in info strings (which invalidate the fence)
    ///
    /// Returns a sorted vector of (start, end) byte offset tuples.
    pub fn detect_code_blocks(content: &str) -> Vec<(usize, usize)> {
        let (blocks, _) = Self::detect_code_blocks_and_spans(content);
        blocks
    }

    /// Returns code block ranges and inline code span ranges in a single pulldown-cmark pass.
    pub fn detect_code_blocks_and_spans(content: &str) -> CodeRanges {
        let mut blocks = Vec::new();
        let mut spans = Vec::new();
        let mut code_block_start: Option<usize> = None;

        // Use pulldown-cmark with all extensions for maximum compatibility
        let options = Options::all();
        let parser = Parser::new_ext(content, options).into_offset_iter();

        for (event, range) in parser {
            match event {
                Event::Start(Tag::CodeBlock(_)) => {
                    // Record start position of code block
                    code_block_start = Some(range.start);
                }
                Event::End(TagEnd::CodeBlock) => {
                    // Complete the code block range
                    if let Some(start) = code_block_start.take() {
                        blocks.push((start, range.end));
                    }
                }
                Event::Code(_) => {
                    spans.push((range.start, range.end));
                }
                _ => {}
            }
        }

        // Handle edge case: unclosed code block at end of content
        // pulldown-cmark should handle this, but be defensive
        if let Some(start) = code_block_start {
            blocks.push((start, content.len()));
        }

        // Sort by start position (should already be sorted, but ensure consistency)
        blocks.sort_by_key(|&(start, _)| start);
        (blocks, spans)
    }

    /// Check if a position is within a code block (for compatibility)
    pub fn is_in_code_block_or_span(blocks: &[(usize, usize)], pos: usize) -> bool {
        // This is a compatibility function - it only checks code blocks now, not spans
        blocks.iter().any(|&(start, end)| pos >= start && pos < end)
    }

    /// Check if a position is within a code block (NOT including inline code spans)
    pub fn is_in_code_block(blocks: &[(usize, usize)], pos: usize) -> bool {
        blocks.iter().any(|&(start, end)| pos >= start && pos < end)
    }

    /// Analyze code block context relative to list parsing
    /// This is the core function implementing Design #3's three-tier classification
    pub fn analyze_code_block_context(
        lines: &[crate::lint_context::LineInfo],
        line_idx: usize,
        min_continuation_indent: usize,
    ) -> CodeBlockContext {
        if let Some(line_info) = lines.get(line_idx) {
            // Rule 1: Indentation Analysis - Is it sufficiently indented for list continuation?
            if line_info.indent >= min_continuation_indent {
                return CodeBlockContext::Indented;
            }

            // Rule 2: Blank Line Context - Check for structural separation indicators
            let (prev_blanks, next_blanks) = Self::count_surrounding_blank_lines(lines, line_idx);

            // Rule 3: Standalone Detection - Insufficient indentation + blank line separation
            // This is the key fix: root-level code blocks with blank lines separate lists
            if prev_blanks > 0 || next_blanks > 0 {
                return CodeBlockContext::Standalone;
            }

            // Rule 4: Default - Adjacent (conservative, non-breaking for edge cases)
            CodeBlockContext::Adjacent
        } else {
            // Fallback for invalid line index
            CodeBlockContext::Adjacent
        }
    }

    /// Count blank lines before and after the given line index
    fn count_surrounding_blank_lines(lines: &[crate::lint_context::LineInfo], line_idx: usize) -> (usize, usize) {
        let mut prev_blanks = 0;
        let mut next_blanks = 0;

        // Count blank lines before (look backwards)
        for i in (0..line_idx).rev() {
            if let Some(line) = lines.get(i) {
                if line.is_blank {
                    prev_blanks += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Count blank lines after (look forwards)
        for i in (line_idx + 1)..lines.len() {
            if let Some(line) = lines.get(i) {
                if line.is_blank {
                    next_blanks += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        (prev_blanks, next_blanks)
    }

    /// Calculate minimum indentation required for code block to continue a list
    /// Based on the most recent list item's marker width
    pub fn calculate_min_continuation_indent(
        content: &str,
        lines: &[crate::lint_context::LineInfo],
        current_line_idx: usize,
    ) -> usize {
        // Look backwards to find the most recent list item
        for i in (0..current_line_idx).rev() {
            if let Some(line_info) = lines.get(i) {
                if let Some(list_item) = &line_info.list_item {
                    // Calculate minimum continuation indent for this list item
                    return if list_item.is_ordered {
                        list_item.marker_column + list_item.marker.len() + 1 // +1 for space after marker
                    } else {
                        list_item.marker_column + 2 // Unordered lists need marker + space (min 2)
                    };
                }

                // Stop at structural separators that would break list context
                if line_info.heading.is_some() || Self::is_structural_separator(line_info.content(content)) {
                    break;
                }
            }
        }

        0 // No list context found
    }

    /// Check if content is a structural separator (headings, horizontal rules, etc.)
    fn is_structural_separator(content: &str) -> bool {
        let trimmed = content.trim();
        trimmed.starts_with("---")
            || trimmed.starts_with("***")
            || trimmed.starts_with("___")
            || crate::utils::skip_context::is_table_line(trimmed)
            || trimmed.starts_with(">") // Blockquotes
    }

    /// Detect fenced code blocks with markdown/md language tag.
    ///
    /// Returns a vector of `MarkdownCodeBlock` containing byte ranges for the
    /// content between the fences (excluding the fence lines themselves).
    ///
    /// Only detects fenced code blocks (``` or ~~~), not indented code blocks,
    /// since indented blocks don't have a language tag.
    pub fn detect_markdown_code_blocks(content: &str) -> Vec<MarkdownCodeBlock> {
        use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

        let mut blocks = Vec::new();
        let mut current_block: Option<MarkdownCodeBlockBuilder> = None;

        let options = Options::all();
        let parser = Parser::new_ext(content, options).into_offset_iter();

        for (event, range) in parser {
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                    // Check if language is markdown or md (first word of info string)
                    let language = info.split_whitespace().next().unwrap_or("");
                    if language.eq_ignore_ascii_case("markdown") || language.eq_ignore_ascii_case("md") {
                        // Find where content starts (after the opening fence line)
                        let block_start = range.start;
                        let content_start = content[block_start..]
                            .find('\n')
                            .map(|i| block_start + i + 1)
                            .unwrap_or(content.len());

                        current_block = Some(MarkdownCodeBlockBuilder { content_start });
                    }
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(builder) = current_block.take() {
                        // Find where content ends (before the closing fence line)
                        let block_end = range.end;

                        // Validate range before slicing
                        if builder.content_start > block_end || builder.content_start > content.len() {
                            continue;
                        }

                        let search_range = &content[builder.content_start..block_end.min(content.len())];
                        let content_end = search_range
                            .rfind('\n')
                            .map(|i| builder.content_start + i)
                            .unwrap_or(builder.content_start);

                        // Only add block if it has valid content range
                        if content_end >= builder.content_start {
                            blocks.push(MarkdownCodeBlock {
                                content_start: builder.content_start,
                                content_end,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        blocks
    }
}

/// Information about a markdown code block for recursive formatting
#[derive(Debug, Clone)]
pub struct MarkdownCodeBlock {
    /// Byte offset where the content starts (after opening fence line)
    pub content_start: usize,
    /// Byte offset where the content ends (before closing fence line)
    pub content_end: usize,
}

/// Builder for MarkdownCodeBlock during parsing
struct MarkdownCodeBlockBuilder {
    content_start: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_fenced_code_blocks() {
        // The function detects fenced blocks and inline code spans
        // Fence markers (``` at line start) are now skipped in inline span detection

        // Basic fenced code block with backticks
        let content = "Some text\n```\ncode here\n```\nMore text";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Should find: 1 fenced block (fences are no longer detected as inline spans)
        assert_eq!(blocks.len(), 1);

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
        // 2 fenced blocks (fence markers no longer detected as inline spans)
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn test_detect_code_blocks_with_language() {
        // Code block with language identifier
        let content = "Text\n```rust\nfn main() {}\n```\nMore";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced block (fence markers no longer detected as inline spans)
        assert_eq!(blocks.len(), 1);
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
    fn test_indented_content_with_list_markers_is_code_block() {
        // Per CommonMark spec: 4-space indented content after blank line IS a code block,
        // even if the content looks like list markers. The indentation takes precedence.
        // Verified with: echo 'List:\n\n    - Item 1' | npx commonmark
        // Output: <pre><code>- Item 1</code></pre>
        let content = "List:\n\n    - Item 1\n    - Item 2\n    * Item 3\n    + Item 4";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1); // This IS a code block per spec

        // Same for numbered list markers
        let content = "List:\n\n    1. First\n    2. Second";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1); // This IS a code block per spec
    }

    #[test]
    fn test_actual_list_items_not_code_blocks() {
        // Actual list items (no preceding blank line + 4 spaces) are NOT code blocks
        let content = "- Item 1\n- Item 2\n* Item 3";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);

        // Nested list items
        let content = "- Item 1\n  - Nested item\n- Item 2";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0);
    }

    #[test]
    fn test_inline_code_spans_not_detected() {
        // Inline code spans should NOT be detected as code blocks
        let content = "Text with `inline code` here";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0); // No blocks, only inline spans

        // Multiple backtick code span
        let content = "Text with ``code with ` backtick`` here";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0); // No blocks, only inline spans

        // Multiple code spans
        let content = "Has `code1` and `code2` spans";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0); // No blocks, only inline spans
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
        // Should only detect the fenced block, NOT the inline spans
        assert_eq!(blocks.len(), 1);

        // Check we have the fenced block only
        assert!(blocks.iter().any(|(s, e)| content[*s..*e].contains("block")));
        // Should NOT detect inline spans
        assert!(!blocks.iter().any(|(s, e)| &content[*s..*e] == "`span1`"));
        assert!(!blocks.iter().any(|(s, e)| &content[*s..*e] == "`span2`"));
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
        // 1 fenced block (fence markers no longer detected as inline spans)
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, 0); // Fenced block starts at 0
    }

    #[test]
    fn test_code_block_at_end() {
        let content = "Text before\n```\ncode\n```";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced block (fence markers no longer detected as inline spans)
        assert_eq!(blocks.len(), 1);
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
        // Code spans should NOT be detected as code blocks
        let content = "Text ` code with spaces ` more";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 0); // No blocks, only inline span
    }

    #[test]
    fn test_fenced_block_with_info_string() {
        // Fenced code blocks with complex info strings
        let content = "```rust,no_run,should_panic\ncode\n```";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // 1 fenced block (fence markers no longer detected as inline spans)
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, 0);
    }

    #[test]
    fn test_indented_fences_not_code_blocks() {
        // Indented fence markers should still work as fences
        let content = "Text\n  ```\n  code\n  ```\nAfter";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Only 1 fenced block (indented fences still work)
        assert_eq!(blocks.len(), 1);
    }

    // Issue #175: Backticks in info string invalidate the fence
    #[test]
    fn test_backticks_in_info_string_not_code_block() {
        // Per CommonMark spec: "If the info string comes after a backtick fence,
        // it may not contain any backtick characters."
        // So ```something``` is NOT a valid fence - the backticks are treated as inline code.
        // Verified with: echo '```something```' | npx commonmark
        // Output: <p><code>something</code></p>
        let content = "```something```\n\n```bash\n# comment\n```";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Should find only the valid ```bash block, NOT the invalid ```something```
        assert_eq!(blocks.len(), 1);
        // The valid block should contain "# comment"
        assert!(content[blocks[0].0..blocks[0].1].contains("# comment"));
    }

    #[test]
    fn test_issue_175_reproduction() {
        // Full reproduction of issue #175
        let content = "```something```\n\n```bash\n# Have a parrot\necho \"🦜\"\n```";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // Only the bash block is a code block
        assert_eq!(blocks.len(), 1);
        assert!(content[blocks[0].0..blocks[0].1].contains("Have a parrot"));
    }

    #[test]
    fn test_tilde_fence_allows_tildes_in_info_string() {
        // Tilde fences CAN have tildes in info string (only backtick restriction exists)
        // ~~~abc~~~ opens an unclosed code block with info string "abc~~~"
        let content = "~~~abc~~~\ncode content\n~~~";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // This is a valid tilde fence that opens and closes
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn test_nested_longer_fence_contains_shorter() {
        // Longer fence (````) can contain shorter fence (```) as content
        let content = "````\n```\nnested content\n```\n````";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert!(content[blocks[0].0..blocks[0].1].contains("nested content"));
    }

    #[test]
    fn test_mixed_fence_types() {
        // Tilde fence contains backtick markers as content
        let content = "~~~\n```\nmixed content\n~~~";
        let blocks = CodeBlockUtils::detect_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert!(content[blocks[0].0..blocks[0].1].contains("mixed content"));
    }

    #[test]
    fn test_indented_code_in_list_issue_276() {
        // Issue #276: Indented code block inside a list should be detected by pulldown-cmark
        let content = r#"1. First item
2. Second item with code:

        # This is a code block in a list
        print("Hello, world!")

4. Third item"#;

        let blocks = CodeBlockUtils::detect_code_blocks(content);
        // pulldown-cmark SHOULD detect this indented code block inside the list
        assert!(!blocks.is_empty(), "Should detect indented code block inside list");

        // Verify the detected block contains our code
        let all_content: String = blocks
            .iter()
            .map(|(s, e)| &content[*s..*e])
            .collect::<Vec<_>>()
            .join("");
        assert!(
            all_content.contains("code block in a list") || all_content.contains("print"),
            "Detected block should contain the code content: {all_content:?}"
        );
    }

    #[test]
    fn test_detect_markdown_code_blocks() {
        let content = r#"# Example

```markdown
# Heading
Content here
```

```md
Another heading
More content
```

```rust
// Not markdown
fn main() {}
```
"#;

        let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);

        // Should detect 2 blocks (markdown and md, not rust)
        assert_eq!(
            blocks.len(),
            2,
            "Should detect exactly 2 markdown blocks, got {blocks:?}"
        );

        // First block should be the ```markdown block
        let first = &blocks[0];
        let first_content = &content[first.content_start..first.content_end];
        assert!(
            first_content.contains("# Heading"),
            "First block should contain '# Heading', got: {first_content:?}"
        );

        // Second block should be the ```md block
        let second = &blocks[1];
        let second_content = &content[second.content_start..second.content_end];
        assert!(
            second_content.contains("Another heading"),
            "Second block should contain 'Another heading', got: {second_content:?}"
        );
    }

    #[test]
    fn test_detect_markdown_code_blocks_empty() {
        let content = "# Just a heading\n\nNo code blocks here\n";
        let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);
        assert_eq!(blocks.len(), 0);
    }

    #[test]
    fn test_detect_markdown_code_blocks_case_insensitive() {
        let content = "```MARKDOWN\nContent\n```\n";
        let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn test_detect_markdown_code_blocks_at_eof_no_trailing_newline() {
        // Block at end of file without trailing newline after closing fence
        let content = "# Doc\n\n```markdown\nContent\n```";
        let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        // Content should be extractable without panic
        let block_content = &content[blocks[0].content_start..blocks[0].content_end];
        assert!(block_content.contains("Content"));
    }

    #[test]
    fn test_detect_markdown_code_blocks_single_line_content() {
        // Single line of content, no extra newlines
        let content = "```markdown\nX\n```\n";
        let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        let block_content = &content[blocks[0].content_start..blocks[0].content_end];
        assert_eq!(block_content, "X");
    }

    #[test]
    fn test_detect_markdown_code_blocks_empty_content() {
        // Block with no content between fences
        let content = "```markdown\n```\n";
        let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);
        // Should detect block but with empty range or not at all
        // Either behavior is acceptable as long as no panic
        if !blocks.is_empty() {
            // If detected, content range should be valid
            assert!(blocks[0].content_start <= blocks[0].content_end);
        }
    }

    #[test]
    fn test_detect_markdown_code_blocks_validates_ranges() {
        // Ensure no panic on various edge cases
        let test_cases = [
            "",                             // Empty content
            "```markdown",                  // Unclosed block
            "```markdown\n",                // Unclosed block with newline
            "```\n```",                     // Non-markdown block
            "```markdown\n```",             // Empty markdown block
            "   ```markdown\n   X\n   ```", // Indented block
        ];

        for content in test_cases {
            // Should not panic
            let blocks = CodeBlockUtils::detect_markdown_code_blocks(content);
            // All detected blocks should have valid ranges
            for block in &blocks {
                assert!(
                    block.content_start <= block.content_end,
                    "Invalid range in content: {content:?}"
                );
                assert!(
                    block.content_end <= content.len(),
                    "Range exceeds content length in: {content:?}"
                );
            }
        }
    }
}
