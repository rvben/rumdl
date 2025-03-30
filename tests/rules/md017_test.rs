use rumdl::rule::Rule;
use rumdl::rules::MD017NoEmphasisAsHeading;

#[test]
fn test_valid_headings() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content = "# Main Heading\n## Sub Heading\n### Third Level";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_valid_emphasis() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content = "Some text with *italic* and **bold** emphasis.\nA line with *multiple* **emphasis** markers.";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_single_emphasis() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content = "*Emphasized Heading*\n\n_Another Heading_";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 1);
    assert_eq!(
        result[0].message,
        "Single emphasis should not be used as a heading"
    );
}

#[test]
fn test_invalid_double_emphasis() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content = "**Bold Heading**\n\n__Yet Another Heading__";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 1);
    assert_eq!(
        result[0].message,
        "Double emphasis should not be used as a heading"
    );
}

#[test]
fn test_mixed_emphasis() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content =
        "*Single Emphasis*\n**Double Emphasis**\n_Single Underscore_\n__Double Underscore__";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 4);
}

#[test]
fn test_code_block() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content = "```markdown\n*Not a Heading*\n**Also Not a Heading**\n```\n# Real Heading";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_single_emphasis() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content = "*Emphasized Heading*\n\n_Another Heading_";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Emphasized Heading\n\n# Another Heading");
}

#[test]
fn test_fix_double_emphasis() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content = "**Bold Heading**\n\n__Yet Another Heading__";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "## Bold Heading\n\n## Yet Another Heading");
}

#[test]
fn test_fix_mixed_emphasis() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content = "*Single*\n**Double**\n_Single_\n__Double__";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Single\n## Double\n# Single\n## Double");
}

#[test]
fn test_emphasis_with_spaces() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content = "  *Indented Heading*  \n  **Indented Bold**  ";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "  # Indented Heading  \n  ## Indented Bold  ");
}

#[test]
fn test_preserve_code_blocks() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content = "# Real Heading\n```\n*Not a Heading*\n```\n# Another Heading";
    let result = rule.fix(content).unwrap();
    assert_eq!(
        result,
        "# Real Heading\n```\n*Not a Heading*\n```\n# Another Heading"
    );
}

#[test]
fn test_multiple_emphasis_markers() {
    let rule = MD017NoEmphasisAsHeading::new();
    let content = "*First* and *Second*\n**Bold** and **Also Bold**";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}
