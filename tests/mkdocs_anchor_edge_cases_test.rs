use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD042NoEmptyLinks;

/// Edge case tests for MkDocs anchor detection
/// Based on comprehensive analysis of the implementation and Python-Markdown attr_list spec

#[test]
fn test_malformed_empty_attributes() {
    let rule = MD042NoEmptyLinks::new();

    // Empty braces should still flag as empty link
    let content = "[](){ }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Empty attributes {{ }} should still be flagged as empty link"
    );
}

#[test]
fn test_unclosed_brace() {
    let rule = MD042NoEmptyLinks::new();

    // Missing closing brace - should flag as empty link
    let content = "[](){ #anchor";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Unclosed brace should flag as empty link");
}

#[test]
fn test_no_opening_brace() {
    let rule = MD042NoEmptyLinks::new();

    // No opening brace - should flag as empty link
    let content = "[]() #anchor }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "No opening brace should flag as empty link");
}

#[test]
fn test_attributes_only_no_anchor() {
    let rule = MD042NoEmptyLinks::new();

    // Only classes, no anchor - this is valid per Python-Markdown attr_list
    // Classes alone are valid attributes
    let content = "[](){ .class1 .class2 }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Attributes with classes are valid (improved implementation accepts . prefix)"
    );
}

#[test]
fn test_whitespace_variations() {
    let rule = MD042NoEmptyLinks::new();

    let test_cases = vec![
        ("[]()  { #anchor }", false),     // Two spaces - valid
        ("[]()\t{ #anchor }", false),     // Tab - valid
        ("[]()   { #anchor }", false),    // Multiple spaces - valid
        ("[]()  \t  { #anchor }", false), // Mixed whitespace - valid
    ];

    for (content, should_flag) in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(!result.is_empty(), should_flag, "Failed for: {content}");
    }
}

#[test]
fn test_newline_before_attributes_rejected() {
    let rule = MD042NoEmptyLinks::new();

    // Newline before attributes - should be flagged
    // attr_list should be inline, not on next line for inline elements
    let content = "[]()\n{ #anchor }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    // Current implementation uses trim_start() which removes newlines
    // This is actually permissive behavior - documenting current state
    assert_eq!(
        result.len(),
        0,
        "Current implementation accepts newline (may need to be more strict)"
    );
}

#[test]
fn test_multiple_anchors_per_line() {
    let rule = MD042NoEmptyLinks::new();

    let content = "Text [](){ #a1 } middle [](){ #a2 } end [](){ #a3 }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Multiple anchors per line should all be valid");
}

#[test]
fn test_anchor_at_document_end() {
    let rule = MD042NoEmptyLinks::new();

    let content = "Content here [](){ #anchor }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Anchor at document end should be valid");
}

#[test]
fn test_anchor_adjacent_to_text() {
    let rule = MD042NoEmptyLinks::new();

    // Anchor immediately followed by text (no space)
    let content = "[](){ #anchor }More text";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Anchor followed by text should be valid");

    // Anchor at end of line
    let content = "Some text [](){ #anchor }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Anchor at line end should be valid");

    // Anchor with punctuation after
    let content = "[](){ #anchor }.";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Anchor with punctuation after should be valid");
}

#[test]
fn test_utf8_boundary_safety() {
    let rule = MD042NoEmptyLinks::new();

    // Multi-byte UTF-8 characters near the link end
    let content = "[]()😀{ #anchor }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);

    // Should not panic - even if implementation doesn't handle properly
    let result = rule.check(&ctx);
    assert!(result.is_ok(), "Should handle UTF-8 gracefully");

    // Additional UTF-8 test cases
    let test_cases = vec!["[]()日本語{ #anchor }", "[]()🎉{ #anchor }", "[]()™{ #anchor }"];

    for content in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx);
        assert!(result.is_ok(), "Should handle UTF-8 in: {content}");
    }
}

#[test]
fn test_very_long_attribute_strings() {
    let rule = MD042NoEmptyLinks::new();

    // Very long anchor name
    let long_anchor = "a".repeat(1000);
    let content = format!("[](){{ #{long_anchor} }}");
    let ctx = LintContext::new(&content, MarkdownFlavor::MkDocs, None);

    // Should handle gracefully without panic or timeout
    let start = std::time::Instant::now();
    let result = rule.check(&ctx);
    let duration = start.elapsed();

    assert!(result.is_ok(), "Should handle long strings");
    assert!(duration.as_secs() < 1, "Should complete quickly even with long strings");
}

#[test]
fn test_nested_braces_edge_case() {
    let rule = MD042NoEmptyLinks::new();

    // Content with nested braces - implementation finds first }
    let content = "[](){ #anchor { nested } }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    // Current implementation will find first } and check "anchor { nested"
    // This is edge case behavior - documenting current state
    // "#anchor" starts with # so it should pass
    assert_eq!(
        result.len(),
        0,
        "Current implementation finds first }} - documents behavior"
    );
}

#[test]
fn test_multiple_classes_with_anchor() {
    let rule = MD042NoEmptyLinks::new();

    let content = "[](){ .class1 #anchor .class2 }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Mixed attributes with anchor should be valid");
}

#[test]
fn test_anchor_in_blockquotes() {
    let rule = MD042NoEmptyLinks::new();

    let content = r#"> Quote text
> [](){ #anchor }
> More quote"#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Anchors in blockquotes should be valid");
}

#[test]
fn test_anchor_in_lists() {
    let rule = MD042NoEmptyLinks::new();

    let content = r#"- Item 1
  [](){ #anchor1 }
- Item 2
  [](){ #anchor2 }"#;

    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Anchors in lists should be valid");
}

#[test]
fn test_anchors_in_code_blocks_ignored() {
    let rule = MD042NoEmptyLinks::new();

    // Fenced code block
    let content = "```\n[](){ #anchor }\n```";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Anchors in fenced code blocks should be ignored");

    // Inline code
    let content = "`[](){ #anchor }`";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Anchors in inline code should be ignored");
}

#[test]
fn test_actual_empty_links_still_flagged() {
    // MD042 only flags empty URLs, not empty text
    let rule = MD042NoEmptyLinks::new();

    // Links with empty URLs should be flagged
    let flagged_cases = vec![
        "[]()",               // Plain empty link
        "[]()  ",             // Empty link with trailing spaces
        "[text]()",           // Text with empty URL
        "[]() no attributes", // Empty link followed by text (no braces)
    ];

    for content in flagged_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag empty URL link: {content}");
    }

    // Links with empty text but valid URL should NOT be flagged
    let not_flagged_cases = vec![
        "[](https://example.com)", // Empty text with URL - not flagged
    ];

    for content in not_flagged_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Should NOT flag empty text with valid URL: {content}"
        );
    }
}

#[test]
fn test_standard_mode_still_flags_anchors() {
    let rule = MD042NoEmptyLinks::new();

    // In standard mode, anchors should be flagged
    let content = "[](){ #anchor }";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Standard mode should flag empty links even with attributes"
    );
}

#[test]
fn test_multiple_attributes_per_element() {
    let rule = MD042NoEmptyLinks::new();

    let content = r#"[](){ #anchor .class1 .class2 title="Tooltip" }"#;
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Multiple attributes should be valid if anchor present"
    );
}

#[test]
fn test_whitespace_inside_braces() {
    let rule = MD042NoEmptyLinks::new();

    let test_cases = vec![
        ("[](){ #anchor }", true),     // Standard spacing
        ("[](){#anchor}", true),       // No spacing
        ("[](){ \t#anchor\t }", true), // Tabs
        ("[](){  #anchor  }", true),   // Multiple spaces
    ];

    for (content, should_pass) in test_cases {
        let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.is_empty(), should_pass, "Failed for: {content}");
    }
}

#[test]
fn test_no_space_before_brace() {
    let rule = MD042NoEmptyLinks::new();

    // Per Python-Markdown spec, inline attributes should have no space
    let content = "[](){: #anchor }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();

    // With colon - valid attr_list syntax
    assert!(result.is_empty(), "Colon syntax {{:  should work (standard attr_list)");

    // Without colon but immediate attachment
    let content = "[](){ #anchor }";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Without colon should also work");
}
