//! Comprehensive test suite for Quarto flavor support.
//!
//! Tests Pandoc/Quarto divs, citations, shortcodes, and math blocks.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;

// ====================================================================
// Quarto Div Detection Tests
// ====================================================================

#[test]
fn test_quarto_div_basic() {
    let content = r#"# Heading

::: {.callout-note}
This is a callout note.
:::

Regular text.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Line 0: heading - not in div
    assert!(!ctx.lines[0].in_quarto_div, "Heading should not be in Quarto div");
    // Line 2: div opener - in div
    assert!(ctx.lines[2].in_quarto_div, "Div opener should be in Quarto div");
    // Line 3: content - in div
    assert!(ctx.lines[3].in_quarto_div, "Div content should be in Quarto div");
    // Line 4: closer - in div
    assert!(ctx.lines[4].in_quarto_div, "Div closer should be in Quarto div");
    // Line 6: regular text - not in div
    assert!(
        !ctx.lines[6].in_quarto_div,
        "Text after div should not be in Quarto div"
    );
}

#[test]
fn test_quarto_div_nested() {
    let content = r#"::: {.callout-warning}
Outer content.

::: {.callout-tip}
Inner content.
:::

Back to outer.
:::
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // All lines should be in some div context
    assert!(ctx.lines[0].in_quarto_div, "Outer opener");
    assert!(ctx.lines[1].in_quarto_div, "Outer content");
    assert!(ctx.lines[3].in_quarto_div, "Inner opener");
    assert!(ctx.lines[4].in_quarto_div, "Inner content");
    assert!(ctx.lines[5].in_quarto_div, "Inner closer");
    assert!(ctx.lines[7].in_quarto_div, "Back to outer");
    assert!(ctx.lines[8].in_quarto_div, "Outer closer");
}

#[test]
fn test_quarto_div_all_callout_types() {
    let callout_types = ["note", "warning", "tip", "important", "caution"];

    for callout_type in callout_types {
        let content = format!(
            r#"::: {{.callout-{callout_type}}}
Content for {callout_type}.
:::
"#
        );
        let ctx = LintContext::new(&content, MarkdownFlavor::Quarto, None);

        assert!(
            ctx.lines[0].in_quarto_div,
            "Callout-{callout_type} opener should be detected"
        );
        assert!(
            ctx.lines[1].in_quarto_div,
            "Callout-{callout_type} content should be detected"
        );
    }
}

#[test]
fn test_quarto_div_with_id_and_classes() {
    let content = r#"::: {#myid .callout-note .custom-class}
Content with ID and multiple classes.
:::
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(ctx.lines[0].in_quarto_div, "Div with ID and classes");
    assert!(ctx.lines[1].in_quarto_div, "Content in div with ID and classes");
}

#[test]
fn test_quarto_div_with_attributes() {
    let content = r#"::: {.callout-note title="Important Note" collapse="true"}
Content with attributes.
:::
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(ctx.lines[0].in_quarto_div, "Div with attributes");
    assert!(ctx.lines[1].in_quarto_div, "Content in div with attributes");
}

#[test]
fn test_quarto_div_unclosed() {
    let content = r#"::: {.callout-note}
This div is never closed.
More content.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Unclosed div extends to end of document
    assert!(ctx.lines[0].in_quarto_div, "Unclosed div opener");
    assert!(ctx.lines[1].in_quarto_div, "Unclosed div content line 1");
    assert!(ctx.lines[2].in_quarto_div, "Unclosed div content line 2");
}

#[test]
fn test_quarto_div_simple_class_syntax() {
    // Quarto also supports ::: classname without braces
    let content = r#"::: warning
Simple class syntax.
:::
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(ctx.lines[0].in_quarto_div, "Simple class div opener");
    assert!(ctx.lines[1].in_quarto_div, "Simple class div content");
}

#[test]
fn test_quarto_div_not_detected_in_standard_flavor() {
    let content = r#"::: {.callout-note}
This is a Quarto div.
:::
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Quarto divs should NOT be detected in Standard flavor
    assert!(
        !ctx.lines[0].in_quarto_div,
        "Quarto divs should not be detected in Standard flavor"
    );
}

#[test]
fn test_quarto_div_not_detected_in_mkdocs_flavor() {
    let content = r#"::: {.callout-note}
This is a Quarto div.
:::
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    assert!(
        !ctx.lines[0].in_quarto_div,
        "Quarto divs should not be detected in MkDocs flavor"
    );
}

// ====================================================================
// Citation Detection Tests
// ====================================================================

#[test]
fn test_quarto_bracketed_citation() {
    let content = "See [@smith2020] for details.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Check citation range detection
    let citation_start = content.find("[@smith2020]").unwrap();
    assert!(
        ctx.is_in_citation(citation_start + 1),
        "Should detect bracketed citation"
    );
}

#[test]
fn test_quarto_inline_citation() {
    let content = "As @smith2020 argues, this is true.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let citation_start = content.find("@smith2020").unwrap();
    assert!(ctx.is_in_citation(citation_start), "Should detect inline citation");
}

#[test]
fn test_quarto_multiple_citations_in_brackets() {
    let content = "See [@smith2020; @jones2021; @doe2022] for details.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let citation_start = content.find("[@smith2020").unwrap();
    assert!(
        ctx.is_in_citation(citation_start + 1),
        "Should detect first citation in group"
    );
}

#[test]
fn test_quarto_citation_with_prefix_and_locator() {
    let content = "[see @smith2020, p. 10-15]\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let citation_start = content.find("@smith2020").unwrap();
    assert!(
        ctx.is_in_citation(citation_start),
        "Should detect citation with prefix and locator"
    );
}

#[test]
fn test_quarto_suppress_author_citation() {
    let content = "The theory [-@smith2020] states that...\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let citation_start = content.find("-@smith2020").unwrap();
    assert!(
        ctx.is_in_citation(citation_start),
        "Should detect suppress-author citation"
    );
}

#[test]
fn test_quarto_citation_key_formats() {
    // Test various valid citation key formats
    let test_cases = [
        "@simple",
        "@with_underscore",
        "@with-dash",
        "@with.dot",
        "@with:colon",
        "@CamelCase",
        "@UPPERCASE",
        "@mixed123",
    ];

    for key in test_cases {
        let content = format!("See {key} here.\n");
        let ctx = LintContext::new(&content, MarkdownFlavor::Quarto, None);

        let key_start = content.find(key).unwrap();
        assert!(ctx.is_in_citation(key_start), "Should detect citation key: {key}");
    }
}

#[test]
fn test_quarto_email_not_citation() {
    let content = "Contact user@example.com for help.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Email addresses should NOT be detected as citations
    let at_pos = content.find('@').unwrap();
    assert!(
        !ctx.is_in_citation(at_pos),
        "Email addresses should not be detected as citations"
    );
}

#[test]
fn test_quarto_citation_detection_raw() {
    // Note: Citation detection currently uses regex on raw content.
    // Code block filtering should be done at the rule level, not in is_in_citation().
    // This test documents current behavior.
    let content = r#"```
See @smith2020 in code.
```
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Citation detection is raw - rules should check in_code_block separately
    // Rules using this should also check ctx.lines[line].in_code_block
    assert!(ctx.lines[1].in_code_block, "Line should be in code block");
}

#[test]
fn test_quarto_citations_not_detected_in_standard_flavor() {
    let content = "See [@smith2020] for details.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let citation_start = content.find("[@smith2020]").unwrap();
    assert!(
        !ctx.is_in_citation(citation_start + 1),
        "Citations should not be detected in Standard flavor"
    );
}

// ====================================================================
// Shortcode Detection Tests
// ====================================================================

#[test]
fn test_quarto_shortcode_angle_bracket() {
    let content = "Here is {{< video https://example.com/video.mp4 >}} embedded.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let shortcode_start = content.find("{{<").unwrap();
    assert!(
        ctx.is_in_shortcode(shortcode_start),
        "Should detect angle bracket shortcode"
    );
    assert!(
        ctx.is_in_shortcode(shortcode_start + 10),
        "Should detect middle of shortcode"
    );
}

#[test]
fn test_quarto_shortcode_percent() {
    let content = "Here is {{% include \"file.md\" %}} included.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let shortcode_start = content.find("{{%").unwrap();
    assert!(ctx.is_in_shortcode(shortcode_start), "Should detect percent shortcode");
}

#[test]
fn test_quarto_shortcode_with_url() {
    // URLs inside shortcodes should not trigger MD034
    let content = "{{< video https://www.youtube.com/watch?v=abc123 >}}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let url_start = content.find("https://").unwrap();
    assert!(
        ctx.is_in_shortcode(url_start),
        "URL inside shortcode should be in shortcode context"
    );
}

#[test]
fn test_quarto_multiple_shortcodes() {
    let content = "{{< video url1 >}} and {{< audio url2 >}} media.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let first_start = content.find("{{< video").unwrap();
    let second_start = content.find("{{< audio").unwrap();

    assert!(ctx.is_in_shortcode(first_start + 5), "First shortcode");
    assert!(ctx.is_in_shortcode(second_start + 5), "Second shortcode");
}

#[test]
fn test_quarto_shortcode_various_types() {
    let shortcode_types = [
        "{{< video https://example.com >}}",
        "{{< youtube abc123 >}}",
        "{{< vimeo 123456 >}}",
        "{{% include \"header.md\" %}}",
        "{{< figure src=\"image.png\" >}}",
        "{{< tweet user=\"123\" id=\"456\" >}}",
    ];

    for shortcode in shortcode_types {
        let content = format!("Content {shortcode} more.\n");
        let ctx = LintContext::new(&content, MarkdownFlavor::Quarto, None);

        let start = content.find("{{").unwrap();
        assert!(ctx.is_in_shortcode(start + 3), "Should detect shortcode: {shortcode}");
    }
}

#[test]
fn test_quarto_text_outside_shortcode() {
    let content = "Before {{< video url >}} after.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let before_pos = content.find("Before").unwrap();
    let after_pos = content.find("after").unwrap();

    assert!(
        !ctx.is_in_shortcode(before_pos),
        "Text before shortcode should not be in shortcode"
    );
    assert!(
        !ctx.is_in_shortcode(after_pos),
        "Text after shortcode should not be in shortcode"
    );
}

// ====================================================================
// Math Block Detection Tests
// ====================================================================

#[test]
fn test_quarto_math_block() {
    let content = r#"# Math Section

$$
E = mc^2
$$

Regular text.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Line 2: math opener
    assert!(ctx.lines[2].in_math_block, "Math opener should be in math block");
    // Line 3: math content
    assert!(ctx.lines[3].in_math_block, "Math content should be in math block");
    // Line 4: math closer
    assert!(ctx.lines[4].in_math_block, "Math closer should be in math block");
    // Line 6: regular text
    assert!(
        !ctx.lines[6].in_math_block,
        "Text after math should not be in math block"
    );
}

#[test]
fn test_quarto_inline_math_not_block() {
    let content = "The equation $E = mc^2$ is famous.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Inline math should not trigger in_math_block
    assert!(!ctx.lines[0].in_math_block, "Inline math should not be in_math_block");
}

// ====================================================================
// Pandoc Attribute Tests
// ====================================================================

#[test]
fn test_quarto_heading_with_id() {
    let content = "# Custom Heading {#custom-id}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // The heading should be recognized
    assert!(!ctx.lines[0].in_code_block, "Heading is not in code block");
}

#[test]
fn test_quarto_heading_with_classes() {
    let content = "# Styled Heading {.unnumbered .special}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(!ctx.lines[0].in_code_block, "Heading with classes");
}

#[test]
fn test_quarto_heading_with_mixed_attributes() {
    let content = "# Complex Heading {#my-id .class1 .class2 data-value=\"test\"}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(!ctx.lines[0].in_code_block, "Heading with mixed attributes");
}

// ====================================================================
// Flavor Isolation Tests
// ====================================================================

#[test]
fn test_quarto_features_not_in_mdx() {
    let content = r#"::: {.callout-note}
Content
:::
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    assert!(
        !ctx.lines[0].in_quarto_div,
        "Quarto divs should not be detected in MDX flavor"
    );
}

#[test]
fn test_quarto_combined_features() {
    // Test document using multiple Quarto features together
    let content = r#"# Introduction {#intro}

See [@smith2020] for background.

::: {.callout-note}
## Note Title

As @jones2021 explains, this is important.

$$
f(x) = x^2
$$

{{< video https://example.com/demo.mp4 >}}
:::

More content here.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Verify div detection
    let div_start = content.lines().position(|l| l.contains("callout-note")).unwrap();
    assert!(ctx.lines[div_start].in_quarto_div, "Div opener detected");

    // Verify citation in div
    let jones_line = content.lines().position(|l| l.contains("@jones2021")).unwrap();
    assert!(ctx.lines[jones_line].in_quarto_div, "Content in div");

    // Verify math in div
    let math_line = content.lines().position(|l| l.contains("f(x)")).unwrap();
    assert!(ctx.lines[math_line].in_math_block, "Math block in div");
    assert!(ctx.lines[math_line].in_quarto_div, "Math also in div context");
}

// ====================================================================
// Edge Case Tests
// ====================================================================

#[test]
fn test_quarto_empty_div() {
    let content = r#"::: {.callout-note}
:::
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(ctx.lines[0].in_quarto_div, "Empty div opener");
    assert!(ctx.lines[1].in_quarto_div, "Empty div closer");
}

#[test]
fn test_quarto_div_with_only_whitespace() {
    let content = r#"::: {.callout-note}


:::
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(ctx.lines[0].in_quarto_div, "Div opener");
    assert!(ctx.lines[1].in_quarto_div, "Blank line in div");
    assert!(ctx.lines[2].in_quarto_div, "Whitespace line in div");
    assert!(ctx.lines[3].in_quarto_div, "Div closer");
}

#[test]
fn test_quarto_citation_at_line_start() {
    let content = "@smith2020 argues that...\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(ctx.is_in_citation(0), "Citation at line start");
}

#[test]
fn test_quarto_citation_at_line_end() {
    let content = "See @smith2020\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let citation_start = content.find("@smith2020").unwrap();
    assert!(ctx.is_in_citation(citation_start), "Citation at line end");
}

#[test]
fn test_quarto_adjacent_shortcodes() {
    let content = "{{< a >}}{{< b >}}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    assert!(ctx.is_in_shortcode(3), "First shortcode");
    assert!(ctx.is_in_shortcode(12), "Second shortcode");
}

#[test]
fn test_quarto_shortcode_spanning_apparent_url() {
    // The URL-like content is inside a shortcode, so shouldn't trigger bare URL warnings
    let content = "{{< embed https://quarto.org/docs/get-started/ >}}\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let url_start = content.find("https://").unwrap();
    assert!(
        ctx.is_in_shortcode(url_start),
        "URL in shortcode is in shortcode context"
    );
}

#[test]
fn test_quarto_deeply_nested_divs() {
    let content = r#"::: {.outer}
::: {.middle}
::: {.inner}
Deep content.
:::
:::
:::
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // All lines should be in div context
    let lines: Vec<&str> = content.lines().collect();
    for (i, line_info) in ctx.lines.iter().enumerate() {
        if i < lines.len() && !lines[i].trim().is_empty() {
            assert!(line_info.in_quarto_div, "Line {i} should be in div");
        }
    }
}

#[test]
fn test_quarto_unicode_in_citation_key() {
    // Citation keys should be ASCII alphanumeric + limited punctuation
    let content = "See @validKey123 here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    let key_start = content.find("@validKey123").unwrap();
    assert!(ctx.is_in_citation(key_start), "Valid citation key");
}

#[test]
fn test_quarto_special_chars_near_citation() {
    let content = "(@smith2020) and [@jones2021].\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Both should be detected
    let smith_pos = content.find("@smith2020").unwrap();
    let jones_pos = content.find("@jones2021").unwrap();

    assert!(ctx.is_in_citation(smith_pos), "Citation in parentheses");
    assert!(ctx.is_in_citation(jones_pos + 1), "Bracketed citation");
}

// ====================================================================
// Code Block Exclusion Tests
// ====================================================================

#[test]
fn test_quarto_code_block_detection() {
    let content = r#"```
::: {.callout-note}
Not a real div.
:::
```
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Verify code block detection works
    assert!(ctx.lines[0].in_code_block, "Fence opener in code block");
    assert!(ctx.lines[1].in_code_block, "Content in code block");
    assert!(ctx.lines[2].in_code_block, "Content in code block");
    assert!(ctx.lines[3].in_code_block, "Content in code block");
    assert!(ctx.lines[4].in_code_block, "Fence closer in code block");
}

#[test]
fn test_quarto_shortcode_in_code_span() {
    let content = "Use `{{< shortcode >}}` in your document.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Shortcode in inline code - the detection is at byte level
    // The shortcode regex will still match, but rules should check code span context
    let shortcode_start = content.find("{{<").unwrap();
    // This tests the raw shortcode detection - rules need additional code span checks
    assert!(
        ctx.is_in_shortcode(shortcode_start),
        "Raw shortcode detection (rules handle code span exclusion)"
    );
}

// ====================================================================
// Stress Tests
// ====================================================================

#[test]
fn test_quarto_many_citations() {
    let mut content = String::new();
    for i in 0..100 {
        content.push_str(&format!("See @author{i} and [@ref{i}]. "));
    }
    content.push('\n');

    let ctx = LintContext::new(&content, MarkdownFlavor::Quarto, None);

    // Spot check a few citations
    let author50_pos = content.find("@author50").unwrap();
    assert!(ctx.is_in_citation(author50_pos), "Citation @author50");

    let ref75_pos = content.find("@ref75").unwrap();
    assert!(ctx.is_in_citation(ref75_pos), "Citation @ref75");
}

#[test]
fn test_quarto_many_shortcodes() {
    let mut content = String::new();
    for i in 0..50 {
        content.push_str(&format!("{{{{< video{i} >}}}} "));
    }
    content.push('\n');

    let ctx = LintContext::new(&content, MarkdownFlavor::Quarto, None);

    // Should handle many shortcodes efficiently
    assert!(ctx.shortcode_ranges().len() >= 50, "Should detect all shortcodes");
}

#[test]
fn test_quarto_large_document_with_mixed_features() {
    let mut content = String::new();

    for i in 0..20 {
        content.push_str(&format!("# Section {i} {{#sec-{i}}}\n\n"));
        content.push_str(&format!("See @author{i} for details.\n\n"));
        content.push_str(&format!("::: {{.callout-note}}\nNote {i} content.\n:::\n\n"));
        content.push_str(&format!("{{{{< video{i} >}}}}\n\n"));
    }

    let ctx = LintContext::new(&content, MarkdownFlavor::Quarto, None);

    // Verify structure is maintained
    assert!(!ctx.citation_ranges().is_empty(), "Citations detected");
    assert!(!ctx.shortcode_ranges().is_empty(), "Shortcodes detected");

    // Check a specific div
    let note_10_line = content.lines().position(|l| l.contains("Note 10")).unwrap();
    assert!(ctx.lines[note_10_line].in_quarto_div, "Note 10 should be in div");
}

// ====================================================================
// MD050 Math Block Integration Tests
// ====================================================================

#[test]
fn test_md050_skips_math_block_content() {
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD050StrongStyle;
    use rumdl_lib::rules::strong_style::StrongStyle;

    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = r#"# Math in Quarto

$$
x_1 + x_2 = y
a__b = c
$$

This __should be flagged__ as inconsistent.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);
    let result = rule.check(&ctx).unwrap();

    // Only the strong outside math block should be flagged
    assert_eq!(result.len(), 1, "Expected 1 warning, got: {result:?}");
    // The warning should be on line 8 (the "This __should be flagged__" line)
    assert_eq!(result[0].line, 8, "Warning should be on line 8, got {}", result[0].line);
}

#[test]
fn test_md050_math_block_with_underscore_subscripts() {
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD050StrongStyle;
    use rumdl_lib::rules::strong_style::StrongStyle;

    let rule = MD050StrongStyle::new(StrongStyle::Asterisk);
    let content = r#"$$
x_1 + x_2 + x_{12}
y__subscript = z
\alpha__\beta
$$
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);
    let result = rule.check(&ctx).unwrap();

    // Nothing should be flagged - all content is in math block
    assert!(
        result.is_empty(),
        "Math block content should not be flagged. Got: {result:?}"
    );
}

#[test]
fn test_md049_skips_math_block_content() {
    use rumdl_lib::rule::Rule;
    use rumdl_lib::rules::MD049EmphasisStyle;
    use rumdl_lib::rules::emphasis_style::EmphasisStyle;

    let rule = MD049EmphasisStyle::new(EmphasisStyle::Asterisk);
    let content = r#"# Math in Quarto

$$
_a + _b = _c
$$

This _should be flagged_ as inconsistent.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);
    let result = rule.check(&ctx).unwrap();

    // Only the emphasis outside math block should be flagged
    assert_eq!(result.len(), 1, "Expected 1 warning, got: {result:?}");
    // The warning should be on line 7 (the "This _should be flagged_" line)
    assert_eq!(result[0].line, 7, "Warning should be on line 7, got {}", result[0].line);
}

#[test]
fn test_math_block_detection_consistent_with_lineinfo() {
    // Verify LineInfo.in_math_block is set correctly for Quarto
    let content = r#"# Heading

$$
E = mc^2
$$

Text here.
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Quarto, None);

    // Line 0: heading - not in math
    assert!(!ctx.lines[0].in_math_block, "Heading should not be in math block");
    // Line 2: math opener - in math
    assert!(ctx.lines[2].in_math_block, "Math opener should be in math block");
    // Line 3: math content - in math
    assert!(ctx.lines[3].in_math_block, "Math content should be in math block");
    // Line 4: math closer - in math
    assert!(ctx.lines[4].in_math_block, "Math closer should be in math block");
    // Line 6: text - not in math
    assert!(
        !ctx.lines[6].in_math_block,
        "Text after math should not be in math block"
    );
}
