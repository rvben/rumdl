use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD002FirstHeadingH1;

#[test]
fn test_custom_level() {
    let rule = MD002FirstHeadingH1::new(2);
    let content = "## Heading\n### Subheading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_first_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading\n### Subheading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_no_headings() {
    let rule = MD002FirstHeadingH1::default();
    let content = "This is a paragraph\nAnother paragraph";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_only_one_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "# Heading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_closed_atx_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading ##\n### Subheading ###";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading #\n### Subheading ###");
}

#[test]
fn test_fix_first_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading\n### Subheading";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading\n### Subheading");
}

#[test]
fn test_fix_closed_atx_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading ##\n### Subheading ###";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading #\n### Subheading ###");
}

#[test]
fn test_mixed_heading_styles() {
    let rule = MD002FirstHeadingH1::default();
    let content = "## Heading\n### Subheading ###\n#### Another heading";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading\n### Subheading ###\n#### Another heading");
}

#[test]
fn test_indented_first_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "  ## Heading\n# Subheading";
    println!("Input: '{}'", content.replace("\n", "\\n"));
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    println!("Output: '{}'", result.replace("\n", "\\n"));
    println!(
        "Expected: '{}' (len {})",
        "  # Heading\n# Subheading".replace("\n", "\\n"),
        "  # Heading\n# Subheading".len()
    );
    println!("Got:      '{}' (len {})", result.replace("\n", "\\n"), result.len());

    // Print each character's byte value
    println!("Expected bytes: ");
    for (i, b) in "  # Heading\n# Subheading".bytes().enumerate() {
        print!("{i}:{b} ");
    }
    println!("\nGot bytes: ");
    for (i, b) in result.bytes().enumerate() {
        print!("{i}:{b} ");
    }
    println!();

    assert_eq!(result, "  # Heading\n# Subheading");
}

#[test]
fn test_setext_heading() {
    let rule = MD002FirstHeadingH1::default();
    let content = "Heading\n-------\n\n### Subheading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Heading\n=======\n\n### Subheading");
}

#[test]
fn test_with_front_matter() {
    let rule = MD002FirstHeadingH1::default();
    let content = "---\ntitle: Test\n---\n## Heading\n### Subheading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "---\ntitle: Test\n---\n# Heading\n### Subheading");
}

#[test]
fn test_setext_with_front_matter() {
    let rule = MD002FirstHeadingH1::default();
    let content = "---\ntitle: Test\n---\n\nHeading\n-------\n\n### Subheading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "---\ntitle: Test\n---\n\nHeading\n=======\n\n### Subheading");
}

// Comprehensive test cases as requested

#[test]
fn test_document_starting_with_h1() {
    // Test case 1: Document starting with H1 (should pass)
    let rule = MD002FirstHeadingH1::default();
    let content = "# Document Title\n\nSome content here.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_document_starting_with_h2() {
    // Test case 2: Document starting with H2 (should fail)
    let rule = MD002FirstHeadingH1::default();
    let content = "## Introduction\n\nSome content here.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].message, "First heading should be level 1, found level 2");
    assert_eq!(result[0].line, 1);

    // Test fix
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Introduction\n\nSome content here.");
}

#[test]
fn test_document_starting_with_h3_to_h6() {
    // Test case 3: Document starting with H3, H4, H5, H6 (should fail)
    let test_cases = vec![
        ("### Section", 3, "# Section"),
        ("#### Subsection", 4, "# Subsection"),
        ("##### Detail", 5, "# Detail"),
        ("###### Minute", 6, "# Minute"),
    ];

    for (input_heading, level, expected_fix) in test_cases {
        let rule = MD002FirstHeadingH1::default();
        let content = format!("{input_heading}\n\nSome content.");
        let ctx = LintContext::new(&content);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].message,
            format!("First heading should be level 1, found level {level}")
        );
        assert_eq!(result[0].line, 1);

        // Test fix
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, format!("{expected_fix}\n\nSome content."));
    }
}

#[test]
fn test_document_with_no_headings_comprehensive() {
    // Test case 4: Document with no headings (should pass)
    let rule = MD002FirstHeadingH1::default();
    let content = "This is a paragraph.\n\nAnother paragraph here.\n\n- List item\n- Another item";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test fix (should return unchanged)
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_document_with_content_before_first_heading() {
    // Test case 5: Document with content before first heading
    let rule = MD002FirstHeadingH1::default();
    let content = "Some introductory text.\n\nMore introduction.\n\n## First Heading\n\nContent here.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].message, "First heading should be level 1, found level 2");
    assert_eq!(result[0].line, 5);

    // Test fix
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "Some introductory text.\n\nMore introduction.\n\n# First Heading\n\nContent here."
    );
}

#[test]
fn test_document_starting_with_html_heading() {
    // Test case 6: Document starting with HTML heading
    let rule = MD002FirstHeadingH1::default();
    let content = "<h2>HTML Heading</h2>\n\n## Markdown Heading\n\nContent.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].message, "First heading should be level 1, found level 2");
    assert_eq!(result[0].line, 3);

    // Test fix (HTML headings are not modified, only the first Markdown heading)
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "<h2>HTML Heading</h2>\n\n# Markdown Heading\n\nContent.");
}

#[test]
fn test_setext_style_h1() {
    // Test case 7: Setext style H1 (==== underline)
    let rule = MD002FirstHeadingH1::default();
    let content = "Document Title\n==============\n\nSome content.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test fix (should return unchanged)
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_setext_style_h2() {
    // Test case 8: Setext style H2 (---- underline)
    let rule = MD002FirstHeadingH1::default();
    let content = "Document Title\n--------------\n\nSome content.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].message, "First heading should be level 1, found level 2");
    assert_eq!(result[0].line, 1);

    // Test fix (convert to Setext H1)
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Document Title\n=======\n\nSome content.");
}

#[test]
fn test_configuration_for_level_parameter() {
    // Test case 9: Configuration for level parameter (e.g., first heading must be H2)
    let rule = MD002FirstHeadingH1::new(2);

    // H2 as first heading should pass
    let content = "## Introduction\n\nContent.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // H1 as first heading should fail
    let content = "# Main Title\n\nContent.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].message, "First heading should be level 2, found level 1");

    // Test fix
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "## Main Title\n\nContent.");

    // H3 as first heading should also fail
    let content = "### Section\n\nContent.";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].message, "First heading should be level 2, found level 3");

    // Test fix
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "## Section\n\nContent.");
}

#[test]
fn test_empty_document() {
    // Test case 10: Empty document
    let rule = MD002FirstHeadingH1::default();
    let content = "";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test fix (should return empty string)
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "");
}

#[test]
fn test_various_edge_cases() {
    // Test with only whitespace
    let rule = MD002FirstHeadingH1::default();
    let content = "   \n   \n   ";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test with code blocks containing headings
    let content = "```\n# Not a heading\n```\n\n## First Real Heading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);

    // Test with block quotes containing headings
    let content = "> # Quoted heading\n\n### First Real Heading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_fix_preserves_heading_style() {
    let rule = MD002FirstHeadingH1::default();

    // Test closed ATX style preservation
    let content = "### Heading ###\n\nContent.";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading #\n\nContent.");

    // Test regular ATX style preservation
    let content = "### Heading\n\nContent.";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading\n\nContent.");

    // Test Setext style preservation
    let content = "Heading\n-------\n\nContent.";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Heading\n=======\n\nContent.");
}

#[test]
fn test_mixed_content_types() {
    let rule = MD002FirstHeadingH1::default();

    // Test with lists before heading
    let content = "- Item 1\n- Item 2\n\n### First Heading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 4);

    // Test with tables before heading
    let content = "| Col1 | Col2 |\n|------|------|\n| A    | B    |\n\n## First Heading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
}

#[test]
fn test_html_headings_comprehensive() {
    let rule = MD002FirstHeadingH1::default();

    // Test various HTML heading levels
    let test_cases = vec![
        ("<h1>HTML H1</h1>\n\n## Markdown H2", false, 3), // First markdown heading is H2
        ("<h2>HTML H2</h2>\n\n# Markdown H1", true, 0),   // First markdown heading is H1 (correct)
        ("<h3>HTML H3</h3>\n<h4>HTML H4</h4>\n\n### Markdown H3", false, 4),
    ];

    for (content, should_pass, expected_line) in test_cases {
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        if should_pass {
            assert!(result.is_empty(), "Expected no errors for content: {content}");
        } else {
            assert_eq!(result.len(), 1, "Expected one error for content: {content}");
            assert_eq!(result[0].line, expected_line);
        }
    }
}

#[test]
fn test_setext_variations() {
    let rule = MD002FirstHeadingH1::default();

    // Test various Setext underline styles
    let many_equals = format!("Title\n{}", "=".repeat(50));
    let many_dashes = format!("Title\n{}", "-".repeat(50));

    let test_cases = vec![
        ("Title\n=", true),            // Single =
        ("Title\n==", true),           // Double ==
        (many_equals.as_str(), true),  // Many =
        ("Title\n-", false),           // Single -
        ("Title\n--", false),          // Double --
        (many_dashes.as_str(), false), // Many -
    ];

    for (content, is_h1) in test_cases {
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();
        if is_h1 {
            assert!(result.is_empty(), "Expected H1 for content: {content}");
        } else {
            assert_eq!(result.len(), 1, "Expected error for H2 content: {content}");
            assert_eq!(result[0].message, "First heading should be level 1, found level 2");
        }
    }
}

#[test]
fn test_complex_fix_scenarios() {
    let rule = MD002FirstHeadingH1::default();

    // Test fix with multiple headings of different styles
    let content = "### First Heading ###\n\nSecond Heading\n--------------\n\n##### Third Heading";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# First Heading #\n\nSecond Heading\n--------------\n\n##### Third Heading"
    );

    // Test fix with deeply indented heading
    let content = "        #### Deeply Indented";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "        # Deeply Indented");
}

#[test]
fn test_edge_cases_with_special_characters() {
    let rule = MD002FirstHeadingH1::default();

    // Test heading with special characters
    let content = "## Heading with #hashtag and *emphasis*";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading with #hashtag and *emphasis*");

    // Test heading with emoji
    let content = "### 🚀 Rocket Launch";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# 🚀 Rocket Launch");
}

#[test]
fn test_custom_level_configurations() {
    // Test various custom level configurations
    for level in 1..=6 {
        let rule = MD002FirstHeadingH1::new(level);

        // Test that the configured level passes
        let hashes = "#".repeat(level as usize);
        let content = format!("{hashes} Heading at level {level}");
        let ctx = LintContext::new(&content);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Level {level} heading should pass for rule configured with level {level}"
        );

        // Test that other levels fail
        if level != 1 {
            let content = "# Level 1 Heading";
            let ctx = LintContext::new(content);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(
                result[0].message,
                format!("First heading should be level {level}, found level 1")
            );
        }
    }
}

#[test]
fn test_whitespace_only_lines() {
    let rule = MD002FirstHeadingH1::default();

    // Test document with only whitespace lines before heading
    let content = "   \n\t\n  \t  \n\n## First Heading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
}

#[test]
fn test_fix_preserves_trailing_content() {
    let rule = MD002FirstHeadingH1::default();

    // Test that fix preserves all content after the first heading
    let content = "## Wrong Level\n\nParagraph 1\n\n### Subheading\n\nParagraph 2\n\n```\nCode block\n```\n\nEnd.";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "# Wrong Level\n\nParagraph 1\n\n### Subheading\n\nParagraph 2\n\n```\nCode block\n```\n\nEnd."
    );
}

#[test]
fn test_line_ending_preservation() {
    let rule = MD002FirstHeadingH1::default();

    // Test that fix preserves line endings
    let content = "## Heading\nContent";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading\nContent");

    // No trailing newline should remain absent
    assert!(!fixed.ends_with('\n'));
}

#[test]
fn test_fix_error_column_positions() {
    let rule = MD002FirstHeadingH1::default();

    // Test that error positions are correct
    let content = "Some text\n\n  ### Indented Heading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3);
    assert_eq!(result[0].column, 1); // Points to start of line
    assert_eq!(result[0].end_line, 3);
    assert_eq!(result[0].end_column, 23); // End of trimmed line content
}
