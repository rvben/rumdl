use rumdl::rule::Rule;
use rumdl::rules::MD018NoMissingSpaceAtx;

#[test]
fn test_valid_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "# Heading 1\n## Heading 2\n### Heading 3";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "#Heading 1\n##Heading 2\n###Heading 3";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 2);
    assert_eq!(result[0].message, "No space after # in ATX style heading");
}

#[test]
fn test_mixed_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "# Heading 1\n##Heading 2\n### Heading 3\n####Heading 4";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_code_block() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "```markdown\n#Not a heading\n##Also not a heading\n```\n# Real Heading";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "#Heading 1\n##Heading 2\n###Heading 3";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "# Heading 1\n## Heading 2\n### Heading 3");
}

#[test]
fn test_fix_mixed_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "# Heading 1\n##Heading 2\n### Heading 3\n####Heading 4";
    let result = rule.fix(content).unwrap();
    assert_eq!(
        result,
        "# Heading 1\n## Heading 2\n### Heading 3\n#### Heading 4"
    );
}

#[test]
fn test_preserve_code_blocks() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "# Real Heading\n```\n#Not a heading\n```\n# Another Heading";
    let result = rule.fix(content).unwrap();
    assert_eq!(
        result,
        "# Real Heading\n```\n#Not a heading\n```\n# Another Heading"
    );
}

#[test]
fn test_heading_with_multiple_hashes() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "######Heading 6";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].message,
        "No space after ###### in ATX style heading"
    );
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "###### Heading 6");
}

#[test]
fn test_not_a_heading() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "This is #not a heading\nAnd this is also #not a heading";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_closed_atx_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "#Heading 1 #\n##Heading 2 ##\n###Heading 3 ###";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "# Heading 1 #\n## Heading 2 ##\n### Heading 3 ###");
}

#[test]
fn test_multiple_spaces() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "# Heading with extra space\n#  Another heading";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_headings() {
    let rule = MD018NoMissingSpaceAtx::new();
    let content = "#\n##\n###";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}
