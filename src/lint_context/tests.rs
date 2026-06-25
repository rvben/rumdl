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
    assert_eq!(ctx.line_info(1).map(|l| l.indent), Some(0));
    assert_eq!(ctx.line_info(2).map(|l| l.indent), Some(4));
    assert_eq!(ctx.line_info(1).map(|l| l.byte_offset), Some(0));
    assert_eq!(ctx.line_info(2).map(|l| l.byte_offset), Some(8));
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
fn test_offset_to_line_col_non_ascii() {
    // Issue #670: the column is a character offset, not a byte offset.
    // "你好x": 你=bytes 0-2, 好=bytes 3-5, x=byte 6.
    let content = "你好x";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.offset_to_line_col(0), (1, 1)); // 你 -> char column 1
    assert_eq!(ctx.offset_to_line_col(3), (1, 2)); // 好 -> char column 2
    assert_eq!(ctx.offset_to_line_col(6), (1, 3)); // x  -> char column 3 (not byte 7)
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

    let bq1 = ctx.lines.first().unwrap().blockquote.as_ref().unwrap();
    assert_eq!(bq1.nesting_level, 2);
    assert_eq!(bq1.prefix, "> > ");
    assert_eq!(bq1.content, "Nested quote content");

    let bq2 = ctx.lines.get(1).unwrap().blockquote.as_ref().unwrap();
    assert_eq!(bq2.nesting_level, 2);
    assert_eq!(bq2.prefix, "> > ");
    assert_eq!(bq2.content, "Additional line");
}

#[test]
fn test_ref_def_with_angle_bracket_destination_containing_space() {
    // CommonMark §6.6 admits <...>-form destinations that contain spaces.
    // Without this, the auto-fix output `[id]: <./has space.md>` (which
    // `format_url_destination` chooses for whitespace-bearing URLs) silently
    // disappears from `ctx.reference_defs` on the next parse, breaking
    // dedup in MD054 and ref-def discovery in MD053/MD057.
    let content = "[docs]: <./has space.md>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1, "angle-bracket destination must parse");
    assert_eq!(ctx.reference_defs[0].id, "docs");
    assert_eq!(
        ctx.reference_defs[0].url, "./has space.md",
        "URL should be the destination content, not the angle-bracketed form"
    );
    assert_eq!(ctx.reference_defs[0].title, None);
}

#[test]
fn test_ref_def_with_angle_bracket_destination_and_title() {
    // The optional title still parses after an angle-bracket destination.
    let content = "[docs]: <./has space.md> \"Help me\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(ctx.reference_defs[0].url, "./has space.md");
    assert_eq!(ctx.reference_defs[0].title.as_deref(), Some("Help me"));
}

#[test]
fn test_ref_def_multiline_title_on_next_line() {
    // CommonMark §4.7: a reference definition's title may sit on the line
    // immediately after the destination. The whole definition - including the
    // title line - is one reference definition.
    let content = "[ref]: https://example.com\n  \"the title\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(ctx.reference_defs[0].title.as_deref(), Some("the title"));
    // The byte range and is_in_reference_def cover the continuation title line.
    let quote = content.find('"').unwrap();
    assert!(
        ctx.is_in_reference_def(quote),
        "title line should be inside the ref def"
    );
    // The title byte range spans the delimiters exactly, and byte_end reaches
    // the end of the continuation line (the byte before its newline).
    let def = &ctx.reference_defs[0];
    assert_eq!(
        def.title_byte_start,
        Some(quote),
        "title_byte_start is the opening quote"
    );
    assert_eq!(
        def.title_byte_end,
        Some(quote + "\"the title\"".len()),
        "title_byte_end is one past the closing quote"
    );
    assert_eq!(
        def.byte_end,
        content.trim_end_matches('\n').len(),
        "byte_end reaches end of title line"
    );
}

#[test]
fn test_ref_def_single_quote_and_paren_title_on_next_line() {
    for content in [
        "[ref]: https://example.com\n  'the title'\n",
        "[ref]: https://example.com\n  (the title)\n",
    ] {
        let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
        assert_eq!(ctx.reference_defs.len(), 1, "input: {content:?}");
        assert_eq!(
            ctx.reference_defs[0].title.as_deref(),
            Some("the title"),
            "input: {content:?}"
        );
    }
}

#[test]
fn test_ref_def_blank_line_breaks_multiline_title() {
    // A blank line between the destination and a quoted line means the quoted
    // line is a separate paragraph, not the definition's title.
    let content = "[ref]: https://example.com\n\n\"not a title\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(ctx.reference_defs[0].title, None);
    let quote = content.find('"').unwrap();
    assert!(
        !ctx.is_in_reference_def(quote),
        "a blank-separated quoted line is not part of the ref def"
    );
}

#[test]
fn test_ref_def_non_title_next_line_not_consumed() {
    // The line after the destination is only a title if it is *only* a quoted
    // or parenthesised title; ordinary prose is a separate paragraph.
    let content = "[ref]: https://example.com\nordinary paragraph text\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(ctx.reference_defs[0].title, None);
    let para = content.find("ordinary").unwrap();
    assert!(
        !ctx.is_in_reference_def(para),
        "a following paragraph is not part of the ref def"
    );
}

#[test]
fn test_ref_def_inline_title_unaffected_by_lookahead() {
    // A definition that already has its title on the destination line is
    // unchanged; the next line is not pulled in.
    let content = "[ref]: https://example.com \"inline\"\n\"separate\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(ctx.reference_defs[0].title.as_deref(), Some("inline"));
    let sep = content.find("\"separate\"").unwrap();
    assert!(
        !ctx.is_in_reference_def(sep),
        "the inline-title def must not absorb the next line"
    );
}

#[test]
fn test_ref_def_multiline_title_in_blockquote() {
    // A reference definition inside a blockquote may also carry its title on
    // the following (still blockquoted) line.
    let content = "> [ref]: https://example.com\n>  \"the title\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(ctx.reference_defs[0].title.as_deref(), Some("the title"));
}

#[test]
fn test_ref_def_paren_title_with_escaped_parens() {
    // CommonMark §4.7 paren-form titles may contain `(`/`)` only when
    // backslash-escaped. Both pulldown-cmark and the rumdl ref-def regex
    // unescape the captured title (per CommonMark §6.1) so downstream rules
    // see the same value regardless of which path produced it.
    let content = "[docs]: https://example.com (title \\(x\\))\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(ctx.reference_defs[0].url, "https://example.com");
    assert_eq!(
        ctx.reference_defs[0].title.as_deref(),
        Some("title (x)"),
        "title must be unescaped to match pulldown-cmark's parsed value"
    );
}

#[test]
fn test_mkdocs_admonition_link_with_paren_title() {
    // pulldown-cmark treats indented MkDocs admonition content as a code block,
    // so the inline link is recovered by the regex fallback in
    // `parse_links_images_pulldown`. The fallback must recognize all three
    // CommonMark §6.7 title delimiter forms — including `(...)` — otherwise
    // a link like `[doc](url (title))` is parsed with title=None and MD054
    // auto-fix silently strips the title when rewriting the link.
    let content = "!!! note\n    See [doc](https://example.com (paren title)) here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let link = ctx
        .links
        .iter()
        .find(|l| l.url == "https://example.com")
        .expect("MkDocs fallback must surface the link");
    assert_eq!(
        link.title.as_deref(),
        Some("paren title"),
        "paren-form title must be captured by the MkDocs link fallback"
    );
}

#[test]
fn test_mkdocs_admonition_image_with_paren_title() {
    // Mirror of the link test for images.
    let content = "!!! note\n    See ![alt](https://example.com/x.png (paren title)) here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::MkDocs, None);
    let img = ctx
        .images
        .iter()
        .find(|i| i.url == "https://example.com/x.png")
        .expect("MkDocs fallback must surface the image");
    assert_eq!(
        img.title.as_deref(),
        Some("paren title"),
        "paren-form title must be captured by the MkDocs image fallback"
    );
}

#[test]
fn test_ref_def_angle_bracket_destination_with_escaped_brackets() {
    // CommonMark §6.6 angle-bracket destinations admit `\<` and `\>` so the
    // round-trip from `format_url_destination` (which emits `<a\<b\>c>` when
    // a URL contains `<` or `>`) is recovered on the next parse instead of
    // silently dropping the def out of `ctx.reference_defs`.
    let content = "[id]: <a\\<b\\>c>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(
        ctx.reference_defs.len(),
        1,
        "escaped angle-bracket destination must round-trip through the regex"
    );
    assert_eq!(ctx.reference_defs[0].id, "id");
    assert_eq!(ctx.reference_defs[0].title, None);
}

#[test]
fn test_ref_def_double_quoted_title_with_escaped_quote() {
    // Title delimiter `"` may appear inside the title only when backslash-escaped;
    // `format_title` produces this form whenever the unescaped title contains `"`,
    // so the regex must accept it or the freshly generated def disappears from
    // the next pass and MD053/MD057/dedup all break. The captured title is
    // unescaped (CommonMark §6.1) so it matches pulldown-cmark's parsed value.
    let content = "[id]: https://example.com \"he said \\\"hi\\\"\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(ctx.reference_defs[0].url, "https://example.com");
    assert_eq!(
        ctx.reference_defs[0].title.as_deref(),
        Some("he said \"hi\""),
        "title must be unescaped to match pulldown-cmark's parsed value"
    );
}

#[test]
fn test_ref_def_single_quoted_title_with_escaped_quote() {
    let content = "[id]: https://example.com 'it\\'s fine'\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(ctx.reference_defs[0].url, "https://example.com");
    assert_eq!(
        ctx.reference_defs[0].title.as_deref(),
        Some("it's fine"),
        "title must be unescaped to match pulldown-cmark's parsed value"
    );
}

#[test]
fn test_ref_def_url_unescapes_backslash_escapes() {
    // CommonMark §6.1: a backslash before any ASCII punctuation character
    // produces the literal character; the backslash itself is removed. The
    // rumdl regex fallback must apply this transform so `ctx.reference_defs[i].url`
    // matches what pulldown-cmark exposes via `Tag::Link`/`Tag::Image`. Without
    // this, MD053/MD054/MD057 would see `https://e.com/path\(1\)` while the
    // parser sees `https://e.com/path(1)`, and any rule that copies the value
    // back into the document would corrupt it.
    let content = "[id]: https://e.com/path\\(1\\)\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(
        ctx.reference_defs[0].url, "https://e.com/path(1)",
        "URL must be unescaped per CommonMark §6.1"
    );
}

#[test]
fn test_ref_def_unescape_preserves_non_punctuation_backslash() {
    // CommonMark §6.1 explicitly limits the escape to ASCII punctuation. A
    // backslash followed by a letter, digit, or whitespace is preserved
    // verbatim (the backslash stays in the output). Verifying this guards
    // against an over-eager unescape that would silently drop backslashes
    // from URL paths and titles.
    let content = "[id]: https://e.com/p\\ath \"a\\b c\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(
        ctx.reference_defs[0].url, "https://e.com/p\\ath",
        "backslash before non-punctuation must remain in URL"
    );
    assert_eq!(
        ctx.reference_defs[0].title.as_deref(),
        Some("a\\b c"),
        "backslash before non-punctuation must remain in title"
    );
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
fn test_is_in_html_tag_with_url_attributes() {
    // Tags with URLs in attributes contain '/' which must not be treated as self-closing
    let content = r#"<input name="fields[url]" value="https://www.example.com">"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let tags = ctx.html_tags();

    assert_eq!(tags.len(), 1, "Should detect one HTML tag");
    assert_eq!(tags[0].tag_name, "input");
    assert!(!tags[0].is_self_closing);
    assert!(ctx.is_in_html_tag(35), "URL position should be inside HTML tag");
}

#[test]
fn test_is_in_html_tag_self_closing_with_slash() {
    let content = "<br />";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let tags = ctx.html_tags();

    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].tag_name, "br");
    assert!(tags[0].is_self_closing);
}

#[test]
fn test_is_in_html_tag_nested_angle_brackets() {
    // Hugo shortcodes: <a href="{{< ref ... >}}"> contain nested '<'
    let content = r#"<a href="{{< ref "../common-parameters" >}}">"#;
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let tags = ctx.html_tags();

    // The regex handles nested '<' by matching the shortest valid tag
    assert!(!tags.is_empty(), "Should detect at least one tag fragment");
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
fn test_reference_lookup_o1_empty_content() {
    let content = "No references here.";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.reference_defs.is_empty());
    assert_eq!(ctx.get_reference_url("anything"), None);
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

#[test]
fn test_is_in_link_title_multiple_ranges_binary_search() {
    // Three reference defs with titles — verifies binary search works across all three
    let content = "[a]: /url1 \"Title A\"\n[b]: /url2 \"Title B\"\n[c]: /url3 \"Title C\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 3, "Should have 3 reference defs");

    // Position inside first title should return true
    if let (Some(start), Some(end)) = (
        ctx.reference_defs[0].title_byte_start,
        ctx.reference_defs[0].title_byte_end,
    ) {
        assert!(ctx.is_in_link_title(start + 1), "Inside first title should return true");
        // Position at exclusive end should return false
        assert!(!ctx.is_in_link_title(end), "At exclusive end should return false");
    }

    // Position between titles (in URL area of def B, before its title) should return false
    if let (Some(end_a), Some(start_b)) = (
        ctx.reference_defs[0].title_byte_end,
        ctx.reference_defs[1].title_byte_start,
    ) && end_a + 1 < start_b
    {
        assert!(!ctx.is_in_link_title(end_a + 1), "Between titles should return false");
    }

    // Position inside third title should return true
    if let Some(start) = ctx.reference_defs[2].title_byte_start {
        assert!(ctx.is_in_link_title(start + 1), "Inside third title should return true");
    }
}

#[test]
fn test_is_in_math_span_between_two_spans() {
    // Position in text between two math spans should return false
    let content = "$a$ text $b$";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let math_spans = ctx.math_spans();
    if math_spans.len() >= 2 {
        let between = math_spans[0].byte_end + 1;
        assert!(
            !ctx.is_in_math_span(between),
            "Position between math spans should return false"
        );
    }
}

// =========================================================================
// Tests for code span and HTML tag detection at boundaries
// =========================================================================

#[test]
fn test_code_span_at_line_start() {
    let content = "Line one\n`code` end\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let spans = ctx.code_spans();
    let line2_spans: Vec<_> = spans.iter().filter(|s| s.line == 2).collect();
    assert!(!line2_spans.is_empty(), "Should detect code span on line 2");
    assert_eq!(line2_spans[0].start_col, 0, "Code span should start at column 0");
}

#[test]
fn test_html_tag_at_byte_zero() {
    let content = "<br/> text";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let tags = ctx.html_tags();
    assert!(!tags.is_empty(), "Should detect HTML tag at byte 0");
    assert_eq!(tags[0].line, 1, "Tag at byte 0 should be on line 1");
}

// =========================================================================
// HTML block detection: CommonMark Type-1 blank-line handling
// =========================================================================
//
// Per CommonMark §4.6, Type-1 HTML blocks open with <pre, <script, <style,
// or <textarea and run until the matching end tag (or EOF). Blank lines do
// not terminate these blocks. Type 6/7 blocks (e.g. <div>, <p>) terminate
// at the first blank line.

#[test]
fn test_html_block_pre_with_blank_line_marks_all_inner_lines() {
    // Reproduces issue #578: a <pre> containing a blank line.
    let content = "# Heading\n\n<pre>\n\nhello  world\n</pre>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.is_in_html_block(3), "line 3 (`<pre>`) should be in html block");
    assert!(
        ctx.is_in_html_block(4),
        "line 4 (blank inside pre) should be in html block"
    );
    assert!(
        ctx.is_in_html_block(5),
        "line 5 (`hello  world`) should be in html block"
    );
    assert!(ctx.is_in_html_block(6), "line 6 (`</pre>`) should be in html block");
}

#[test]
fn test_html_block_textarea_with_blank_line_marks_all_inner_lines() {
    let content = "<textarea>\n\ninner  content\n</textarea>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.is_in_html_block(1), "line 1 (`<textarea>`) should be in html block");
    assert!(ctx.is_in_html_block(2), "line 2 (blank) should be in html block");
    assert!(
        ctx.is_in_html_block(3),
        "line 3 (inner content) should be in html block"
    );
    assert!(
        ctx.is_in_html_block(4),
        "line 4 (`</textarea>`) should be in html block"
    );
}

#[test]
fn test_html_block_long_pre_exceeds_arbitrary_line_cap() {
    // A <pre> with 120 inner lines (no blanks) must mark every inner line,
    // regardless of any internal line cap.
    let mut content = String::from("<pre>\n");
    for i in 0..120 {
        content.push_str(&format!("inner line {i}\n"));
    }
    content.push_str("</pre>\n");

    let ctx = LintContext::new(&content, MarkdownFlavor::Standard, None);

    // 1-indexed: line 1 = <pre>, lines 2..=121 = inner, line 122 = </pre>.
    for line_num in 1..=122 {
        assert!(
            ctx.is_in_html_block(line_num),
            "line {line_num} of a 122-line <pre> block should be marked in_html_block",
        );
    }
}

#[test]
fn test_html_block_div_still_terminates_on_blank_line() {
    // Type-6 guardrail: <div> is not Type-1 and must terminate at blank line.
    let content = "<div>\ninner\n\nafter blank\n</div>\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    assert!(ctx.is_in_html_block(1), "line 1 (`<div>`) should be in html block");
    assert!(ctx.is_in_html_block(2), "line 2 (inner) should be in html block");
    assert!(
        !ctx.is_in_html_block(4),
        "line 4 (`after blank`) must NOT be in html block"
    );
}

#[test]
fn test_html_block_unclosed_pre_extends_to_eof() {
    // Per CommonMark, an unclosed Type-1 block extends to end of document.
    let content = "<pre>\nline a\n\nline b\nline c\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);

    for line_num in 1..=5 {
        assert!(
            ctx.is_in_html_block(line_num),
            "line {line_num} of an unclosed <pre> should extend to EOF",
        );
    }
}

// ---------------------------------------------------------------------------
// Pulldown-cmark gives the same empty CowStr for both "no title" and "explicit
// empty title" (`""`/`''`/`()`). The link parser now rescans the source span
// to recover the distinction so MD054's auto-fix can't silently drop the
// delimiters when converting `[t](url "")` to autolink.
// ---------------------------------------------------------------------------

#[test]
fn test_link_no_title_yields_none() {
    let ctx = LintContext::new("[t](https://x.com)\n", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links.len(), 1);
    assert!(ctx.links[0].title.is_none(), "no title delimiter must be None");
}

#[test]
fn test_link_explicit_empty_double_quote_title_yields_some_empty() {
    let ctx = LintContext::new(r#"[t](https://x.com "")"#, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links.len(), 1);
    assert_eq!(
        ctx.links[0].title.as_deref(),
        Some(""),
        "`\"\"` must be preserved as Some(\"\"), not collapsed to None"
    );
}

#[test]
fn test_link_explicit_empty_single_quote_title_yields_some_empty() {
    let ctx = LintContext::new("[t](https://x.com '')\n", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links.len(), 1);
    assert_eq!(ctx.links[0].title.as_deref(), Some(""));
}

#[test]
fn test_link_explicit_empty_paren_title_yields_some_empty() {
    let ctx = LintContext::new("[t](https://x.com ())\n", MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links.len(), 1);
    assert_eq!(ctx.links[0].title.as_deref(), Some(""));
}

#[test]
fn test_image_explicit_empty_title_yields_some_empty() {
    let ctx = LintContext::new(r#"![alt](https://x.com/img.png "")"#, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.images.len(), 1);
    assert_eq!(ctx.images[0].title.as_deref(), Some(""));
}

#[test]
fn test_link_non_empty_title_is_unaffected() {
    let ctx = LintContext::new(r#"[t](https://x.com "real")"#, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links.len(), 1);
    assert_eq!(ctx.links[0].title.as_deref(), Some("real"));
}

#[test]
fn test_link_title_with_trailing_whitespace_inside_parens() {
    // CommonMark allows whitespace between the closing title delimiter and
    // the link's closing `)`. The detector must skip that whitespace so it
    // still recognizes the explicit-empty-title pair.
    let ctx = LintContext::new(r#"[t](https://x.com ""    )"#, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.links.len(), 1);
    assert_eq!(ctx.links[0].title.as_deref(), Some(""));
}

#[test]
fn test_reference_link_empty_title_in_definition() {
    // Reference links carry their title in the *definition*, parsed by the
    // REF_DEF_PATTERN regex (which already distinguishes `Some("")` from
    // `None` via `cap.get(...)`); make sure that path keeps working.
    let content = "[t][r]\n\n[r]: https://x.com \"\"\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(ctx.reference_defs.len(), 1);
    assert_eq!(ctx.reference_defs[0].title.as_deref(), Some(""));
}

#[test]
fn test_pandoc_flavor_detects_div_blocks() {
    let content = "::: {.callout-note}\nA note.\n:::\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    assert!(
        ctx.is_in_div_block(content.find(":::").unwrap()),
        "Pandoc flavor should detect div block ranges"
    );
}

#[test]
fn test_pandoc_flavor_detects_citations() {
    let content = "See [@smith2020] for details.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("[@smith2020]").unwrap() + 1;
    assert!(ctx.is_in_citation(pos), "Pandoc flavor should detect citation ranges");
}

#[test]
fn test_pandoc_flavor_detects_inline_footnotes() {
    let content = "Text ^[note here] more.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("^[").unwrap() + 1;
    assert!(
        ctx.is_in_inline_footnote(pos),
        "Pandoc flavor should detect inline footnote ranges"
    );
}

#[test]
fn test_standard_flavor_skips_inline_footnotes() {
    let content = "Text ^[note here] more.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("^[").unwrap() + 1;
    assert!(
        !ctx.is_in_inline_footnote(pos),
        "Standard flavor should not detect inline footnote ranges"
    );
}

#[test]
fn test_pandoc_flavor_resolves_implicit_header_reference() {
    let content = "# My Section\n\nSee [My Section] for details.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    assert!(ctx.matches_implicit_header_reference("My Section"));
    assert!(!ctx.matches_implicit_header_reference("Nonexistent"));
}

#[test]
fn test_standard_flavor_does_not_resolve_implicit_header_reference() {
    let content = "# My Section\n\nSee [My Section] for details.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(!ctx.matches_implicit_header_reference("My Section"));
}

#[test]
fn test_pandoc_flavor_detects_example_list_markers() {
    use crate::config::MarkdownFlavor;
    let content = "(@) First item.\n(@good) Second item.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("(@)").unwrap();
    assert!(ctx.is_in_example_list_marker(pos));
    let pos2 = content.find("(@good)").unwrap();
    assert!(ctx.is_in_example_list_marker(pos2));
}

#[test]
fn test_pandoc_flavor_detects_example_references() {
    use crate::config::MarkdownFlavor;
    let content = "(@good) First.\n\nAs shown in (@good), it works.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let ref_pos = content.rfind("(@good)").unwrap();
    assert!(ctx.is_in_example_reference(ref_pos));
    // The line-start marker is NOT a reference (filtered out).
    let marker_pos = content.find("(@good)").unwrap();
    assert!(!ctx.is_in_example_reference(marker_pos));
}

#[test]
fn test_standard_flavor_skips_example_lists() {
    use crate::config::MarkdownFlavor;
    let content = "(@) First.\nAs shown in (@good), it works.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("(@)").unwrap();
    assert!(!ctx.is_in_example_list_marker(pos));
    let ref_pos = content.find("(@good)").unwrap();
    assert!(!ctx.is_in_example_reference(ref_pos));
}

#[test]
fn test_pandoc_flavor_detects_subscript() {
    use crate::config::MarkdownFlavor;
    let content = "H~2~O is water.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("~2~").unwrap() + 1;
    assert!(ctx.is_in_subscript_or_superscript(pos));
}

#[test]
fn test_pandoc_flavor_detects_superscript() {
    use crate::config::MarkdownFlavor;
    let content = "2^10^ is 1024.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("^10^").unwrap() + 1;
    assert!(ctx.is_in_subscript_or_superscript(pos));
}

#[test]
fn test_pandoc_flavor_does_not_match_strikethrough() {
    use crate::config::MarkdownFlavor;
    let content = "This is ~~struck~~.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("~~struck~~").unwrap() + 2;
    assert!(!ctx.is_in_subscript_or_superscript(pos));
}

#[test]
fn test_standard_flavor_skips_sub_super() {
    use crate::config::MarkdownFlavor;
    let content = "H~2~O and 2^10^.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("~2~").unwrap() + 1;
    assert!(!ctx.is_in_subscript_or_superscript(pos));
}

#[test]
fn test_pandoc_flavor_detects_inline_code_attribute() {
    use crate::config::MarkdownFlavor;
    let content = "Use `print()`{.python} for output.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("{.python}").unwrap() + 1;
    assert!(ctx.is_in_inline_code_attr(pos));
}

#[test]
fn test_pandoc_flavor_skips_bare_brace_block() {
    use crate::config::MarkdownFlavor;
    // A `{...}` not preceded by `` `code` `` is not an inline-code attribute.
    let content = "Use {.example} for the class.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("{.example}").unwrap() + 1;
    assert!(!ctx.is_in_inline_code_attr(pos));
}

#[test]
fn test_standard_flavor_skips_inline_code_attribute() {
    use crate::config::MarkdownFlavor;
    let content = "Use `print()`{.python} for output.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("{.python}").unwrap() + 1;
    assert!(!ctx.is_in_inline_code_attr(pos));
}

#[test]
fn test_pandoc_flavor_detects_bracketed_span() {
    use crate::config::MarkdownFlavor;
    let content = "This is [some text]{.smallcaps} here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("[some text]").unwrap();
    assert!(ctx.is_in_bracketed_span(pos));
}

#[test]
fn test_pandoc_flavor_skips_link() {
    use crate::config::MarkdownFlavor;
    let content = "A [link](http://example.com) here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("[link]").unwrap();
    assert!(!ctx.is_in_bracketed_span(pos));
}

#[test]
fn test_standard_flavor_skips_bracketed_span() {
    use crate::config::MarkdownFlavor;
    let content = "This is [some text]{.smallcaps} here.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("[some text]").unwrap();
    assert!(!ctx.is_in_bracketed_span(pos));
}

#[test]
fn test_pandoc_flavor_detects_line_block() {
    use crate::config::MarkdownFlavor;
    let content = "| The Lord of the Rings\n| by J.R.R. Tolkien\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("Lord").unwrap();
    assert!(ctx.is_in_line_block(pos));
}

#[test]
fn test_pandoc_flavor_line_block_does_not_match_pipe_table() {
    use crate::config::MarkdownFlavor;
    let content = "| col1 | col2 |\n|------|------|\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("col1").unwrap();
    assert!(!ctx.is_in_line_block(pos));
}

#[test]
fn test_standard_flavor_skips_line_block() {
    use crate::config::MarkdownFlavor;
    let content = "| The Lord of the Rings\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("Lord").unwrap();
    assert!(!ctx.is_in_line_block(pos));
}

#[test]
fn test_pandoc_flavor_line_block_continuation_is_in_block() {
    // The continuation line (whitespace-indented, no leading pipe) belongs
    // to the active block, so a position inside it must report true.
    use crate::config::MarkdownFlavor;
    let content = "| First line\n  continuation here\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("continuation").unwrap();
    assert!(ctx.is_in_line_block(pos));
}

#[test]
fn test_pandoc_flavor_detects_pipe_table_caption_below() {
    use crate::config::MarkdownFlavor;
    let content = "\
| col1 | col2 |
|------|------|
| a    | b    |

: My caption
";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("My caption").unwrap();
    assert!(ctx.is_in_pipe_table_caption(pos));
}

#[test]
fn test_pandoc_flavor_definition_term_is_not_pipe_table_caption() {
    use crate::config::MarkdownFlavor;
    let content = "Term\n: definition\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("definition").unwrap();
    assert!(!ctx.is_in_pipe_table_caption(pos));
}

#[test]
fn test_standard_flavor_skips_pipe_table_caption() {
    use crate::config::MarkdownFlavor;
    let content = "\
| col1 |
|------|
| a    |

: Caption
";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("Caption").unwrap();
    assert!(!ctx.is_in_pipe_table_caption(pos));
}

#[test]
fn test_pandoc_flavor_detects_pipe_table_caption_above() {
    use crate::config::MarkdownFlavor;
    let content = "\
: Caption first

| col1 | col2 |
|------|------|
| a    | b    |
";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("Caption first").unwrap();
    assert!(ctx.is_in_pipe_table_caption(pos));
}

#[test]
fn test_pandoc_flavor_detects_metadata_block_at_start() {
    use crate::config::MarkdownFlavor;
    let content = "---\ntitle: Doc\n---\n\nBody.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("title").unwrap();
    assert!(ctx.is_in_pandoc_metadata(pos));
    let body_pos = content.find("Body").unwrap();
    assert!(!ctx.is_in_pandoc_metadata(body_pos));
}

#[test]
fn test_pandoc_flavor_detects_mid_document_metadata() {
    use crate::config::MarkdownFlavor;
    let content = "Intro.\n\n---\nauthor: X\n---\n\nBody.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find("author").unwrap();
    assert!(ctx.is_in_pandoc_metadata(pos));
}

#[test]
fn test_standard_flavor_skips_pandoc_metadata() {
    use crate::config::MarkdownFlavor;
    let content = "---\ntitle: Doc\n---\n\nBody.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("title").unwrap();
    assert!(!ctx.is_in_pandoc_metadata(pos));
}

#[test]
fn test_pandoc_flavor_detects_grid_table() {
    use crate::config::MarkdownFlavor;
    let content = "\
+---+---+
| a | b |
+---+---+
| 1 | 2 |
+---+---+
";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let pos = content.find('a').unwrap();
    assert!(ctx.is_in_grid_table(pos));
}

#[test]
fn test_pandoc_flavor_grid_table_excludes_surrounding_text() {
    use crate::config::MarkdownFlavor;
    let content = "Before.\n\n+---+---+\n| a | b |\n+---+---+\n\nAfter.\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let before_pos = content.find("Before").unwrap();
    let after_pos = content.find("After").unwrap();
    assert!(!ctx.is_in_grid_table(before_pos));
    assert!(!ctx.is_in_grid_table(after_pos));
}

#[test]
fn test_standard_flavor_skips_grid_table() {
    use crate::config::MarkdownFlavor;
    let content = "+---+---+\n| a | b |\n+---+---+\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find('a').unwrap();
    assert!(!ctx.is_in_grid_table(pos));
}

#[test]
fn test_pandoc_flavor_detects_multi_line_table() {
    use crate::config::MarkdownFlavor;
    let content = "\
-------------------------------------------------------------
 Centered   Default           Right Left
  Header    Aligned         Aligned Aligned
----------- ------- --------------- -------------------------
   First    row                12.0 Example of a row that
                                    spans multiple lines.

  Second    row                 5.0 Here's another one. Note
                                    the blank line between
                                    rows.
-------------------------------------------------------------
";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    // The entire content should be detected as a single multi-line table.
    let first_pos = content.find("First").unwrap();
    let second_pos = content.find("Second").unwrap();
    assert!(ctx.is_in_multi_line_table(first_pos));
    assert!(ctx.is_in_multi_line_table(second_pos));
    // The detection covers byte 0 (the top border) through content.len().
    assert!(ctx.is_in_multi_line_table(0));
}

#[test]
fn test_pandoc_flavor_multi_line_table_excludes_surrounding_text() {
    use crate::config::MarkdownFlavor;
    let content = "\
Before text.

-------------------------------------------------------------
 Centered   Default           Right Left
  Header    Aligned         Aligned Aligned
----------- ------- --------------- -------------------------
   First    row                12.0 Example.
-------------------------------------------------------------

After text.
";
    let ctx = LintContext::new(content, MarkdownFlavor::Pandoc, None);
    let before_pos = content.find("Before").unwrap();
    let after_pos = content.find("After").unwrap();
    let inside_pos = content.find("First").unwrap();
    assert!(!ctx.is_in_multi_line_table(before_pos));
    assert!(!ctx.is_in_multi_line_table(after_pos));
    assert!(ctx.is_in_multi_line_table(inside_pos));
}

#[test]
fn test_standard_flavor_skips_multi_line_table() {
    use crate::config::MarkdownFlavor;
    let content = "\
-------------------------------------------------------------
 Centered   Default           Right Left
  Header    Aligned         Aligned Aligned
----------- ------- --------------- -------------------------
   First    row                12.0 Example.
-------------------------------------------------------------
";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let pos = content.find("First").unwrap();
    assert!(!ctx.is_in_multi_line_table(pos));
}
