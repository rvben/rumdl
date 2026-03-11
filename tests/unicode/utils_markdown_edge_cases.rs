use rumdl_lib::utils::{ElementQuality, ElementType, MarkdownElements};

#[test]
fn test_nested_elements() {
    // Test document with nested elements
    let content = r#"> # Heading inside blockquote
>
> - List inside blockquote
>   - Nested list inside blockquote
>     ```
>     Code block inside nested list inside blockquote
>     ```
>
> Regular text in blockquote

```
# Not a heading (in code block)
- Not a list (in code block)
```

- List item with `code span`
- List with **strong** and *emphasis*
- List with [link](https://example.com)
"#;

    let headings = MarkdownElements::detect_headings(content);
    // In the current implementation, headings in blockquotes aren't detected
    // This is a clearly defined behavior
    assert_eq!(
        headings.len(),
        0,
        "No headings should be detected in blockquotes or code blocks"
    );

    let code_blocks = MarkdownElements::detect_code_blocks(content);
    // Only one code block is detected (not the one in blockquote as we don't have blockquote detection yet)
    assert_eq!(code_blocks.len(), 1, "Expected exactly one code block");

    // Verify code block properties
    let code_block = &code_blocks[0];
    assert_eq!(code_block.element_type, ElementType::CodeBlock);
    assert_eq!(code_block.start_line, 10, "Code block should start at line 10");
    assert_eq!(code_block.end_line, 13, "Code block should end at line 13");
    assert_eq!(code_block.quality, ElementQuality::Valid, "Code block should be valid");

    let lists = MarkdownElements::detect_lists(content);
    // Only the 3 list items outside the blockquote/code block
    assert_eq!(lists.len(), 3, "Expected exactly three list items");

    // Verify each list item
    for list in &lists {
        assert_eq!(list.element_type, ElementType::List);
        assert_eq!(list.quality, ElementQuality::Valid, "List should be well-formed");
        assert!(list.start_line >= 15, "Lists should start after line 15");
    }
}

#[test]
fn test_indentation_edge_cases() {
    let content = r#"   # Indented heading

   - Indented list
     - Deeply indented list
       - Very deeply indented list

    ```
    Indented code block
    ```

   Paragraph with
     continued line with different indentation

   1. Ordered list
      1. Nested ordered
         1. Deeply nested
"#;

    let headings = MarkdownElements::detect_headings(content);
    assert_eq!(headings.len(), 1, "Expected exactly one heading");
    assert_eq!(headings[0].text, "Indented heading", "Heading text should match");
    assert_eq!(
        headings[0].quality,
        ElementQuality::Valid,
        "Indented heading should be valid"
    );

    let code_blocks = MarkdownElements::detect_code_blocks(content);
    assert_eq!(code_blocks.len(), 1, "Expected exactly one code block");
    assert_eq!(
        code_blocks[0].quality,
        ElementQuality::Valid,
        "Code block should be valid"
    );

    let lists = MarkdownElements::detect_lists(content);
    assert_eq!(lists.len(), 6, "Expected exactly 6 list items (3 unordered, 3 ordered)");

    // Count unordered and ordered lists
    let unordered_count = lists
        .iter()
        .filter(|l| {
            l.metadata
                .as_ref()
                .is_some_and(|m| m.contains("minus") || m.contains("asterisk") || m.contains("plus"))
        })
        .count();
    let ordered_count = lists
        .iter()
        .filter(|l| l.metadata.as_ref().is_some_and(|m| m.contains("ordered")))
        .count();

    assert_eq!(unordered_count, 3, "Should have 3 unordered list items");
    assert_eq!(ordered_count, 3, "Should have 3 ordered list items");

    // All lists should be valid
    for list in &lists {
        assert_eq!(list.quality, ElementQuality::Valid, "Indented lists should be valid");
    }
}

#[test]
fn test_malformed_elements() {
    let content = r#"#Heading without space

##  Multiple spaces after hashes

######

Heading
==

Heading
--

```js
Unclosed code block

- Malformed list
* Mixed
+ List
  markers

1.No space after marker
2) Wrong marker
"#;

    // Test malformed headings
    let headings = MarkdownElements::detect_headings(content);
    assert_eq!(headings.len(), 5, "Expected exactly 5 headings");

    // Validate headings properties - ATX no space
    assert_eq!(headings[0].text, "Heading without space");
    assert_eq!(
        headings[0].quality,
        ElementQuality::Malformed,
        "Heading without space should be malformed"
    );

    // ATX with multiple spaces
    assert_eq!(headings[1].text, "Multiple spaces after hashes");
    assert_eq!(
        headings[1].quality,
        ElementQuality::Valid,
        "Heading with spaces should be valid"
    );

    // Empty heading
    assert_eq!(headings[2].text, "");
    assert_eq!(
        headings[2].quality,
        ElementQuality::Valid,
        "Empty heading should be valid"
    );

    // Setext headings
    assert_eq!(headings[3].text, "Heading");
    assert_eq!(
        headings[3].quality,
        ElementQuality::Valid,
        "Setext heading = should be valid"
    );

    assert_eq!(headings[4].text, "Heading");
    assert_eq!(
        headings[4].quality,
        ElementQuality::Valid,
        "Setext heading - should be valid"
    );

    // Test code blocks
    let code_blocks = MarkdownElements::detect_code_blocks(content);
    assert_eq!(code_blocks.len(), 1, "Expected exactly one code block");
    assert_eq!(
        code_blocks[0].metadata,
        Some("js".to_string()),
        "Code block language should be js"
    );
    assert_eq!(
        code_blocks[0].quality,
        ElementQuality::Malformed,
        "Unclosed code block should be malformed"
    );

    // Test malformed lists
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
    assert_eq!(lists.len(), 1, "Current implementation detects 1 list item");

    // Check malformed ordered lists
    let malformed_ordered = lists
        .iter()
        .filter(|l| {
            l.metadata
                .as_ref()
                .is_some_and(|m| m.contains("ordered:no_space") || m.contains("ordered:wrong_marker"))
        })
        .count();
    assert_eq!(
        malformed_ordered, 0,
        "Current implementation does not detect malformed ordered lists yet"
    );
}

#[test]
fn test_empty_and_special_cases() {
    let content = r#"

#
##
### Empty heading

-
*
+ Empty list items

```
```

```


```

---
---
Empty front matter
---
"#;

    let headings = MarkdownElements::detect_headings(content);
    println!("Detected {} headings:", headings.len());
    for (i, heading) in headings.iter().enumerate() {
        println!(
            "  {}. Line {}-{}: '{}' (Level: {:?}, Quality: {:?})",
            i + 1,
            heading.start_line,
            heading.end_line,
            heading.text,
            heading.metadata,
            heading.quality
        );
    }
    assert_eq!(headings.len(), 7, "Should detect 7 empty headings");

    // Check that empty headings are recognized
    assert_eq!(headings[0].text, "", "First heading should be empty");
    assert_eq!(headings[1].text, "", "Second heading should be empty");
    assert_eq!(headings[2].text, "Empty heading", "Third heading should have content");

    // All are valid because they have proper spacing
    for heading in &headings {
        if heading.start_line == 3 {
            // The ## heading without space is considered malformed
            assert_eq!(
                heading.quality,
                ElementQuality::Malformed,
                "Heading without space is malformed"
            );
        } else {
            assert_eq!(heading.quality, ElementQuality::Valid, "Other headings should be valid");
        }
    }

    let empty_code_blocks = MarkdownElements::detect_code_blocks(content);
    assert_eq!(empty_code_blocks.len(), 2, "Should detect 2 empty code blocks");

    // Check that empty code blocks are detected and valid
    for block in &empty_code_blocks {
        assert_eq!(
            block.quality,
            ElementQuality::Valid,
            "Empty code blocks should be valid"
        );
    }

    let empty_lists = MarkdownElements::detect_lists(content);
    println!("Detected {} lists:", empty_lists.len());
    for (i, list) in empty_lists.iter().enumerate() {
        println!(
            "  {}. Line {}: '{}' (Marker: {:?}, Quality: {:?})",
            i + 1,
            list.start_line,
            list.text,
            list.metadata,
            list.quality
        );
    }
    assert_eq!(empty_lists.len(), 3, "Should detect 3 empty list items");

    // Check that list items with markers but no content are valid
    assert_eq!(empty_lists[0].metadata, Some("minus".to_string()));
    assert_eq!(empty_lists[0].quality, ElementQuality::Valid);

    assert_eq!(empty_lists[1].metadata, Some("asterisk".to_string()));
    assert_eq!(empty_lists[1].quality, ElementQuality::Valid);

    assert_eq!(empty_lists[2].metadata, Some("plus".to_string()));
    assert_eq!(empty_lists[2].quality, ElementQuality::Valid);

    // Empty front matter
    let front_matter = MarkdownElements::detect_front_matter(content);
    assert!(
        front_matter.is_none(),
        "Not valid front matter - doesn't start at beginning of document"
    );
}

#[test]
fn test_escaped_characters() {
    let content = r#"\# Not a heading

\- Not a list

\```
Not a code block
\```

\*Not emphasis\*

`\# Escaped inside code span`

    \# Indented code block
"#;

    let headings = MarkdownElements::detect_headings(content);
    assert_eq!(
        headings.len(),
        0,
        "No headings should be detected with escaped characters"
    );

    let code_blocks = MarkdownElements::detect_code_blocks(content);
    assert_eq!(
        code_blocks.len(),
        0,
        "No code blocks should be detected with escaped characters"
    );

    let lists = MarkdownElements::detect_lists(content);
    assert_eq!(lists.len(), 0, "No lists should be detected with escaped characters");
}

#[test]
fn test_unclosed_elements() {
    let content = r#"```javascript
function test() {
  console.log("This code block is not closed");
}

*This emphasis is not closed

`This code span is not closed
"#;

    let code_blocks = MarkdownElements::detect_code_blocks(content);
    assert_eq!(code_blocks.len(), 1, "Should detect the unclosed code block");
    assert_eq!(
        code_blocks[0].quality,
        ElementQuality::Malformed,
        "Unclosed code block should be malformed"
    );
    assert_eq!(
        code_blocks[0].metadata,
        Some("javascript".to_string()),
        "Language should be preserved"
    );
}

#[test]
fn test_performance_with_large_document() {
    // Generate a large document with many different elements
    let mut content = String::with_capacity(50_000);

    // Add 100 headings, lists and code blocks
    for i in 1..=100 {
        content.push_str(&format!("# Heading {i}\n\n"));
        content.push_str(&format!("Paragraph {i} with some content.\n\n"));

        // Add lists
        content.push_str(&format!("- List item {i}.1\n"));
        content.push_str(&format!("- List item {i}.2\n"));
        content.push_str(&format!("- List item {i}.3\n\n"));

        // Add code blocks
        content.push_str("```\n");
        content.push_str(&format!("Code block {i} content\n"));
        content.push_str("```\n\n");
    }

    // Measure performance
    let start = std::time::Instant::now();

    let headings = MarkdownElements::detect_headings(&content);
    let lists = MarkdownElements::detect_lists(&content);
    let code_blocks = MarkdownElements::detect_code_blocks(&content);

    let duration = start.elapsed();

    // Verify counts
    assert_eq!(headings.len(), 100, "Should detect 100 headings");
    assert_eq!(lists.len(), 300, "Should detect 300 list items");
    assert_eq!(code_blocks.len(), 100, "Should detect 100 code blocks");

    // Verify performance - should process in under 500ms
    assert!(
        duration.as_millis() < 500,
        "Processing should take less than 500ms, took {duration:?}"
    );
}
