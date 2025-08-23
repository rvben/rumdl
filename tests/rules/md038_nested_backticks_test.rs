use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD038NoSpaceInCode;

#[test]
fn test_md038_nested_backticks_not_flagged() {
    // When code spans contain nested backticks, they should not be flagged
    // to avoid breaking the nested structure
    let rule = MD038NoSpaceInCode::strict(); // Even in strict mode

    let test_cases = vec![
        "Code with ` nested `code` example ` should not be flagged",
        "Double `` `backticks` inside `` should work",
        "Example `` code with ` nested ` backticks `` here",
    ];

    for case in test_cases {
        let ctx = LintContext::new(case);
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
    let rule = MD038NoSpaceInCode::strict();

    // These should still be detected as having spaces (no nested backticks)
    let cases_with_warnings = vec![
        "Code with ` plain text ` should flag spaces",
        "Double ` spaces here ` with spaces",
        "Example ` code ` here",
    ];

    for case in cases_with_warnings {
        let ctx = LintContext::new(case);
        let warnings = rule.check(&ctx).unwrap();
        assert!(!warnings.is_empty(), "Should detect spaces in: {case}");
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
        let ctx = LintContext::new(case);
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
    let ctx = LintContext::new(content);
    let warnings = rule.check(&ctx).unwrap();

    assert_eq!(warnings.len(), 0, "Nested backtick examples should not be flagged");
}
