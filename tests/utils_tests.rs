use rumdl::utils::{ElementQuality, ElementType, MarkdownElements};

#[test]
fn test_markdown_elements_detection() {
    let content = r#"---
title: Test Document
---

# Heading 1

This is a paragraph with `code span` and more text.

## Heading 2

- List item 1
- List item 2
* List item 3
+ List item 4

1. Ordered item 1
2. Ordered item 2

```rust
fn test() {
    println!("Hello, world!");
}
```

Setext heading
=============

Another paragraph
"#;

    // Test code block detection
    let code_blocks = MarkdownElements::detect_code_blocks(content);
    assert_eq!(code_blocks.len(), 1);
    assert_eq!(code_blocks[0].element_type, ElementType::CodeBlock);
    assert_eq!(code_blocks[0].metadata, Some("rust".to_string()));

    // Test heading detection
    let headings = MarkdownElements::detect_headings(content);

    // We have 3 headings: H1, H2, and the Setext heading (frontmatter is not treated as a heading)
    assert_eq!(headings.len(), 3);

    // Check each heading's level
    let levels: Vec<Option<u32>> = headings
        .iter()
        .map(MarkdownElements::get_heading_level)
        .collect();

    // Regular ATX headings
    assert_eq!(levels[0], Some(1)); // # Heading 1
    assert_eq!(levels[1], Some(2)); // ## Heading 2
                                    // Setext heading with = is level 1
    assert_eq!(levels[2], Some(1)); // Setext heading

    // Test list detection
    let lists = MarkdownElements::detect_lists(content);
    println!("Detected {} lists:", lists.len());
    for (i, list) in lists.iter().enumerate() {
        println!(
            "  {}. Line {}: '{}' (Marker: {:?}, Quality: {:?})",
            i + 1,
            list.start_line,
            list.text,
            list.metadata,
            list.quality
        );
    }
    assert_eq!(lists.len(), 6);

    // First list should be the first actual list in the document
    assert_eq!(lists[0].element_type, ElementType::List);
    assert_eq!(lists[0].metadata, Some("minus".to_string()));
    assert_eq!(lists[0].quality, ElementQuality::Valid);

    // Check all list marker types
    assert_eq!(lists[0].metadata, Some("minus".to_string()));
    assert_eq!(lists[1].metadata, Some("minus".to_string()));
    assert_eq!(lists[2].metadata, Some("asterisk".to_string()));
    assert_eq!(lists[3].metadata, Some("plus".to_string()));
    assert_eq!(lists[4].metadata, Some("ordered".to_string()));
    assert_eq!(lists[5].metadata, Some("ordered".to_string()));

    // Test front matter detection
    let front_matter = MarkdownElements::detect_front_matter(content);
    assert!(front_matter.is_some());
    assert_eq!(front_matter.unwrap().element_type, ElementType::FrontMatter);

    // Test code span detection
    let line = "This is a paragraph with `code span` and more text.";
    assert!(MarkdownElements::is_in_code_span(line, 25)); // Inside code span
    assert!(!MarkdownElements::is_in_code_span(line, 5)); // Outside code span

    // Test heading to fragment conversion
    assert_eq!(
        MarkdownElements::heading_to_fragment("Heading 1"),
        "heading-1"
    );
}

#[test]
fn test_markdown_elements_with_edge_cases() {
    let content = r#"
Some text

```
Unclosed code block
"#;

    // Test unclosed code block
    let code_blocks = MarkdownElements::detect_code_blocks(content);
    assert_eq!(code_blocks.len(), 1);

    // Test empty content
    let empty_content = "";
    assert_eq!(MarkdownElements::detect_code_blocks(empty_content).len(), 0);
    assert_eq!(MarkdownElements::detect_headings(empty_content).len(), 0);
    assert_eq!(MarkdownElements::detect_lists(empty_content).len(), 0);
    assert!(MarkdownElements::detect_front_matter(empty_content).is_none());

    // Test content with only whitespace
    let whitespace_content = "   \n  \n";
    assert_eq!(
        MarkdownElements::detect_code_blocks(whitespace_content).len(),
        0
    );
    assert_eq!(
        MarkdownElements::detect_headings(whitespace_content).len(),
        0
    );
    assert_eq!(MarkdownElements::detect_lists(whitespace_content).len(), 0);
}
