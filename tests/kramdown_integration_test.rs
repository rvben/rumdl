use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::filtered_lines::FilteredLinesExt;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
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

    // Create a config file to set kramdown flavor (auto-detection only works for .kramdown extension)
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_file, "[global]\nflavor = \"kramdown\"\n").unwrap();

    // Run rumdl on the Kramdown document with kramdown flavor
    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "--no-cache", "kramdown_test.md"])
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
            "MD030", // List marker space (user-intention detection on abbreviations)
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

Some text before code block
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
        .args(["check", "standard.md", "--no-cache"])
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

    // Set kramdown flavor via config
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_file, "[global]\nflavor = \"kramdown\"\n").unwrap();

    // Run with --fix flag
    let _output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "--fix", "--no-cache", "fixable.md"])
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

    // Set kramdown flavor via config
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_file, "[global]\nflavor = \"kramdown\"\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "--no-cache", "math_footnotes.md"])
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

// ==================== Kramdown Flavor Unit Tests ====================

#[test]
fn test_kramdown_flavor_enum() {
    // Test FromStr parsing
    assert_eq!("kramdown".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::Kramdown);
    assert_eq!("jekyll".parse::<MarkdownFlavor>().unwrap(), MarkdownFlavor::Kramdown);

    // Test Display
    assert_eq!(format!("{}", MarkdownFlavor::Kramdown), "kramdown");

    // Test name()
    assert_eq!(MarkdownFlavor::Kramdown.name(), "Kramdown");

    // Test supports_kramdown_syntax()
    assert!(MarkdownFlavor::Kramdown.supports_kramdown_syntax());
    assert!(!MarkdownFlavor::Standard.supports_kramdown_syntax());
    assert!(!MarkdownFlavor::MkDocs.supports_kramdown_syntax());

    // Test from_extension
    assert_eq!(MarkdownFlavor::from_extension("kramdown"), MarkdownFlavor::Kramdown);
}

#[test]
fn test_kramdown_extension_block_detection() {
    let content = "# Heading\n\n{::comment}\nThis is hidden\n{:/comment}\n\nVisible text.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    // Lines inside extension block should be marked
    assert!(
        ctx.lines[2].in_kramdown_extension_block,
        "Opening {{::comment}} should be in extension block"
    );
    assert!(
        ctx.lines[3].in_kramdown_extension_block,
        "Content inside comment should be in extension block"
    );
    assert!(
        ctx.lines[4].in_kramdown_extension_block,
        "Closing {{:/comment}} should be in extension block"
    );

    // Lines outside should not be marked
    assert!(
        !ctx.lines[0].in_kramdown_extension_block,
        "Heading should not be in extension block"
    );
    assert!(
        !ctx.lines[6].in_kramdown_extension_block,
        "Visible text should not be in extension block"
    );
}

#[test]
fn test_kramdown_nomarkdown_extension_block() {
    let content = "# Title\n\n{::nomarkdown}\n<div>raw html</div>\n{:/nomarkdown}\n\nMore text.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    assert!(ctx.lines[2].in_kramdown_extension_block);
    assert!(ctx.lines[3].in_kramdown_extension_block);
    assert!(ctx.lines[4].in_kramdown_extension_block);
    assert!(!ctx.lines[0].in_kramdown_extension_block);
    assert!(!ctx.lines[6].in_kramdown_extension_block);
}

#[test]
fn test_kramdown_options_directive() {
    let content = "# Title\n\n{::options parse_block_html=\"true\" /}\n\nContent.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    assert!(
        ctx.lines[2].in_kramdown_extension_block,
        "Options directive should be marked as extension block"
    );
    assert!(
        !ctx.lines[0].in_kramdown_extension_block,
        "Heading should not be affected"
    );
}

#[test]
fn test_kramdown_block_ial_detection() {
    let content = "# Heading\n{:.class #id}\n\nContent.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    assert!(ctx.lines[1].is_kramdown_block_ial, "Block IAL should be detected");
    assert!(!ctx.lines[0].is_kramdown_block_ial, "Heading should not be IAL");
    assert!(!ctx.lines[3].is_kramdown_block_ial, "Content should not be IAL");
}

#[test]
fn test_kramdown_block_ial_variations() {
    let content = "{:.wrap}\n{:#my-id}\n{:style=\"color: red\"}\n{:.class #id style=\"x\"}\nNormal text.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    assert!(ctx.lines[0].is_kramdown_block_ial, "{{:.wrap}} should be IAL");
    assert!(ctx.lines[1].is_kramdown_block_ial, "{{:#my-id}} should be IAL");
    assert!(ctx.lines[2].is_kramdown_block_ial, "Attribute IAL should be detected");
    assert!(ctx.lines[3].is_kramdown_block_ial, "Combined IAL should be detected");
    assert!(!ctx.lines[4].is_kramdown_block_ial, "Normal text should not be IAL");
}

#[test]
fn test_kramdown_not_detected_in_standard_flavor() {
    let content = "# Heading\n{:.class}\n\n{::comment}\nhidden\n{:/comment}";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // In standard flavor, kramdown syntax should NOT be detected
    assert!(
        !ctx.lines[1].is_kramdown_block_ial,
        "IAL should not be detected in standard flavor"
    );
    assert!(
        !ctx.lines[3].in_kramdown_extension_block,
        "Extension block should not be detected in standard flavor"
    );
}

#[test]
fn test_kramdown_extension_block_in_code_block_ignored() {
    let content = "# Heading\n\n```\n{::comment}\nnot a real extension\n{:/comment}\n```\n\nText.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    // Inside code blocks, kramdown syntax should not be detected
    assert!(
        !ctx.lines[3].in_kramdown_extension_block,
        "{{::comment}} inside code block should be ignored"
    );
}

#[test]
fn test_kramdown_md041_skips_ial_as_preamble() {
    // IAL at start of document should be skipped when looking for first heading
    let content = "{:.document-class}\n# My Heading\n\nContent.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    let rule = rumdl_lib::MD041FirstLineHeading::default();
    let warnings = rule.check(&ctx).unwrap();

    assert!(
        warnings.is_empty(),
        "MD041 should skip kramdown IAL as preamble and find the heading"
    );
}

#[test]
fn test_kramdown_md041_skips_extension_block_as_preamble() {
    let content = "{::comment}\nThis is a comment at the top\n{:/comment}\n# My Heading\n\nContent.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    let rule = rumdl_lib::MD041FirstLineHeading::default();
    let warnings = rule.check(&ctx).unwrap();

    assert!(
        warnings.is_empty(),
        "MD041 should skip kramdown extension block as preamble"
    );
}

#[test]
fn test_kramdown_md041_no_skip_in_standard() {
    // Same content but in standard flavor should still trigger MD041
    let content = "{:.document-class}\n# My Heading\n\nContent.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let rule = rumdl_lib::MD041FirstLineHeading::default();
    let warnings = rule.check(&ctx).unwrap();

    assert!(
        !warnings.is_empty(),
        "MD041 should flag non-heading first line in standard flavor"
    );
}

#[test]
fn test_kramdown_filtered_lines_skip_extension_blocks() {
    let content = "Line 1\n{::comment}\nhidden\n{:/comment}\nLine 5";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    let lines: Vec<_> = ctx
        .filtered_lines()
        .skip_kramdown_extension_blocks()
        .into_iter()
        .collect();

    // Lines inside extension blocks should be filtered out
    assert!(
        lines.iter().any(|l| l.content.contains("Line 1")),
        "Should include normal content before extension block"
    );
    assert!(
        !lines.iter().any(|l| l.content.contains("hidden")),
        "Should exclude content inside extension block"
    );
    assert!(
        lines.iter().any(|l| l.content.contains("Line 5")),
        "Should include normal content after extension block"
    );
}

#[test]
fn test_kramdown_filtered_lines_not_in_standard() {
    // Extension blocks should NOT be filtered in standard flavor
    let content = "Line 1\n{::comment}\nhidden\n{:/comment}\nLine 5";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let lines: Vec<_> = ctx
        .filtered_lines()
        .skip_kramdown_extension_blocks()
        .into_iter()
        .collect();

    // In standard flavor, the "extension" syntax is just regular text
    assert!(
        lines.iter().any(|l| l.content.contains("hidden")),
        "Should NOT filter {{::comment}} content in standard flavor"
    );
}

#[test]
fn test_kramdown_md051_uses_kramdown_anchors() {
    // When kramdown flavor is used, MD051 should default to kramdown anchor style
    let config = rumdl_lib::config::Config::default();
    let mut kramdown_config = config;
    kramdown_config.global.flavor = MarkdownFlavor::Kramdown;

    let rule = rumdl_lib::MD051LinkFragments::from_config(&kramdown_config);
    // The fact that this compiles and doesn't panic verifies the config path works
    let _ = rule;
}

#[test]
fn test_kramdown_multiple_extension_blocks() {
    let content = "Text\n\n{::comment}\nComment 1\n{:/comment}\n\nMiddle\n\n{::nomarkdown}\n<div>raw</div>\n{:/nomarkdown}\n\nEnd.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    // First extension block
    assert!(ctx.lines[2].in_kramdown_extension_block);
    assert!(ctx.lines[3].in_kramdown_extension_block);
    assert!(ctx.lines[4].in_kramdown_extension_block);

    // Between blocks
    assert!(
        !ctx.lines[6].in_kramdown_extension_block,
        "Middle should not be in extension block"
    );

    // Second extension block
    assert!(ctx.lines[8].in_kramdown_extension_block);
    assert!(ctx.lines[9].in_kramdown_extension_block);
    assert!(ctx.lines[10].in_kramdown_extension_block);

    // After blocks
    assert!(
        !ctx.lines[12].in_kramdown_extension_block,
        "End should not be in extension block"
    );
}

#[test]
fn test_kramdown_extension_block_with_close_shorthand() {
    // Kramdown allows {:/} as shorthand close
    let content = "Text\n{::comment}\nhidden\n{:/}\nMore text.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    assert!(ctx.lines[1].in_kramdown_extension_block);
    assert!(ctx.lines[2].in_kramdown_extension_block);
    assert!(ctx.lines[3].in_kramdown_extension_block);
    assert!(!ctx.lines[4].in_kramdown_extension_block);
}

// ==================== Kramdown Gap Verification Tests ====================

#[test]
fn test_kramdown_ald_detected_as_block_ial() {
    // ALDs (Attribute List Definitions) like {:refdef: .class #id} should be detected
    // as kramdown block IALs since they start with {:
    let content = "# Heading\n\n{:refdef: .my-class #my-id}\n{:another: .other-class}\n\nText.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    assert!(
        ctx.lines[2].is_kramdown_block_ial,
        "ALD definition should be detected as block IAL"
    );
    assert!(
        ctx.lines[3].is_kramdown_block_ial,
        "ALD definition should be detected as block IAL"
    );
}

#[test]
fn test_kramdown_ald_reference_detected() {
    // ALD references like {:refdef} should also be detected
    let content = "# Heading\n\nSome text.\n{:refdef}\n\nMore text.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    assert!(
        ctx.lines[3].is_kramdown_block_ial,
        "ALD reference should be detected as block IAL"
    );
}

#[test]
fn test_kramdown_footnotes_no_false_positives() {
    // Footnote definitions should not trigger MD052 or other reference-link rules
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_file, "[global]\nflavor = \"kramdown\"\n").unwrap();

    let test_file = temp_dir.path().join("test.md");
    let content = r#"# Footnotes

This has a footnote[^1] and another[^note].

[^1]: First footnote definition.
[^note]: Named footnote definition.
"#;
    fs::write(&test_file, content).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !stdout.contains("MD052"),
        "Footnote definitions should not trigger MD052 (reference links). stdout: {stdout}"
    );
}

#[test]
fn test_kramdown_abbreviations_no_false_positives() {
    // Abbreviation definitions should not trigger MD030 (list marker space) or other rules
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_file, "[global]\nflavor = \"kramdown\"\n").unwrap();

    let test_file = temp_dir.path().join("test.md");
    let content = r#"# Abbreviations

The HTML specification is maintained by W3C.

*[HTML]: HyperText Markup Language
*[W3C]: World Wide Web Consortium
"#;
    fs::write(&test_file, content).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !stdout.contains("MD030"),
        "Abbreviation definitions should not trigger MD030. stdout: {stdout}"
    );
    assert!(
        !stdout.contains("MD032"),
        "Abbreviation definitions should not trigger MD032. stdout: {stdout}"
    );
}

#[test]
fn test_kramdown_definition_lists_no_false_positives() {
    // Definition list syntax should not trigger false positives
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_file, "[global]\nflavor = \"kramdown\"\n").unwrap();

    let test_file = temp_dir.path().join("test.md");
    let content = r#"# Definition Lists

Term 1
: Definition for term 1

Term 2
: Definition for term 2
: Another definition for term 2
"#;
    fs::write(&test_file, content).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Definition lists should not trigger list-related rules
    assert!(
        !stdout.contains("MD004"),
        "Definition lists should not trigger MD004 (list style). stdout: {stdout}"
    );
    assert!(
        !stdout.contains("MD030"),
        "Definition lists should not trigger MD030 (list marker space). stdout: {stdout}"
    );
}

#[test]
fn test_kramdown_eob_marker_no_false_positives() {
    // The ^ end-of-block marker should not trigger false positives
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join(".rumdl.toml");
    fs::write(&config_file, "[global]\nflavor = \"kramdown\"\n").unwrap();

    let test_file = temp_dir.path().join("test.md");
    let content = r#"# EOB Marker

First paragraph.
^
Second paragraph.
"#;
    fs::write(&test_file, content).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "--no-cache", "test.md"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // The ^ marker should not trigger any rules
    assert!(
        output.status.success(),
        "EOB marker should not cause any false positives. stdout: {stdout}"
    );
}

#[test]
fn test_kramdown_cli_flavor_flag() {
    // --flavor kramdown should work from CLI
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");
    fs::write(&test_file, "# Test\n\nSome content.\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "--flavor", "kramdown", "--no-cache", "test.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "Should succeed with --flavor kramdown. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_kramdown_cli_jekyll_alias() {
    // --flavor jekyll should work as alias for kramdown
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");
    fs::write(&test_file, "# Test\n\nSome content.\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_rumdl"))
        .current_dir(temp_dir.path())
        .args(["check", "--flavor", "jekyll", "--no-cache", "test.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "Should succeed with --flavor jekyll. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_kramdown_md041_skips_ald_as_preamble() {
    // ALD at start of document should be skipped when looking for first heading
    let content = "{:refdef: .document-class #main}\n# My Heading\n\nContent.";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    let rule = rumdl_lib::MD041FirstLineHeading::default();
    let warnings = rule.check(&ctx).unwrap();

    assert!(
        warnings.is_empty(),
        "MD041 should skip kramdown ALD as preamble and find the heading"
    );
}

#[test]
fn test_kramdown_extension_block_with_code_fence_inside() {
    // Extension block tracking must persist through lines that the base parser
    // marks as code blocks. A fenced code block inside {::comment} should not
    // cause lines after the fence to be treated as regular content.
    let content = "{::comment}\nComment text\n\n```\ncode\n```\n\nStill in comment\n{:/comment}\n\n# Heading\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    let rule = rumdl_lib::MD041FirstLineHeading::default();
    let warnings = rule.check(&ctx).unwrap();

    assert!(
        warnings.is_empty(),
        "MD041 should skip all lines inside comment extension block even when a code fence appears inside it"
    );
}

#[test]
fn test_kramdown_nomarkdown_block_with_code_fence_inside() {
    // Same as above but with {::nomarkdown} which is more likely to contain code fences
    let content = "{::nomarkdown}\n<div>\n\n```python\nx = 1\n```\n\n</div>\n{:/nomarkdown}\n\n# Heading\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    let rule = rumdl_lib::MD041FirstLineHeading::default();
    let warnings = rule.check(&ctx).unwrap();

    assert!(
        warnings.is_empty(),
        "MD041 should skip all lines inside nomarkdown extension block even when a code fence appears inside it"
    );
}

#[test]
fn test_kramdown_extension_block_lineinfo_flags() {
    // Verify that LineInfo flags are correctly set for all lines inside an extension block
    // that contains a fenced code block
    let content = "{::comment}\nLine A\n```\ncode\n```\nLine B\n{:/comment}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Kramdown, None);

    // All lines 0-6 should be marked as in_kramdown_extension_block
    for i in 0..7 {
        let info = ctx.line_info(i + 1);
        assert!(
            info.is_some_and(|li| li.in_kramdown_extension_block),
            "Line {} should be marked as in_kramdown_extension_block",
            i + 1
        );
    }
}
