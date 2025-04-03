use rumdl::rule::Rule;
use rumdl::rules::MD008ULStyle;

#[test]
fn test_valid_list_style() {
    let rule = MD008ULStyle::default(); // Default style is '*'
    
    // Testing a list with all asterisk markers should pass
    let content = "* Item 1\n  * Item 2\n    * Item 3";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty(), "Expected no warnings for consistent asterisk markers");
}

#[test]
fn test_invalid_list_style() {
    let rule = MD008ULStyle::default();
    // First marker is asterisk, so others should be asterisks too
    let content = "* Item 1\n  + Item 2\n    - Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);

    // First marker is dash, so others should be dashes too
    let rule = MD008ULStyle::new('-');
    let content = "- Item 1\n  + Item 2\n    * Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);

    // First marker is plus, so others should be pluses too
    let rule = MD008ULStyle::new('+');
    let content = "+ Item 1\n  - Item 2\n    * Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_mixed_list_style() {
    let rule = MD008ULStyle::default();
    // First marker is asterisk, so all should be asterisks
    let content = "* Item 1\n  * Item 2\n    + Item 3\n      - Item 4";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_fix_list_style() {
    let rule = MD008ULStyle::default();
    // First marker is asterisk, so all should be fixed to asterisks
    let content = "* Item 1\n  + Item 2\n    - Item 3";
    let result = rule.fix(content).unwrap();
    let expected = "* Item 1\n  * Item 2\n    * Item 3";
    assert_eq!(result, expected);

    // First marker is dash, so all should be fixed to dashes
    let rule = MD008ULStyle::new('-');
    let content = "- Item 1\n  + Item 2\n    * Item 3";
    let result = rule.fix(content).unwrap();
    let expected = "- Item 1\n  - Item 2\n    - Item 3";
    assert_eq!(result, expected);

    // First marker is plus, so all should be fixed to pluses
    let rule = MD008ULStyle::new('+');
    let content = "+ Item 1\n  - Item 2\n    * Item 3";
    let result = rule.fix(content).unwrap();
    let expected = "+ Item 1\n  + Item 2\n    + Item 3";
    assert_eq!(result, expected);
}

#[test]
fn test_code_block_skipping() {
    let rule = MD008ULStyle::default();
    let content =
        "* Item 1\n\n```\n* Not a real list item\n+ Also not a real item\n```\n\n* Item 2";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_explicitly_configured_style() {
    // When explicitly configured with a specific style,
    // it should enforce that style regardless of what's in the document
    let rule = MD008ULStyle::new('-');
    let content = "* Item 1\n  * Item 2\n    * Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);

    let fixed = rule.fix(content).unwrap();
    let expected = "- Item 1\n  - Item 2\n    - Item 3";
    assert_eq!(fixed, expected);
}
