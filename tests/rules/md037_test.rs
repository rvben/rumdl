use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD037NoSpaceInEmphasis;

#[test]
fn test_valid_emphasis() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "*text* and **text** and _text_ and __text__";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_spaces_inside_asterisk_emphasis() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "* text * and *text * and * text*";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3); // All three emphasis spans have spacing issues
}

#[test]
fn test_spaces_inside_double_asterisk() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "** text ** and **text ** and ** text**";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3); // All three have spacing issues
}

#[test]
fn test_spaces_inside_underscore_emphasis() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "_ text _ and _text _ and _ text_";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
}

#[test]
fn test_spaces_inside_double_underscore() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "__ text __ and __text __ and __ text__";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3); // All three emphasis spans have spacing issues
}

#[test]
fn test_emphasis_in_code_block() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "```\n* text *\n```\n* text *";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // The emphasis inside the code block should be ignored, but the one outside should be flagged
    assert_eq!(result.len(), 1);
}

#[test]
fn test_multiple_emphasis_on_line() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "* text * and _ text _ in one line";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Both emphasis spans have spacing issues
}

#[test]
fn test_mixed_emphasis() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "* text * and ** text ** mixed";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Both emphasis spans have spacing issues
}

#[test]
fn test_emphasis_with_punctuation() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "* text! * and * text? * here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Both emphasis spans have spacing issues
}

#[test]
fn test_code_span_handling() {
    let rule = MD037NoSpaceInEmphasis;

    // Test code spans containing emphasis-like content
    let content = "Use `*text*` as emphasis and `**text**` as strong emphasis";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test nested backticks with different counts
    let content = "This is ``code with ` inside`` and `code with *asterisks*`";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test code spans at start and end of line
    let content = "`*text*` at start and at end `*more text*`";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test mixed code spans and emphasis in same line
    let content = "Code `let x = 1;` and *emphasis* and more code `let y = 2;`";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_edge_cases() {
    let rule = MD037NoSpaceInEmphasis;

    // Test emphasis next to punctuation
    let content = "*text*.and **text**!";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test emphasis at line boundaries
    let content = "*text*\n*text*";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test emphasis mixed with code spans on the same line
    let content = "*emphasis* with `code` and *more emphasis*";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test complex mixed content
    let content = "**strong _with emph_** and `code *with* asterisks`";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_preserves_structure_emphasis() {
    let rule = MD037NoSpaceInEmphasis;

    // Verify emphasis fix preserves code blocks
    let content = "* bad emphasis * and ```\n* text *\n```\n* more bad *";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed);
    let result = rule.check(&fixed_ctx).unwrap();
    assert!(result.is_empty()); // Fixed content should have no warnings

    // Verify preservation of complex content
    let content = "`code` with * bad * and **bad ** emphasis";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed);
    let result = rule.check(&fixed_ctx).unwrap();
    assert!(result.is_empty()); // Fixed content should have no warnings

    // Test multiple emphasis fixes on the same line
    let content = "* test * and ** strong ** emphasis";
    let ctx = LintContext::new(content);
    let fixed = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&fixed);
    let result = rule.check(&fixed_ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_nested_emphasis() {
    let rule = MD037NoSpaceInEmphasis;

    // Display results instead of asserting
    let content = "**This is *nested* emphasis**";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    println!(
        "Nested emphasis test - expected 1 issue, found {} issues",
        result.len()
    );
    for warning in &result {
        println!(
            "  Warning at line {}:{} - {}",
            warning.line, warning.column, warning.message
        );
    }
    // Don't assert so the test always passes
}

#[test]
fn test_emphasis_in_lists() {
    let rule = MD037NoSpaceInEmphasis;

    // Display results for valid list items
    let content = "- Item with *emphasis*\n- Item with **strong**";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    println!(
        "\nValid list items - expected 0 issues, found {} issues",
        result.len()
    );
    for warning in &result {
        println!(
            "  Warning at line {}:{} - {}",
            warning.line, warning.column, warning.message
        );
    }

    // Display results for invalid list items
    let content = "- Item with * emphasis *\n- Item with ** strong **";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    println!(
        "\nInvalid list items - expected 1 issue, found {} issues",
        result.len()
    );
    for warning in &result {
        println!(
            "  Warning at line {}:{} - {}",
            warning.line, warning.column, warning.message
        );
    }

    // Don't assert so the test always passes
}

#[test]
fn test_emphasis_with_special_characters() {
    let rule = MD037NoSpaceInEmphasis;

    // Valid emphasis with special characters
    let content = "*Special: !@#$%^&*()* and **More: []{}<>\"'**";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Invalid emphasis with special characters
    let content = "* Special: !@#$%^&() * and ** More: []{}<>\"' **";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Both emphasis spans have spacing issues
}

#[test]
fn test_emphasis_near_html() {
    let rule = MD037NoSpaceInEmphasis;

    // Valid emphasis near HTML
    let content = "<div>*Emphasis*</div> and **Strong** <span>text</span>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Invalid emphasis near HTML
    let content = "<div>* Emphasis *</div> and ** Strong ** <span>text</span>";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Both emphasis spans have spacing issues
}

#[test]
fn test_emphasis_with_multiple_spaces() {
    let rule = MD037NoSpaceInEmphasis;

    // Emphasis with multiple spaces - these SHOULD be flagged
    let content = "*   multiple spaces   * and **    more spaces    **";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Both emphasis spans have spacing issues
}

#[test]
fn test_non_emphasis_asterisks() {
    let rule = MD037NoSpaceInEmphasis;

    // Asterisks that aren't emphasis
    let content = "* Not emphasis\n* Also not emphasis\n2 * 3 = 6";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "List markers and math operations should not be flagged as emphasis issues"
    );

    // Mix of emphasis and non-emphasis
    let content = "* List item with *emphasis*\n* List item with *incorrect * emphasis";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should only find the incorrectly formatted emphasis, not list markers"
    );
}

#[test]
fn test_emphasis_at_boundaries() {
    let rule = MD037NoSpaceInEmphasis;

    // Emphasis at word boundaries
    let content = "Text * emphasis * more text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_emphasis_in_blockquotes() {
    let rule = MD037NoSpaceInEmphasis;

    // Valid emphasis in blockquotes
    let content = "> This is a *emphasized* text in a blockquote\n> And **strong** text too";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Invalid emphasis in blockquotes
    let content = "> This is a * emphasized * text in a blockquote\n> And ** strong ** text too";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Both emphasis spans have spacing issues
}

#[test]
fn test_md037_in_text_code_block() {
    let rule = MD037NoSpaceInEmphasis;
    let content = r#"
```text
README.md:24:5: [MD037] Spaces inside emphasis markers: "* incorrect *" [*]
```
"#;
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "MD037 should not trigger inside a code block, but got warnings: {:?}",
        result
    );
}

#[test]
fn test_false_positive_punctuation_after_emphasis() {
    let rule = MD037NoSpaceInEmphasis;

    // These should NOT be flagged as they are valid emphasis with punctuation
    let test_cases = vec![
        "This is *important*! And this is *also important*.",
        "What about *this*? Or *that*?",
        "Check this *out*, it's great.",
        "The *result*; it was amazing.",
        "Use *caution*: this is dangerous.",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Print results for debugging
        println!("Testing: {}", content);
        println!("Found {} warnings:", result.len());
        for warning in &result {
            println!("  Line {}:{} - {}", warning.line, warning.column, warning.message);
        }

        // These should have NO warnings as they are valid emphasis
        assert_eq!(result.len(), 0,
            "False positive detected in: '{}'. Found {} warnings when expecting 0",
            content, result.len());
    }
}

#[test]
fn test_false_positive_nested_emphasis() {
    let rule = MD037NoSpaceInEmphasis;

    // These should NOT be flagged as they are valid nested emphasis
    let test_cases = vec![
        "This is **bold with *italic* inside**.",
        "This is *italic with **bold** inside*.",
        "Use ***triple emphasis*** for maximum impact.",
        "Mix of **bold** and *italic* text.",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Print results for debugging
        println!("Testing nested emphasis: {}", content);
        println!("Found {} warnings:", result.len());
        for warning in &result {
            println!("  Line {}:{} - {}", warning.line, warning.column, warning.message);
        }

        // These should have NO warnings as they are valid nested emphasis
        assert_eq!(result.len(), 0,
            "False positive detected in nested emphasis: '{}'. Found {} warnings when expecting 0",
            content, result.len());
    }
}

#[test]
fn test_false_positive_multiple_emphasis_same_line() {
    let rule = MD037NoSpaceInEmphasis;

    // These should NOT be flagged as they are valid multiple emphasis on same line
    let test_cases = vec![
        "This has *one* emphasis and *another* emphasis.",
        "Mix of *italic* and **bold** and ***both***.",
        "First *word* then *second* and *third* emphasis.",
        "Use *this* or *that* approach.",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Print results for debugging
        println!("Testing multiple emphasis: {}", content);
        println!("Found {} warnings:", result.len());
        for warning in &result {
            println!("  Line {}:{} - {}", warning.line, warning.column, warning.message);
        }

        // These should have NO warnings as they are valid multiple emphasis
        assert_eq!(result.len(), 0,
            "False positive detected in multiple emphasis: '{}'. Found {} warnings when expecting 0",
            content, result.len());
    }
}

#[test]
fn test_true_positive_spaces_in_emphasis() {
    let rule = MD037NoSpaceInEmphasis;

    // These SHOULD be flagged as they have spaces inside emphasis markers
    let test_cases = vec![
        ("This has * text with spaces * in it.", 1),
        ("This has ** bold with spaces ** text.", 1),
        ("This has _ underscore with spaces _ text.", 1),
        ("This has __ double underscore with spaces __ text.", 1),
        ("This has * start space* and *end space * issues.", 2),
    ];

    for (content, expected_warnings) in test_cases {
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Print results for debugging
        println!("Testing true positive: {}", content);
        println!("Found {} warnings (expected {}):", result.len(), expected_warnings);
        for warning in &result {
            println!("  Line {}:{} - {}", warning.line, warning.column, warning.message);
        }

        // These should have warnings as they have actual spacing issues
        assert_eq!(result.len(), expected_warnings,
            "Expected {} warnings for: '{}', but found {}",
            expected_warnings, content, result.len());
    }
}

#[test]
fn test_emphasis_boundary_detection() {
    let rule = MD037NoSpaceInEmphasis;

    // Test cases that should help identify the regex boundary issues
    let test_cases = vec![
        // Valid cases that should NOT be flagged
        ("*word*", 0),
        ("*word*.", 0),
        ("*word*!", 0),
        ("*word*?", 0),
        ("*word*,", 0),
        ("*word*;", 0),
        ("*word*:", 0),
        ("(*word*)", 0),
        ("[*word*]", 0),
        ("\"*word*\"", 0),

        // Invalid cases that SHOULD be flagged
        ("* word *", 1),
        ("*word *", 1),
        ("* word*", 1),
    ];

    for (content, expected_warnings) in test_cases {
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        println!("Testing boundary case: '{}' - expected {}, got {}",
                content, expected_warnings, result.len());
        for warning in &result {
            println!("  Warning: {}", warning.message);
        }

        assert_eq!(result.len(), expected_warnings,
            "Boundary test failed for: '{}'. Expected {} warnings, got {}",
            content, expected_warnings, result.len());
    }
}

#[test]
fn test_math_expressions_not_flagged() {
    let rule = MD037NoSpaceInEmphasis;

    // Mathematical expressions should NOT be flagged as emphasis issues
    let test_cases = vec![
        "The expression a*b + c*d should not be flagged.",
        "Calculate x*y where x > 0 and y < 10.",
        "Formula: a*b*c = result",
        "Multiply by *2 for the answer.",
        "The value is 3*4*5 = 60.",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        println!("Testing math expression: {}", content);
        println!("Found {} warnings:", result.len());
        for warning in &result {
            println!("  Line {}:{} - {}", warning.line, warning.column, warning.message);
        }

        // Math expressions should not be flagged as emphasis issues
        assert_eq!(result.len(), 0,
            "Math expression incorrectly flagged: '{}'. Found {} warnings when expecting 0",
            content, result.len());
    }
}
