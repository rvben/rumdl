use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD027MultipleSpacesBlockquote;

#[test]
fn test_md027_valid() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = "> Quote\n> Another line\n> Third line\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_md027_invalid() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = ">  Quote\n>   Another line\n>    Third line\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[1].line, 2);
    assert_eq!(result[2].line, 3);
}

#[test]
fn test_md027_mixed() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = "> Quote\n>  Another line\n> Third line\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
}

#[test]
fn test_md027_fix() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = ">  Quote\n>   Another line\n>    Third line\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    assert_eq!(result, "> Quote\n> Another line\n> Third line\n");
}

// =============================================================================
// HTML Block Edge Cases - CommonMark spec compliance
// =============================================================================
// Per CommonMark spec: HTML blocks (type 6) continue until a blank line.
// Content immediately following closing tags (without blank line) is still
// part of the HTML block and should NOT be parsed as markdown.

/// Test: Content immediately after closing div tag (no blank line)
/// Per CommonMark: HTML block continues until blank line
#[test]
fn test_md027_html_block_no_blank_after_closing_div() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = "<div>\ncontent\n</div>\n>  After div no blank\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Should NOT flag - line 4 is still in HTML block (no blank line after </div>)
    assert!(
        result.is_empty(),
        "Content after closing tag (no blank line) should be in HTML block, got {} warnings",
        result.len()
    );
}

/// Test: Content after closing div with blank line separation
/// The blank line terminates the HTML block, so the blockquote IS markdown
#[test]
fn test_md027_html_block_with_blank_after_closing_div() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = "<div>\ncontent\n</div>\n\n>  After div with blank\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // SHOULD flag - line 5 is a real blockquote (blank line terminated HTML block)
    assert_eq!(result.len(), 1, "Blockquote after blank line should be flagged");
    assert_eq!(result[0].line, 5);
}

/// Test: Table with blockquote-like content in cells
#[test]
fn test_md027_table_with_gt_symbols() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = "<table>\n<tr>\n<td>>  Cell content</td>\n</tr>\n</table>\n>  After table\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Neither should flag - both are in HTML block context
    assert!(
        result.is_empty(),
        "Content in table and after (no blank) should not be flagged, got {} warnings",
        result.len()
    );
}

/// Test: Nested HTML blocks with blockquote-like content
#[test]
fn test_md027_nested_html_blocks() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = "<div>\n<table>\n>  In nested HTML\n</table>\n>  Still in div\n</div>\n>  After div\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // All lines should be in HTML block
    assert!(
        result.is_empty(),
        "All lines should be in HTML block, got {} warnings",
        result.len()
    );
}

/// Test: Multiple HTML blocks with varying blank line patterns
#[test]
fn test_md027_multiple_html_blocks() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = r#"<div>
content
</div>
>  No blank after div1

<article>
more
</article>

>  Blank after article - should flag
"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Only the last one (after blank line) should flag
    assert_eq!(result.len(), 1, "Only blockquote after blank line should flag");
    assert_eq!(result[0].line, 10);
}

/// Test: HTML5 media elements (figure, video, audio, picture)
#[test]
fn test_md027_html5_media_elements() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = r#"<figure>
>  Figure caption
</figure>
>  After figure

<video>
>  Video description
</video>
>  After video
"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // All should be in HTML blocks (no blank lines)
    assert!(
        result.is_empty(),
        "Content in HTML5 media elements should not flag, got {} warnings",
        result.len()
    );
}

/// Test: Self-closing tags followed by blockquote-like content
#[test]
fn test_md027_self_closing_tags() {
    let rule = MD027MultipleSpacesBlockquote;
    // Self-closing tags like <br/> don't create block context
    // So >  after them should flag
    let content = "<br/>\n>  After self-closing\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Self-closing tag doesn't create block context, so this should flag
    assert_eq!(result.len(), 1, "Content after self-closing tag should flag");
}

/// Test: HTML block with style tag (can contain blank lines)
#[test]
fn test_md027_style_tag_allows_blanks() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = "<style>\n.class {\n  color: red;\n}\n\n.other {\n  color: blue;\n}\n</style>\n>  After style\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Style tag context should continue through blank lines
    // And content after </style> (no blank) should not flag
    assert!(result.is_empty(), "Content after style tag should not flag");
}

/// Test: HTML block with script tag (can contain blank lines)
#[test]
fn test_md027_script_tag_allows_blanks() {
    let rule = MD027MultipleSpacesBlockquote;
    let content = "<script>\nfunction test() {\n  return true;\n}\n\nconsole.log();\n</script>\n>  After script\n";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Script tag context should continue through blank lines
    assert!(result.is_empty(), "Content after script tag should not flag");
}
