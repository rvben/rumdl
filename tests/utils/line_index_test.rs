use rumdl::utils::range_utils::LineIndex;

#[test]
fn test_backtick_code_fence_detection() {
    let content = "# Heading\n\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\n\nText after.";
    let index = LineIndex::new(content.to_string());

    // Not in code block
    assert!(!index.is_code_block(0)); // # Heading
    assert!(!index.is_code_block(1)); // Empty line

    // Inside code block
    assert!(index.is_code_block(2)); // ```rust - fence marker
    assert!(index.is_code_block(3)); // fn main() {
    assert!(index.is_code_block(4)); // println!("Hello");
    assert!(index.is_code_block(5)); // }
    assert!(index.is_code_block(6)); // ``` - fence marker

    // Not in code block
    assert!(!index.is_code_block(7)); // Empty line
    assert!(!index.is_code_block(8)); // Text after.
}

#[test]
fn test_tilde_code_fence_detection() {
    let content = "# Heading\n\n~~~\nSome code\nMore code\n~~~\n\nText after.";
    let index = LineIndex::new(content.to_string());

    // Not in code block
    assert!(!index.is_code_block(0)); // # Heading
    assert!(!index.is_code_block(1)); // Empty line

    // Inside code block
    assert!(index.is_code_block(2)); // ~~~ - fence marker
    assert!(index.is_code_block(3)); // Some code
    assert!(index.is_code_block(4)); // More code
    assert!(index.is_code_block(5)); // ~~~ - fence marker

    // Not in code block
    assert!(!index.is_code_block(6)); // Empty line
    assert!(!index.is_code_block(7)); // Text after.
}

#[test]
fn test_indented_code_block_detection() {
    let content = "# Heading\n\n    This is an indented code block\n    More indented code\n\nText after.";
    let index = LineIndex::new(content.to_string());

    // Not in code block
    assert!(!index.is_code_block(0)); // # Heading
    assert!(!index.is_code_block(1)); // Empty line

    // Inside code block
    assert!(index.is_code_block(2)); // Indented code
    assert!(index.is_code_block(3)); // More indented code

    // Not in code block
    assert!(!index.is_code_block(4)); // Empty line
    assert!(!index.is_code_block(5)); // Text after.
}

#[test]
fn test_tab_indented_code_block_detection() {
    let content = "# Heading\n\n\tThis is a tab-indented code block\n\tMore tab-indented code\n\nText after.";
    let index = LineIndex::new(content.to_string());

    // Not in code block
    assert!(!index.is_code_block(0)); // # Heading
    assert!(!index.is_code_block(1)); // Empty line

    // Inside code block
    assert!(index.is_code_block(2)); // Tab-indented code
    assert!(index.is_code_block(3)); // More tab-indented code

    // Not in code block
    assert!(!index.is_code_block(4)); // Empty line
    assert!(!index.is_code_block(5)); // Text after.
}

#[test]
fn test_nested_code_blocks() {
    // In standard Markdown, nested fences don't close the outer block
    let content = "# Heading\n\n```markdown\n# Example markdown\n\n```code\nnested code\n```\n\nStill in outer block\n```\n\nText after.";
    println!("Test content:\n{content}");

    // Print each line with line numbers for clarity
    println!("\nContent by line numbers:");
    content.lines().enumerate().for_each(|(i, line)| {
        println!("Line {i}: '{line}'");
    });

    let index = LineIndex::new(content.to_string());

    // Debug information
    println!("\nContent lines with is_code_block status:");
    content.lines().enumerate().for_each(|(i, line)| {
        println!("Line {}: '{}' - is_code_block: {}", i, line, index.is_code_block(i));
    });

    // Not in code block
    assert!(!index.is_code_block(0), "Line 0 should not be in a code block"); // # Heading
    assert!(!index.is_code_block(1), "Line 1 should not be in a code block"); // Empty line

    // Outer code block
    assert!(index.is_code_block(2), "Line 2 should be in a code block"); // ```markdown - fence marker (OPENS block)

    // Everything inside the block should be marked as inside a code block,
    // including any nested fence markers which should be treated as content
    assert!(index.is_code_block(3), "Line 3 should be in a code block"); // # Example markdown
    assert!(index.is_code_block(4), "Line 4 should be in a code block"); // Empty line
    assert!(index.is_code_block(5), "Line 5 should be in a code block"); // ```code - this is just content inside the block
    assert!(index.is_code_block(6), "Line 6 should be in a code block"); // nested code
    assert!(index.is_code_block(7), "Line 7 should be in a code block"); // ``` - this does NOT close the block because it's nested
    assert!(index.is_code_block(8), "Line 8 should be in a code block"); // Empty line
    assert!(index.is_code_block(9), "Line 9 should be in a code block"); // Still in outer block
    assert!(index.is_code_block(10), "Line 10 should be in a code block"); // ``` - fence marker (CLOSES block)

    // Not in code block
    assert!(!index.is_code_block(11), "Line 11 should not be in a code block"); // Empty line
    assert!(!index.is_code_block(12), "Line 12 should not be in a code block"); // Text after.
}

#[test]
fn test_mixed_code_blocks() {
    let content = "# Heading\n\n```\nBacktick code\n```\n\n~~~\nTilde code\n~~~\n\n    Indented code\n\nText after.";
    let index = LineIndex::new(content.to_string());

    // Backtick block
    assert!(index.is_code_block(2)); // ```
    assert!(index.is_code_block(3)); // Backtick code
    assert!(index.is_code_block(4)); // ```

    // Tilde block
    assert!(index.is_code_block(6)); // ~~~
    assert!(index.is_code_block(7)); // Tilde code
    assert!(index.is_code_block(8)); // ~~~

    // Indented block
    assert!(index.is_code_block(10)); // Indented code

    // Regular text
    assert!(!index.is_code_block(0)); // # Heading
    assert!(!index.is_code_block(1)); // Empty line
    assert!(!index.is_code_block(5)); // Empty line after backtick block
    assert!(!index.is_code_block(9)); // Empty line after tilde block
    assert!(!index.is_code_block(11)); // Empty line after indented block
    assert!(!index.is_code_block(12)); // Text after.
}

#[test]
fn test_edge_cases() {
    // Test for empty content
    let empty = "";
    let _empty_index = LineIndex::new(empty.to_string());

    // Test for single line content
    let single_line = "Just one line";
    let single_line_index = LineIndex::new(single_line.to_string());
    assert!(!single_line_index.is_code_block(0));

    // Test for code block at start of document
    let start_code = "```\nCode at start\n```\nText after";
    let start_code_index = LineIndex::new(start_code.to_string());
    assert!(start_code_index.is_code_block(0));
    assert!(start_code_index.is_code_block(1));
    assert!(start_code_index.is_code_block(2));
    assert!(!start_code_index.is_code_block(3));

    // Test for code block at end of document
    let end_code = "Text before\n```\nCode at end\n```";
    let end_code_index = LineIndex::new(end_code.to_string());
    assert!(!end_code_index.is_code_block(0));
    assert!(end_code_index.is_code_block(1));
    assert!(end_code_index.is_code_block(2));
    assert!(end_code_index.is_code_block(3));

    // Test for unclosed code block
    let unclosed = "```\nUnclosed code block";
    let unclosed_index = LineIndex::new(unclosed.to_string());
    assert!(unclosed_index.is_code_block(0));
    assert!(unclosed_index.is_code_block(1));
}

#[test]
fn test_is_code_fence() {
    let content = "# Heading\n\n```rust\nCode\n```\n\n~~~\nMore code\n~~~";
    let index = LineIndex::new(content.to_string());

    // Test backtick fence
    assert!(index.is_code_fence(2)); // ```rust
    assert!(index.is_code_fence(4)); // ```

    // Test tilde fence
    assert!(index.is_code_fence(6)); // ~~~
    assert!(index.is_code_fence(8)); // ~~~

    // Test non-fences
    assert!(!index.is_code_fence(0)); // # Heading
    assert!(!index.is_code_fence(3)); // Code
    assert!(!index.is_code_fence(7)); // More code
}

#[test]
fn test_is_tilde_code_block() {
    let content = "# Heading\n\n```rust\nCode\n```\n\n~~~\nMore code\n~~~";
    let index = LineIndex::new(content.to_string());

    // Test tilde fence
    assert!(index.is_tilde_code_block(6)); // ~~~
    assert!(index.is_tilde_code_block(8)); // ~~~

    // Test non-tilde fences
    assert!(!index.is_tilde_code_block(2)); // ```rust
    assert!(!index.is_tilde_code_block(4)); // ```
    assert!(!index.is_tilde_code_block(0)); // # Heading
}

#[test]
fn test_get_content() {
    let content = "# Test content";
    let index = LineIndex::new(content.to_string());

    assert_eq!(index.get_content(), content);
}
