use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD035HRStyle;

#[test]
fn test_valid_hr_style() {
    let rule = MD035HRStyle::default();
    let content = "Some text\n\n---\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_hr_style() {
    let rule = MD035HRStyle::default();
    let content = "Some text\n\n***\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_mixed_hr_styles() {
    let rule = MD035HRStyle::default();
    let content = "Some text\n\n---\n\nMiddle text\n\n***\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_fix_hr_style() {
    let rule = MD035HRStyle::default();
    let content = "Some text\n\n***\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "Some text\n\n---\n\nMore text");
}

#[test]
fn test_indented_hr() {
    let rule = MD035HRStyle::default();
    let content = "Some text\n\n  ***\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Some text\n\n---\n\nMore text");
}

#[test]
fn test_spaced_hr() {
    let rule = MD035HRStyle::default();
    let content = "Some text\n\n* * *\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Some text\n\n---\n\nMore text");
}

#[test]
fn test_consistent_style_first_hr_asterisks() {
    let rule = MD035HRStyle::new("consistent".to_string());
    let content = "Some text\n\n***\n\n---\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Some text\n\n***\n\n***\n\nMore text");
}

#[test]
fn test_consistent_style_first_hr_underscores() {
    let rule = MD035HRStyle::new("consistent".to_string());
    let content = "Some text\n\n___\n\n***\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Some text\n\n___\n\n___\n\nMore text");
}

#[test]
fn test_consistent_style_no_hr_defaults_to_dash() {
    let rule = MD035HRStyle::new("consistent".to_string());
    let content = "Some text\n\nNo HR here\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content); // No HR, so nothing to fix
}

#[test]
fn test_empty_string_style_behaves_like_consistent() {
    let rule = MD035HRStyle::new("".to_string());
    let content = "Some text\n\n***\n\n---\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "Some text\n\n***\n\n***\n\nMore text");
}

#[test]
fn test_consistent_style_most_prevalent_dash() {
    let rule = MD035HRStyle::new("consistent".to_string());
    let content = "Some text\n\n---\n\n***\n\n---\n\nMore text\n\n***";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    // '---' is most prevalent (2 vs 2, but '---' appears first)
    assert_eq!(fixed, "Some text\n\n---\n\n---\n\n---\n\nMore text\n\n---");
}

#[test]
fn test_consistent_style_most_prevalent_asterisk() {
    let rule = MD035HRStyle::new("consistent".to_string());
    let content = "Some text\n\n***\n\n---\n\n***\n\nMore text\n\n***";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    // '***' is most prevalent (3 times)
    assert_eq!(fixed, "Some text\n\n***\n\n***\n\n***\n\nMore text\n\n***");
}

#[test]
fn test_consistent_style_tie_first_encountered() {
    let rule = MD035HRStyle::new("consistent".to_string());
    let content = "Some text\n\n***\n\n---\n\n---\n\n***\n\nMore text";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    // Both '***' and '---' appear twice, but '***' is first
    assert_eq!(fixed, "Some text\n\n***\n\n***\n\n***\n\n***\n\nMore text");
}

#[test]
fn test_empty_string_style_most_prevalent() {
    let rule = MD035HRStyle::new("".to_string());
    let content = "Some text\n\n___\n\n***\n\n___\n\nMore text\n\n***";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(!result.is_empty());
    let fixed = rule.fix(&ctx).unwrap();
    // '___' is most prevalent (2 vs 2, but '___' appears first)
    assert_eq!(fixed, "Some text\n\n___\n\n___\n\n___\n\nMore text\n\n___");
}
