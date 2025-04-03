use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

// Sample CommonMark specification examples for different Markdown elements
const COMMONMARK_ATX_HEADING: &str = "# Heading 1\n## Heading 2\n### Heading 3";
const COMMONMARK_SETEXT_HEADING: &str = "Heading 1\n=========\n\nHeading 2\n---------";
const COMMONMARK_LISTS: &str = "- Item 1\n- Item 2\n  - Nested item\n  - Another nested item\n- Item 3\n\n1. Ordered item 1\n2. Ordered item 2\n   1. Nested ordered item\n3. Ordered item 3";
const COMMONMARK_CODE_BLOCKS: &str = "```\nCode block without language\n```\n\n```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```";
const COMMONMARK_EMPHASIS: &str = "This is *emphasized* text and this is **strong** text.";
const COMMONMARK_LINKS: &str =
    "[Link](https://example.com) and [Reference link][ref]\n\n[ref]: https://example.org";
const COMMONMARK_IMAGES: &str =
    "![Alt text](image.png) and ![Referenced image][img]\n\n[img]: other-image.jpg";
const COMMONMARK_BLOCKQUOTES: &str =
    "> This is a blockquote\n> With multiple lines\n>\n> And a paragraph break";
const COMMONMARK_HTML: &str = "<div>\n  Some text in HTML\n</div>";

#[test]
fn test_rules_produce_commonmark_compliant_output() {
    let temp_dir = tempdir().unwrap();

    // Test all CommonMark elements
    validate_commonmark_compliance(COMMONMARK_ATX_HEADING, "headings-atx.md", temp_dir.path());
    validate_commonmark_compliance(
        COMMONMARK_SETEXT_HEADING,
        "headings-setext.md",
        temp_dir.path(),
    );
    validate_commonmark_compliance(COMMONMARK_LISTS, "lists.md", temp_dir.path());
    validate_commonmark_compliance(COMMONMARK_CODE_BLOCKS, "code-blocks.md", temp_dir.path());
    validate_commonmark_compliance(COMMONMARK_EMPHASIS, "emphasis.md", temp_dir.path());
    validate_commonmark_compliance(COMMONMARK_LINKS, "links.md", temp_dir.path());
    validate_commonmark_compliance(COMMONMARK_IMAGES, "images.md", temp_dir.path());
    validate_commonmark_compliance(COMMONMARK_BLOCKQUOTES, "blockquotes.md", temp_dir.path());
    validate_commonmark_compliance(COMMONMARK_HTML, "html.md", temp_dir.path());
}

/// Validate that applying all rules to CommonMark-compliant content produces valid CommonMark output
fn validate_commonmark_compliance(content: &str, filename: &str, dir_path: &std::path::Path) {
    let file_path: PathBuf = dir_path.join(filename);

    // Create the file with the test content
    fs::write(&file_path, content).unwrap();

    // Run rumdl with --fix to apply all rules
    let output = Command::cargo_bin("rumdl")
        .unwrap()
        .arg("--fix")
        .arg(file_path.to_str().unwrap())
        .output()
        .unwrap();

    // Check that the command produced output indicating it processed the file
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("STDOUT: {}", stdout);
    println!("STDERR: {}", stderr);

    // If there's an error message unrelated to rule violations, fail the test
    assert!(
        !stderr.contains("Error:"),
        "rumdl command error on file: {}: {}",
        filename,
        stderr
    );

    // Read the fixed content
    let fixed_content = fs::read_to_string(&file_path).expect("Failed to read fixed content");

    // Verify the fixed content maintains its essential structure
    verify_commonmark_elements(&fixed_content, filename);
}

/// Verify that the content maintains essential CommonMark elements after fixing
fn verify_commonmark_elements(content: &str, filename: &str) {
    match filename {
        "headings-atx.md" => {
            // Verify ATX headings are preserved
            assert!(content.contains("# Heading 1"), "Missing level 1 heading");
            assert!(content.contains("## Heading 2"), "Missing level 2 heading");
            assert!(content.contains("### Heading 3"), "Missing level 3 heading");
        }
        "headings-setext.md" => {
            // Verify Setext headings are preserved or converted to ATX (both valid CommonMark)
            assert!(
                content.contains("Heading 1"),
                "Missing level 1 heading content"
            );
            assert!(
                content.contains("Heading 2"),
                "Missing level 2 heading content"
            );
            // The format may be converted to ATX, so don't check for =======
        }
        "lists.md" => {
            // Verify lists are preserved
            assert!(content.contains("- Item 1"), "Missing unordered list items");
            assert!(content.contains("1. Ordered"), "Missing ordered list items");
            assert!(content.contains("- Nested"), "Missing nested list items");
        }
        "code-blocks.md" => {
            // Verify code blocks are preserved
            assert!(content.contains("```"), "Missing code block markers");
            assert!(
                content.contains("```rust"),
                "Missing code block with language"
            );
            assert!(content.contains("println!"), "Missing code block content");
        }
        "emphasis.md" => {
            // Simple content check for emphasis and strong
            assert!(content.contains("*emphasized*"), "Missing emphasis markers");
            assert!(content.contains("**strong**"), "Missing strong markers");
        }
        "links.md" => {
            // Check for link syntax
            assert!(content.contains("[Link]"), "Missing link text");
            assert!(content.contains("https://example.com"), "Missing link URL");
        }
        "images.md" => {
            // Check for image syntax
            assert!(content.contains("![Alt text]"), "Missing image alt text");
            assert!(content.contains("image.png"), "Missing image source");
        }
        "blockquotes.md" => {
            // Check for blockquote syntax
            assert!(
                content.contains("> This is a blockquote"),
                "Missing blockquote content"
            );
        }
        "html.md" => {
            // HTML may be removed by rules, so we don't assert about it
            // Just check that the file exists and has some content
            assert!(!content.is_empty(), "Empty content");
        }
        _ => {
            panic!("Unknown test file: {}", filename);
        }
    }
}

#[test]
fn test_rule_transformations_preserve_document_structure() {
    let temp_dir = tempdir().unwrap();
    let complex_markdown_path = temp_dir.path().join("complex.md");

    // Create complex Markdown with mixed elements
    let complex_markdown = r#"---
title: Complex Document
author: Test Author
---

# Main Heading

This paragraph has *emphasized* text and **strong** text. It also has 
a [link](https://example.com) and a ![image](test.png) with alt text.

## Secondary Heading

> This is a blockquote
> with multiple lines
> and a [link](https://example.org) inside it.

- List item 1
- List item 2
  - Nested item with *emphasis*
  - Nested item with `code span`
- List item 3 with a [link](https://example.net)

1. Ordered item 1
2. Ordered item 2
   ```rust
   fn main() {
       // This is a code block inside a list
       println!("Hello!");
   }
   ```
3. Ordered item 3

<div>Some HTML that might be removed</div>

Final paragraph with a footnote[^1] and a horizontal rule:

---

[^1]: This is a footnote.

[Reference link][ref] at the end.

[ref]: https://example.com/reference
"#;

    fs::write(&complex_markdown_path, complex_markdown).unwrap();

    // Run rumdl with --fix to apply all rules
    let output = Command::cargo_bin("rumdl")
        .unwrap()
        .arg("--fix")
        .arg(complex_markdown_path.to_str().unwrap())
        .output()
        .unwrap();

    // Print output for debugging
    println!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
    println!("STDERR: {}", String::from_utf8_lossy(&output.stderr));

    // Check that there was no critical error
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Error:"),
        "rumdl command error on complex markdown: {}",
        stderr
    );

    // Read the fixed content
    let fixed_content =
        fs::read_to_string(&complex_markdown_path).expect("Failed to read fixed content");

    // Verify structure preservation
    verify_document_structure(&fixed_content);
}

/// Verify that the complex document structure is preserved
fn verify_document_structure(content: &str) {
    // Verify headings
    assert!(content.contains("# Main Heading"), "Missing main heading");
    assert!(
        content.contains("## Secondary Heading"),
        "Missing secondary heading"
    );

    // Verify lists
    assert!(
        content.contains("- List item"),
        "Missing unordered list items"
    );
    assert!(content.contains("1. Ordered"), "Missing ordered list items");

    // Verify code blocks
    assert!(content.contains("```rust"), "Missing code block");
    assert!(content.contains("println!"), "Missing code block content");

    // Verify front matter is preserved or properly handled
    assert!(
        content.contains("title") && content.contains("author"),
        "Front matter content lost"
    );

    // Verify paragraph content
    assert!(
        content.contains("*emphasized*") && content.contains("**strong**"),
        "Paragraph content missing emphasis/strong"
    );

    // Verify links are preserved
    assert!(content.contains("https://example.com"), "Missing links");

    // Verify blockquotes
    assert!(content.contains('>'), "Missing blockquotes");

    // Verify horizontal rule
    assert!(
        content.contains("---") || content.contains("***") || content.contains("___"),
        "Missing horizontal rule"
    );
}
