use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD009TrailingSpaces;

#[test]
fn test_md009_valid() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line without trailing spaces\nAnother line without trailing spaces\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md009_invalid() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line with trailing spaces  \nAnother line with trailing spaces   \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the second line should be flagged (3 spaces)
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].message, "3 trailing spaces found");
}

#[test]
fn test_md009_empty_lines() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line without trailing spaces\n  \nAnother line without trailing spaces\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(
        result[0].message,
        "Empty line should not have trailing spaces"
    );
}

#[test]
fn test_md009_code_blocks() {
    let rule = MD009TrailingSpaces::default();
    let content = "Normal line\n```\nCode with spaces    \nMore code  \n```\nNormal line  \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // Code block spaces are allowed
}

#[test]
fn test_md009_strict_mode() {
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "Line with two spaces  \nCode block```\nWith spaces  \n```\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2); // Both lines should be flagged in strict mode
}

#[test]
fn test_md009_line_breaks() {
    let rule = MD009TrailingSpaces::default();
    let content = "This is a line  \nWith hard breaks  \nBut this has three   \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the line with 3 spaces should be flagged
    assert_eq!(result[0].line, 3);
}

#[test]
fn test_md009_custom_br_spaces() {
    let rule = MD009TrailingSpaces::new(3, false);
    let content = "Line with two spaces  \nLine with three   \n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only the line with 2 spaces should be flagged
    assert_eq!(result[0].line, 1);
}

#[test]
fn test_md009_fix() {
    let rule = MD009TrailingSpaces::default();
    let content = "Line with spaces   \nAnother line  \nNo spaces\n  \n```\nCode   \n```\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "Line with spaces  \nAnother line  \nNo spaces\n\n```\nCode   \n```\n"
    );
}

#[test]
fn test_md009_fix_strict() {
    let rule = MD009TrailingSpaces::new(2, true);
    let content = "Line with spaces   \nAnother line  \nNo spaces\n  \n```\nCode   \n```\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "Line with spaces\nAnother line\nNo spaces\n\n```\nCode\n```\n"
    );
}
