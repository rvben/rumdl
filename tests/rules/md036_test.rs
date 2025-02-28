use rumdl::rules::MD036NoEmphasisOnlyFirst;
use rumdl::rule::Rule;

#[test]
fn test_valid_emphasis() {
    let rule = MD036NoEmphasisOnlyFirst::default();
    let content = "This is *emphasized* text\nThis text is also *emphasized*";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_only() {
    let rule = MD036NoEmphasisOnlyFirst::default();
    let content = "*Emphasized*\n_Also emphasized_";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Emphasized\n# Also emphasized");
}

#[test]
fn test_strong_only() {
    let rule = MD036NoEmphasisOnlyFirst::default();
    let content = "**Strong emphasis**\n__Also strong__";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "## Strong emphasis\n## Also strong");
}

#[test]
fn test_emphasis_in_code_block() {
    let rule = MD036NoEmphasisOnlyFirst::default();
    let content = "```\n*Emphasized*\n```\n*Emphasized*";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "```\n*Emphasized*\n```\n# Emphasized");
}

#[test]
fn test_multiple_emphasis() {
    let rule = MD036NoEmphasisOnlyFirst::default();
    let content = "*First emphasis*\nNormal line\n_Second emphasis_";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# First emphasis\nNormal line\n# Second emphasis");
}

#[test]
fn test_not_first_word() {
    let rule = MD036NoEmphasisOnlyFirst::default();
    let content = "The *second* word\nA _middle_ emphasis";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_first_word_only() {
    let rule = MD036NoEmphasisOnlyFirst::default();
    let content = "*First* word emphasized\n**First** word strong";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_mixed_emphasis() {
    let rule = MD036NoEmphasisOnlyFirst::default();
    let content = "*First* is _second_ emphasis\n**First** is __second__ strong";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_emphasis_with_punctuation() {
    let rule = MD036NoEmphasisOnlyFirst::default();
    let content = "*Hello with punctuation!*\n*Hi there!*";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Hello with punctuation!\n# Hi there!");
} 