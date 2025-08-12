use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_comprehensive_kramdown_document() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("kramdown_test.md");

    // A realistic Kramdown document that might be used in Jekyll
    let content = r#"---
title: Kramdown Test Document
---

# Main Header {#main-header}

This document tests **various Kramdown features**{:.emphasis} that should not trigger false positives.

{::comment}
This is a comment that won't be rendered
It might contain <div>HTML-like</div> content
{:/comment}

## Features Overview {#features}

### Block Attributes {#block-attrs}

```ruby
puts "Hello World"
```
{:.language-ruby .numberLines}

> This is a blockquote
> with multiple lines
{:#special-quote .highlighted}

### Inline Attributes

This has *emphasized text*{:.red} and **bold text**{:#important}.

You can also have [links](https://example.com){:target="_blank"} with attributes.

### Footnotes

This text has a footnote[^1] reference.

[^1]: This is the footnote content.
      It can span multiple lines.

### Abbreviations

The HTML specification is maintained by W3C.

*[HTML]: HyperText Markup Language
*[W3C]: World Wide Web Consortium

### Math Blocks

$$
\begin{align}
  f(x) &= x^2 + 2x + 1 \\
  &= (x + 1)^2
\end{align}
$$

Inline math: $a^2 + b^2 = c^2$ is the Pythagorean theorem.

### Definition Lists

Term 1
: Definition 1
: Another definition for term 1

Term 2
: Definition 2

### Extensions

{::nomarkdown}
<div class="custom">
  This HTML is intentionally allowed
  <span>Even nested tags</span>
</div>
{:/nomarkdown}

{::options parse_block_html="true" /}

### Tables with Attributes

| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |
| Cell 3   | Cell 4   |
{:.table-striped .table-bordered}

### End of Block Marker

First paragraph
^
Second paragraph starts a new block

### Mixed Content

1. List item with **bold**{:.strong}
2. Another item with `code`{:.code}
   - Nested with *emphasis*{:#nested-em}
   - More nesting

## Conclusion {#conclusion}

This document should lint cleanly with no false positives from Kramdown syntax.

<!-- rumdl-disable MD033 -->
<div>This HTML is explicitly allowed via inline config</div>
<!-- rumdl-enable MD033 -->
"#;

    fs::write(&test_file, content).unwrap();

    // Run rumdl on the Kramdown document
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "kramdown_test.md"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check for specific false positives that shouldn't occur
    assert!(
        !stdout.contains("MD026") || !stdout.contains("ends with punctuation"),
        "MD026 should not flag headers with Kramdown IDs"
    );

    assert!(
        !stdout.contains("MD037") || !stdout.contains("Spaces inside emphasis"),
        "MD037 should not flag emphasis with span IAL"
    );

    // MD033 should not flag HTML inside {::nomarkdown} blocks
    assert!(
        !stdout.contains("line 80") || !stdout.contains("MD033"),
        "MD033 should not flag HTML inside {{::nomarkdown}} blocks"
    );

    // MD033 should respect inline config comments
    assert!(
        !stdout.contains("line 114") || !stdout.contains("MD033"),
        "MD033 should respect inline config comments"
    );

    // The document might have some legitimate warnings (like MD041 for front matter)
    // but should not have Kramdown-specific false positives

    if !output.status.success() {
        println!("STDOUT:\n{stdout}");
        println!("STDERR:\n{stderr}");

        // Check if failures are only from acceptable rules
        let acceptable_patterns = [
            "MD041", // First line (front matter)
            "MD022", // Blank lines around headings
            "MD025", // Multiple top-level headings
            "MD013", // Line length
            "MD052", // Reference links (abbreviations look like broken references)
        ];

        let has_unexpected = stdout
            .lines()
            .filter(|line| line.contains("MD"))
            .any(|line| !acceptable_patterns.iter().any(|p| line.contains(p)));

        assert!(!has_unexpected, "Found unexpected rule violations in Kramdown document");
    }
}

#[test]
fn test_kramdown_does_not_affect_standard_markdown() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("standard.md");

    // Standard Markdown should still trigger appropriate warnings
    let content = r#"# Header with trailing punctuation.

This has * spaces in emphasis * that should be flagged.

<div>Regular HTML should be flagged</div>
```
Code block without blank line above
```

List with wrong marker:
* Item 1
+ Item 2
"#;

    fs::write(&test_file, content).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "standard.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should flag these issues
    assert!(stdout.contains("MD026"), "Should flag trailing punctuation");
    assert!(stdout.contains("MD037"), "Should flag spaces in emphasis");
    assert!(stdout.contains("MD033"), "Should flag HTML tags");
    assert!(stdout.contains("MD031"), "Should flag missing blank lines");
    assert!(stdout.contains("MD004"), "Should flag inconsistent list markers");
}

#[test]
fn test_kramdown_edge_cases() {
    let temp_dir = TempDir::new().unwrap();

    // Test 1: Brace-like content that isn't Kramdown
    let test_file1 = temp_dir.path().join("braces.md");
    let content1 = r#"# JavaScript Object {not: "kramdown"}

This uses {braces} but isn't Kramdown.

The formula {x | x > 0} represents positive numbers.
"#;

    fs::write(&test_file1, content1).unwrap();

    let output1 = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "braces.md"])
        .output()
        .unwrap();

    let stdout1 = String::from_utf8_lossy(&output1.stdout);

    // Should not cause Kramdown-related warnings for these braces
    assert!(
        !stdout1.contains("MD026") || !stdout1.contains("JavaScript Object"),
        "Non-Kramdown braces shouldn't trigger MD026"
    );

    // Test 2: Almost-Kramdown syntax
    let test_file2 = temp_dir.path().join("almost.md");
    let content2 = r#"# Header {#id needs hash}.

This is * almost IAL * {.missing-colon} text.

{:comment} Wrong format
Not a comment
{:/comment}
"#;

    fs::write(&test_file2, content2).unwrap();

    let output2 = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "almost.md"])
        .output()
        .unwrap();

    let stdout2 = String::from_utf8_lossy(&output2.stdout);

    // Should still catch the spaces in emphasis
    assert!(
        stdout2.contains("MD037"),
        "Should flag spaces in emphasis even with invalid IAL"
    );

    // Should flag the trailing period since the header ID is malformed
    assert!(
        stdout2.contains("MD026"),
        "Should flag trailing punctuation when header ID is malformed"
    );
}

#[test]
fn test_kramdown_with_fixes() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("fixable.md");

    let content = r#"# Header with punctuation. {#my-id}

## Another header! {#another-id}

Paragraph with * spaced emphasis *{:.highlight} text.

### Regular header with punctuation.
"#;

    fs::write(&test_file, content).unwrap();

    // Run with --fix flag
    let _output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "--fix", "fixable.md"])
        .output()
        .unwrap();

    // The command returns 1 if it found and fixed issues, 0 if no issues
    // Both are acceptable for this test

    // Read the fixed content
    let fixed = fs::read_to_string(&test_file).unwrap();

    // Verify Kramdown syntax is preserved
    assert!(fixed.contains("{#my-id}"), "Should preserve header ID");
    assert!(fixed.contains("{#another-id}"), "Should preserve another header ID");
    assert!(fixed.contains("{:.highlight}"), "Should preserve span IAL");

    // Verify the regular header was fixed (period removed)
    assert!(
        fixed.contains("### Regular header with punctuation")
            && !fixed.contains("### Regular header with punctuation."),
        "Should fix regular header without Kramdown ID"
    );
}

#[test]
fn test_kramdown_math_and_footnotes() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("math_footnotes.md");

    let content = r#"# Math and Footnotes Test

This equation[^1] shows the quadratic formula:

$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

Inline math like $E = mc^2$ should work too.

[^1]: The quadratic formula solves $ax^2 + bx + c = 0$.

More text with another footnote[^note].

[^note]: This is another footnote.
    With multiple lines.
    And code: `example()`
"#;

    fs::write(&test_file, content).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "math_footnotes.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Math blocks and footnotes shouldn't trigger unrelated rules
    assert!(
        !stdout.contains("MD031") || !stdout.contains("$$"),
        "Math blocks should not need blank lines like code blocks"
    );

    assert!(
        !stdout.contains("MD039") || !stdout.contains("[^"),
        "Footnote references should not be flagged as link issues"
    );
}
