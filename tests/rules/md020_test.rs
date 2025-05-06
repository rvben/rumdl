use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD020NoMissingSpaceClosedAtx;

#[test]
fn test_valid_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading 1#\n## Heading 2##\n### Heading 3###";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 1);
    assert_eq!(
        result[0].message,
        "Missing space inside hashes on closed ATX style heading with 1 hashes"
    );
}

#[test]
fn test_mixed_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading 1 #\n## Heading 2##\n### Heading 3 ###\n#### Heading 4####";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_code_block() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "```markdown\n# Not a heading#\n## Also not a heading##\n```\n# Real Heading #";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading 1#\n## Heading 2##\n### Heading 3###";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_fix_mixed_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Heading 1 #\n## Heading 2##\n### Heading 3 ###\n#### Heading 4####";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###\n#### Heading 4 ####"
    );
}

#[test]
fn test_preserve_code_blocks() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# Real Heading #\n```\n# Not a heading#\n```\n# Another Heading #";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "# Real Heading #\n```\n# Not a heading#\n```\n# Another Heading #"
    );
}

#[test]
fn test_heading_with_multiple_hashes() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "###### Heading 6######";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].message,
        "Missing space inside hashes on closed ATX style heading with 6 hashes"
    );
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "###### Heading 6 ######");
}

#[test]
fn test_not_a_heading() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "This is #not a heading#\nAnd this is also #not a heading#";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_indented_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "  # Heading 1#\n    ## Heading 2##\n      ### Heading 3###";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "  # Heading 1 #\n    ## Heading 2##\n      ### Heading 3###"
    );
}

#[test]
fn test_empty_closed_atx_headings() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "# #\n## ##\n### ###";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_missing_space_at_start() {
    let rule = MD020NoMissingSpaceClosedAtx::new();
    let content = "#Heading 1 #\n##Heading 2 ##\n###Heading 3 ###";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}
