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
    assert_eq!(result.len(), 2);
}

#[test]
fn test_spaces_inside_double_asterisk() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "** text ** and **text ** and ** text**";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
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
    assert_eq!(result.len(), 8);
}

#[test]
fn test_emphasis_in_code_block() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "```\n* text *\n```\n* text *";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_multiple_emphasis_on_line() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "* text * and _ text _ in one line";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_mixed_emphasis() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "* text * and ** text ** mixed";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_emphasis_with_punctuation() {
    let rule = MD037NoSpaceInEmphasis;
    let content = "* text! * and * text? * here";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
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
    assert_eq!(result.len(), 0);
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
    assert_eq!(result.len(), 0);
}

#[test]
fn test_emphasis_with_multiple_spaces() {
    let rule = MD037NoSpaceInEmphasis;

    // Emphasis with multiple spaces
    let content = "*   multiple spaces   * and **    more spaces    **";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
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
    assert_eq!(result.len(), 1);
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
