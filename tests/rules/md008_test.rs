use rumdl::rules::MD008ULStyle;
use rumdl::rule::Rule;

#[test]
fn test_valid_list_style() {
    let rule = MD008ULStyle::default();
    let content = "* Item 1\n  * Item 2\n    * Item 3";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_list_style() {
    let rule = MD008ULStyle::default();
    let content = "* Item 1\n  + Item 2\n    - Item 3";
    let result = rule.check(content).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_mixed_list_style() {
    let rule = MD008ULStyle::default();
    let content = "* Item 1\n  * Item 2\n    + Item 3\n      - Item 4";
    let result = rule.check(content).unwrap();
    assert!(!result.is_empty());
}

#[test]
fn test_fix_list_style() {
    let rule = MD008ULStyle::default();
    let content = "* Item 1\n  + Item 2\n    - Item 3";
    let result = rule.fix(content).unwrap();
    assert_eq!(result, "* Item 1\n  * Item 2\n    * Item 3");
} 