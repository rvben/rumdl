use rumdl_lib::rules::code_block_utils::*;

#[test]
fn test_is_in_code_block() {
    // Test with fenced code blocks
    let content = "Normal text\n```\nCode block\n```\nMore text";
    let cbinfo = CodeBlockInfo::new(content);
    assert!(!cbinfo.is_in_code_block(0)); // Line 1
    assert!(cbinfo.is_in_code_block(1)); // Line 2
    assert!(cbinfo.is_in_code_block(2)); // Line 3
    assert!(cbinfo.is_in_code_block(3)); // Line 4
    assert!(!cbinfo.is_in_code_block(4)); // Line 5

    // Test with language specifier
    let content = "Normal text\n```rust\nlet x = 1;\n```\nMore text";
    let cbinfo = CodeBlockInfo::new(content);
    assert!(!cbinfo.is_in_code_block(0)); // Line 1
    assert!(cbinfo.is_in_code_block(1)); // Line 2
    assert!(cbinfo.is_in_code_block(2)); // Line 3
    assert!(cbinfo.is_in_code_block(3)); // Line 4
    assert!(!cbinfo.is_in_code_block(4)); // Line 5

    // Test with indented code blocks
    let content = "Normal text\n    Indented code\nMore text";
    let cbinfo = CodeBlockInfo::new(content);
    assert!(!cbinfo.is_in_code_block(0)); // Line 1
    assert!(cbinfo.is_in_code_block(1)); // Line 2
    assert!(!cbinfo.is_in_code_block(2)); // Line 3

    // Test empty content
    let content = "";
    let cbinfo = CodeBlockInfo::new(content);
    assert!(!cbinfo.is_in_code_block(0));

    // Test out of bounds line number
    let content = "Just one line";
    let cbinfo = CodeBlockInfo::new(content);
    assert!(!cbinfo.is_in_code_block(1)); // Line 2 doesn't exist
}

#[test]
fn test_code_block_info() {
    // Test document with code blocks
    let content = "Normal text\n```javascript\nlet x = 1;\n```\nMore text";
    let info = CodeBlockInfo::new(content);
    assert!(info.has_code_blocks());
    assert!(!info.has_code_spans());

    // Test document with code spans
    let content = "Text with `code` and more";
    let info = CodeBlockInfo::new(content);
    assert!(!info.has_code_blocks());
    assert!(info.has_code_spans());

    // Test document with both
    let content = "Text with `span`\n```\nBlock\n```";
    let info = CodeBlockInfo::new(content);
    assert!(info.has_code_blocks());
    assert!(info.has_code_spans());

    // Test document with neither
    let content = "Plain text\nNo code here";
    let info = CodeBlockInfo::new(content);
    assert!(!info.has_code_blocks());
    assert!(!info.has_code_spans());
}

#[test]
fn test_is_code_block_delimiter() {
    // Test standard fenced code blocks
    assert!(CodeBlockUtils::is_code_block_delimiter("```"));
    assert!(CodeBlockUtils::is_code_block_delimiter("```rust"));
    assert!(CodeBlockUtils::is_code_block_delimiter("``` "));
    assert!(CodeBlockUtils::is_code_block_delimiter("   ```")); // With leading whitespace

    // Test alternate fence style
    assert!(CodeBlockUtils::is_code_block_delimiter("~~~"));
    assert!(CodeBlockUtils::is_code_block_delimiter("~~~css"));
    assert!(CodeBlockUtils::is_code_block_delimiter("  ~~~"));

    // Test non-delimiters
    assert!(!CodeBlockUtils::is_code_block_delimiter("Code ```"));
    assert!(!CodeBlockUtils::is_code_block_delimiter("``"));
    assert!(!CodeBlockUtils::is_code_block_delimiter("Some text"));

    // The implementation actually recognizes indented delimiters
    assert!(CodeBlockUtils::is_code_block_delimiter("    ```"));
}

#[test]
fn test_compute_code_spans() {
    // Test with no spans
    let content = "This has no code spans";
    let spans = compute_code_spans(content);
    assert_eq!(spans.len(), 0);

    // Basic test with code spans - just check number of spans
    let content = "This is `code` span";
    let spans = compute_code_spans(content);
    assert_eq!(spans.len(), 1);

    // Test with multiple spans
    let content = "This has `one` and `two` spans";
    let spans = compute_code_spans(content);
    assert_eq!(spans.len(), 2);

    // Test with unclosed span
    let content = "This has an `unclosed span";
    let spans = compute_code_spans(content);
    assert_eq!(spans.len(), 0);

    // Test with escaped backticks
    let content = "This has \\`escaped\\` backticks";
    let spans = compute_code_spans(content);
    assert_eq!(spans.len(), 0);

    // Test with longer backtick sequence
    let content = "This has ``longer ` backtick`` sequence";
    let spans = compute_code_spans(content);
    assert_eq!(spans.len(), 1);
}

#[test]
fn test_performance_code_block_utils() {
    // Create a smaller document with code elements
    let mut content = String::new();
    for i in 0..50 {
        content.push_str(&format!("Line {i}\n"));

        if i % 10 == 0 {
            content.push_str("```\nCode block content\nMore code\n```\n");
        }

        if i % 5 == 0 {
            content.push_str("Text with `code span` and `another span`\n");
        }
    }

    // Measure the time to create a CodeBlockInfo object
    let start = std::time::Instant::now();
    let info = CodeBlockInfo::new(&content);
    let create_time = start.elapsed();

    // Measure the time to check if lines are in code blocks
    let start = std::time::Instant::now();
    for i in 0..content.lines().count() {
        let _ = info.is_in_code_block(i);
    }
    let is_in_code_block_time = start.elapsed();

    // Verify that has_code_blocks and has_code_spans work correctly
    assert!(info.has_code_blocks());
    assert!(info.has_code_spans());

    println!("CodeBlockInfo creation time: {create_time:?}");
    println!("is_in_code_block check time for all lines: {is_in_code_block_time:?}");

    // Simple verification that our test code works
    let total_lines = content.lines().count();
    assert!(total_lines > 50);
}

#[test]
fn test_indented_list_items_not_code_blocks() {
    // Test that indented list items are not detected as code blocks
    let content = "- Item 1\n    - Nested item\n    - Another nested\n- Item 2";
    // Using LintContext to get proper code block detection
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    assert!(
        ctx.code_blocks.is_empty(),
        "Indented list items should not be detected as code blocks"
    );
}

#[test]
fn test_numbered_list_indentation() {
    // Test numbered lists with various formats
    let content = "1. First item\n    1) Nested with parenthesis\n    2. Another nested\n2. Second item";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    assert!(
        ctx.code_blocks.is_empty(),
        "Indented numbered list items should not be detected as code blocks"
    );
}

#[test]
fn test_code_block_requires_blank_line() {
    // Test that indented code blocks require a blank line before them
    let content = "Some text\n    This should not be a code block\n\n    This should be a code block";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    assert_eq!(ctx.code_blocks.len(), 1, "Should have code block after blank line");
}

#[test]
fn test_document_start_indented_code() {
    // Test that indented content at document start needs blank line
    let content = "    Not a code block at start\n\n    This is a code block";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    assert_eq!(ctx.code_blocks.len(), 1, "Should have code block after blank line");
}

#[test]
fn test_mixed_list_markers() {
    // Test various list markers
    let content = "- Dash list\n    - Nested\n* Star list\n    * Nested\n+ Plus list\n    + Nested";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    assert!(ctx.code_blocks.is_empty(), "All list types should be recognized");
}

#[test]
fn test_list_continuation_with_code() {
    // Test list items that contain actual code blocks
    let content = r#"1. List item
    More content in the list

    ```rust
    fn code_in_list() {}
    ```

2. Next item"#;
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    // Should find the fenced code block in the list
    assert!(!ctx.code_blocks.is_empty(), "Should find the fenced code block");
}

#[test]
fn test_tab_indented_lists() {
    // Test with tab indentation
    let content = "-\tTab after marker\n\t-\tNested with tabs\n\t\tContent";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    assert!(
        ctx.code_blocks.is_empty(),
        "Tab-indented lists should not be code blocks"
    );
}

#[test]
fn test_edge_case_single_digit_lists() {
    // Test edge cases with single digit followed by period/paren
    let content = "5. Five\n    5) Sub item\n6) Six with paren\n    6. Sub item";
    let ctx = rumdl_lib::lint_context::LintContext::new(content);
    assert!(
        ctx.code_blocks.is_empty(),
        "All numbered list formats should be recognized"
    );
}
