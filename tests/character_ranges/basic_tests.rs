//! Basic Character Range Tests
//!
//! This module contains basic tests to verify the character range testing framework
//! works correctly with a few representative rules.

use super::{multi_warning_test, simple_test, test_character_ranges, ExpectedWarning};

#[test]
fn test_md001_heading_increment() {
    let test = simple_test(
        "MD001",
        "# Heading 1\n### Heading 3",
        ExpectedWarning::new(2, 1, 2, 14, "### Heading 3"),
    );
    test_character_ranges(test);
}

#[test]
fn test_md002_first_heading_h1() {
    let test = simple_test(
        "MD002",
        "## Second level heading",
        ExpectedWarning::new(1, 1, 1, 24, "## Second level heading"),
    );
    test_character_ranges(test);
}

#[test]
fn test_md018_missing_space_atx() {
    let test = simple_test(
        "MD018",
        "#Heading without space",
        ExpectedWarning::new(1, 2, 1, 2, ""),
    );
    test_character_ranges(test);
}

#[test]
fn test_md019_multiple_spaces_atx() {
    let test = simple_test(
        "MD019",
        "##  Heading with multiple spaces",
        ExpectedWarning::new(1, 3, 1, 5, "  "),
    );
    test_character_ranges(test);
}

#[test]
fn test_md009_trailing_spaces() {
    let test = simple_test(
        "MD009",
        "Line with trailing spaces   ",
        ExpectedWarning::new(1, 26, 1, 29, "   "),
    );
    test_character_ranges(test);
}

#[test]
fn test_md012_multiple_blank_lines() {
    let test = simple_test(
        "MD012",
        "Line 1\n\n\nLine 2",
        ExpectedWarning::new(3, 1, 3, 1, ""),
    );
    test_character_ranges(test);
}

#[test]
fn test_md026_trailing_punctuation() {
    let test = simple_test(
        "MD026",
        "# Heading with punctuation!",
        ExpectedWarning::new(1, 27, 1, 28, "!"),
    );
    test_character_ranges(test);
}

#[test]
fn test_multiple_warnings_same_rule() {
    let test = multi_warning_test(
        "MD018",
        "#First heading\n#Second heading",
        vec![
            ExpectedWarning::new(1, 2, 1, 2, ""),
            ExpectedWarning::new(2, 2, 2, 2, ""),
        ],
    );
    test_character_ranges(test);
}

#[test]
fn test_md036_emphasis_as_heading() {
    let test = simple_test(
        "MD036",
        "*Emphasis used as heading*",
        ExpectedWarning::new(1, 1, 1, 27, "*Emphasis used as heading*"),
    );
    test_character_ranges(test);
}

#[test]
fn test_md037_spaces_around_emphasis() {
    let test = simple_test(
        "MD037",
        "This is * text with spaces *",
        ExpectedWarning::new(1, 9, 1, 29, "* text with spaces *"),
    );
    test_character_ranges(test);
}

#[test]
fn test_md038_spaces_around_code() {
    let test = simple_test(
        "MD038",
        "This is ` code with spaces `",
        ExpectedWarning::new(1, 9, 1, 28, "` code with spaces "),
    );
    test_character_ranges(test);
}

#[test]
fn test_md040_fenced_code_language() {
    let test = simple_test(
        "MD040",
        "```\ncode without language\n```",
        ExpectedWarning::new(1, 1, 1, 4, "```"),
    );
    test_character_ranges(test);
}
