use rumdl_lib::rules::code_block_utils::*;

#[test]
fn test_is_code_block_start() {
    // Test standard fenced code blocks
    assert!(CodeBlockUtils::is_code_block_start("```"));
    assert!(CodeBlockUtils::is_code_block_start("```rust"));
    assert!(CodeBlockUtils::is_code_block_start("```javascript"));
    assert!(CodeBlockUtils::is_code_block_start("   ```")); // With leading whitespace
    assert!(CodeBlockUtils::is_code_block_start("   ```python"));

    // Test alternate fence style
    assert!(CodeBlockUtils::is_code_block_start("~~~"));
    assert!(CodeBlockUtils::is_code_block_start("~~~css"));
    assert!(CodeBlockUtils::is_code_block_start("  ~~~"));
    assert!(CodeBlockUtils::is_code_block_start("  ~~~ruby"));

    // Test non-starters
    assert!(!CodeBlockUtils::is_code_block_start("Code ```"));
    assert!(!CodeBlockUtils::is_code_block_start("```code```"));
    assert!(!CodeBlockUtils::is_code_block_start("``"));
    assert!(!CodeBlockUtils::is_code_block_start("Some text"));
    assert!(CodeBlockUtils::is_code_block_start("    ```"));
}

#[test]
fn test_is_code_block_end() {
    // Test standard fenced code blocks
    assert!(CodeBlockUtils::is_code_block_end("```"));
    assert!(CodeBlockUtils::is_code_block_end("``` "));
    assert!(CodeBlockUtils::is_code_block_end("```  "));
    assert!(CodeBlockUtils::is_code_block_end("   ```")); // With leading whitespace

    // Test alternate fence style
    assert!(CodeBlockUtils::is_code_block_end("~~~"));
    assert!(CodeBlockUtils::is_code_block_end("~~~ "));
    assert!(CodeBlockUtils::is_code_block_end("  ~~~"));

    // Test non-enders
    assert!(!CodeBlockUtils::is_code_block_end("``` code"));
    assert!(!CodeBlockUtils::is_code_block_end("```code"));
    assert!(!CodeBlockUtils::is_code_block_end("``"));
    assert!(!CodeBlockUtils::is_code_block_end("Some text"));
    assert!(CodeBlockUtils::is_code_block_end("    ```"));
}

#[test]
fn test_is_indented_code_block() {
    // Test valid indented code blocks
    assert!(CodeBlockUtils::is_indented_code_block("    code"));
    assert!(CodeBlockUtils::is_indented_code_block("     code with more indent"));
    assert!(CodeBlockUtils::is_indented_code_block("      still indented"));
    assert!(CodeBlockUtils::is_indented_code_block("\tcode with tab")); // Tabs are treated as 4 spaces

    // Test invalid indented code blocks
    assert!(!CodeBlockUtils::is_indented_code_block("code"));
    assert!(!CodeBlockUtils::is_indented_code_block("  code")); // Only 2 spaces
    assert!(!CodeBlockUtils::is_indented_code_block("   code")); // Only 3 spaces
    assert!(!CodeBlockUtils::is_indented_code_block("")); // Empty line
}

#[test]
fn test_get_language_specifier() {
    // Test standard fenced code blocks with language
    assert_eq!(
        CodeBlockUtils::get_language_specifier("```rust"),
        Some("rust".to_string())
    );
    assert_eq!(
        CodeBlockUtils::get_language_specifier("```javascript"),
        Some("javascript".to_string())
    );
    assert_eq!(
        CodeBlockUtils::get_language_specifier("   ```python"),
        Some("python".to_string())
    );
    assert_eq!(
        CodeBlockUtils::get_language_specifier("```js "),
        Some("js ".to_string())
    );

    // Test alternate fence style with language
    assert_eq!(
        CodeBlockUtils::get_language_specifier("~~~css"),
        Some("css".to_string())
    );
    assert_eq!(
        CodeBlockUtils::get_language_specifier("  ~~~ruby"),
        Some("ruby".to_string())
    );

    // Test without language specifier
    assert_eq!(CodeBlockUtils::get_language_specifier("```"), None);
    assert_eq!(CodeBlockUtils::get_language_specifier("~~~"), None);
    assert_eq!(CodeBlockUtils::get_language_specifier("   ```"), None);
    assert_eq!(CodeBlockUtils::get_language_specifier("  ~~~"), None);

    // Test invalid cases
    assert_eq!(CodeBlockUtils::get_language_specifier("Code"), None);
    assert_eq!(CodeBlockUtils::get_language_specifier("``rust"), None);
    assert_eq!(CodeBlockUtils::get_language_specifier("```code```"), None);
}

#[test]
fn test_identify_code_block_lines() {
    // Test standard fenced code blocks
    let content = "Normal text\n```\nCode block\n```\nMore text";
    let result = CodeBlockUtils::identify_code_block_lines(content);
    assert_eq!(result, vec![false, true, true, true, false]);

    // Test alternate fence style
    let content = "Normal text\n~~~\nCode block\n~~~\nMore text";
    let result = CodeBlockUtils::identify_code_block_lines(content);
    assert_eq!(result, vec![false, true, true, true, false]);

    // Test indented code blocks
    let content = "Normal text\n    Indented code\nMore text";
    let result = CodeBlockUtils::identify_code_block_lines(content);
    assert_eq!(result, vec![false, true, false]);

    // Test with language specifier
    let content = "Normal text\n```rust\nlet x = 1;\n```\nMore text";
    let result = CodeBlockUtils::identify_code_block_lines(content);
    assert_eq!(result, vec![false, true, true, true, false]);

    // Test nested blocks (shouldn't be valid Markdown, but we should still handle it)
    let content = "```\nOuter block\n    ```\n    Inner block\n    ```\n```\nNormal text";
    let result = CodeBlockUtils::identify_code_block_lines(content);
    assert_eq!(result, vec![true, true, true, true, true, true, false]);

    // Test empty content
    let content = "";
    let result = CodeBlockUtils::identify_code_block_lines(content);
    assert_eq!(result, Vec::<bool>::new());

    // Test unclosed code block
    let content = "Normal text\n```\nUnclosed code block";
    let result = CodeBlockUtils::identify_code_block_lines(content);
    assert_eq!(result, vec![false, true, true]);

    // Test with multiple code blocks
    let content = "```\nBlock 1\n```\nText\n~~~\nBlock 2\n~~~";
    let result = CodeBlockUtils::identify_code_block_lines(content);
    assert_eq!(result, vec![true, true, true, false, true, true, true]);
}

#[test]
fn test_has_code_blocks_and_spans() {
    // Test document with code blocks
    let content = "Text\n```\nCode block\n```";
    let info = CodeBlockInfo::new(content);
    assert!(info.has_code_blocks());
    assert!(!info.has_code_spans());

    // Test document with code spans
    let content = "Text with `code span`";
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

    // Test empty document
    let content = "";
    let info = CodeBlockInfo::new(content);
    assert!(!info.has_code_blocks());
    assert!(!info.has_code_spans());
}

#[test]
fn test_code_block_state_enum() {
    // Just testing the enum equality
    assert_eq!(CodeBlockState::None, CodeBlockState::None);
    assert_eq!(CodeBlockState::Fenced, CodeBlockState::Fenced);
    assert_eq!(CodeBlockState::Indented, CodeBlockState::Indented);

    assert!(CodeBlockState::None != CodeBlockState::Fenced);
    assert!(CodeBlockState::None != CodeBlockState::Indented);
    assert!(CodeBlockState::Fenced != CodeBlockState::Indented);
}

#[test]
fn test_compute_code_blocks() {
    // Test with standard fenced code blocks
    let content = "Normal text\n```\nCode block\n```\nMore text";
    let result = compute_code_blocks(content);
    assert_eq!(
        result,
        vec![
            CodeBlockState::None,
            CodeBlockState::Fenced,
            CodeBlockState::Fenced,
            CodeBlockState::Fenced,
            CodeBlockState::None
        ]
    );

    // Test with indented code blocks
    let content = "Normal text\n    Indented code\nMore text";
    let result = compute_code_blocks(content);
    assert_eq!(
        result,
        vec![CodeBlockState::None, CodeBlockState::Indented, CodeBlockState::None]
    );

    // Test mixed styles
    let content = "Text\n```\nFenced\n```\n    Indented\nMore";
    let result = compute_code_blocks(content);
    assert_eq!(
        result,
        vec![
            CodeBlockState::None,
            CodeBlockState::Fenced,
            CodeBlockState::Fenced,
            CodeBlockState::Fenced,
            CodeBlockState::Indented,
            CodeBlockState::None
        ]
    );

    // Test unclosed fenced block
    let content = "Text\n```\nUnclosed";
    let result = compute_code_blocks(content);
    assert_eq!(
        result,
        vec![CodeBlockState::None, CodeBlockState::Fenced, CodeBlockState::Fenced]
    );

    // Test empty content
    let content = "";
    let result = compute_code_blocks(content);
    assert_eq!(result, Vec::<CodeBlockState>::new());
}

#[test]
fn test_edge_cases() {
    // Test with backtick confusion
    let content = "Text ```not a code block but might be confused as one```";
    let cbinfo = CodeBlockInfo::new(content);
    assert!(!cbinfo.is_in_code_block(0));
    assert!(cbinfo.has_code_spans());

    // Test with code blocks that have whitespace variations
    let content = "Text\n```   \nStill a code block\n```  \nMore text";
    let cbinfo = CodeBlockInfo::new(content);
    assert!(cbinfo.is_in_code_block(2));
    assert!(!cbinfo.is_in_code_block(4));

    // Test code block with tabs
    let content = "Text\n\t\t\t\tIndented with tabs\nMore text";
    let cbinfo = CodeBlockInfo::new(content);
    assert!(cbinfo.is_in_code_block(1));

    // Test code spans with escaped backticks
    let content = "Text with \\`not a code span\\` but escaped";
    let cbinfo = CodeBlockInfo::new(content);
    assert!(!cbinfo.has_code_spans());

    // Test code block markers inside code spans
    let content = "Text with ````not a code block```` span";
    let cbinfo = CodeBlockInfo::new(content);
    assert!(!cbinfo.has_code_blocks());
    assert!(cbinfo.has_code_spans());

    // Test nested indentation
    let content = "Text\n    Indented\n        More indented\n    Back to first level\nNormal";
    let result = CodeBlockUtils::identify_code_block_lines(content);
    assert_eq!(result, vec![false, true, true, true, false]);
}

#[test]
fn test_complex_document() {
    // A more complex document with various code elements
    let content = r#"# Heading

Normal paragraph with `inline code`.

```rust
fn main() {
    println!("Hello, world!");
    // Comment with `backticks`
}
```

More text with ``nested ` backtick`` syntax.

    Indented code block
    Still indented

~~~
Another code block style
~~~

Final text."#;

    let info = CodeBlockInfo::new(content);

    // Debug: Print out what lines are detected as code blocks
    println!("--- Line-by-line breakdown ---");
    for (i, line) in content.lines().enumerate() {
        println!(
            "Line {}: '{}' - Is in code block: {}",
            i,
            line,
            info.is_in_code_block(i)
        );
    }

    // Check specific lines are in code blocks
    assert!(!info.is_in_code_block(0)); // Heading
    assert!(!info.is_in_code_block(2)); // Normal paragraph
    assert!(info.is_in_code_block(4)); // code block start
    assert!(info.is_in_code_block(6)); // code block content
    assert!(info.is_in_code_block(8)); // code block content
    assert!(info.is_in_code_block(9)); // code block end
    assert!(!info.is_in_code_block(10)); // empty line
    assert!(!info.is_in_code_block(11)); // More text
    assert!(!info.is_in_code_block(12)); // empty line
    assert!(info.is_in_code_block(13)); // indented code block
    assert!(info.is_in_code_block(14)); // still indented
    assert!(!info.is_in_code_block(15)); // empty indented line
    assert!(info.is_in_code_block(16)); // tilde block start
    assert!(info.is_in_code_block(17)); // tilde block content
    assert!(info.is_in_code_block(18)); // tilde block end
    assert!(!info.is_in_code_block(19)); // empty line
    assert!(!info.is_in_code_block(20)); // final text

    // Verify we detect both kinds of code elements
    assert!(info.has_code_blocks());
    assert!(info.has_code_spans());
}

#[test]
fn test_performance_code_block_specific_functions() {
    // Create a smaller document with various code elements
    let mut content = String::with_capacity(5_000);
    for i in 0..50 {
        content.push_str(&format!("Line {i}\n"));

        if i % 10 == 0 {
            content.push_str("```\nCode block content\nMore code\n```\n");
        }

        if i % 15 == 0 {
            content.push_str("    Indented code block\n    More indented code\n\n");
        }
    }

    // Test identify_code_block_lines performance
    let start = std::time::Instant::now();
    let block_lines = CodeBlockUtils::identify_code_block_lines(&content);
    let identify_time = start.elapsed();

    // Test compute_code_blocks performance
    let start = std::time::Instant::now();
    let block_states = compute_code_blocks(&content);
    let compute_time = start.elapsed();

    // Test CodeBlockInfo creation performance
    let start = std::time::Instant::now();
    let info = CodeBlockInfo::new(&content);
    let creation_time = start.elapsed();

    // Test is_in_code_block performance
    let start = std::time::Instant::now();
    for i in 0..content.lines().count() {
        let _ = info.is_in_code_block(i);
    }
    let check_time = start.elapsed();

    println!("identify_code_block_lines: {identify_time:?}");
    println!("compute_code_blocks: {compute_time:?}");
    println!("CodeBlockInfo creation: {creation_time:?}");
    println!("is_in_code_block checks: {check_time:?}");

    // Verify that we got results
    assert!(block_lines.len() > 50);
    assert!(block_states.len() > 50);
    assert!(info.has_code_blocks());
}
