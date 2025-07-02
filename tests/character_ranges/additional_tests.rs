//! Additional Character Range Tests
//!
//! This module contains character range tests for additional rules
//! beyond the basic set.

use super::{ExpectedWarning, multi_warning_test, simple_test, test_character_ranges};

// MD003 - Heading style consistency
#[test]
fn test_md003_heading_style() {
    let test = simple_test(
        "MD003",
        "# ATX Heading\n\nSetext Heading\n==============",
        ExpectedWarning::new(3, 1, 3, 15, "Setext Heading"),
    );
    test_character_ranges(test);
}

// MD004 - Unordered list style consistency
#[test]
fn test_md004_unordered_list_style() {
    let test = simple_test(
        "MD004",
        "- First item\n* Second item",
        ExpectedWarning::new(2, 1, 2, 2, "*"),
    );
    test_character_ranges(test);
}

// MD005 - List indentation consistency
#[test]
fn test_md005_list_indentation() {
    let test = simple_test(
        "MD005",
        "- Item 1\n  - Nested item\n   - Wrong indent",
        ExpectedWarning::new(3, 1, 3, 4, "   "),
    );
    test_character_ranges(test);
}

// MD022 - Blanks around headings
#[test]
fn test_md022_blanks_around_headings() {
    let test = multi_warning_test(
        "MD022",
        "Some text\n# Heading\nMore text",
        vec![
            ExpectedWarning::new(2, 1, 2, 10, "# Heading"), // Missing blank above
            ExpectedWarning::new(2, 1, 2, 10, "# Heading"), // Missing blank below
        ],
    );
    test_character_ranges(test);
}

// Test multiple warnings in one document
#[test]
fn test_multiple_md004_warnings() {
    let test = multi_warning_test(
        "MD004",
        "- First item\n* Second item\n+ Third item",
        vec![
            ExpectedWarning::new(2, 1, 2, 2, "*"),
            ExpectedWarning::new(3, 1, 3, 2, "+"),
        ],
    );
    test_character_ranges(test);
}

// Test MD003 with mixed heading styles
#[test]
fn test_md003_mixed_styles() {
    let test = multi_warning_test(
        "MD003",
        "# First ATX\n\nSecond Heading\n--------------\n\n### Third ATX\n\nFourth Heading\n==============",
        vec![
            ExpectedWarning::new(3, 1, 3, 15, "Second Heading"),
            ExpectedWarning::new(8, 1, 8, 15, "Fourth Heading"),
        ],
    );
    test_character_ranges(test);
}

// Test edge cases
#[test]
fn test_md022_multiple_blank_lines() {
    // Test when there are already some blank lines but not enough
    let test = simple_test(
        "MD022",
        "Text\n\n# Heading\nMore text",
        ExpectedWarning::new(3, 1, 3, 10, "# Heading"),
    );
    test_character_ranges(test);
}

// Test MD005 with deeply nested lists
#[test]
fn test_md005_deep_nesting() {
    let test = simple_test(
        "MD005",
        "- Level 1\n  - Level 2\n    - Level 3\n     - Wrong level 4",
        ExpectedWarning::new(4, 1, 4, 6, "     "),
    );
    test_character_ranges(test);
}
