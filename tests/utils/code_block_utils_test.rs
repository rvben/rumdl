use rumdl::rules::code_block_utils::*;

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
        content.push_str(&format!("Line {}\n", i));

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

    println!("CodeBlockInfo creation time: {:?}", create_time);
    println!(
        "is_in_code_block check time for all lines: {:?}",
        is_in_code_block_time
    );

    // Simple verification that our test code works
    let total_lines = content.lines().count();
    assert!(total_lines > 50);
}
