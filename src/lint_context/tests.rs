use super::*;

#[test]
fn test_empty_content() {
    let ctx = LintContext::new("", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.content, "");
    assert_eq!(ctx.line_offsets, vec![0]);
    assert_eq!(ctx.offset_to_line_col(0), (1, 1));
    assert_eq!(ctx.lines.len(), 0);
}

#[test]
fn test_single_line() {
    let ctx = LintContext::new("# Hello", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.content, "# Hello");
    assert_eq!(ctx.line_offsets, vec![0]);
    assert_eq!(ctx.offset_to_line_col(0), (1, 1));
    assert_eq!(ctx.offset_to_line_col(3), (1, 4));
}

#[test]
fn test_multi_line() {
    let content = "# Title\n\nSecond line\nThird line";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.line_offsets, vec![0, 8, 9, 21]);
    // Test offset to line/col
    assert_eq!(ctx.offset_to_line_col(0), (1, 1)); // start
    assert_eq!(ctx.offset_to_line_col(8), (2, 1)); // start of blank line
    assert_eq!(ctx.offset_to_line_col(9), (3, 1)); // start of 'Second line'
    assert_eq!(ctx.offset_to_line_col(15), (3, 7)); // middle of 'Second line'
    assert_eq!(ctx.offset_to_line_col(21), (4, 1)); // start of 'Third line'
}

#[test]
fn test_line_info() {
    let content = "# Title\n    indented\n\ncode:\n```rust\nfn main() {}\n```";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Test line info
    assert_eq!(ctx.lines.len(), 7);

    // Line 1: "# Title"
    let line1 = &ctx.lines[0];
    assert_eq!(line1.content(ctx.content), "# Title");
    assert_eq!(line1.byte_offset, 0);
    assert_eq!(line1.indent, 0);
    assert!(!line1.is_blank);
    assert!(!line1.in_code_block);
    assert!(line1.list_item.is_none());

    // Line 2: "    indented"
    let line2 = &ctx.lines[1];
    assert_eq!(line2.content(ctx.content), "    indented");
    assert_eq!(line2.byte_offset, 8);
    assert_eq!(line2.indent, 4);
    assert!(!line2.is_blank);

    // Line 3: "" (blank)
    let line3 = &ctx.lines[2];
    assert_eq!(line3.content(ctx.content), "");
    assert!(line3.is_blank);

    // Test helper methods
    assert_eq!(ctx.line_to_byte_offset(1), Some(0));
    assert_eq!(ctx.line_to_byte_offset(2), Some(8));
    assert_eq!(ctx.line_info(1).map(|l| l.indent), Some(0));
    assert_eq!(ctx.line_info(2).map(|l| l.indent), Some(4));
}

#[test]
fn test_list_item_detection() {
    let content = "- Unordered item\n  * Nested item\n1. Ordered item\n   2) Nested ordered\n\nNot a list";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Line 1: "- Unordered item"
    let line1 = &ctx.lines[0];
    assert!(line1.list_item.is_some());
    let list1 = line1.list_item.as_ref().unwrap();
    assert_eq!(list1.marker, "-");
    assert!(!list1.is_ordered);
    assert_eq!(list1.marker_column, 0);
    assert_eq!(list1.content_column, 2);

    // Line 2: "  * Nested item"
    let line2 = &ctx.lines[1];
    assert!(line2.list_item.is_some());
    let list2 = line2.list_item.as_ref().unwrap();
    assert_eq!(list2.marker, "*");
    assert_eq!(list2.marker_column, 2);

    // Line 3: "1. Ordered item"
    let line3 = &ctx.lines[2];
    assert!(line3.list_item.is_some());
    let list3 = line3.list_item.as_ref().unwrap();
    assert_eq!(list3.marker, "1.");
    assert!(list3.is_ordered);
    assert_eq!(list3.number, Some(1));

    // Line 6: "Not a list"
    let line6 = &ctx.lines[5];
    assert!(line6.list_item.is_none());
}

#[test]
fn test_offset_to_line_col_edge_cases() {
    let content = "a\nb\nc";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    // line_offsets: [0, 2, 4]
    assert_eq!(ctx.offset_to_line_col(0), (1, 1)); // 'a'
    assert_eq!(ctx.offset_to_line_col(1), (1, 2)); // after 'a'
    assert_eq!(ctx.offset_to_line_col(2), (2, 1)); // 'b'
    assert_eq!(ctx.offset_to_line_col(3), (2, 2)); // after 'b'
    assert_eq!(ctx.offset_to_line_col(4), (3, 1)); // 'c'
    assert_eq!(ctx.offset_to_line_col(5), (3, 2)); // after 'c'
}

#[test]
fn test_mdx_esm_blocks() {
    let content = r##"import {Chart} from './snowfall.js'
export const year = 2023

# Last year's snowfall

In {year}, the snowfall was above average.
It was followed by a warm spring which caused
flood conditions in many of the nearby rivers.

<Chart color="#fcb32c" year={year} />
"##;

    let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);

    // Check that lines 1 and 2 are marked as ESM blocks
    assert_eq!(ctx.lines.len(), 10);
    assert!(ctx.lines[0].in_esm_block, "Line 1 (import) should be in_esm_block");
    assert!(ctx.lines[1].in_esm_block, "Line 2 (export) should be in_esm_block");
    assert!(!ctx.lines[2].in_esm_block, "Line 3 (blank) should NOT be in_esm_block");
    assert!(
        !ctx.lines[3].in_esm_block,
        "Line 4 (heading) should NOT be in_esm_block"
    );
    assert!(!ctx.lines[4].in_esm_block, "Line 5 (blank) should NOT be in_esm_block");
    assert!(!ctx.lines[5].in_esm_block, "Line 6 (text) should NOT be in_esm_block");
}

#[test]
fn test_mdx_esm_blocks_not_detected_in_standard_flavor() {
    let content = r#"import {Chart} from './snowfall.js'
export const year = 2023

# Last year's snowfall
"#;

    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // ESM blocks should NOT be detected in Standard flavor
    assert!(
        !ctx.lines[0].in_esm_block,
        "Line 1 should NOT be in_esm_block in Standard flavor"
    );
    assert!(
        !ctx.lines[1].in_esm_block,
        "Line 2 should NOT be in_esm_block in Standard flavor"
    );
}

#[test]
fn test_blockquote_with_indented_content() {
    // Lines with `>` followed by heavily-indented content should be detected as blockquotes.
    // The content inside the blockquote may also be detected as a code block (which is correct),
    // but for MD046 purposes, we need to know the line is inside a blockquote.
    let content = r#"# Heading

>      -S socket-path
>                    More text
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Line 3 (index 2) should be detected as blockquote
    assert!(
        ctx.lines.get(2).is_some_and(|l| l.blockquote.is_some()),
        "Line 3 should be a blockquote"
    );
    // Line 4 (index 3) should also be blockquote
    assert!(
        ctx.lines.get(3).is_some_and(|l| l.blockquote.is_some()),
        "Line 4 should be a blockquote"
    );

    // Verify blockquote content is correctly parsed
    // Note: spaces_after includes the spaces between `>` and content
    let bq3 = ctx.lines.get(2).unwrap().blockquote.as_ref().unwrap();
    assert_eq!(bq3.content, "-S socket-path");
    assert_eq!(bq3.nesting_level, 1);
    // 6 spaces after the `>` marker
    assert!(bq3.has_multiple_spaces_after_marker);

    let bq4 = ctx.lines.get(3).unwrap().blockquote.as_ref().unwrap();
    assert_eq!(bq4.content, "More text");
    assert_eq!(bq4.nesting_level, 1);
}

#[test]
fn test_blockquote_spaced_nested_markers_are_detected() {
    let content = r#"> > Nested quote content
> > Additional line
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let bq1 = ctx.lines.get(0).unwrap().blockquote.as_ref().unwrap();
    assert_eq!(bq1.nesting_level, 2);
    assert_eq!(bq1.prefix, "> > ");
    assert_eq!(bq1.content, "Nested quote content");

    let bq2 = ctx.lines.get(1).unwrap().blockquote.as_ref().unwrap();
    assert_eq!(bq2.nesting_level, 2);
    assert_eq!(bq2.prefix, "> > ");
    assert_eq!(bq2.content, "Additional line");
}

#[test]
fn test_footnote_definitions_not_parsed_as_reference_defs() {
    // Footnote definitions use [^id]: syntax and should NOT be parsed as reference definitions
    let content = r#"# Title

A footnote[^1].

[^1]: This is the footnote content.

[^note]: Another footnote with [link](https://example.com).

[regular]: ./path.md "A real reference definition"
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should only have one reference definition (the regular one)
    assert_eq!(
        ctx.reference_defs.len(),
        1,
        "Footnotes should not be parsed as reference definitions"
    );

    // The only reference def should be the regular one
    assert_eq!(ctx.reference_defs[0].id, "regular");
    assert_eq!(ctx.reference_defs[0].url, "./path.md");
    assert_eq!(
        ctx.reference_defs[0].title,
        Some("A real reference definition".to_string())
    );
}

#[test]
fn test_footnote_with_inline_link_not_misidentified() {
    // Regression test for issue #286: footnote containing an inline link
    // was incorrectly parsed as a reference definition with URL "[link](url)"
    let content = r#"# Title

A footnote[^1].

[^1]: [link](https://www.google.com).
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should have no reference definitions
    assert!(
        ctx.reference_defs.is_empty(),
        "Footnote with inline link should not create a reference definition"
    );
}

#[test]
fn test_various_footnote_formats_excluded() {
    // Test various footnote ID formats are all excluded
    let content = r#"[^1]: Numeric footnote
[^note]: Named footnote
[^a]: Single char footnote
[^long-footnote-name]: Long named footnote
[^123abc]: Mixed alphanumeric

[ref1]: ./file1.md
[ref2]: ./file2.md
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should only have the two regular reference definitions
    assert_eq!(
        ctx.reference_defs.len(),
        2,
        "Only regular reference definitions should be parsed"
    );

    let ids: Vec<&str> = ctx.reference_defs.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"ref1"));
    assert!(ids.contains(&"ref2"));
    assert!(!ids.iter().any(|id| id.starts_with('^')));
}

// =========================================================================
// Tests for has_char and char_count methods
// =========================================================================

#[test]
fn test_has_char_tracked_characters() {
    // Test all 12 tracked characters
    let content =
        "# Heading\n* list item\n_emphasis_ and -hyphen-\n+ plus\n> quote\n| table |\n[link]\n`code`\n<html>\n!image";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // All tracked characters should be detected
    assert!(ctx.has_char('#'), "Should detect hash");
    assert!(ctx.has_char('*'), "Should detect asterisk");
    assert!(ctx.has_char('_'), "Should detect underscore");
    assert!(ctx.has_char('-'), "Should detect hyphen");
    assert!(ctx.has_char('+'), "Should detect plus");
    assert!(ctx.has_char('>'), "Should detect gt");
    assert!(ctx.has_char('|'), "Should detect pipe");
    assert!(ctx.has_char('['), "Should detect bracket");
    assert!(ctx.has_char('`'), "Should detect backtick");
    assert!(ctx.has_char('<'), "Should detect lt");
    assert!(ctx.has_char('!'), "Should detect exclamation");
    assert!(ctx.has_char('\n'), "Should detect newline");
}

#[test]
fn test_has_char_absent_characters() {
    let content = "Simple text without special chars";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // None of the tracked characters should be present
    assert!(!ctx.has_char('#'), "Should not detect hash");
    assert!(!ctx.has_char('*'), "Should not detect asterisk");
    assert!(!ctx.has_char('_'), "Should not detect underscore");
    assert!(!ctx.has_char('-'), "Should not detect hyphen");
    assert!(!ctx.has_char('+'), "Should not detect plus");
    assert!(!ctx.has_char('>'), "Should not detect gt");
    assert!(!ctx.has_char('|'), "Should not detect pipe");
    assert!(!ctx.has_char('['), "Should not detect bracket");
    assert!(!ctx.has_char('`'), "Should not detect backtick");
    assert!(!ctx.has_char('<'), "Should not detect lt");
    assert!(!ctx.has_char('!'), "Should not detect exclamation");
    // Note: single line content has no newlines
    assert!(!ctx.has_char('\n'), "Should not detect newline in single line");
}

#[test]
fn test_has_char_fallback_for_untracked() {
    let content = "Text with @mention and $dollar and %percent";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Untracked characters should fall back to content.contains()
    assert!(ctx.has_char('@'), "Should detect @ via fallback");
    assert!(ctx.has_char('$'), "Should detect $ via fallback");
    assert!(ctx.has_char('%'), "Should detect % via fallback");
    assert!(!ctx.has_char('^'), "Should not detect absent ^ via fallback");
}

#[test]
fn test_char_count_tracked_characters() {
    let content =
        "## Heading ##\n***bold***\n__emphasis__\n---\n+++\n>> nested\n|| table ||\n[[link]]\n``code``\n<<html>>\n!!";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Count each tracked character
    assert_eq!(ctx.char_count('#'), 4, "Should count 4 hashes");
    assert_eq!(ctx.char_count('*'), 6, "Should count 6 asterisks");
    assert_eq!(ctx.char_count('_'), 4, "Should count 4 underscores");
    assert_eq!(ctx.char_count('-'), 3, "Should count 3 hyphens");
    assert_eq!(ctx.char_count('+'), 3, "Should count 3 pluses");
    assert_eq!(ctx.char_count('>'), 4, "Should count 4 gt (2 nested + 2 in <<html>>)");
    assert_eq!(ctx.char_count('|'), 4, "Should count 4 pipes");
    assert_eq!(ctx.char_count('['), 2, "Should count 2 brackets");
    assert_eq!(ctx.char_count('`'), 4, "Should count 4 backticks");
    assert_eq!(ctx.char_count('<'), 2, "Should count 2 lt");
    assert_eq!(ctx.char_count('!'), 2, "Should count 2 exclamations");
    assert_eq!(ctx.char_count('\n'), 10, "Should count 10 newlines");
}

#[test]
fn test_char_count_zero_for_absent() {
    let content = "Plain text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.char_count('#'), 0);
    assert_eq!(ctx.char_count('*'), 0);
    assert_eq!(ctx.char_count('_'), 0);
    assert_eq!(ctx.char_count('\n'), 0);
}

#[test]
fn test_char_count_fallback_for_untracked() {
    let content = "@@@ $$ %%%";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.char_count('@'), 3, "Should count 3 @ via fallback");
    assert_eq!(ctx.char_count('$'), 2, "Should count 2 $ via fallback");
    assert_eq!(ctx.char_count('%'), 3, "Should count 3 % via fallback");
    assert_eq!(ctx.char_count('^'), 0, "Should count 0 for absent char");
}

#[test]
fn test_char_count_empty_content() {
    let ctx = LintContext::new("", MarkdownFlavor::Standard, None);

    assert_eq!(ctx.char_count('#'), 0);
    assert_eq!(ctx.char_count('*'), 0);
    assert_eq!(ctx.char_count('@'), 0);
    assert!(!ctx.has_char('#'));
    assert!(!ctx.has_char('@'));
}

// =========================================================================
// Tests for is_in_html_tag method
// =========================================================================

#[test]
fn test_is_in_html_tag_simple() {
    let content = "<div>content</div>";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Inside opening tag
    assert!(ctx.is_in_html_tag(0), "Position 0 (<) should be in tag");
    assert!(ctx.is_in_html_tag(1), "Position 1 (d) should be in tag");
    assert!(ctx.is_in_html_tag(4), "Position 4 (>) should be in tag");

    // Outside tag (in content)
    assert!(!ctx.is_in_html_tag(5), "Position 5 (c) should not be in tag");
    assert!(!ctx.is_in_html_tag(10), "Position 10 (t) should not be in tag");

    // Inside closing tag
    assert!(ctx.is_in_html_tag(12), "Position 12 (<) should be in tag");
    assert!(ctx.is_in_html_tag(17), "Position 17 (>) should be in tag");
}

#[test]
fn test_is_in_html_tag_self_closing() {
    let content = "Text <br/> more text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Before tag
    assert!(!ctx.is_in_html_tag(0), "Position 0 should not be in tag");
    assert!(!ctx.is_in_html_tag(4), "Position 4 (space) should not be in tag");

    // Inside self-closing tag
    assert!(ctx.is_in_html_tag(5), "Position 5 (<) should be in tag");
    assert!(ctx.is_in_html_tag(8), "Position 8 (/) should be in tag");
    assert!(ctx.is_in_html_tag(9), "Position 9 (>) should be in tag");

    // After tag
    assert!(!ctx.is_in_html_tag(10), "Position 10 (space) should not be in tag");
}

#[test]
fn test_is_in_html_tag_with_attributes() {
    let content = r#"<a href="url" class="link">text</a>"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // All positions inside opening tag with attributes
    assert!(ctx.is_in_html_tag(0), "Start of tag");
    assert!(ctx.is_in_html_tag(10), "Inside href attribute");
    assert!(ctx.is_in_html_tag(20), "Inside class attribute");
    assert!(ctx.is_in_html_tag(26), "End of opening tag");

    // Content between tags
    assert!(!ctx.is_in_html_tag(27), "Start of content");
    assert!(!ctx.is_in_html_tag(30), "End of content");

    // Closing tag
    assert!(ctx.is_in_html_tag(31), "Start of closing tag");
}

#[test]
fn test_is_in_html_tag_multiline() {
    let content = "<div\n  class=\"test\"\n>\ncontent\n</div>";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Opening tag spans multiple lines
    assert!(ctx.is_in_html_tag(0), "Start of multiline tag");
    assert!(ctx.is_in_html_tag(5), "After first newline in tag");
    assert!(ctx.is_in_html_tag(15), "Inside attribute");

    // After closing > of opening tag
    let closing_bracket_pos = content.find(">\n").unwrap();
    assert!(!ctx.is_in_html_tag(closing_bracket_pos + 2), "Content after tag");
}

#[test]
fn test_is_in_html_tag_no_tags() {
    let content = "Plain text without any HTML";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // No position should be in an HTML tag
    for i in 0..content.len() {
        assert!(!ctx.is_in_html_tag(i), "Position {i} should not be in tag");
    }
}

// =========================================================================
// Tests for is_in_jinja_range method
// =========================================================================

#[test]
fn test_is_in_jinja_range_expression() {
    let content = "Hello {{ name }}!";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Before Jinja
    assert!(!ctx.is_in_jinja_range(0), "H should not be in Jinja");
    assert!(!ctx.is_in_jinja_range(5), "Space before Jinja should not be in Jinja");

    // Inside Jinja expression (positions 6-15 for "{{ name }}")
    assert!(ctx.is_in_jinja_range(6), "First brace should be in Jinja");
    assert!(ctx.is_in_jinja_range(7), "Second brace should be in Jinja");
    assert!(ctx.is_in_jinja_range(10), "name should be in Jinja");
    assert!(ctx.is_in_jinja_range(14), "Closing brace should be in Jinja");
    assert!(ctx.is_in_jinja_range(15), "Second closing brace should be in Jinja");

    // After Jinja
    assert!(!ctx.is_in_jinja_range(16), "! should not be in Jinja");
}

#[test]
fn test_is_in_jinja_range_statement() {
    let content = "{% if condition %}content{% endif %}";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Inside opening statement
    assert!(ctx.is_in_jinja_range(0), "Start of Jinja statement");
    assert!(ctx.is_in_jinja_range(5), "condition should be in Jinja");
    assert!(ctx.is_in_jinja_range(17), "End of opening statement");

    // Content between
    assert!(!ctx.is_in_jinja_range(18), "content should not be in Jinja");

    // Inside closing statement
    assert!(ctx.is_in_jinja_range(25), "Start of endif");
    assert!(ctx.is_in_jinja_range(32), "endif should be in Jinja");
}

#[test]
fn test_is_in_jinja_range_multiple() {
    let content = "{{ a }} and {{ b }}";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // First Jinja expression
    assert!(ctx.is_in_jinja_range(0));
    assert!(ctx.is_in_jinja_range(3));
    assert!(ctx.is_in_jinja_range(6));

    // Between expressions
    assert!(!ctx.is_in_jinja_range(8));
    assert!(!ctx.is_in_jinja_range(11));

    // Second Jinja expression
    assert!(ctx.is_in_jinja_range(12));
    assert!(ctx.is_in_jinja_range(15));
    assert!(ctx.is_in_jinja_range(18));
}

#[test]
fn test_is_in_jinja_range_no_jinja() {
    let content = "Plain text with single braces but not Jinja";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // No position should be in Jinja
    for i in 0..content.len() {
        assert!(!ctx.is_in_jinja_range(i), "Position {i} should not be in Jinja");
    }
}

// =========================================================================
// Tests for is_in_link_title method
// =========================================================================

#[test]
fn test_is_in_link_title_with_title() {
    let content = r#"[ref]: https://example.com "Title text"

Some content."#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Verify we have a reference def with title
    assert_eq!(ctx.reference_defs.len(), 1);
    let def = &ctx.reference_defs[0];
    assert!(def.title_byte_start.is_some());
    assert!(def.title_byte_end.is_some());

    let title_start = def.title_byte_start.unwrap();
    let title_end = def.title_byte_end.unwrap();

    // Before title (in URL)
    assert!(!ctx.is_in_link_title(10), "URL should not be in title");

    // Inside title
    assert!(ctx.is_in_link_title(title_start), "Title start should be in title");
    assert!(
        ctx.is_in_link_title(title_start + 5),
        "Middle of title should be in title"
    );
    assert!(ctx.is_in_link_title(title_end - 1), "End of title should be in title");

    // After title
    assert!(
        !ctx.is_in_link_title(title_end),
        "After title end should not be in title"
    );
}

#[test]
fn test_is_in_link_title_without_title() {
    let content = "[ref]: https://example.com\n\nSome content.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Reference def without title
    assert_eq!(ctx.reference_defs.len(), 1);
    let def = &ctx.reference_defs[0];
    assert!(def.title_byte_start.is_none());
    assert!(def.title_byte_end.is_none());

    // No position should be in a title
    for i in 0..content.len() {
        assert!(!ctx.is_in_link_title(i), "Position {i} should not be in title");
    }
}

#[test]
fn test_is_in_link_title_multiple_refs() {
    let content = r#"[ref1]: /url1 "Title One"
[ref2]: /url2
[ref3]: /url3 "Title Three"
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Should have 3 reference defs
    assert_eq!(ctx.reference_defs.len(), 3);

    // ref1 has title
    let ref1 = ctx.reference_defs.iter().find(|r| r.id == "ref1").unwrap();
    assert!(ref1.title_byte_start.is_some());

    // ref2 has no title
    let ref2 = ctx.reference_defs.iter().find(|r| r.id == "ref2").unwrap();
    assert!(ref2.title_byte_start.is_none());

    // ref3 has title
    let ref3 = ctx.reference_defs.iter().find(|r| r.id == "ref3").unwrap();
    assert!(ref3.title_byte_start.is_some());

    // Check positions in ref1's title
    if let (Some(start), Some(end)) = (ref1.title_byte_start, ref1.title_byte_end) {
        assert!(ctx.is_in_link_title(start + 1));
        assert!(!ctx.is_in_link_title(end + 5));
    }

    // Check positions in ref3's title
    if let (Some(start), Some(_end)) = (ref3.title_byte_start, ref3.title_byte_end) {
        assert!(ctx.is_in_link_title(start + 1));
    }
}

#[test]
fn test_is_in_link_title_single_quotes() {
    let content = "[ref]: /url 'Single quoted title'\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.reference_defs.len(), 1);
    let def = &ctx.reference_defs[0];

    if let (Some(start), Some(end)) = (def.title_byte_start, def.title_byte_end) {
        assert!(ctx.is_in_link_title(start));
        assert!(ctx.is_in_link_title(start + 5));
        assert!(!ctx.is_in_link_title(end));
    }
}

#[test]
fn test_is_in_link_title_parentheses() {
    // Note: The reference def parser may not support parenthesized titles
    // This test verifies the is_in_link_title method works when titles exist
    let content = "[ref]: /url (Parenthesized title)\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Parser behavior: may or may not parse parenthesized titles
    // We test that is_in_link_title correctly reflects whatever was parsed
    if ctx.reference_defs.is_empty() {
        // Parser didn't recognize this as a reference def
        for i in 0..content.len() {
            assert!(!ctx.is_in_link_title(i));
        }
    } else {
        let def = &ctx.reference_defs[0];
        if let (Some(start), Some(end)) = (def.title_byte_start, def.title_byte_end) {
            assert!(ctx.is_in_link_title(start));
            assert!(ctx.is_in_link_title(start + 5));
            assert!(!ctx.is_in_link_title(end));
        } else {
            // Title wasn't parsed, so no position should be in title
            for i in 0..content.len() {
                assert!(!ctx.is_in_link_title(i));
            }
        }
    }
}

#[test]
fn test_is_in_link_title_no_refs() {
    let content = "Just plain text without any reference definitions.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.reference_defs.is_empty());

    for i in 0..content.len() {
        assert!(!ctx.is_in_link_title(i));
    }
}

// =========================================================================
// Math span tests (Issue #289)
// =========================================================================

#[test]
fn test_math_spans_inline() {
    let content = "Text with inline math $[f](x)$ in it.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1, "Should detect one inline math span");

    let span = &math_spans[0];
    assert!(!span.is_display, "Should be inline math, not display");
    assert_eq!(span.content, "[f](x)", "Content should be extracted correctly");
}

#[test]
fn test_math_spans_display_single_line() {
    let content = "$$X(\\zeta) = \\mathcal Z [x](\\zeta)$$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1, "Should detect one display math span");

    let span = &math_spans[0];
    assert!(span.is_display, "Should be display math");
    assert!(
        span.content.contains("[x](\\zeta)"),
        "Content should contain the link-like pattern"
    );
}

#[test]
fn test_math_spans_display_multiline() {
    let content = "Before\n\n$$\n[x](\\zeta) = \\sum_k x(k)\n$$\n\nAfter";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1, "Should detect one display math span");

    let span = &math_spans[0];
    assert!(span.is_display, "Should be display math");
}

#[test]
fn test_is_in_math_span() {
    let content = "Text $[f](x)$ more text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Position inside the math span
    let math_start = content.find('$').unwrap();
    let math_end = content.rfind('$').unwrap() + 1;

    assert!(
        ctx.is_in_math_span(math_start + 1),
        "Position inside math span should return true"
    );
    assert!(
        ctx.is_in_math_span(math_start + 3),
        "Position inside math span should return true"
    );

    // Position outside the math span
    assert!(!ctx.is_in_math_span(0), "Position before math span should return false");
    assert!(
        !ctx.is_in_math_span(math_end + 1),
        "Position after math span should return false"
    );
}

#[test]
fn test_math_spans_mixed_with_code() {
    let content = "Math $[f](x)$ and code `[g](y)` mixed";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    let code_spans = ctx.code_spans();

    assert_eq!(math_spans.len(), 1, "Should have one math span");
    assert_eq!(code_spans.len(), 1, "Should have one code span");

    // Verify math span content
    assert_eq!(math_spans[0].content, "[f](x)");
    // Verify code span content
    assert_eq!(code_spans[0].content, "[g](y)");
}

#[test]
fn test_math_spans_no_math() {
    let content = "Regular text without any math at all.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert!(math_spans.is_empty(), "Should have no math spans");
}

#[test]
fn test_math_spans_multiple() {
    let content = "First $a$ and second $b$ and display $$c$$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 3, "Should detect three math spans");

    // Two inline, one display
    let inline_count = math_spans.iter().filter(|s| !s.is_display).count();
    let display_count = math_spans.iter().filter(|s| s.is_display).count();

    assert_eq!(inline_count, 2, "Should have two inline math spans");
    assert_eq!(display_count, 1, "Should have one display math span");
}

#[test]
fn test_is_in_math_span_boundary_positions() {
    // Test exact boundary positions: $[f](x)$
    // Byte positions:                0123456789
    let content = "$[f](x)$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1, "Should have one math span");

    let span = &math_spans[0];

    // Position at opening $ should be in span (byte 0)
    assert!(
        ctx.is_in_math_span(span.byte_offset),
        "Start position should be in span"
    );

    // Position just inside should be in span
    assert!(
        ctx.is_in_math_span(span.byte_offset + 1),
        "Position after start should be in span"
    );

    // Position at closing $ should be in span (exclusive end means we check byte_end - 1)
    assert!(
        ctx.is_in_math_span(span.byte_end - 1),
        "Position at end-1 should be in span"
    );

    // Position at byte_end should NOT be in span (exclusive end)
    assert!(
        !ctx.is_in_math_span(span.byte_end),
        "Position at byte_end should NOT be in span (exclusive)"
    );
}

#[test]
fn test_math_spans_at_document_start() {
    let content = "$x$ text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1);
    assert_eq!(math_spans[0].byte_offset, 0, "Math should start at byte 0");
}

#[test]
fn test_math_spans_at_document_end() {
    let content = "text $x$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    assert_eq!(math_spans.len(), 1);
    assert_eq!(math_spans[0].byte_end, content.len(), "Math should end at document end");
}

#[test]
fn test_math_spans_consecutive() {
    let content = "$a$$b$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    // pulldown-cmark should parse these as separate spans
    assert!(!math_spans.is_empty(), "Should detect at least one math span");

    // All positions should be in some math span
    for i in 0..content.len() {
        assert!(ctx.is_in_math_span(i), "Position {i} should be in a math span");
    }
}

#[test]
fn test_math_spans_currency_not_math() {
    // Unbalanced $ should not create math spans
    let content = "Price is $100";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    let math_spans = ctx.math_spans();
    // pulldown-cmark requires balanced delimiters for math
    // $100 alone is not math
    assert!(
        math_spans.is_empty() || !math_spans.iter().any(|s| s.content.contains("100")),
        "Unbalanced $ should not create math span containing 100"
    );
}

// =========================================================================
// Tests for O(1) reference definition lookups via HashMap
// =========================================================================

#[test]
fn test_reference_lookup_o1_basic() {
    let content = r#"[ref1]: /url1
[REF2]: /url2 "Title"
[Ref3]: /url3

Use [link][ref1] and [link][REF2]."#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Verify we have 3 reference defs
    assert_eq!(ctx.reference_defs.len(), 3);

    // Test get_reference_url with various cases
    assert_eq!(ctx.get_reference_url("ref1"), Some("/url1"));
    assert_eq!(ctx.get_reference_url("REF1"), Some("/url1")); // case insensitive
    assert_eq!(ctx.get_reference_url("Ref1"), Some("/url1")); // case insensitive
    assert_eq!(ctx.get_reference_url("ref2"), Some("/url2"));
    assert_eq!(ctx.get_reference_url("REF2"), Some("/url2"));
    assert_eq!(ctx.get_reference_url("ref3"), Some("/url3"));
    assert_eq!(ctx.get_reference_url("nonexistent"), None);
}

#[test]
fn test_reference_lookup_o1_get_reference_def() {
    let content = r#"[myref]: https://example.com "My Title"
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Test get_reference_def
    let def = ctx.get_reference_def("myref").expect("Should find myref");
    assert_eq!(def.url, "https://example.com");
    assert_eq!(def.title.as_deref(), Some("My Title"));

    // Case insensitive
    let def2 = ctx.get_reference_def("MYREF").expect("Should find MYREF");
    assert_eq!(def2.url, "https://example.com");

    // Non-existent
    assert!(ctx.get_reference_def("nonexistent").is_none());
}

#[test]
fn test_reference_lookup_o1_has_reference_def() {
    let content = r#"[foo]: /foo
[BAR]: /bar
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    // Test has_reference_def
    assert!(ctx.has_reference_def("foo"));
    assert!(ctx.has_reference_def("FOO")); // case insensitive
    assert!(ctx.has_reference_def("bar"));
    assert!(ctx.has_reference_def("Bar")); // case insensitive
    assert!(!ctx.has_reference_def("baz")); // doesn't exist
}

#[test]
fn test_reference_lookup_o1_empty_content() {
    let content = "No references here.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.reference_defs.is_empty());
    assert_eq!(ctx.get_reference_url("anything"), None);
    assert!(ctx.get_reference_def("anything").is_none());
    assert!(!ctx.has_reference_def("anything"));
}

#[test]
fn test_reference_lookup_o1_special_characters_in_id() {
    let content = r#"[ref-with-dash]: /url1
[ref_with_underscore]: /url2
[ref.with.dots]: /url3
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.get_reference_url("ref-with-dash"), Some("/url1"));
    assert_eq!(ctx.get_reference_url("ref_with_underscore"), Some("/url2"));
    assert_eq!(ctx.get_reference_url("ref.with.dots"), Some("/url3"));
}

#[test]
fn test_reference_lookup_o1_unicode_id() {
    let content = r#"[日本語]: /japanese
[émoji]: /emoji
"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert_eq!(ctx.get_reference_url("日本語"), Some("/japanese"));
    assert_eq!(ctx.get_reference_url("émoji"), Some("/emoji"));
    assert_eq!(ctx.get_reference_url("ÉMOJI"), Some("/emoji")); // uppercase
}
