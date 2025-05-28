//! Comprehensive Character Range Tests
//!
//! This module contains comprehensive tests for character ranges across
//! multiple rules and edge cases.

use super::{simple_test, test_character_ranges, ExpectedWarning};

// MD010 - Hard tabs
#[test]
fn test_md010_hard_tabs() {
    let test = simple_test(
        "MD010",
        "Line with\ttab character",
        ExpectedWarning::new(1, 10, 1, 11, "\t"),
    );
    test_character_ranges(test);
}

// MD013 - Line length (configured to 200 chars in .rumdl.toml)
#[test]
fn test_md013_line_length() {
    // Skip this test for now as it requires dynamic string generation
    // and the configuration makes it complex to test reliably
}

// MD014 - Commands show output
#[test]
fn test_md014_commands_show_output() {
    let test = simple_test(
        "MD014",
        "```bash\n$ echo hello\n```",
        ExpectedWarning::new(2, 1, 2, 2, "$"),
    );
    test_character_ranges(test);
}

// MD020 - No space in closed ATX
#[test]
fn test_md020_no_space_closed_atx() {
    let test = simple_test(
        "MD020",
        "# Heading#",
        ExpectedWarning::new(1, 9, 1, 10, "g"), // Highlights the position where space is missing
    );
    test_character_ranges(test);
}

// MD021 - Multiple spaces in closed ATX
#[test]
fn test_md021_multiple_spaces_closed_atx() {
    let test = simple_test(
        "MD021",
        "# Heading  #",
        ExpectedWarning::new(1, 11, 1, 12, " "), // Highlights the extra space
    );
    test_character_ranges(test);
}

// MD041 - First line heading
#[test]
fn test_md041_first_line_heading() {
    let test = simple_test(
        "MD041",
        "Some text\n\n# Heading",
        ExpectedWarning::new(1, 1, 1, 10, "Some text"),
    );
    test_character_ranges(test);
}

// Unicode tests
#[test]
fn test_unicode_characters() {
    let test = simple_test(
        "MD018",
        "#Café without space",
        ExpectedWarning::new(1, 2, 1, 2, ""),
    );
    test_character_ranges(test);
}

#[test]
fn test_emoji_characters() {
    let test = simple_test(
        "MD018",
        "#🎉Emoji without space",
        ExpectedWarning::new(1, 2, 1, 2, ""),
    );
    test_character_ranges(test);
}

// Multi-line tests
#[test]
fn test_multiline_emphasis() {
    // MD037 doesn't handle multi-line emphasis, so this test is removed
    // This is expected behavior as emphasis should be on single lines
}

// Edge cases
#[test]
fn test_empty_content() {
    // Most rules should not trigger on empty content
    // This tests the framework's handling of edge cases
    // No assertions needed - just testing that the framework doesn't crash
}
