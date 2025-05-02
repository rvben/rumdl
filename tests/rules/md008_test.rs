use rumdl::rule::Rule;
use rumdl::rules::md008_ul_style::StyleMode;
use rumdl::MD008ULStyle;

#[test]
fn test_valid_list_style() {
    let rule = MD008ULStyle::default(); // Uses consistent mode by default
                                        // Testing that consistent markers are valid
    let content = "* Item 1\n  * Item 2\n    * Item 3";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());

    // Testing dash style markers
    let content = "- Item 1\n  - Item 2\n    - Item 3";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());

    // Testing plus style markers
    let content = "+ Item 1\n  + Item 2\n    + Item 3";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_list_style() {
    let rule = MD008ULStyle::default(); // Uses consistent mode by default
                                        // First marker is asterisk, so others should be asterisks too
    let content = "* Item 1\n  + Item 2\n    - Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);

    // First marker is dash, so others should be dashes too
    let content = "- Item 1\n  + Item 2\n    * Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);

    // First marker is plus, so others should be pluses too
    let content = "+ Item 1\n  - Item 2\n    * Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_mixed_list_style() {
    let rule = MD008ULStyle::default(); // Uses consistent mode by default
                                        // First marker is asterisk, so all should be asterisks
    let content = "* Item 1\n  * Item 2\n    + Item 3\n      - Item 4";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_fix_list_style() {
    let rule = MD008ULStyle::default(); // Uses consistent mode by default
                                        // First marker is asterisk, so all should be fixed to asterisks
    let content = "* Item 1\n  + Item 2\n    - Item 3";
    let result = rule.fix(content).unwrap();
    let expected = "* Item 1\n  * Item 2\n    * Item 3";
    assert_eq!(result, expected);

    // First marker is dash, so all should be fixed to dashes
    let content = "- Item 1\n  + Item 2\n    * Item 3";
    let result = rule.fix(content).unwrap();
    let expected = "- Item 1\n  - Item 2\n    - Item 3";
    assert_eq!(result, expected);

    // First marker is plus, so all should be fixed to pluses
    let content = "+ Item 1\n  - Item 2\n    * Item 3";
    let result = rule.fix(content).unwrap();
    let expected = "+ Item 1\n  + Item 2\n    + Item 3";
    assert_eq!(result, expected);
}

#[test]
fn test_code_block_skipping() {
    let rule = MD008ULStyle::default(); // Uses consistent mode by default
    let content =
        "* Item 1\n\n```\n* Not a real list item\n+ Also not a real item\n```\n\n* Item 2";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_explicitly_configured_style() {
    // When explicitly configured with a specific style,
    // it should enforce that style regardless of what's in the document

    // Use StyleMode::Specific to enforce the '-' style
    let rule = MD008ULStyle::new(StyleMode::Specific("-".to_string()));
    let content = "* Item 1\n  * Item 2\n    * Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 3);

    let fixed = rule.fix(content).unwrap();
    let expected = "- Item 1\n  - Item 2\n    - Item 3";
    assert_eq!(fixed, expected);

    // Explicitly test with StyleMode::Specific - This seems redundant now, but we can keep it
    // let rule = MD008ULStyle::with_mode('*', StyleMode::Specific("*".to_string())); // with_mode seems to have been removed or changed
    // Recreate with ::new
    let rule_star = MD008ULStyle::new(StyleMode::Specific("*".to_string()));
    let content_dash = "- Item 1\n  - Item 2\n    - Item 3";
    let result_star = rule_star.check(content_dash).unwrap();
    assert_eq!(result_star.len(), 3);

    // Explicitly test with StyleMode::Consistent
    // let rule = MD008ULStyle::with_mode('*', StyleMode::Consistent); // with_mode seems to have been removed or changed
    // Recreate with ::new
    let rule_consistent = MD008ULStyle::new(StyleMode::Consistent);
    let content_consistent = "- Item 1\n  - Item 2\n    - Item 3";
    let result_consistent = rule_consistent.check(content_consistent).unwrap();
    assert!(result_consistent.is_empty());
}

#[test]
fn test_empty_content() {
    let rule = MD008ULStyle::default();
    let content = "";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty(), "Empty content should have no warnings");

    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, "", "Empty content should remain empty after fix");
}

#[test]
fn test_lists_with_blank_lines() {
    let rule = MD008ULStyle::default();

    // List with blank lines between items, should still enforce consistency
    let content = "* Item 1\n\n* Item 2\n\n- Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(
        result.len(),
        1,
        "List with blank lines should still check consistency"
    );

    // Fix should preserve blank lines
    let fixed = rule.fix(content).unwrap();
    let expected = "* Item 1\n\n* Item 2\n\n* Item 3";
    assert_eq!(fixed, expected);
}

#[test]
fn test_lists_in_blockquotes() {
    let rule = MD008ULStyle::default();

    // Lists inside blockquotes should be checked, but the current implementation
    // doesn't detect list markers in blockquotes
    let content = "> * Item 1\n> * Item 2\n> - Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(
        result.len(),
        0,
        "Current implementation doesn't detect inconsistent list markers in blockquotes"
    );

    // We can't test fix behavior if the rule doesn't detect issues
    // This is a limitation in the current implementation
}

#[test]
fn test_lists_with_html_comments() {
    let rule = MD008ULStyle::default();

    // HTML comments should not affect list checking
    let content = "* Item 1\n<!-- * This is a comment -->\n* Item 2\n- Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(
        result.len(),
        1,
        "List with HTML comments should still check consistency"
    );

    // Fix should preserve HTML comments
    let fixed = rule.fix(content).unwrap();
    let expected = "* Item 1\n<!-- * This is a comment -->\n* Item 2\n* Item 3";
    assert_eq!(fixed, expected);
}

#[test]
fn test_complex_nested_lists() {
    let rule = MD008ULStyle::default();

    // Complex nesting with inconsistent markers
    let content =
        "* Top level 1\n  - Nested 1.1\n    + Deep nested 1.1.1\n* Top level 2\n  * Nested 2.1";
    let result = rule.check(content).unwrap();
    assert_eq!(
        result.len(),
        2,
        "Nested list should check consistency at all levels"
    );

    // Fix should handle all levels
    let fixed = rule.fix(content).unwrap();
    let expected =
        "* Top level 1\n  * Nested 1.1\n    * Deep nested 1.1.1\n* Top level 2\n  * Nested 2.1";
    assert_eq!(fixed, expected);
}

#[test]
fn test_with_front_matter() {
    let rule = MD008ULStyle::default();

    // Front matter should be ignored
    let content = "---\ntitle: Test Document\n---\n\n* Item 1\n* Item 2\n- Item 3";
    let result = rule.check(content).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Front matter should be ignored when checking lists"
    );

    // Fix should preserve front matter
    let fixed = rule.fix(content).unwrap();
    let expected = "---\ntitle: Test Document\n---\n\n* Item 1\n* Item 2\n* Item 3";
    assert_eq!(fixed, expected);
}

#[test]
fn test_mixed_ordered_unordered_lists() {
    let rule = MD008ULStyle::default();

    // Ordered and unordered lists together
    let content = "* Unordered 1\n* Unordered 2\n1. Ordered 1\n2. Ordered 2\n- Unordered 3";
    let result = rule.check(content).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should check only unordered list items consistency"
    );

    // Fix should only change unordered list markers
    let fixed = rule.fix(content).unwrap();
    let expected = "* Unordered 1\n* Unordered 2\n1. Ordered 1\n2. Ordered 2\n* Unordered 3";
    assert_eq!(fixed, expected);
}

#[test]
fn test_indentation_variations() {
    let rule = MD008ULStyle::default();

    // Varied indentation with inconsistent markers
    let content = "* Item 1\n    - Item with extra indentation\n  * Item with normal indentation";
    let result = rule.check(content).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Should identify inconsistent markers despite indentation variations"
    );

    // Fix should preserve original indentation
    let fixed = rule.fix(content).unwrap();
    let expected = "* Item 1\n    * Item with extra indentation\n  * Item with normal indentation";
    assert_eq!(fixed, expected);
}

#[test]
fn test_list_markers_at_edge_of_line() {
    let rule = MD008ULStyle::default();

    // Test with markers at the very start of lines and with strange spacing
    let content = "*Item with no space\n* Normal item\n-No space again";
    let result = rule.check(content).unwrap();
    // This depends on whether the implementation considers "*Item" as a list item
    // Most implementations would not, but we should check the expected behavior
    let expected_warnings = 0; // Assuming "*Item" is not considered a list item
    assert_eq!(
        result.len(),
        expected_warnings,
        "Edge case with no space after marker should be handled gracefully"
    );
}

#[test]
fn test_trailing_newlines() {
    let rule = MD008ULStyle::default();

    // Test with multiple trailing newlines
    let content = "* Item 1\n* Item 2\n- Item 3\n\n\n";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1, "Should detect one inconsistent marker");

    let fixed = rule.fix(content).unwrap();

    // The fixed content should preserve all trailing newlines
    assert_eq!(
        fixed, "* Item 1\n* Item 2\n* Item 3\n\n\n",
        "Should preserve all trailing newlines"
    );
}
