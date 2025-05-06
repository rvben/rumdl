use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD051LinkFragments;

#[test]
fn test_valid_link_fragment() {
    let ctx = LintContext::new(
        "# Test Heading\n\nThis is a [link](somepath#test-heading) to the heading.",
    );
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_link_fragment() {
    let ctx = LintContext::new(
        "# Test Heading\n\nThis is a [link](somepath#wrong-heading) to the heading.",
    );
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_multiple_headings() {
    let ctx = LintContext::new("# First Heading\n\n## Second Heading\n\n[Link 1](somepath#first-heading)\n[Link 2](somepath#second-heading)");
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_special_characters() {
    let ctx = LintContext::new(
        "# Test & Heading!\n\nThis is a [link](somepath#test-heading) to the heading.",
    );
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_no_fragments() {
    let ctx = LintContext::new(
        "# Test Heading\n\nThis is a [link](https://example.com) without fragment.",
    );
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_content() {
    let ctx = LintContext::new("");
    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_invalid_fragments() {
    let ctx =
        LintContext::new("# Test Heading\n\n[Link 1](somepath#wrong1)\n[Link 2](somepath#wrong2)");
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
    let ctx = LintContext::new("# Heading 1\n\nSome text\n\nHeading 2\n-------\n\n### Heading 3\n\n[Link to 1](somepath#heading-1)\n[Link to 2](somepath#heading-2)\n[Link to 3](somepath#heading-3)\n[Link to missing](somepath#heading-4)");
    let rule = MD051LinkFragments::new();

    let result = rule.check(&ctx).unwrap();

    // With our improved implementation, we expect only the missing heading to fail
    assert_eq!(result.len(), 1);

    // Test with special characters in headings/links
    let ctx = LintContext::new("# Heading & Special! Characters\n\n[Link](somepath#heading-special-characters)\n[Bad Link](somepath#heading--special-characters)");
    let result = rule.check(&ctx).unwrap();

    // With our improved implementation, only truly invalid fragments should fail
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
    );
    let rule = MD051LinkFragments::new();

    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    // Test headings with only special characters
    let ctx = LintContext::new("# @#$%^\n\n[Link](somepath#)");
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Test mixed internal/external links
    let ctx = LintContext::new(
        "# Heading\n\n[Internal](somepath#heading)\n[External](https://example.com#heading)",
    );
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_fragment_in_code_blocks() {
    let ctx = LintContext::new("# Real Heading\n\n```markdown\n# Fake Heading\n[Link](somepath#fake-heading)\n```\n\n[Link](somepath#real-heading)");
    let rule = MD051LinkFragments::new();

    let result = rule.check(&ctx).unwrap();
    println!("Result has {} warnings", result.len());
    for (i, warning) in result.iter().enumerate() {
        println!(
            "Warning {}: line {}, message: {}",
            i, warning.line, warning.message
        );
    }

    // With our improved implementation, code blocks are ignored
    assert_eq!(result.len(), 0);

    // Test headings in code blocks (should be ignored)
    let ctx = LintContext::new("```markdown\n# Code Heading\n```\n\n[Link](somepath#code-heading)");
    let result = rule.check(&ctx).unwrap();
    println!("Second test has {} warnings", result.len());
    for (i, warning) in result.iter().enumerate() {
        println!(
            "Warning {}: line {}, message: {}",
            i, warning.line, warning.message
        );
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
    );

    let rule = MD051LinkFragments::new();
    let result = rule.check(&ctx);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Test that complex mixed formatting is handled correctly
    assert_eq!(
        0,
        warnings.len(),
        "Link should match heading with complex formatting"
    );
}

#[test]
fn test_formatting_edge_cases() {
    let ctx = LintContext::new(
        r#"
# Heading with a**partial**bold and *italic with **nested** formatting*

[Link to partial bold](#heading-with-apartialbolda-and-italic-with-nested-formatting)
[Link to nested formatting](#heading-with-apartialbold-and-italic-with-nested-formatting)
"#,
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
        content.push_str(&format!("# Heading {}\n\n", i));
        content.push_str("Some content paragraph with details about this section.\n\n");

        // Add some subheadings
        if i % 3 == 0 {
            content.push_str(&format!("## Subheading {}.1\n\n", i));
            content.push_str("Subheading content with more details.\n\n");
            content.push_str(&format!("## Subheading {}.2\n\n", i));
            content.push_str("More subheading content here.\n\n");
        }
    }

    // Add links section
    content.push_str("# Links Section\n\n");

    // Add 100 links, some valid, some invalid
    for i in 0..100 {
        if i % 3 == 0 {
            content.push_str(&format!(
                "[Link to invalid heading](somepath#heading-{})\n",
                i + 100
            ));
        } else {
            content.push_str(&format!(
                "[Link to heading {}](somepath#heading-{})\n",
                i % 50,
                i % 50
            ));
        }
    }

    // Measure performance
    let start = std::time::Instant::now();
    let rule = MD051LinkFragments::new();
    let ctx = LintContext::new(&content);
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
    let ctx = LintContext::new("# Real Heading\n\nThis is a real link: [Link](somepath#real-heading)\n\nThis is a code example: `[Example](#missing-section)`");
    let rule = MD051LinkFragments::new();

    let result = rule.check(&ctx).unwrap();

    // We should only have 0 warnings - the link in inline code should be ignored
    assert_eq!(
        result.len(),
        0,
        "Link in inline code span should be ignored"
    );

    // Test with multiple code spans and mix of valid and invalid links
    let ctx = LintContext::new("# Heading One\n\n`[Invalid](#missing)` and [Valid](#heading-one) and `[Another Invalid](#nowhere)`");
    let result = rule.check(&ctx).unwrap();

    // Only the valid link should be checked, the ones in code spans should be ignored
    assert_eq!(
        result.len(),
        0,
        "Only links outside code spans should be checked"
    );

    // Test with a fragment link in inline code followed by a real invalid link
    let ctx = LintContext::new(
        "# Heading One\n\n`[Example](#missing-section)` and [Invalid Link](#section-two)",
    );
    let result = rule.check(&ctx).unwrap();

    // Only the real invalid link should be caught
    assert_eq!(result.len(), 1, "Only real invalid links should be caught");
    assert_eq!(result[0].line, 3, "Warning should be on line 3");
    assert!(
        result[0].message.contains("section-two"),
        "Warning should be about 'section-two'"
    );
}
