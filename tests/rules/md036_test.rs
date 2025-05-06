use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD036NoEmphasisAsHeading;

#[test]
fn test_valid_emphasis() {
    let rule = MD036NoEmphasisAsHeading;
    let content = "This is *emphasized* text\nThis text is also *emphasized*";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_only() {
    let rule = MD036NoEmphasisAsHeading;
    let content = "*Emphasized*\n_Also emphasized_";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Emphasized\n# Also emphasized");
}

#[test]
fn test_strong_only() {
    let rule = MD036NoEmphasisAsHeading;
    let content = "**Strong emphasis**\n__Also strong__";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "## Strong emphasis\n## Also strong");
}

#[test]
fn test_emphasis_in_code_block() {
    let rule = MD036NoEmphasisAsHeading;
    let content = "```\n*Emphasized*\n```\n\n*Emphasized*";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "```\n*Emphasized*\n```\n\n# Emphasized");
}

#[test]
fn test_multiple_emphasis() {
    let rule = MD036NoEmphasisAsHeading;
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
    let rule = MD036NoEmphasisAsHeading;
    let content = "The *second* word\nA _middle_ emphasis";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_first_word_only() {
    let rule = MD036NoEmphasisAsHeading;
    let content = "*First* word emphasized\n**First** word strong";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_emphasis() {
    let rule = MD036NoEmphasisAsHeading;
    let content = "*First* is _second_ emphasis\n**First** is __second__ strong";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_with_punctuation() {
    let rule = MD036NoEmphasisAsHeading;
    let content = "\n*Hello with punctuation!*\n\n*Hi there!*\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "\n# Hello with punctuation!\n\n# Hi there!\n");
}
