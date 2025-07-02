//! Extended Character Range Tests
//!
//! This module contains character range tests for additional rules
//! covering MD006-MD053.

use super::{ExpectedWarning, multi_warning_test, simple_test, test_character_ranges};

// MD006 - Start bullets at beginning of line
#[test]
fn test_md006_start_bullets() {
    let test = simple_test("MD006", "  - Indented bullet", ExpectedWarning::new(1, 1, 1, 5, "  - "));
    test_character_ranges(test);
}

// MD007 - Unordered list indentation
#[test]
fn test_md007_ul_indent() {
    let test = simple_test(
        "MD007",
        "- Item 1\n   - Wrong indent",
        ExpectedWarning::new(2, 1, 2, 4, "   "),
    );
    test_character_ranges(test);
}

// MD008 - Unordered list style (commented out - rule not implemented)
// #[test]
// fn test_md008_ul_style() {
//     let test = simple_test(
//         "MD008",
//         "- First item\n+ Second item",
//         ExpectedWarning::new(2, 1, 2, 2, "+")
//     );
//     test_character_ranges(test);
// }

// MD011 - Reversed link syntax
#[test]
fn test_md011_reversed_links() {
    let test = simple_test(
        "MD011",
        "(Reversed link)[http://example.com]",
        ExpectedWarning::new(1, 1, 1, 36, "(Reversed link)[http://example.com]"),
    );
    test_character_ranges(test);
}

// MD023 - Headings must start at the beginning of the line
#[test]
fn test_md023_heading_start_left() {
    let test = simple_test("MD023", "  # Indented heading", ExpectedWarning::new(1, 1, 1, 3, "  "));
    test_character_ranges(test);
}

// MD024 - Multiple headings with the same content (commented out - rule not implemented)
// #[test]
// fn test_md024_multiple_headings() {
//     let test = simple_test(
//         "MD024",
//         "# Heading\n\n# Heading",
//         ExpectedWarning::new(3, 1, 3, 10, "# Heading")
//     );
//     test_character_ranges(test);
// }

// MD025 - Multiple top level headings in the same document
#[test]
fn test_md025_single_title() {
    let test = simple_test(
        "MD025",
        "# First Title\n\n# Second Title",
        ExpectedWarning::new(3, 3, 3, 15, "Second Title"),
    );
    test_character_ranges(test);
}

// MD027 - Multiple spaces after blockquote symbol
#[test]
fn test_md027_multiple_space_blockquote() {
    let test = simple_test("MD027", ">  Multiple spaces", ExpectedWarning::new(1, 3, 1, 4, " "));
    test_character_ranges(test);
}

// MD028 - Blank line inside blockquote
#[test]
fn test_md028_no_blank_line_blockquote() {
    let test = simple_test(
        "MD028",
        "> First line\n>\n> Second line",
        ExpectedWarning::new(2, 1, 2, 2, ">"),
    );
    test_character_ranges(test);
}

// MD029 - Ordered list style (commented out - rule not implemented)
// #[test]
// fn test_md029_ol_style() {
//     let test = simple_test(
//         "MD029",
//         "1. First item\n1. Second item",
//         ExpectedWarning::new(2, 1, 2, 3, "1.")
//     );
//     test_character_ranges(test);
// }

// MD030 - Spaces after list markers
#[test]
fn test_md030_list_marker_space() {
    let test = simple_test("MD030", "-  Two spaces", ExpectedWarning::new(1, 2, 1, 4, "  "));
    test_character_ranges(test);
}

// MD031 - Fenced code blocks should be surrounded by blank lines
#[test]
fn test_md031_blanks_around_fences() {
    let test = multi_warning_test(
        "MD031",
        "Text\n```\ncode\n```\nMore text",
        vec![
            ExpectedWarning::new(2, 1, 2, 4, "```"),
            ExpectedWarning::new(4, 1, 4, 4, "```"),
        ],
    );
    test_character_ranges(test);
}

// MD032 - Lists should be surrounded by blank lines
#[test]
fn test_md032_blanks_around_lists() {
    let test = multi_warning_test(
        "MD032",
        "Text\n- List item\nMore text",
        vec![
            ExpectedWarning::new(2, 1, 2, 12, "- List item"),
            ExpectedWarning::new(2, 1, 2, 12, "- List item"),
        ],
    );
    test_character_ranges(test);
}

// MD033 - Inline HTML
#[test]
fn test_md033_no_inline_html() {
    let test = multi_warning_test(
        "MD033",
        "Some <b>bold</b> text",
        vec![
            ExpectedWarning::new(1, 6, 1, 9, "<b>"),
            ExpectedWarning::new(1, 13, 1, 17, "</b>"),
        ],
    );
    test_character_ranges(test);
}

// MD034 - Bare URL used
#[test]
fn test_md034_no_bare_urls() {
    let test = simple_test(
        "MD034",
        "Visit http://example.com for more info",
        ExpectedWarning::new(1, 7, 1, 25, "http://example.com"),
    );
    test_character_ranges(test);
}

// MD035 - Horizontal rule style
#[test]
fn test_md035_hr_style() {
    let test = simple_test("MD035", "---\n\n***", ExpectedWarning::new(3, 1, 3, 4, "***"));
    test_character_ranges(test);
}

// MD039 - Spaces inside link text
#[test]
fn test_md039_no_space_in_links() {
    let test = simple_test(
        "MD039",
        "[ link text ](http://example.com)",
        ExpectedWarning::new(1, 1, 1, 34, "[ link text ](http://example.com)"),
    );
    test_character_ranges(test);
}

// MD042 - No empty links
#[test]
fn test_md042_no_empty_links() {
    let test = simple_test(
        "MD042",
        "[empty link]()",
        ExpectedWarning::new(1, 1, 1, 15, "[empty link]()"),
    );
    test_character_ranges(test);
}

// MD045 - Images should have alternate text (alt text)
#[test]
fn test_md045_no_alt_text() {
    let test = simple_test(
        "MD045",
        "![](image.png)",
        ExpectedWarning::new(1, 1, 1, 15, "![](image.png)"),
    );
    test_character_ranges(test);
}

// MD047 - Files should end with a single newline character
#[test]
fn test_md047_file_end_newline() {
    let test = simple_test(
        "MD047",
        "Text without newline",
        ExpectedWarning::new(1, 21, 1, 21, ""), // Now highlights end of line
    );
    test_character_ranges(test);
}

// MD051 - Link fragments should be valid
#[test]
fn test_md051_link_fragments() {
    let test = simple_test(
        "MD051",
        "[link](#nonexistent)",
        ExpectedWarning::new(1, 1, 1, 21, "[link](#nonexistent)"),
    );
    test_character_ranges(test);
}

// MD053 - Link and image reference definitions should be needed
#[test]
fn test_md053_link_image_reference_definitions() {
    let test = simple_test(
        "MD053",
        "[unused]: http://example.com",
        ExpectedWarning::new(1, 1, 1, 29, "[unused]: http://example.com"),
    );
    test_character_ranges(test);
}
