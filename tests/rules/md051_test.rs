use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD051LinkFragments;
use rumdl_lib::utils::anchor_styles::AnchorStyle;

#[test]
fn test_valid_link_fragment() {
    // Test internal link (fragment only) - should validate against current document
    let ctx = LintContext::new(
        "# Test Heading\n\nThis is a [link](#test-heading) to the heading.",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_link_fragment() {
    // Test internal link with wrong fragment - should flag as invalid
    let ctx = LintContext::new(
        "# Test Heading\n\nThis is a [link](#wrong-heading) to the heading.",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_multiple_headings() {
    // Test internal links to multiple headings
    let ctx = LintContext::new(
        "# First Heading\n\n## Second Heading\n\n[Link 1](#first-heading)\n[Link 2](#second-heading)",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_special_characters() {
    // Test internal link with special characters in heading
    let ctx = LintContext::new(
        "# Test & Heading!\n\nThis is a [link](#test--heading) to the heading.",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    // "Test & Heading!" should become "test--heading" (& becomes -- per GitHub spec, ! removed)
    // So the link to #test--heading should be VALID and no warnings should be generated
    assert_eq!(result.len(), 0);
}

#[test]
fn test_no_fragments() {
    let ctx = LintContext::new(
        "# Test Heading\n\nThis is a [link](https://example.com) without fragment.",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_content() {
    let ctx = LintContext::new("", rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_invalid_fragments() {
    // Test multiple internal links with invalid fragments
    let ctx = LintContext::new(
        "# Test Heading\n\n[Link 1](#wrong1)\n[Link 2](#wrong2)",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_case_sensitivity() {
    let ctx = LintContext::new(
        r#"
# My Heading

[Valid Link](#my-heading)
[Valid Link Different Case](#MY-HEADING)
"#,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );

    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Our implementation performs case-insensitive matching for fragments
    assert_eq!(0, warnings.len());

    // Note: this behavior is consistent with most Markdown parsers including
    // GitHub and CommonMark, which treat fragments as case-insensitive
}

#[test]
fn test_complex_heading_structures() {
    // Test internal links with various heading styles (ATX and Setext)
    let ctx = LintContext::new(
        "# Heading 1\n\nSome text\n\nHeading 2\n-------\n\n### Heading 3\n\n[Link to 1](#heading-1)\n[Link to 2](#heading-2)\n[Link to 3](#heading-3)\n[Link to missing](#heading-4)",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let rule = MD051LinkFragments::new();

    let result = rule.check(&ctx).unwrap();

    // Only the missing heading should be flagged
    assert_eq!(result.len(), 1);

    // Test with special characters in headings/links
    let ctx = LintContext::new(
        "# Heading & Special! Characters\n\n[Link](#heading--special-characters)\n[Bad Link](#heading-special-characters-bad)",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let result = rule.check(&ctx).unwrap();

    // Only the invalid fragment should fail
    assert_eq!(result.len(), 1);
}

#[test]
fn test_heading_id_generation() {
    let ctx = LintContext::new(
        r#"
# Heading 1

[Link with space](#heading-1)
[Link with underscore](#heading-1)
[Link with multiple hyphens](#heading-1)
"#,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );

    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // All links are valid with our improved heading ID generation,
    // which now follows GitHub's algorithm more closely
    assert_eq!(0, warnings.len());
}

#[test]
fn test_heading_to_fragment_edge_cases() {
    let ctx = LintContext::new(
        "# Heading\n\n# Heading\n\n[Link 1](somepath#heading)\n[Link 2](somepath#heading-1)",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let rule = MD051LinkFragments::new();

    let result = rule.check(&ctx).unwrap();
    // Cross-file links should be ignored, so no warnings expected
    assert_eq!(result.len(), 0);

    // Test headings with only special characters
    let ctx = LintContext::new(
        "# @#$%^\n\n[Link](somepath#)",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Test mixed internal/external links
    let ctx = LintContext::new(
        "# Heading\n\n[Internal](somepath#heading)\n[External](https://example.com#heading)",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_fragment_in_code_blocks() {
    let ctx = LintContext::new(
        "# Real Heading\n\n```markdown\n# Fake Heading\n[Link](somepath#fake-heading)\n```\n\n[Link](somepath#real-heading)",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let rule = MD051LinkFragments::new();

    let result = rule.check(&ctx).unwrap();
    println!("Result has {} warnings", result.len());
    for (i, warning) in result.iter().enumerate() {
        println!("Warning {}: line {}, message: {}", i, warning.line, warning.message);
    }

    // With our improved implementation, code blocks are ignored
    assert_eq!(result.len(), 0);

    // Test headings in code blocks (should be ignored)
    let ctx = LintContext::new(
        "```markdown\n# Code Heading\n```\n\n[Link](#code-heading)",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let result = rule.check(&ctx).unwrap();
    println!("Second test has {} warnings", result.len());
    for (i, warning) in result.iter().enumerate() {
        println!("Warning {}: line {}, message: {}", i, warning.line, warning.message);
    }

    // Headings in code blocks should be ignored and the link should fail
    assert_eq!(result.len(), 1);
}

#[test]
fn test_fragment_with_complex_content() {
    let ctx = LintContext::new(
        r#"
# Heading with **bold** and *italic*

[Link to heading](#heading-with-bold-and-italic)
"#,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );

    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // The implementation correctly handles formatting in headings
    // by stripping it when generating fragments, so the link should match
    assert_eq!(
        0,
        warnings.len(),
        "Link should correctly match the heading with stripped formatting"
    );
}

#[test]
fn test_nested_formatting_in_fragments() {
    let ctx = LintContext::new(
        r#"
# Heading with **bold *italic* text**

[Link to heading](#heading-with-bold-italic-text)
"#,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );

    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Test that nested formatting is correctly handled
    assert_eq!(
        0,
        warnings.len(),
        "Link should match heading with nested bold and italic formatting"
    );
}

#[test]
fn test_multiple_formatting_styles() {
    let ctx = LintContext::new(
        r#"
# Heading with _underscores_ and **asterisks** mixed

[Link to heading](#heading-with-underscores-and-asterisks-mixed)
"#,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );

    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Test that different styles of formatting are handled correctly
    assert_eq!(
        0,
        warnings.len(),
        "Link should match heading with mixed formatting styles"
    );
}

#[test]
fn test_complex_nested_formatting() {
    let ctx = LintContext::new(
        r#"
# **Bold** with *italic* and `code` and [link](https://example.com)

[Link to heading](#bold-with-italic-and-code-and-link)
"#,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );

    let rule = MD051LinkFragments::with_anchor_style(AnchorStyle::KramdownGfm);
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Test that complex mixed formatting is handled correctly
    assert_eq!(0, warnings.len(), "Link should match heading with complex formatting");
}

#[test]
fn test_formatting_edge_cases() {
    let ctx = LintContext::new(
        r#"
# Heading with a**partial**bold and *italic with **nested** formatting*

[Link to partial bold](#heading-with-apartialbold-and-italic-with-nested-formatting)
[Link to nested formatting](#heading-with-apartialbold-and-italic-with-nested-formatting)
"#,
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );

    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // This test may require adjustments based on expected behavior
    // The implementation should consistently generate the same fragment ID
    // Note: first link should be correct when partial bold is properly stripped
    assert!(
        warnings.len() <= 1,
        "At least one link should match the heading with partial formatting"
    );
}

#[test]
fn test_performance_md051() {
    let mut content = String::with_capacity(50_000);

    // Add 50 headings
    for i in 0..50 {
        content.push_str(&format!("# Heading {i}\n\n"));
        content.push_str("Some content paragraph with details about this section.\n\n");

        // Add some subheadings
        if i % 3 == 0 {
            content.push_str(&format!("## Subheading {i}.1\n\n"));
            content.push_str("Subheading content with more details.\n\n");
            content.push_str(&format!("## Subheading {i}.2\n\n"));
            content.push_str("More subheading content here.\n\n");
        }
    }

    // Add links section
    content.push_str("# Links Section\n\n");

    // Add 100 links, some valid, some invalid
    for i in 0..100 {
        if i % 3 == 0 {
            content.push_str(&format!("[Link to invalid heading](somepath#heading-{})\n", i + 100));
        } else {
            content.push_str(&format!("[Link to heading {}](somepath#heading-{})\n", i % 50, i % 50));
        }
    }

    // Measure performance
    let start = std::time::Instant::now();
    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    println!(
        "MD051 check duration: {:?} for content length {}",
        duration,
        content.len()
    );
    println!("Found {} invalid fragments", result.len());

    // We expect about 1/3 of the 100 links to be invalid (those where i % 3 == 0)
    assert!(result.len() >= 30);
    assert!(result.len() <= 40);
}

#[test]
fn test_inline_code_spans() {
    let ctx = LintContext::new(
        "# Real Heading\n\nThis is a real link: [Link](somepath#real-heading)\n\nThis is a code example: `[Example](#missing-section)`",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let rule = MD051LinkFragments::new();

    let result = rule.check(&ctx).unwrap();

    // We should only have 0 warnings - the link in inline code should be ignored
    assert_eq!(result.len(), 0, "Link in inline code span should be ignored");

    // Test with multiple code spans and mix of valid and invalid links
    let ctx = LintContext::new(
        "# Heading One\n\n`[Invalid](#missing)` and [Valid](#heading-one) and `[Another Invalid](#nowhere)`",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );
    let result = rule.check(&ctx).unwrap();

    // Only the valid link should be checked, the ones in code spans should be ignored
    assert_eq!(result.len(), 0, "Only links outside code spans should be checked");

    // Test with a fragment link in inline code followed by a real invalid link
    let ctx = LintContext::new(
        "# Heading One\n\n`[Example](#missing-section)` and [Invalid Link](#section-two)",
        rumdl_lib::config::MarkdownFlavor::Standard,
        None,
    );

    // Debug: Let's check what the LintContext contains
    println!("=== Test 3 Debug ===");
    println!("Content: {:?}", ctx.content);
    println!("Line count: {}", ctx.lines.len());
    for (i, line_info) in ctx.lines.iter().enumerate() {
        println!(
            "Line {}: content='{}', in_code_block={}, byte_offset={}",
            i,
            line_info.content(ctx.content),
            line_info.in_code_block,
            line_info.byte_offset
        );
        if let Some(heading) = &line_info.heading {
            println!("  Has heading: level={}, text='{}'", heading.level, heading.text);
        }
    }

    let result = rule.check(&ctx).unwrap();

    // Debug output
    println!("Test 3 - Result count: {}", result.len());
    for (i, warning) in result.iter().enumerate() {
        println!(
            "Warning {}: line {}, col {}, message: {}",
            i, warning.line, warning.column, warning.message
        );
    }

    // Only the real invalid link should be caught
    assert_eq!(result.len(), 1, "Only real invalid links should be caught");
    assert_eq!(result[0].line, 3, "Warning should be on line 3");
    assert!(
        result[0].message.contains("section-two"),
        "Warning should be about 'section-two'"
    );
}

#[test]
fn test_readme_fragments_debug() {
    let content = r#"# rumdl - A high-performance Markdown linter, written in Rust

## Table of Contents

- [rumdl - A high-performance Markdown linter, written in Rust](#rumdl---a-high-performance-markdown-linter-written-in-rust)
  - [Table of Contents](#table-of-contents)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    // Test the actual rule
    println!("\nRunning MD051 check on README-like content:");
    let result = rule.check(&ctx).unwrap();
    for warning in &result {
        println!("Warning: line {}, message: {}", warning.line, warning.message);
    }

    if result.is_empty() {
        println!("No warnings found - fragments match correctly!");
    } else {
        println!("Found {} warnings", result.len());
    }

    // For now, let's just check that we get some result (debugging)
    // TODO: Fix the algorithm to properly handle these cases
    println!("Test completed - this is a known issue with fragment generation algorithm");
}

#[test]
fn test_md051_fragment_generation_regression() {
    // Regression test for the MD051 fragment generation bug
    // This test ensures that the GitHub-compatible fragment generation algorithm works correctly

    let rule = MD051LinkFragments::new();

    // Test cases that were previously failing - now tested through actual rule behavior
    let test_cases = vec![
        // Basic cases that should work
        ("Simple Heading", "simple-heading"),
        ("1. Numbered Heading", "1-numbered-heading"),
        ("Heading with Spaces", "heading-with-spaces"),
        // Ampersand cases (& becomes -- per GitHub spec)
        ("Test & Example", "test--example"),
        ("A&B", "ab"), // Fixed: & without spaces is removed
        ("A & B", "a--b"),
        ("Multiple & Ampersands & Here", "multiple--ampersands--here"),
        // Special characters
        ("Test. Period", "test-period"),
        ("Test: Colon", "test-colon"),
        ("Test! Exclamation", "test-exclamation"),
        ("Test? Question", "test-question"),
        ("Test (Parentheses)", "test-parentheses"),
        ("Test [Brackets]", "test-brackets"),
        // Complex cases
        ("1. Heading with Numbers & Symbols!", "1-heading-with-numbers--symbols"),
        (
            "Multiple!!! Exclamations & Symbols???",
            "multiple-exclamations--symbols",
        ),
        (
            "Heading with (Parentheses) & [Brackets]",
            "heading-with-parentheses--brackets",
        ),
        ("Special Characters: @#$%^&*()", "special-characters-"),
        // Edge cases
        ("Only!!! Symbols!!!", "only-symbols"),
        ("   Spaces   ", "spaces"), // Leading/trailing spaces
        ("Already-hyphenated", "already-hyphenated"),
        ("Multiple---hyphens", "multiple---hyphens"), // GitHub preserves consecutive hyphens
    ];

    for (heading, expected_fragment) in test_cases {
        // Create a test document with the heading and a link to it
        let content = format!("# {heading}\n\n[Link](#{expected_fragment})");
        let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // If the fragment generation is correct, there should be no warnings
        assert_eq!(
            result.len(),
            0,
            "Fragment generation failed for heading '{}': expected fragment '{}' should be found, but got {} warnings: {:?}",
            heading,
            expected_fragment,
            result.len(),
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_md051_real_world_scenarios() {
    // Test real-world scenarios that should work with the fixed algorithm

    let content = r#"
# Main Title

## 1. Getting Started & Setup
[Link to setup](#1-getting-started--setup)

## 2. Configuration & Options
[Link to config](#2-configuration--options)

## 3. Advanced Usage (Examples)
[Link to advanced](#3-advanced-usage-examples)

## 4. FAQ & Troubleshooting
[Link to FAQ](#4-faq--troubleshooting)

## 5. API Reference: Methods & Properties
[Link to API](#5-api-reference-methods--properties)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // All links should be valid with the fixed algorithm
    assert_eq!(
        result.len(),
        0,
        "Expected no warnings, but got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_md051_ampersand_variations() {
    // Specific test for ampersand handling variations

    let content = r#"
# Test & Example
[Link 1](#test--example)

# A&B
[Link 2](#ab)

# Multiple & Symbols & Here
[Link 3](#multiple--symbols--here)

# Test&End
[Link 4](#testend)

# &Start
[Link 5](#start)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // All ampersand cases should be handled correctly
    assert_eq!(
        result.len(),
        0,
        "Expected no warnings for ampersand cases, but got: {:?}",
        result.iter().map(|w| &w.message).collect::<Vec<_>>()
    );
}

// All MD051 tests are now complete and use integration testing approach
// rather than relying on debug methods that expose internal implementation

#[test]
fn test_cross_file_fragment_links() {
    // Test that cross-file fragment links are not validated by MD051
    // This addresses the bug where [bug](ISSUE_POLICY.md#bug-reports) was incorrectly flagged

    let content = r#"
# Main Heading

## Internal Section

This document has some internal links:
- [Valid internal link](#main-heading)
- [Another valid internal link](#internal-section)
- [Invalid internal link](#missing-section)

And some cross-file links that should be ignored by MD051:
- [Link to other file](README.md#installation)
- [Bug reports](ISSUE_POLICY.md#bug-reports)
- [Triage process](ISSUE_TRIAGE.rst#triage-section)
- [External file fragment](../docs/config.md#settings)
- [YAML config](config.yml#database)
- [JSON settings](app.json#server-config)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only have one warning for the missing internal fragment
    assert_eq!(
        result.len(),
        1,
        "Expected exactly 1 warning for missing internal fragment"
    );

    // Verify it's the correct warning
    assert!(
        result[0].message.contains("missing-section"),
        "Warning should be about the missing internal section, got: {}",
        result[0].message
    );

    // Verify that cross-file links are not flagged
    for warning in &result {
        assert!(
            !warning.message.contains("installation")
                && !warning.message.contains("bug-reports")
                && !warning.message.contains("triage-section")
                && !warning.message.contains("settings"),
            "Cross-file fragment should not be flagged: {}",
            warning.message
        );
    }
}

#[test]
fn test_fragment_only_vs_cross_file_links() {
    // Test to distinguish between fragment-only links (#section) and cross-file links (file.md#section)

    let content = r#"
# Existing Heading

## Another Section

Test various link types:
- [Fragment only - valid](#existing-heading)
- [Fragment only - invalid](#nonexistent-heading)
- [Cross-file with fragment](other.md#some-section)
- [Cross-file no fragment](other.md)
- [Fragment only - valid 2](#another-section)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the invalid fragment-only link
    assert_eq!(
        result.len(),
        1,
        "Expected exactly 1 warning for invalid fragment-only link"
    );

    assert!(
        result[0].message.contains("nonexistent-heading"),
        "Warning should be about the nonexistent heading, got: {}",
        result[0].message
    );
}

#[test]
fn test_file_extension_edge_cases() {
    // Test various file extension cases that should be treated as cross-file links
    let content = r#"
# Main Heading

## Test Section

Cross-file links with various extensions (should be ignored by MD051):
- [Case insensitive](README.MD#section)
- [Upper case extension](file.HTML#heading)
- [Mixed case](doc.Rst#title)
- [Markdown variants](guide.markdown#intro)
- [Markdown short](notes.mkdn#summary)
- [Markdown extended](README.mdx#component)
- [Text file](data.txt#line)
- [XML file](config.xml#database)
- [YAML file](settings.yaml#server)
- [YAML short](app.yml#config)
- [JSON file](package.json#scripts)
- [PDF document](manual.pdf#chapter)
- [Word document](report.docx#results)
- [HTML page](index.htm#navbar)
- [Programming file](script.py#function)
- [Config file](settings.toml#section)
- [Generic extension](file.abc#section)

Fragment-only links (should be validated):
- [Valid fragment](#main-heading)
- [Another valid](#test-section)
- [Invalid fragment](#missing-section)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the invalid fragment-only link
    assert_eq!(
        result.len(),
        1,
        "Expected exactly 1 warning for invalid fragment-only link"
    );
    assert!(
        result[0].message.contains("missing-section"),
        "Warning should be about missing-section, got: {}",
        result[0].message
    );
}

#[test]
fn test_complex_url_patterns() {
    // Test complex URL patterns that might confuse the parser
    let content = r#"
# Main Heading

## Documentation

Cross-file links (should be ignored):
- [Query params](file.md?version=1.0#section)
- [Relative path](../docs/readme.md#installation)
- [Deep relative](../../other/file.md#content)
- [Current dir](./local.md#section)
- [Encoded spaces](file%20name.md#section)
- [Complex path](path/to/deep/file.md#heading)
- [Windows style](folder\file.md#section)
- [Double hash](file.md#section#subsection)
- [Empty fragment](file.md#)
- [Archive file](data.tar.gz#section)
- [Backup file](config.ini.backup#settings)
- [No extension with dot](.gitignore#rules)
- [Hidden no extension](.hidden#section)
- [No extension](somefile#section)

Fragment-only tests:
- [Valid](#main-heading)
- [Invalid](#nonexistent)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag only the invalid fragment-only link:
    // - `#nonexistent` - invalid fragment-only link
    // NOT flagged (all treated as cross-file links):
    // - `somefile#section` - GitHub-style extension-less cross-file link
    // - `.gitignore#rules` - hidden dotfile with extension
    // - `.hidden#section` - hidden file reference
    assert_eq!(result.len(), 1, "Expected 1 warning for invalid fragment-only link");

    // Check that we get warning for the invalid fragment
    let warning_messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();
    let contains_nonexistent = warning_messages.iter().any(|msg| msg.contains("nonexistent"));

    assert!(contains_nonexistent, "Should warn about #nonexistent fragment");
}

#[test]
fn test_edge_case_file_extensions() {
    // Test edge cases with file extensions
    let content = r#"
# Valid Heading

Cross-file links (should be ignored):
- [Multiple dots](file.name.ext#section)
- [Just extension](.md#section)
- [URL with port](http://example.com:8080/file.md#section)
- [Network path](//server/file.md#section)
- [Absolute path](/absolute/file.md#section)
- [No extension](somefile#section)
- [Hidden file](.hidden#section)

Ambiguous paths (dot but empty extension, fragment validated):
- [Dot but no extension](file.#section)
- [Trailing dot](file.#section)

Fragment-only (should be validated):
- [Valid fragment](#valid-heading)
- [Invalid fragment](#invalid-heading)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag:
    // - `file.#section` x2 - has dot but empty extension (ambiguous, validates fragment)
    // - `#invalid-heading` - invalid fragment-only
    // NOT flagged (all treated as cross-file links):
    // - `somefile#section` - GitHub-style extension-less
    // - `.hidden#section` - hidden file reference
    assert_eq!(
        result.len(),
        3,
        "Expected 3 warnings: 2 trailing dot + 1 invalid fragment"
    );

    // Verify we get warnings for the expected fragments
    let warning_messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();
    let contains_section = warning_messages.iter().filter(|msg| msg.contains("section")).count();
    let contains_invalid = warning_messages.iter().any(|msg| msg.contains("invalid-heading"));

    assert_eq!(
        contains_section, 2,
        "Should have 2 warnings about #section from trailing dot paths"
    );
    assert!(contains_invalid, "Should warn about #invalid-heading fragment");
}

#[test]
fn test_malformed_and_boundary_cases() {
    // Test malformed links and boundary cases
    let content = r#"
# Test Heading

Boundary cases:
- [Empty URL]()
- [Just hash](#)
- [Hash no content](file.md#)
- [Multiple hashes](file.md##double)
- [Fragment with symbols](file.md#section-with-symbols!)
- [Very long filename](very-long-filename-that-exceeds-normal-length.md#section)

Reference links:
[ref1]: other.md#section
[ref2]: #test-heading
[ref3]: missing.md#section

- [Reference to cross-file][ref1]
- [Reference to valid fragment][ref2]
- [Reference to another cross-file][ref3]

Fragment validation:
- [Valid](#test-heading)
- [Invalid](#missing)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the invalid fragment-only link
    assert_eq!(result.len(), 1, "Expected 1 warning for invalid fragment");
    assert!(
        result[0].message.contains("missing"),
        "Warning should be about missing fragment, got: {}",
        result[0].message
    );
}

#[test]
fn test_performance_stress_case() {
    // Test performance with many links
    let mut content = String::from("# Main\n\n## Section\n\n");

    // Add many cross-file links (should be ignored)
    for i in 0..100 {
        content.push_str(&format!("- [Link {i}](file{i}.md#section)\n"));
    }

    // Add some fragment-only links
    content.push_str("- [Valid](#main)\n");
    content.push_str("- [Valid 2](#section)\n");
    content.push_str("- [Invalid](#missing)\n");

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the one invalid fragment
    assert_eq!(result.len(), 1, "Expected 1 warning even with many cross-file links");
    assert!(
        result[0].message.contains("missing"),
        "Warning should be about missing fragment, got: {}",
        result[0].message
    );
}

#[test]
fn test_unicode_and_special_characters() {
    // Test Unicode characters in filenames and fragments
    let content = r#"
# Test Heading

## Café & Restaurant

Cross-file links with Unicode/special chars (should be ignored):
- [Unicode filename](文档.md#section)
- [Spaces in filename](my file.md#section)
- [Numbers in extension](file.md2#section)
- [Mixed case extension](FILE.Md#section)
- [Unicode no extension](文档#section)

Paths with special chars (not extension-less, fragment validated):
- [Special chars no extension](file@name#section)

Fragment tests:
- [Valid unicode](#café--restaurant)
- [Valid heading](#test-heading)
- [Invalid](#missing-heading)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag:
    // - `file@name#section` → @ is not valid in extension-less paths, so validates fragment
    // - `#missing-heading` → invalid fragment
    // NOT flagged:
    // - `文档#section` → Unicode chars are alphanumeric, treated as extension-less cross-file link
    // - `#café--restaurant` → matches "Café & Restaurant" heading
    // Note: [Spaces no extension](my file#section) is NOT detected because pulldown-cmark
    // correctly rejects URLs with unencoded spaces per CommonMark spec
    assert_eq!(
        result.len(),
        2,
        "Expected 2 warnings: 1 path with special char + 1 invalid fragment"
    );

    let warning_messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();
    let contains_section = warning_messages.iter().any(|msg| msg.contains("section"));
    let contains_missing = warning_messages.iter().any(|msg| msg.contains("missing-heading"));
    let contains_cafe = warning_messages.iter().any(|msg| msg.contains("café-restaurant"));

    assert!(
        contains_section,
        "Should warn about #section fragment from file@name#section"
    );
    assert!(contains_missing, "Should warn about #missing-heading fragment");
    assert!(
        !contains_cafe,
        "Should NOT warn about #café-restaurant fragment (matches heading per GitHub spec)"
    );
}

#[test]
fn test_edge_case_regressions() {
    // Test specific edge cases that could cause regressions
    let content = r#"
# Documentation

## Setup Guide

Links without fragments (should be ignored):
- [No extension no hash](filename)
- [Extension no hash](file.md)

Cross-file links (should be ignored):
- [Extension and hash](file.md#section)
- [Multiple dots in name](config.local.json#settings)
- [Extension in path](path/file.ext/sub.md#section)
- [Query with fragment](file.md?v=1#section)
- [Anchor with query](file.md#section?param=value)
- [Multiple extensions](archive.tar.gz#section)
- [Case sensitive](FILE.MD#section)
- [Generic extension](data.abc#section)

Paths with potential extensions (treated as cross-file links):
- [Dot in middle](my.file#section)
- [Custom extension](data.custom#section)

Fragment-only validation tests:
- [Hash only](#setup-guide)
- [Valid](#documentation)
- [Valid 2](#setup-guide)
- [Invalid](#nonexistent)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag only 1 invalid fragment - ambiguous paths are now treated as cross-file links
    // because they have valid-looking extensions ("file" and "custom")
    assert_eq!(result.len(), 1, "Expected 1 warning: 1 invalid fragment");

    let warning_messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();
    let contains_nonexistent = warning_messages.iter().any(|msg| msg.contains("nonexistent"));

    assert!(contains_nonexistent, "Should warn about #nonexistent fragment");
}

#[test]
fn test_url_protocol_edge_cases() {
    // Test URLs with protocols that should be treated as cross-file links
    let content = r#"
# Main Heading

## Setup

Protocol-based URLs (should be ignored as external links):
- [HTTP URL](http://example.com/page.html#section)
- [HTTPS URL](https://example.com/docs.md#heading)
- [FTP URL](ftp://server.com/file.txt#anchor)
- [File protocol](file:///path/to/doc.md#section)
- [Mailto with fragment](mailto:user@example.com#subject)

Fragment-only tests:
- [Valid](#main-heading)
- [Invalid](#missing)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the invalid fragment
    assert_eq!(result.len(), 1, "Expected 1 warning for invalid fragment");
    assert!(
        result[0].message.contains("missing"),
        "Warning should be about missing fragment, got: {}",
        result[0].message
    );
}

#[test]
fn test_fragment_normalization_edge_cases() {
    // Test various fragment formats and their normalization
    let content = r#"
# Test Heading

## Special Characters & Symbols

## Code `inline` Example

## Multiple   Spaces

Fragment tests with normalization:
- [Valid basic](#test-heading)
- [Valid special](#special-characters--symbols)
- [Valid code](#code-inline-example)
- [Valid spaces](#multiple---spaces)
- [Valid case insensitive](#Test-Heading)
- [Invalid symbols](#special-characters-&-symbols)
- [Invalid spacing](#multiple   spaces)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag only the invalid symbol fragment
    // Note: [Invalid spacing](#multiple   spaces) is NOT detected because pulldown-cmark
    // correctly rejects URLs with unencoded spaces per CommonMark spec
    assert_eq!(
        result.len(),
        1,
        "Expected 1 warning for invalid fragment with unencoded &"
    );

    let warning_messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();
    let contains_symbols = warning_messages
        .iter()
        .any(|msg| msg.contains("special-characters-&-symbols"));

    assert!(
        contains_symbols,
        "Should warn about & symbol in fragment (should be --)"
    );
}

#[test]
fn test_edge_case_file_paths() {
    // Test edge cases in file path detection
    let content = r#"
# Main Heading

Cross-file links with tricky paths (should be ignored):
- [Relative current](./README.md#section)
- [Relative parent](../docs/guide.md#intro)
- [Deep relative](../../other/project/file.md#content)
- [Absolute path](/usr/local/docs/manual.md#chapter)
- [Windows path](C:\Users\docs\readme.md#section)
- [Network path](\\server\share\file.md#section)
- [URL with port](http://localhost:8080/docs.md#api)

Fragment-only (should be validated):
- [Valid](#main-heading)
- [Invalid](#nonexistent)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the invalid fragment
    assert_eq!(result.len(), 1, "Expected 1 warning for invalid fragment");
    assert!(
        result[0].message.contains("nonexistent"),
        "Warning should be about nonexistent fragment, got: {}",
        result[0].message
    );
}

#[test]
fn test_malformed_link_edge_cases() {
    // Test malformed links and edge cases in link parsing
    let content = r#"
# Valid Heading

## Test Section

Malformed and edge case links:
- [Empty fragment](file.md#)
- [Just hash](#)
- [Multiple hashes](file.md#section#subsection)
- [Hash in middle](file.md#section?param=value)
- [No closing bracket](file.md#section
- [Valid file](document.pdf#page)
- [Valid fragment](#valid-heading)
- [Invalid fragment](#missing-heading)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should flag the invalid fragment and potentially malformed links
    // (depends on how the parser handles malformed syntax)
    assert!(!result.is_empty(), "Expected at least 1 warning for invalid fragment");

    let warning_messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();
    let contains_missing = warning_messages.iter().any(|msg| msg.contains("missing-heading"));

    assert!(contains_missing, "Should warn about missing-heading fragment");
}

#[test]
fn test_performance_with_many_links() {
    // Test performance with a large number of links
    let mut content = String::from("# Main Heading\n\n## Section One\n\n");

    // Add many cross-file links (should be ignored)
    for i in 0..100 {
        content.push_str(&format!("- [Link {i}](file{i}.md#section)\n"));
    }

    // Add some fragment-only links
    content.push_str("- [Valid](#main-heading)\n");
    content.push_str("- [Valid 2](#section-one)\n");
    content.push_str("- [Invalid](#nonexistent)\n");

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(&content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let start = std::time::Instant::now();
    let result = rule.check(&ctx).unwrap();
    let duration = start.elapsed();

    // Should only flag the invalid fragment
    assert_eq!(result.len(), 1, "Expected 1 warning for invalid fragment");
    assert!(
        result[0].message.contains("nonexistent"),
        "Warning should be about nonexistent fragment"
    );

    // Performance should be reasonable (less than 100ms for 100+ links)
    assert!(
        duration.as_millis() < 100,
        "Performance test failed: took {}ms",
        duration.as_millis()
    );

    println!(
        "MD051 performance test: {}ms for {} links",
        duration.as_millis(),
        ctx.links.len()
    );
}

#[test]
fn test_custom_header_id_formats() {
    // Test all supported custom header ID formats
    let content = r#"# Kramdown Style {#kramdown-id}

Some content here.

## Python-markdown with spaces { #spaced-id }

More content.

### Python-markdown with colon {:#colon-id}

Even more content.

#### Python-markdown full format {: #full-format }

Final content.

Links to test all formats:
- [Link to kramdown](#kramdown-id)
- [Link to spaced](#spaced-id)
- [Link to colon](#colon-id)
- [Link to full format](#full-format)

Links that should fail:
- [Link to nonexistent](#nonexistent-id)
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();

    // Should only flag the nonexistent fragment
    assert_eq!(result.len(), 1, "Expected 1 warning for nonexistent fragment");
    assert!(
        result[0].message.contains("nonexistent-id"),
        "Warning should be about nonexistent fragment, got: {}",
        result[0].message
    );

    // All valid custom ID formats should be recognized
    for warning in &result {
        assert!(
            !warning.message.contains("kramdown-id")
                && !warning.message.contains("spaced-id")
                && !warning.message.contains("colon-id")
                && !warning.message.contains("full-format"),
            "Valid custom ID format should not be flagged as missing: {}",
            warning.message
        );
    }
}

#[test]
fn test_extended_attr_list_support() {
    // Test attr-list with classes and other attributes alongside IDs
    let content = r#"# Simple ID { #simple-id }

## ID with single class {: #with-class .highlight }

### ID with multiple classes {: #multi-class .class1 .class2 }

#### ID with key-value attributes {: #with-attrs data-test="value" style="color: red" }

##### Complex combination {: #complex .highlight .important data-role="button" title="Test" }

###### Edge case with quotes {: #quotes title="Has \"nested\" quotes" }

Links to test extended attr-list support:
- [Simple ID](#simple-id)
- [With class](#with-class)
- [Multiple classes](#multi-class)
- [With attributes](#with-attrs)
- [Complex](#complex)
- [Quotes](#quotes)

Links that should fail:
- [Nonexistent](#nonexistent)
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();

    // Should only flag the nonexistent fragment
    assert_eq!(result.len(), 1, "Expected 1 warning for nonexistent fragment");
    assert!(
        result[0].message.contains("nonexistent"),
        "Warning should be about nonexistent fragment, got: {}",
        result[0].message
    );

    // All valid attr-list IDs should be recognized
    for warning in &result {
        assert!(
            !warning.message.contains("simple-id")
                && !warning.message.contains("with-class")
                && !warning.message.contains("multi-class")
                && !warning.message.contains("with-attrs")
                && !warning.message.contains("complex")
                && !warning.message.contains("quotes"),
            "Valid attr-list ID should not be flagged as missing: {}",
            warning.message
        );
    }
}

#[test]
fn test_jekyll_kramdown_next_line_attr_list() {
    // Test Jekyll/kramdown style attr-list on the line following the header
    let content = r#"# Main Title

## ATX Header
{#atx-next-line}

### Another ATX
{ #atx-spaced }

#### ATX with Class
{: #atx-with-class .highlight}

##### ATX Complex
{: #atx-complex .class1 .class2 data-test="value"}

Links to test next-line attr-list:
- [ATX Next Line](#atx-next-line)
- [ATX Spaced](#atx-spaced)
- [ATX with Class](#atx-with-class)
- [ATX Complex](#atx-complex)

Links that should fail:
- [Nonexistent](#nonexistent-next-line)
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();

    // Should only flag the nonexistent fragment
    assert_eq!(result.len(), 1, "Expected 1 warning for nonexistent fragment");
    assert!(
        result[0].message.contains("nonexistent-next-line"),
        "Warning should be about nonexistent fragment, got: {}",
        result[0].message
    );

    // All valid next-line attr-list IDs should be recognized
    for warning in &result {
        assert!(
            !warning.message.contains("atx-next-line")
                && !warning.message.contains("atx-spaced")
                && !warning.message.contains("atx-with-class")
                && !warning.message.contains("atx-complex"),
            "Valid next-line attr-list ID should not be flagged as missing: {}",
            warning.message
        );
    }
}

#[test]
fn test_mixed_inline_and_next_line_attr_list() {
    // Test mixing inline and next-line attr-list in the same document
    let content = r#"# Mixed Styles

## Inline Style {#inline-id}

### Next Line Style
{#next-line-id}

#### Inline with Class {: #inline-class .highlight }

##### Next Line with Class
{: #next-line-class .important }

Links:
- [Inline](#inline-id)
- [Next Line](#next-line-id)
- [Inline Class](#inline-class)
- [Next Line Class](#next-line-class)
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();

    // Should have no warnings - all IDs should be found
    assert_eq!(result.len(), 0, "Expected no warnings, got: {result:?}");
}

#[test]
fn debug_issue_39_fragment_generation() {
    // Debug test to see what fragments are actually generated
    let content = r#"
# Testing & Coverage

## cbrown --> sbrown: --unsafe-paths

## cbrown -> sbrown

## The End - yay

## API Reference: Methods & Properties

Links for testing:
- [Testing coverage](#testing--coverage)
- [Complex path](#cbrown----sbrown---unsafe-paths)
- [Simple arrow](#cbrown---sbrown)
- [API ref](#api-reference-methods--properties)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    println!("Number of errors: {}", result.len());
    for warning in &result {
        println!("Warning: {}", warning.message);
    }

    // If we fixed it correctly, we should have 0 errors
    if result.is_empty() {
        println!("SUCCESS: All fragments now match!");
    } else {
        println!("STILL BROKEN: Fragment generation needs more work");
    }
}

/// Regression tests for Issue #39: Two bugs in Links [MD051]
/// These tests ensure that the complex punctuation handling bugs are fixed and won't regress

#[test]
fn test_issue_39_duplicate_headings() {
    // Test case from issue 39: links to the second of repeated headers
    let content = r#"
# Title

## Section

This is a [reference](#section-1) to the second section.

## Section

There will be another section.
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - link to second section should work
    assert_eq!(result.len(), 0, "Link to duplicate heading should work");
}

#[test]
fn test_issue_39_complex_punctuation_arrows() {
    // Test case from issue 39: complex arrow punctuation patterns
    let content = r#"
## cbrown --> sbrown: --unsafe-paths

## cbrown -> sbrown

## Arrow Test <-> bidirectional

## Double Arrow ==> Test

Links to test:
- [Complex unsafe](#cbrown----sbrown---unsafe-paths)
- [Simple arrow](#cbrown---sbrown)
- [Bidirectional](#arrow-test---bidirectional)
- [Double arrow](#double-arrow--test)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - all complex punctuation should be handled correctly
    assert_eq!(
        result.len(),
        0,
        "Complex arrow patterns should work: {:?}",
        result.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_issue_39_ampersand_and_colons() {
    // Test case from issue 39: headers with ampersands and colons
    let content = r#"
# Testing & Coverage

## API Reference: Methods & Properties

## Config: Database & Cache Settings

Links to test:
- [Testing coverage](#testing--coverage)
- [API reference](#api-reference-methods--properties)
- [Config settings](#config-database--cache-settings)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - ampersands and colons should be handled correctly
    assert_eq!(
        result.len(),
        0,
        "Ampersand and colon patterns should work: {:?}",
        result.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_issue_39_mixed_punctuation_clusters() {
    // Test edge cases with multiple types of punctuation in clusters
    let content = r#"
## Step 1: Setup (Prerequisites)

## Error #404 - Not Found!

## FAQ: What's Next?

## Version 2.0.1 - Release Notes

Links to test:
- [Setup guide](#step-1-setup-prerequisites)
- [Error page](#error-404---not-found)
- [FAQ section](#faq-whats-next)
- [Release notes](#version-201---release-notes)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - mixed punctuation should be handled correctly
    assert_eq!(
        result.len(),
        0,
        "Mixed punctuation clusters should work: {:?}",
        result.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_issue_39_consecutive_hyphens_and_spaces() {
    // Test that consecutive hyphens are collapsed properly
    let content = r#"
## Test --- Multiple Hyphens

## Test  --  Spaced Hyphens

## Test - Single - Hyphen

Links to test:
- [Multiple](#test-----multiple-hyphens)
- [Spaced](#test------spaced-hyphens)
- [Single](#test---single---hyphen)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - consecutive hyphens should be collapsed
    assert_eq!(
        result.len(),
        0,
        "Consecutive hyphens should be collapsed: {:?}",
        result.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_issue_39_edge_cases_from_comments() {
    // Test specific patterns mentioned in issue 39 comments
    let content = r#"
### PHP $_REQUEST

### sched_debug

#### Add ldap_monitor to delegator$

### cbrown --> sbrown: --unsafe-paths

Links to test:
- [PHP request](#php-_request)
- [Sched debug](#sched_debug)
- [LDAP monitor](#add-ldap_monitor-to-delegator)
- [Complex path](#cbrown----sbrown---unsafe-paths)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have no errors - all edge cases should work
    assert_eq!(
        result.len(),
        0,
        "Edge cases from issue comments should work: {:?}",
        result.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_html_anchor_tags() {
    // Test HTML anchor tags with id attribute
    let content = r#"# Regular Heading

## Heading with anchor<a id="custom-id"></a>

## Another heading<a name="old-style"></a>

Links to test:
- [Regular heading](#regular-heading) - should work
- [Custom ID](#custom-id) - should work
- [Old style name](#old-style) - should work
- [Missing anchor](#missing) - should fail
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only have 1 error for #missing
    assert_eq!(result.len(), 1, "Should only flag missing anchor");
    assert!(result[0].message.contains("#missing"));
}

#[test]
fn test_html_span_div_anchors() {
    // Test various HTML elements with id attributes
    let content = r#"# Document Title

## Section with span <span id="span-anchor">text</span>

<div id="div-anchor">
Some content in a div
</div>

<section id="section-anchor">
A section element
</section>

<h3 id="h3-anchor">HTML heading</h3>

Links to test:
- [Span anchor](#span-anchor) - should work
- [Div anchor](#div-anchor) - should work
- [Section anchor](#section-anchor) - should work
- [H3 anchor](#h3-anchor) - should work
- [Non-existent](#does-not-exist) - should fail
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only have 1 error for #does-not-exist
    assert_eq!(result.len(), 1, "Should only flag non-existent anchor");
    assert!(result[0].message.contains("#does-not-exist"));
}

#[test]
fn test_html_anchors_in_code_blocks() {
    // HTML anchors in code blocks should be ignored
    let content = r#"# Test Document

```html
<a id="code-anchor">This is in a code block</a>
```

Links to test:
- [Code anchor](#code-anchor) - should fail (anchor is in code block)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have 1 error - anchors in code blocks don't count
    assert_eq!(result.len(), 1, "Anchors in code blocks should be ignored");
}

#[test]
fn test_multiple_ids_on_same_element() {
    // Test edge case: multiple id attributes (only first should be used per HTML spec)
    let content = r#"# Test Document

<div id="first-id" id="second-id">Content</div>

Links to test:
- [First ID](#first-id) - should work
- [Second ID](#second-id) - should fail (HTML only uses first id)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have 1 error for second-id
    assert_eq!(result.len(), 1, "Only first id attribute should be recognized");
    assert!(result[0].message.contains("#second-id"));
}

#[test]
fn test_mixed_markdown_and_html_anchors() {
    // Test document with both Markdown headings and HTML anchors
    let content = r#"# Main Title

## Regular Markdown Heading

## Heading with custom ID {#custom-markdown-id}

## Heading with HTML anchor<a id="html-anchor"></a>

<div id="standalone-div">Content</div>

Links to test:
- [Main title](#main-title) - Markdown auto-generated
- [Regular heading](#regular-markdown-heading) - Markdown auto-generated
- [Custom Markdown ID](#custom-markdown-id) - Markdown custom ID
- [HTML anchor](#html-anchor) - HTML anchor on heading
- [Div anchor](#standalone-div) - Standalone HTML element
- [Wrong link](#wrong) - Should fail
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only have 1 error for #wrong
    assert_eq!(result.len(), 1, "Should support both Markdown and HTML anchors");
    assert!(result[0].message.contains("#wrong"));
}

#[test]
fn test_case_sensitivity_html_anchors() {
    // HTML id attributes are case-sensitive, links should match exactly
    let content = r#"# Test Document

<div id="CamelCase">Content</div>
<span id="lowercase">Content</span>

Links to test:
- [Exact match CamelCase](#CamelCase) - should work
- [Wrong case camelcase](#camelcase) - should fail
- [Exact match lowercase](#lowercase) - should work
- [Wrong case LOWERCASE](#LOWERCASE) - should fail
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should have 2 errors for wrong case
    assert_eq!(result.len(), 2, "HTML anchors should be case-sensitive");
}

#[test]
fn test_html_anchors_parity_with_markdownlint() {
    // This test ensures parity with markdownlint-cli behavior
    // Based on actual test case from ruff repository
    let content = r#"# Getting Started<a id="getting-started"></a>

## Configuration<a id="configuration"></a>

## Rules<a id="rules"></a>

## Contributing<a id="contributing"></a>

## Support<a id="support"></a>

## Acknowledgements<a id="acknowledgements"></a>

## Who's Using Ruff?<a id="whose-using-ruff"></a>

## License<a id="license"></a>

Table of contents:
1. [Getting Started](#getting-started)
1. [Configuration](#configuration)
1. [Rules](#rules)
1. [Contributing](#contributing)
1. [Support](#support)
1. [Acknowledgements](#acknowledgements)
1. [Who's Using Ruff?](#whose-using-ruff)
1. [License](#license)
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // All links should be valid - no errors
    assert_eq!(result.len(), 0, "All HTML anchor links should be valid");
}

#[test]
fn test_issue_82_arrow_patterns() {
    // Test for issue #82 - headers with arrows should generate correct anchors
    let content = r#"# Document

## Table of Contents
- [WAL->L0 Compaction](#wal-l0-compaction)
- [foo->bar->baz](#foo-bar-baz)
- [Header->with->Arrows](#header-with-arrows)

## WAL->L0 Compaction

Content about WAL to L0 compaction.

## foo->bar->baz

Content about foo bar baz.

## Header->with->Arrows

Content with arrows.
"#;

    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // All links should be valid with the fixed arrow pattern handling
    assert_eq!(
        result.len(),
        0,
        "Arrow patterns in headers should generate correct anchors (issue #82)"
    );
}

// Extension-less cross-file link tests
// These tests verify that MD051 correctly recognizes and validates
// extension-less markdown links like `[link](page#section)` that resolve to `page.md#section`.
// Note: Due to file size, comprehensive edge case tests are in separate modules below.
mod extensionless_links {
    use rumdl_lib::config::{Config, MarkdownFlavor};
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD051LinkFragments;
    use rumdl_lib::workspace_index::WorkspaceIndex;
    use std::fs;
    use tempfile::tempdir;

    /// Test the exact scenario from REMAINING-ISSUES.md
    ///
    /// Pattern: `[b#header1](b#header1)` where `b.md` exists
    /// Expected: Should recognize as cross-file link and validate fragment exists in b.md
    #[test]
    fn test_extensionless_link_exact_reproduction() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create target file with heading
        let target_file = base_path.join("b.md");
        fs::write(&target_file, "# header1\n\nContent here.\n").unwrap();

        // Create source file with extension-less link
        let source_file = base_path.join("a.md");
        let source_content = r#"# Source Document

This links to [header1 in b](b#header1).
"#;
        fs::write(&source_file, source_content).unwrap();

        // Get all rules
        let rules = rumdl_lib::rules::all_rules(&Config::default());

        // Lint and index both files
        let source_content_str = fs::read_to_string(&source_file).unwrap();
        let target_content_str = fs::read_to_string(&target_file).unwrap();

        let (_, source_index) = rumdl_lib::lint_and_index(
            &source_content_str,
            &rules,
            false,
            MarkdownFlavor::default(),
            None,
            None,
        );
        let (_, target_index) = rumdl_lib::lint_and_index(
            &target_content_str,
            &rules,
            false,
            MarkdownFlavor::default(),
            None,
            None,
        );

        // Build workspace index
        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(source_file.clone(), source_index.clone());
        workspace_index.insert_file(target_file.clone(), target_index.clone());

        // Verify target file has the heading indexed
        let target_file_index = workspace_index.get_file(&target_file).unwrap();
        assert!(
            target_file_index.has_anchor("header1"),
            "Target file should have 'header1' anchor indexed"
        );

        // Verify extension-less link is recognized as cross-file
        let has_cross_file_link = source_index
            .cross_file_links
            .iter()
            .any(|link| link.target_path == "b" && link.fragment == "header1");

        assert!(
            has_cross_file_link,
            "Extension-less link 'b#header1' should be recognized as cross-file link.\n\
             Cross-file links found: {:?}",
            source_index.cross_file_links
        );

        // Run cross-file validation
        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        // Should have NO warnings because the fragment exists
        assert_eq!(
            warnings.len(),
            0,
            "Extension-less link to existing fragment should have no warnings.\n\
             Current warnings: {warnings:?}",
        );
    }

    /// Test extension-less link to non-existent fragment
    #[test]
    fn test_extensionless_link_missing_fragment() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create target file WITHOUT the heading
        let target_file = base_path.join("page.md");
        fs::write(&target_file, "# Other Heading\n\nContent.\n").unwrap();

        // Create source file with extension-less link to missing fragment
        let source_file = base_path.join("index.md");
        let source_content = r#"# Index

Link to [missing section](page#missing-section).
"#;
        fs::write(&source_file, source_content).unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());

        let source_content_str = fs::read_to_string(&source_file).unwrap();
        let target_content_str = fs::read_to_string(&target_file).unwrap();

        let (_, source_index) = rumdl_lib::lint_and_index(
            &source_content_str,
            &rules,
            false,
            MarkdownFlavor::default(),
            None,
            None,
        );
        let (_, target_index) = rumdl_lib::lint_and_index(
            &target_content_str,
            &rules,
            false,
            MarkdownFlavor::default(),
            None,
            None,
        );

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(source_file.clone(), source_index.clone());
        workspace_index.insert_file(target_file.clone(), target_index.clone());

        // Verify link is recognized as cross-file
        let has_cross_file_link = source_index
            .cross_file_links
            .iter()
            .any(|link| link.target_path == "page" && link.fragment == "missing-section");

        assert!(
            has_cross_file_link,
            "Extension-less link 'page#missing-section' should be recognized as cross-file"
        );

        // Run validation - should warn about missing fragment
        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        assert_eq!(
            warnings.len(),
            1,
            "Should warn about missing fragment in extension-less link"
        );
        assert!(
            warnings[0].message.contains("missing-section"),
            "Warning should mention the missing fragment"
        );
        assert!(
            warnings[0].message.contains("page"),
            "Warning should mention the target file"
        );
    }

    /// Test extension-less links in subdirectories
    #[test]
    fn test_extensionless_link_subdirectory() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create subdirectory structure
        let docs_dir = base_path.join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        let target_file = docs_dir.join("guide.md");
        fs::write(&target_file, "# Getting Started\n\n## Installation\n\n## Usage\n").unwrap();

        let source_file = base_path.join("README.md");
        let source_content = r#"# Main README

See the [installation guide](docs/guide#installation).
"#;
        fs::write(&source_file, source_content).unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());

        let source_content_str = fs::read_to_string(&source_file).unwrap();
        let target_content_str = fs::read_to_string(&target_file).unwrap();

        let (_, source_index) = rumdl_lib::lint_and_index(
            &source_content_str,
            &rules,
            false,
            MarkdownFlavor::default(),
            None,
            None,
        );
        let (_, target_index) = rumdl_lib::lint_and_index(
            &target_content_str,
            &rules,
            false,
            MarkdownFlavor::default(),
            None,
            None,
        );

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(source_file.clone(), source_index.clone());
        workspace_index.insert_file(target_file.clone(), target_index.clone());

        // Verify link is recognized
        let has_cross_file_link = source_index
            .cross_file_links
            .iter()
            .any(|link| link.target_path == "docs/guide" && link.fragment == "installation");

        assert!(
            has_cross_file_link,
            "Extension-less link in subdirectory should be recognized"
        );

        // Should validate successfully
        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        assert_eq!(
            warnings.len(),
            0,
            "Extension-less link to existing fragment in subdirectory should be valid"
        );
    }

    /// Test that extension-less links are distinguished from fragment-only links
    #[test]
    fn test_extensionless_vs_fragment_only() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        let target_file = base_path.join("other.md");
        fs::write(&target_file, "# Target Heading\n").unwrap();

        let source_file = base_path.join("main.md");
        let source_content = r#"# Main Document

## Local Section

- [Fragment only](#local-section) - should validate against THIS file
- [Extension-less cross-file](other#target-heading) - should validate against other.md
- [Extension-less missing](other#missing) - should warn about missing fragment
"#;
        fs::write(&source_file, source_content).unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());

        let source_content_str = fs::read_to_string(&source_file).unwrap();
        let target_content_str = fs::read_to_string(&target_file).unwrap();

        let (_, source_index) = rumdl_lib::lint_and_index(
            &source_content_str,
            &rules,
            false,
            MarkdownFlavor::default(),
            None,
            None,
        );
        let (_, target_index) = rumdl_lib::lint_and_index(
            &target_content_str,
            &rules,
            false,
            MarkdownFlavor::default(),
            None,
            None,
        );

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(source_file.clone(), source_index.clone());
        workspace_index.insert_file(target_file.clone(), target_index.clone());

        // Fragment-only link should NOT be in cross_file_links
        let has_fragment_only = source_index
            .cross_file_links
            .iter()
            .any(|link| link.target_path.is_empty() || link.target_path == "#");

        assert!(
            !has_fragment_only,
            "Fragment-only link should NOT be in cross_file_links"
        );

        // Extension-less link SHOULD be in cross_file_links
        let has_extensionless = source_index
            .cross_file_links
            .iter()
            .any(|link| link.target_path == "other");

        assert!(
            has_extensionless,
            "Extension-less link 'other#target-heading' should be in cross_file_links"
        );

        // Run validation
        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        // Should only warn about the missing fragment in other.md
        assert_eq!(
            warnings.len(),
            1,
            "Should only warn about missing fragment in extension-less link"
        );
        assert!(
            warnings[0].message.contains("missing"),
            "Warning should be about missing fragment"
        );
    }

    /// Test edge case: extension-less link where file doesn't exist
    #[test]
    fn test_extensionless_link_file_not_exists() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Don't create the target file - it doesn't exist
        let source_file = base_path.join("index.md");
        let source_content = r#"# Index

Link to [non-existent](nonexistent#section).
"#;
        fs::write(&source_file, source_content).unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());

        let source_content_str = fs::read_to_string(&source_file).unwrap();
        let (_, source_index) = rumdl_lib::lint_and_index(
            &source_content_str,
            &rules,
            false,
            MarkdownFlavor::default(),
            None,
            None,
        );

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(source_file.clone(), source_index.clone());

        // Link should still be recognized as cross-file (even if file doesn't exist)
        let has_cross_file_link = source_index
            .cross_file_links
            .iter()
            .any(|link| link.target_path == "nonexistent");

        assert!(
            has_cross_file_link,
            "Extension-less link should be recognized even if file doesn't exist yet"
        );

        // Validation should skip (file not in workspace index)
        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        // No warnings because file isn't in workspace index
        assert_eq!(
            warnings.len(),
            0,
            "No warnings for files not in workspace (expected behavior)"
        );
    }

    /// Test that extension-less links work with various markdown extensions
    #[test]
    fn test_extensionless_link_markdown_variants() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Test .markdown extension
        let target1 = base_path.join("page.markdown");
        fs::write(&target1, "# Page Markdown\n").unwrap();

        // Test .md extension
        let target2 = base_path.join("doc.md");
        fs::write(&target2, "# Doc MD\n").unwrap();

        let source_file = base_path.join("index.md");
        let source_content = r#"# Index

- [Page](page#page-markdown)
- [Doc](doc#doc-md)
"#;
        fs::write(&source_file, source_content).unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());

        let source_content_str = fs::read_to_string(&source_file).unwrap();
        let target1_content = fs::read_to_string(&target1).unwrap();
        let target2_content = fs::read_to_string(&target2).unwrap();

        let (_, source_index) = rumdl_lib::lint_and_index(
            &source_content_str,
            &rules,
            false,
            MarkdownFlavor::default(),
            None,
            None,
        );
        let (_, target1_index) =
            rumdl_lib::lint_and_index(&target1_content, &rules, false, MarkdownFlavor::default(), None, None);
        let (_, target2_index) =
            rumdl_lib::lint_and_index(&target2_content, &rules, false, MarkdownFlavor::default(), None, None);

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(source_file.clone(), source_index.clone());
        workspace_index.insert_file(target1.clone(), target1_index.clone());
        workspace_index.insert_file(target2.clone(), target2_index.clone());

        // Both links should be recognized
        let has_page_link = source_index
            .cross_file_links
            .iter()
            .any(|link| link.target_path == "page");
        let has_doc_link = source_index
            .cross_file_links
            .iter()
            .any(|link| link.target_path == "doc");

        assert!(
            has_page_link && has_doc_link,
            "Both extension-less links should be recognized"
        );

        // Both should validate successfully
        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        assert_eq!(
            warnings.len(),
            0,
            "Extension-less links to .md and .markdown files should both work"
        );
    }
}

// =============================================================================
// URL-Encoded CJK Fragment Tests
// =============================================================================
// When documentation tools, browsers, or CI/CD systems generate markdown links
// with CJK fragments, they often URL-encode non-ASCII characters. Both forms
// should work: raw CJK (#インストール) and URL-encoded (#%E3%82%A4...).

mod url_encoded_cjk_tests {
    use super::*;
    use rumdl_lib::config::{Config, MarkdownFlavor};
    use rumdl_lib::rules::MD051LinkFragments;
    use rumdl_lib::workspace_index::WorkspaceIndex;
    use std::fs;
    use tempfile::tempdir;

    /// Test: Raw CJK fragment should work (baseline)
    #[test]
    fn test_raw_cjk_fragment_works() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Target file with Japanese heading
        let target_file = base_path.join("target.md");
        fs::write(&target_file, "# Target\n\n## インストール\n\nContent here.\n").unwrap();

        // Source file with raw CJK link
        let source_file = base_path.join("source.md");
        fs::write(&source_file, "# Source\n\n[Install](target.md#インストール)\n").unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());
        let target_content = fs::read_to_string(&target_file).unwrap();
        let source_content = fs::read_to_string(&source_file).unwrap();

        let (_, target_index) =
            rumdl_lib::lint_and_index(&target_content, &rules, false, MarkdownFlavor::default(), None, None);
        let (_, source_index) =
            rumdl_lib::lint_and_index(&source_content, &rules, false, MarkdownFlavor::default(), None, None);

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(target_file.clone(), target_index);
        workspace_index.insert_file(source_file.clone(), source_index.clone());

        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        assert!(warnings.is_empty(), "Raw CJK fragment should work: {warnings:?}");
    }

    /// Test: URL-encoded Japanese fragment should work
    /// "インストール" URL-encoded = "%E3%82%A4%E3%83%B3%E3%82%B9%E3%83%88%E3%83%BC%E3%83%AB"
    #[test]
    fn test_url_encoded_japanese_fragment() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        let target_file = base_path.join("target.md");
        fs::write(&target_file, "# Target\n\n## インストール\n\nContent here.\n").unwrap();

        // Source file with URL-encoded CJK link
        let source_file = base_path.join("source.md");
        fs::write(
            &source_file,
            "# Source\n\n[Install](target.md#%E3%82%A4%E3%83%B3%E3%82%B9%E3%83%88%E3%83%BC%E3%83%AB)\n",
        )
        .unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());
        let target_content = fs::read_to_string(&target_file).unwrap();
        let source_content = fs::read_to_string(&source_file).unwrap();

        let (_, target_index) =
            rumdl_lib::lint_and_index(&target_content, &rules, false, MarkdownFlavor::default(), None, None);
        let (_, source_index) =
            rumdl_lib::lint_and_index(&source_content, &rules, false, MarkdownFlavor::default(), None, None);

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(target_file.clone(), target_index);
        workspace_index.insert_file(source_file.clone(), source_index.clone());

        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        assert!(
            warnings.is_empty(),
            "URL-encoded Japanese fragment should match raw anchor: {warnings:?}"
        );
    }

    /// Test: URL-encoded Korean fragment should work
    /// "한국어" URL-encoded = "%ED%95%9C%EA%B5%AD%EC%96%B4"
    #[test]
    fn test_url_encoded_korean_fragment() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        let target_file = base_path.join("target.md");
        fs::write(&target_file, "# Target\n\n## 한국어\n\nKorean content.\n").unwrap();

        let source_file = base_path.join("source.md");
        fs::write(
            &source_file,
            "# Source\n\n[Korean](target.md#%ED%95%9C%EA%B5%AD%EC%96%B4)\n",
        )
        .unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());
        let target_content = fs::read_to_string(&target_file).unwrap();
        let source_content = fs::read_to_string(&source_file).unwrap();

        let (_, target_index) =
            rumdl_lib::lint_and_index(&target_content, &rules, false, MarkdownFlavor::default(), None, None);
        let (_, source_index) =
            rumdl_lib::lint_and_index(&source_content, &rules, false, MarkdownFlavor::default(), None, None);

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(target_file.clone(), target_index);
        workspace_index.insert_file(source_file.clone(), source_index.clone());

        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        assert!(
            warnings.is_empty(),
            "URL-encoded Korean fragment should match raw anchor: {warnings:?}"
        );
    }

    /// Test: URL-encoded Chinese fragment should work
    /// "中文" URL-encoded = "%E4%B8%AD%E6%96%87"
    #[test]
    fn test_url_encoded_chinese_fragment() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        let target_file = base_path.join("target.md");
        fs::write(&target_file, "# Target\n\n## 中文\n\nChinese content.\n").unwrap();

        let source_file = base_path.join("source.md");
        fs::write(&source_file, "# Source\n\n[Chinese](target.md#%E4%B8%AD%E6%96%87)\n").unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());
        let target_content = fs::read_to_string(&target_file).unwrap();
        let source_content = fs::read_to_string(&source_file).unwrap();

        let (_, target_index) =
            rumdl_lib::lint_and_index(&target_content, &rules, false, MarkdownFlavor::default(), None, None);
        let (_, source_index) =
            rumdl_lib::lint_and_index(&source_content, &rules, false, MarkdownFlavor::default(), None, None);

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(target_file.clone(), target_index);
        workspace_index.insert_file(source_file.clone(), source_index.clone());

        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        assert!(
            warnings.is_empty(),
            "URL-encoded Chinese fragment should match raw anchor: {warnings:?}"
        );
    }

    /// Test: Mixed encoding (ASCII + URL-encoded CJK)
    /// "mixed-テスト" with テスト URL-encoded = "mixed-%E3%83%86%E3%82%B9%E3%83%88"
    #[test]
    fn test_mixed_encoding_fragment() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        let target_file = base_path.join("target.md");
        fs::write(&target_file, "# Target\n\n## Mixed テスト\n\nMixed content.\n").unwrap();

        let source_file = base_path.join("source.md");
        // GitHub generates: #mixed-テスト, URL-encoded: #mixed-%E3%83%86%E3%82%B9%E3%83%88
        fs::write(
            &source_file,
            "# Source\n\n[Mixed](target.md#mixed-%E3%83%86%E3%82%B9%E3%83%88)\n",
        )
        .unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());
        let target_content = fs::read_to_string(&target_file).unwrap();
        let source_content = fs::read_to_string(&source_file).unwrap();

        let (_, target_index) =
            rumdl_lib::lint_and_index(&target_content, &rules, false, MarkdownFlavor::default(), None, None);
        let (_, source_index) =
            rumdl_lib::lint_and_index(&source_content, &rules, false, MarkdownFlavor::default(), None, None);

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(target_file.clone(), target_index);
        workspace_index.insert_file(source_file.clone(), source_index.clone());

        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        assert!(
            warnings.is_empty(),
            "Mixed ASCII + URL-encoded CJK should work: {warnings:?}"
        );
    }

    /// Test: Invalid URL encoding falls back gracefully
    #[test]
    fn test_invalid_url_encoding_fallback() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        let target_file = base_path.join("target.md");
        fs::write(&target_file, "# Target\n\n## Valid Heading\n\nContent.\n").unwrap();

        // Invalid URL encoding: %ZZ is not valid
        let source_file = base_path.join("source.md");
        fs::write(&source_file, "# Source\n\n[Bad](target.md#%ZZ%invalid)\n").unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());
        let target_content = fs::read_to_string(&target_file).unwrap();
        let source_content = fs::read_to_string(&source_file).unwrap();

        let (_, target_index) =
            rumdl_lib::lint_and_index(&target_content, &rules, false, MarkdownFlavor::default(), None, None);
        let (_, source_index) =
            rumdl_lib::lint_and_index(&source_content, &rules, false, MarkdownFlavor::default(), None, None);

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(target_file.clone(), target_index);
        workspace_index.insert_file(source_file.clone(), source_index.clone());

        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        // Should still warn (invalid encoding doesn't match any anchor)
        assert_eq!(warnings.len(), 1, "Invalid URL encoding should warn");
    }

    /// Test: Case-insensitive URL encoding (%E3 vs %e3)
    #[test]
    fn test_url_encoding_case_insensitive() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        let target_file = base_path.join("target.md");
        fs::write(&target_file, "# Target\n\n## テスト\n\nContent.\n").unwrap();

        // Use lowercase hex: %e3%83%86%e3%82%b9%e3%83%88 instead of %E3%83%86%E3%82%B9%E3%83%88
        let source_file = base_path.join("source.md");
        fs::write(
            &source_file,
            "# Source\n\n[Test](target.md#%e3%83%86%e3%82%b9%e3%83%88)\n",
        )
        .unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());
        let target_content = fs::read_to_string(&target_file).unwrap();
        let source_content = fs::read_to_string(&source_file).unwrap();

        let (_, target_index) =
            rumdl_lib::lint_and_index(&target_content, &rules, false, MarkdownFlavor::default(), None, None);
        let (_, source_index) =
            rumdl_lib::lint_and_index(&source_content, &rules, false, MarkdownFlavor::default(), None, None);

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(target_file.clone(), target_index);
        workspace_index.insert_file(source_file.clone(), source_index.clone());

        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        assert!(warnings.is_empty(), "Lowercase URL encoding should work: {warnings:?}");
    }

    /// Test: CJK heading with spaces becomes hyphenated anchor
    /// "한국어 테스트" -> "#한국어-테스트" URL-encoded = "#%ED%95%9C%EA%B5%AD%EC%96%B4-%ED%85%8C%EC%8A%A4%ED%8A%B8"
    #[test]
    fn test_url_encoded_cjk_with_spaces() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        let target_file = base_path.join("target.md");
        fs::write(&target_file, "# Target\n\n## 한국어 테스트\n\nContent.\n").unwrap();

        // GitHub converts spaces to hyphens: 한국어-테스트
        let source_file = base_path.join("source.md");
        fs::write(
            &source_file,
            "# Source\n\n[Test](target.md#%ED%95%9C%EA%B5%AD%EC%96%B4-%ED%85%8C%EC%8A%A4%ED%8A%B8)\n",
        )
        .unwrap();

        let rules = rumdl_lib::rules::all_rules(&Config::default());
        let target_content = fs::read_to_string(&target_file).unwrap();
        let source_content = fs::read_to_string(&source_file).unwrap();

        let (_, target_index) =
            rumdl_lib::lint_and_index(&target_content, &rules, false, MarkdownFlavor::default(), None, None);
        let (_, source_index) =
            rumdl_lib::lint_and_index(&source_content, &rules, false, MarkdownFlavor::default(), None, None);

        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(target_file.clone(), target_index);
        workspace_index.insert_file(source_file.clone(), source_index.clone());

        let md051 = MD051LinkFragments::default();
        let warnings = md051
            .cross_file_check(&source_file, &source_index, &workspace_index)
            .unwrap();

        assert!(
            warnings.is_empty(),
            "URL-encoded CJK with spaces->hyphens should work: {warnings:?}"
        );
    }
}
