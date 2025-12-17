use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD038NoSpaceInCode;

#[test]
fn test_md038_nested_backticks_not_flagged() {
    // When code spans contain nested backticks, they should not be flagged
    // to avoid breaking the nested structure
    let rule = MD038NoSpaceInCode::new(); // Even in strict mode

    let test_cases = vec![
        "Code with ` nested `code` example ` should not be flagged",
        "Double `` `backticks` inside `` should work",
        "Example `` code with ` nested ` backticks `` here",
    ];

    for case in test_cases {
        let ctx = LintContext::new(case, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            0,
            "Should not flag code spans with nested backticks: {case}"
        );
    }
}

#[test]
fn test_md038_still_detects_regular_spaces() {
    let rule = MD038NoSpaceInCode::new();

    // CommonMark: Single space at BOTH ends is valid (spaces are stripped)
    // These all have space at both ends, so they should NOT be flagged
    let valid_commonmark_cases = vec![
        "Code with ` plain text ` should not flag spaces",
        "Double ` spaces here ` with spaces",
        "Example ` code ` here",
    ];

    for case in valid_commonmark_cases {
        let ctx = LintContext::new(case, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Single space at both ends is valid CommonMark: {case}"
        );
    }

    // Cases with space at only ONE end should be flagged
    let invalid_cases = vec![
        "Code with ` leading only` should flag",
        "Code with `trailing only ` should flag",
    ];

    for case in invalid_cases {
        let ctx = LintContext::new(case, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(!warnings.is_empty(), "Space at only one end should be flagged: {case}");
    }
}

#[test]
fn test_md038_lenient_mode_allows_nested_backticks() {
    let rule = MD038NoSpaceInCode::new(); // Default lenient mode

    // In lenient mode, nested backticks should also not be flagged
    let test_cases = vec![
        "Code with ` nested `code` example ` in lenient mode",
        "Double `` `backticks` inside `` in lenient mode",
    ];

    for case in test_cases {
        let ctx = LintContext::new(case, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            0,
            "Should not flag nested backticks in lenient mode: {case}"
        );
    }
}

#[test]
fn test_md038_documentation_example() {
    // Document this behavior for users
    let rule = MD038NoSpaceInCode::new();

    // This example shows why we don't fix spaces when backticks are nested
    let content = "To show a backtick in code, use `` ` `` or ``` `` ```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let warnings = rule.check(&ctx).unwrap();

    assert_eq!(warnings.len(), 0, "Nested backtick examples should not be flagged");
}
