use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD036NoEmphasisAsHeading;

#[test]
fn test_valid_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "This is *emphasized* text\nThis text is also *emphasized*";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_only() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "*Emphasized*\n_Also emphasized_";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Emphasized\n# Also emphasized");
}

#[test]
fn test_strong_only() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "**Strong**\n__Also strong__";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "## Strong\n## Also strong");
}

#[test]
fn test_multiple_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "\n*First emphasis*\n\nNormal line\n\n_Second emphasis_\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "\n# First emphasis\n\nNormal line\n\n# Second emphasis\n"
    );
}

#[test]
fn test_not_first_word() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "The *second* word\nA _middle_ emphasis";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_first_word_only() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "*First* word emphasized\n**First** word strong";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "*First* is _second_ emphasis\n**First** is __second__ strong";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_with_punctuation() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "\n*Hello with punctuation!*\n\n*Hi there!*\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    // Trailing punctuation should be removed from headings
    assert_eq!(fixed, "\n# Hello with punctuation\n\n# Hi there\n");
}

#[test]
fn test_emphasis_with_trailing_colon() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "**Example output:**";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    // Colon should be removed from heading
    assert_eq!(fixed, "## Example output");
}

#[test]
fn test_preserve_punctuation_when_disabled() {
    let rule = MD036NoEmphasisAsHeading::new("".to_string());
    let content = "**Example output:**";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    // Colon should be preserved when punctuation removal is disabled
    assert_eq!(fixed, "## Example output:");
}

#[test]
fn test_emphasis_with_various_punctuation() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "**Title with period.**\n\n*Question?*\n\n**Exclamation!**\n\n*Semicolon;*";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 4);
    let fixed = rule.fix(&ctx).unwrap();
    // All trailing punctuation should be removed
    assert_eq!(
        fixed,
        "## Title with period\n\n# Question\n\n## Exclamation\n\n# Semicolon"
    );
}

#[test]
fn test_emphasis_in_list() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "- *Not a heading*\n  - **Also not a heading**";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_in_blockquote() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "> *Not a heading*\n> **Also not a heading**";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_in_code_block() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "```\n*Not a heading*\n**Also not a heading**\n```";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_existing_heading() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "# Already a heading\n## Also a heading";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_with_other_text() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "This line has *emphasis* in it\nThis line has **strong** text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "**\n__";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_partial_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "*Incomplete emphasis\n**Incomplete strong";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_table_of_contents_labels() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "**Table of Contents**\n\n*Contents*\n\n__TOC__\n\n_Index_";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // None of these should be flagged as they are legitimate TOC labels
    assert!(result.is_empty());
}

#[test]
fn test_table_of_contents_with_other_emphasis() {
    let rule = MD036NoEmphasisAsHeading::new(".,;:!?".to_string());
    let content = "**Table of Contents**\n\n**This should be a heading**\n\n*Contents*\n\n*This should also be a heading*";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Only the non-TOC emphasis should be flagged
    assert_eq!(result.len(), 2);
    assert!(result[0].message.contains("This should be a heading"));
    assert!(result[1].message.contains("This should also be a heading"));
}
