use rumdl::rule::Rule;
use rumdl::rules::MD038NoSpaceInCode;

#[test]
fn test_valid_code_spans() {
    let rule = MD038NoSpaceInCode::new();
    let content = "`code` and `another code` here";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_spaces_both_ends() {
    let rule = MD038NoSpaceInCode::new();
    let content = "` code ` and ` another code ` here";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "`code` and `another code` here");
}

#[test]
fn test_space_at_start() {
    let rule = MD038NoSpaceInCode::new();
    let content = "` code` and ` another code` here";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "`code` and `another code` here");
}

#[test]
fn test_space_at_end() {
    let rule = MD038NoSpaceInCode::new();
    let content = "`code ` and `another code ` here";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "`code` and `another code` here");
}

#[test]
fn test_code_in_code_block() {
    let rule = MD038NoSpaceInCode::new();
    let content = "```\n` code `\n```\n` code `";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "```\n` code `\n```\n`code`");
}

#[test]
fn test_multiple_code_spans() {
    let rule = MD038NoSpaceInCode::new();
    let content = "` code ` and ` another ` in one line";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "`code` and `another` in one line");
}

#[test]
fn test_code_with_internal_spaces() {
    let rule = MD038NoSpaceInCode::new();
    let content = "`this is code` and ` this is also code `";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "`this is code` and `this is also code`");
}

#[test]
fn test_code_with_punctuation() {
    let rule = MD038NoSpaceInCode::new();
    let content = "` code! ` and ` code? ` here";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "`code!` and `code?` here");
}
