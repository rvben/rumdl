use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD012NoMultipleBlanks;

#[test]
fn test_md012_valid() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n\nLine 2\n\nLine 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md012_invalid() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 3);
    assert_eq!(
        result[0].message,
        "Multiple consecutive blank lines between content (Expected: 1; Actual: 2)"
    );
    assert_eq!(result[1].line, 6);
    assert_eq!(
        result[1].message,
        "Multiple consecutive blank lines between content (Expected: 1; Actual: 3)"
    );
    assert_eq!(result[2].line, 7);
    assert_eq!(
        result[2].message,
        "Multiple consecutive blank lines between content (Expected: 1; Actual: 3)"
    );
}

#[test]
fn test_md012_start_end() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "\n\nLine 1\nLine 2\n\n\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line, 2);
    assert_eq!(
        result[0].message,
        "Multiple consecutive blank lines at start of file (Expected: 1; Actual: 2)"
    );
    assert_eq!(result[1].line, 6);
    assert_eq!(
        result[1].message,
        "Multiple consecutive blank lines at end of file (Expected: 1; Actual: 2)"
    );
}

#[test]
fn test_md012_code_blocks() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n\n```\n\n\nCode\n\n\n```\nLine 2\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Multiple blank lines in code blocks are allowed
}

#[test]
fn test_md012_front_matter() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "---\ntitle: Test\n\n\ndescription: Test\n---\n\nContent\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Multiple blank lines in front matter are allowed
}

#[test]
fn test_md012_custom_maximum() {
    let rule = MD012NoMultipleBlanks::new(2);
    let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Only the second group (3 blanks > maximum 2) is invalid, reporting 1 excess line
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 7); // The third blank line in the second sequence
    assert_eq!(
        result[0].message,
        "Multiple consecutive blank lines between content (Expected: 2; Actual: 3)"
    );
}

#[test]
fn test_md012_fix() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n\n\nLine 2\n\n\n\nLine 3\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Line 1\n\nLine 2\n\nLine 3\n");
}

#[test]
fn test_md012_fix_with_code_blocks() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n\n\n```\n\n\nCode\n\n\n```\nLine 2\n\n\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Line 1\n\n```\n\n\nCode\n\n\n```\nLine 2\n\n");
}

#[test]
fn test_md012_fix_with_front_matter() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "---\ntitle: Test\n\n\ndescription: Test\n---\n\n\n\nContent\n";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(
        result,
        "---\ntitle: Test\n\n\ndescription: Test\n---\n\nContent\n"
    );
}

#[test]
fn test_md012_whitespace_lines() {
    let rule = MD012NoMultipleBlanks::default();
    let content = "Line 1\n  \n \t \nLine 2\n";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    // Expects 1 warning for the excess whitespace line
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 3); // The second whitespace line is excess
    assert_eq!(
        result[0].message,
        "Multiple consecutive blank lines between content (Expected: 1; Actual: 2)"
    );
}
