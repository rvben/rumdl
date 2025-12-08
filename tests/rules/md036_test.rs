use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD036NoEmphasisAsHeading;

#[test]
fn test_valid_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "This is *emphasized* text\nThis text is also *emphasized*";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_only() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "*Emphasized*\n_Also emphasized_";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    // MD036 no longer provides automatic fixes
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_strong_only() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "**Strong**\n__Also strong__";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    // MD036 no longer provides automatic fixes
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_multiple_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "\n*First emphasis*\n\nNormal line\n\n_Second emphasis_\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    // MD036 no longer provides automatic fixes
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_not_first_word() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "The *second* word\nA _middle_ emphasis";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_first_word_only() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "*First* word emphasized\n**First** word strong";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "*First* is _second_ emphasis\n**First** is __second__ strong";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_with_punctuation() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "\n*Hello with punctuation!*\n\n*Hi there!*\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Emphasis with punctuation should NOT be flagged (markdownlint parity)
    assert_eq!(result.len(), 0);
}

#[test]
fn test_emphasis_with_trailing_colon() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "**Example output:**";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Emphasis with colon should NOT be flagged (markdownlint parity)
    assert_eq!(result.len(), 0);
}

#[test]
fn test_preserve_punctuation_when_disabled() {
    let rule = MD036NoEmphasisAsHeading::new("".to_string());
    let content = "**Example output:**";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    // MD036 no longer provides automatic fixes
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_emphasis_with_various_punctuation() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "**Title with period.**\n\n*Question?*\n\n**Exclamation!**\n\n*Semicolon;*";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // All emphasis with punctuation should NOT be flagged (markdownlint parity)
    assert_eq!(result.len(), 0);
}

#[test]
fn test_emphasis_in_list() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "- *Not a heading*\n  - **Also not a heading**";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_in_blockquote() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "> *Not a heading*\n> **Also not a heading**";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_in_code_block() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "```\n*Not a heading*\n**Also not a heading**\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_existing_heading() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "# Already a heading\n## Also a heading";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_with_other_text() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "This line has *emphasis* in it\nThis line has **strong** text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "**\n__";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_partial_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "*Incomplete emphasis\n**Incomplete strong";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_table_of_contents_labels() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "**Table of Contents**\n\n*Contents*\n\n__TOC__\n\n_Index_";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // None of these should be flagged as they are legitimate TOC labels
    assert!(result.is_empty());
}

#[test]
fn test_table_of_contents_with_other_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content =
        "**Table of Contents**\n\n**This should be a heading**\n\n*Contents*\n\n*This should also be a heading*";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Only the non-TOC emphasis should be flagged
    assert_eq!(result.len(), 2);
    assert!(result[0].message.contains("This should be a heading"));
    assert!(result[1].message.contains("This should also be a heading"));
}

#[test]
fn test_markdownlint_parity_comprehensive() {
    // Comprehensive test to ensure full parity with markdownlint behavior
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?。，；：！？".to_string());

    // Test various punctuation types that should NOT be flagged
    let not_flagged_cases = vec![
        // ASCII punctuation
        "**Arguments:**",
        "*Note.*",
        "__Warning!__",
        "_Question?_",
        "**Important;**",
        "*Items,*",
        // Full-width Asian punctuation
        "**注意：**",
        "**重要。**",
        "**什么？**",
        "**警告！**",
        // Mixed punctuation
        "**Really?!**",
        "**Note:!**",
        // With whitespace
        "  **Arguments:**  ",
        "\t*Options:*\t",
    ];

    for content in not_flagged_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "MD036 should not flag '{}' (emphasis with punctuation) but got: {:?}",
            content.trim(),
            warnings
        );
    }
}

#[test]
fn test_custom_punctuation_config() {
    // Test with custom punctuation configuration
    let content = r#"**Heading!**
**Another@**
**Custom#**
**Default:**"#;

    // With custom punctuation that includes ! @ # but not :
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD036NoEmphasisAsHeading::new("!@#".to_string());
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag "Default:" since : is not in punctuation list
    assert_eq!(warnings.len(), 1, "Should flag only one item");
    assert!(warnings[0].message.contains("Default:"));
}

#[test]
fn test_empty_punctuation_config() {
    // Test with empty punctuation (flag everything)
    let content = r#"**Heading:**
**Another.**
**Plain**"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD036NoEmphasisAsHeading::new("".to_string());
    let warnings = rule.check(&ctx).unwrap();

    // Should flag all three
    assert_eq!(
        warnings.len(),
        3,
        "Should flag all emphasis with empty punctuation config"
    );
}

#[test]
fn test_real_world_documentation_patterns() {
    // Test real-world documentation patterns
    let content = r#"# API Documentation

**Parameters:**
- `name` - The name parameter
- `value` - The value to set

**Returns:**
The function returns a boolean indicating success.

**Example Usage**

```javascript
const result = myFunction("test", 42);
```

**Note:** This function may throw exceptions.

**See Also:**
- Related function A
- Related function B

**TODO**

**Questions?** Contact support@example.com
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?。，；：！？".to_string());
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag "Example Usage" and "TODO"
    assert_eq!(warnings.len(), 2, "Should flag exactly 2 items");

    let messages: Vec<String> = warnings.iter().map(|w| w.message.clone()).collect();
    assert!(messages.iter().any(|m| m.contains("Example Usage")));
    assert!(messages.iter().any(|m| m.contains("TODO")));
}

#[test]
fn test_fix_without_punctuation() {
    // Test that MD036 detects emphasis without punctuation but doesn't auto-fix
    let test_cases = vec![
        "**Introduction**",
        "*Setup*",
        "**Configure**",
        "__Note__",
        "**What**",
        "**First Second**",
        "**Comma**",
    ];

    for content in test_cases {
        let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());

        // Should be flagged
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            1,
            "Emphasis without punctuation '{content}' should be flagged"
        );

        // MD036 no longer provides automatic fixes
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Content should remain unchanged for '{content}'");
    }
}
