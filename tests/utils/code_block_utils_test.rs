use rumdl::rules::code_block_utils::*;

#[test]
fn test_is_in_code_block() {
    // Test standard fenced code blocks
    let content = "Normal text\n```\nCode block\n```\nMore text";
    assert!(!is_in_code_block(content, 0)); // First line not in code block
    assert!(is_in_code_block(content, 2)); // Third line in code block
    assert!(!is_in_code_block(content, 4)); // Last line not in code block
    
    // Test alternate fence style
    let content = "Normal text\n~~~\nCode block\n~~~\nMore text";
    assert!(!is_in_code_block(content, 0));
    assert!(is_in_code_block(content, 2));
    assert!(!is_in_code_block(content, 4));
    
    // Test indented code blocks
    let content = "Normal text\n    Indented code\nMore text";
    assert!(!is_in_code_block(content, 0));
    assert!(is_in_code_block(content, 1));
    assert!(!is_in_code_block(content, 2));
    
    // Test mixed styles
    let content = "Normal text\n```\nFenced code\n```\n    Indented code\nMore text";
    assert!(is_in_code_block(content, 2));
    assert!(is_in_code_block(content, 4));
    assert!(!is_in_code_block(content, 5));
    
    // Test nested blocks (shouldn't be valid Markdown, but we should still handle it)
    let content = "```\nOuter block\n    ```\n    Inner block\n    ```\n```\nNormal text";
    assert!(is_in_code_block(content, 1));
    assert!(is_in_code_block(content, 3));
    assert!(!is_in_code_block(content, 6));
}

#[test]
fn test_is_code_block_delimiter() {
    assert!(is_code_block_delimiter("```"));
    assert!(is_code_block_delimiter("```javascript"));
    assert!(is_code_block_delimiter("~~~"));
    assert!(is_code_block_delimiter("~~~python"));
    assert!(!is_code_block_delimiter("Normal text"));
    assert!(!is_code_block_delimiter("    ```")); // Indented code fences are not delimiters
    assert!(!is_code_block_delimiter("## Heading"));
}

#[test]
fn test_compute_code_spans() {
    // Test basic code spans
    let spans = compute_code_spans("This is `code` span");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0], (8, 14));
    
    // Test multiple code spans
    let spans = compute_code_spans("Multiple `code` spans in `one` line");
    assert_eq!(spans.len(), 2);
    
    // Test code spans with backticks inside them
    let spans = compute_code_spans("Code with ``nested ` backtick`` inside");
    assert_eq!(spans.len(), 1);
    
    // Test no code spans
    let spans = compute_code_spans("No code spans here");
    assert_eq!(spans.len(), 0);
    
    // Test unclosed code spans (should not be detected)
    let spans = compute_code_spans("Unclosed `code span");
    assert_eq!(spans.len(), 0);
    
    // Test code spans at the edges
    let spans = compute_code_spans("`At start` and `at end`");
    assert_eq!(spans.len(), 2);
    
    // Test triple backticks (fenced code block marker, not a code span)
    let spans = compute_code_spans("```\nNot a code span\n```");
    assert_eq!(spans.len(), 0);
}

#[test]
fn test_code_block_info() {
    // Test with standard fenced code blocks
    let content = "Normal text\n```javascript\nlet x = 1;\n```\nMore text";
    let info = CodeBlockInfo::new(content);
    assert!(!info.is_in_code_block(0));
    assert!(info.is_in_code_block(2));
    assert!(!info.is_in_code_block(4));
    
    // Test with multiple code blocks
    let content = "```\nBlock 1\n```\nText\n```\nBlock 2\n```";
    let info = CodeBlockInfo::new(content);
    assert!(info.is_in_code_block(1));
    assert!(!info.is_in_code_block(3));
    assert!(info.is_in_code_block(5));
    
    // Test with code spans and code blocks
    let content = "Text with `code span`\n```\ncode block\nwith `more code span`\n```";
    let info = CodeBlockInfo::new(content);
    assert!(!info.is_in_code_block(0));
    assert!(info.is_in_code_block(2));
    assert!(info.is_in_code_block(3));
    
    // Test with edge cases
    let content = "Text\n\n```\n\n```\n\nMore text";
    let info = CodeBlockInfo::new(content);
    assert!(!info.is_in_code_block(0));
    assert!(info.is_in_code_block(3));
    assert!(!info.is_in_code_block(6));
}

#[test]
fn test_performance_code_block_utils() {
    // Create a large document with many code blocks
    let mut content = String::with_capacity(50_000);
    for i in 0..500 {
        content.push_str(&format!("Line {}\n", i));
        if i % 20 == 0 {
            content.push_str("```\nCode block content\nMore code\n```\n");
        }
        if i % 30 == 0 {
            content.push_str("Text with `code span` and `another span`\n");
        }
    }
    
    // Benchmark CodeBlockInfo creation
    let start = std::time::Instant::now();
    let info = CodeBlockInfo::new(&content);
    let creation_time = start.elapsed();
    
    // Benchmark code block checks
    let start = std::time::Instant::now();
    for i in 0..500 {
        let _ = info.is_in_code_block(i);
    }
    let check_time = start.elapsed();
    
    // Benchmark code span computation
    let start = std::time::Instant::now();
    let spans = compute_code_spans(&content);
    let spans_time = start.elapsed();
    
    println!("CodeBlockInfo creation: {:?}", creation_time);
    println!("500 is_in_code_block checks: {:?}", check_time);
    println!("compute_code_spans: {:?}", spans_time);
    
    // Just a simple assertion to make sure the test runs
    assert!(spans.len() > 0);
} 