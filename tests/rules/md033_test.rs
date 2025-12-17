use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD033NoInlineHtml;

#[test]
fn test_no_html() {
    let rule = MD033NoInlineHtml::default();
    let content = "Just regular markdown\n\n# Heading\n\n* List item";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_simple_html_tag() {
    let rule = MD033NoInlineHtml::default();
    let content = "Some <b>bold</b> text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Only reports opening tags (only opening tags)
    assert_eq!(result.len(), 1); // Only <b>
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 6); // <b> tag
}

#[test]
fn test_self_closing_tag() {
    let rule = MD033NoInlineHtml::default();
    let content = "An image: <img src=\"test.png\" />";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Current implementation detects self-closing HTML tags
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 1);
    assert_eq!(result[0].column, 11); // <img src="test.png" />
}

#[test]
fn test_allowed_elements() {
    let rule = MD033NoInlineHtml::with_allowed(vec!["b".to_string(), "i".to_string()]);
    let content = "Some <b>bold</b> and <i>italic</i> but not <u>underlined</u>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_html_in_code_block() {
    let rule = MD033NoInlineHtml::default();
    let content = "Normal text\n```\n<div>This is in a code block</div>\n```\nMore text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_fix_html_tags() {
    let rule = MD033NoInlineHtml::default();
    let content = "Some <b>bold</b> text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_fix_self_closing_tags() {
    let rule = MD033NoInlineHtml::default();
    let content = "Line break<br/>here";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_multiple_tags() {
    let rule = MD033NoInlineHtml::default();
    let content = "<div><p>Nested</p></div>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Only reports opening tags (only opening tags)
    assert_eq!(result.len(), 2); // Only <div> and <p>
    assert_eq!(result[0].column, 1); // <div> tag
    assert_eq!(result[1].column, 6); // <p> tag
}

#[test]
fn test_attributes() {
    let rule = MD033NoInlineHtml::default();
    let content = "<div class=\"test\" id=\"main\">Content</div>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Only reports opening tags (only opening tags)
    assert_eq!(result.len(), 1); // Only <div>
    assert_eq!(result[0].column, 1); // <div> tag
}

#[test]
fn test_mixed_content() {
    let rule = MD033NoInlineHtml::default();
    let content = "# Heading\n\n<div>HTML content</div>\n\n* List item\n\n<span>More HTML</span>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Only reports opening tags (only opening tags)
    // Two lines with HTML: line 3 and line 7
    assert_eq!(result.len(), 2); // Only <div> and <span>
    assert_eq!(result[0].line, 3); // <div> line
    assert_eq!(result[1].line, 7); // <span> line
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_preserve_content() {
    let rule = MD033NoInlineHtml::default();
    let content = "Text with <strong>important</strong> content";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_multiline_html() {
    let rule = MD033NoInlineHtml::default();
    let content = "<div>\nMultiline\ncontent\n</div>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // Only detects opening tags (only opening tags)
    assert_eq!(result.len(), 1);
}

#[test]
fn test_ignore_code_spans() {
    let rule = MD033NoInlineHtml::default();
    let content = "Use `<div>` for a block element";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_complex_code_block_patterns() {
    let rule = MD033NoInlineHtml::default();

    // Test with mixed fence styles
    let content = "Text\n```\n<div>Code block 1</div>\n```\nMore text\n~~~\n<span>Code block 2</span>\n~~~\nEnd text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);

    // Test with code block at start of document
    let content = "```\n<div>Starts with code</div>\n```\nText with <b>bold</b>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only <b> outside code block

    // Test with code block at end of document
    let content = "Text with <i>italic</i>\n```\n<div>Ends with code</div>\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only <i> outside code block

    // Test adjacent code blocks
    let content = "```\n<div>Block 1</div>\n```\n```\n<span>Block 2</span>\n```";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_code_span_binary_search() {
    let rule = MD033NoInlineHtml::default();

    // Test HTML tag immediately before a code span
    let content = "<span>`code`</span>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only <span> outside code span

    // Test HTML tag immediately after a code span
    let content = "`code`<div>text</div>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only <div> outside code span

    // Test HTML tag exactly at position boundaries
    let content = "Text `<div>` more text";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0); // <div> is inside code span

    // Test many code spans to trigger binary search optimization
    let content = "`1` `2` `3` `4` `5` `6` `7` `8` `9` `10` `11` `12` <span>text</span>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Only <span> outside code spans
}

#[test]
fn test_fix_preserves_structure_html() {
    let rule = MD033NoInlineHtml::default();

    // Verify HTML fix is a no-op (output equals input)
    let content = "Normal <b>bold</b>\n```\n<div>Code block</div>\n```\nMore <i>italic</i>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);

    let content = "Text with `<span>` and <div>block</div>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);

    let content = "<div><p>Nested content</p></div>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_markdown_comments() {
    let rule = MD033NoInlineHtml::default();

    // Test with markdownlint comments
    let content =
        "Some content\n<!-- markdownlint-disable -->\nIgnored content\n<!-- markdownlint-enable -->\nMore content";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // These should not be flagged as HTML tags
    assert!(result.is_empty(), "Markdown comments should not be flagged as HTML");

    // Test with regular HTML comments
    let content = "Some content\n<!-- This is a comment -->\nMore content";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Comments should not be flagged
    assert!(result.is_empty(), "HTML comments should not be flagged as HTML tags");
}

#[test]
fn test_urls_in_angle_brackets() {
    let rule = MD033NoInlineHtml::default();

    // Test various URL schemes in angle brackets
    let content = "Visit <https://example.com> or <http://test.org>\n\
                   Download from <ftp://files.example.com/file.zip>\n\
                   Secure transfer: <ftps://secure.example.com/data>\n\
                   Contact us: <mailto:user@example.com>\n\
                   Complex URL: <https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // URLs in angle brackets should not be flagged as HTML
    assert!(
        result.is_empty(),
        "URLs in angle brackets should not be flagged as HTML tags"
    );
}

#[test]
fn test_mixed_urls_and_html() {
    let rule = MD033NoInlineHtml::default();

    // Test content with both URLs in angle brackets and real HTML tags
    let content = "Visit <https://example.com> for more info.\n\
                   This has <strong>real HTML</strong> tags.\n\
                   Email us at <mailto:test@example.com> or use <em>emphasis</em>.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the real HTML tags (opening tags only), not the URLs
    assert_eq!(result.len(), 2); // Only <strong> and <em> opening tags

    // Verify the flagged tags are the HTML ones
    let flagged_content: Vec<String> = result
        .iter()
        .map(|w| {
            let line_content = content.lines().nth(w.line - 1).unwrap();
            let start = w.column - 1;
            let tag_end = line_content[start..].find('>').unwrap() + start + 1;
            line_content[start..tag_end].to_string()
        })
        .collect();

    assert!(flagged_content.contains(&"<strong>".to_string()));
    assert!(flagged_content.contains(&"<em>".to_string()));

    // Verify URLs are not in the flagged content
    assert!(!flagged_content.iter().any(|tag| tag.contains("https://")));
    assert!(!flagged_content.iter().any(|tag| tag.contains("mailto:")));
}

#[test]
fn test_edge_case_urls() {
    let rule = MD033NoInlineHtml::default();

    // Test edge cases that might be confused
    // Now MD033 only flags actual HTML elements, not placeholder syntax like <notaurl>
    let content = "Not a URL: <notaurl>\n\
                   Real URL: <https://example.com>\n\
                   Fake tag: <https>\n\
                   Real tag: <div>";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag <div> - the only actual HTML element
    // <notaurl> and <https> are placeholder syntax, not HTML elements
    // <https://example.com> is a valid autolink URL
    assert_eq!(result.len(), 1);

    let flagged_positions: Vec<(usize, usize)> = result.iter().map(|w| (w.line, w.column)).collect();

    // <div> should be flagged (line 4)
    assert!(flagged_positions.contains(&(4, 11)));
}

// REGRESSION TESTS: Ensure MD033 properly follows CommonMark specification for indented code blocks

#[test]
fn test_md033_commonmark_indented_html_ignored() {
    let content = r#"# Test Document

Regular HTML: <div>should be flagged</div>

Indented HTML (CommonMark code block):

    <div>should NOT be flagged</div>
    <p>also should NOT be flagged</p>

More regular HTML: <span>should be flagged</span>"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD033NoInlineHtml::default();
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the regular HTML (opening tags only), not the indented HTML
    assert_eq!(warnings.len(), 2); // <div>, <span> (opening tags only)

    // Verify the flagged lines are only the regular HTML
    let flagged_lines: Vec<usize> = warnings.iter().map(|w| w.line).collect();
    assert!(flagged_lines.contains(&3)); // <div>should be flagged</div>
    assert!(flagged_lines.contains(&10)); // <span>should be flagged</span>

    // Verify indented HTML lines are NOT flagged
    assert!(!flagged_lines.contains(&7)); // indented <div>
    assert!(!flagged_lines.contains(&8)); // indented <p>
}

#[test]
fn test_md033_exactly_four_spaces_is_code_block() {
    let content = r#"# Test Document

Three spaces (not code):   <div>flagged</div>

Exactly four spaces (code block):

    <div>not flagged</div>

Five spaces (also code block):

     <div>not flagged</div>

Tab indented (code block):

	<div>not flagged</div>

Regular HTML: <p>flagged</p>"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD033NoInlineHtml::default();
    let warnings = rule.check(&ctx).unwrap();

    // Should flag: line 3 (three spaces), line 17 (regular)
    // Should NOT flag: lines 7, 11, 15 (all properly indented)
    let flagged_lines: Vec<usize> = warnings.iter().map(|w| w.line).collect();

    assert!(flagged_lines.contains(&3)); // Three spaces
    assert!(flagged_lines.contains(&17)); // Regular HTML

    assert!(!flagged_lines.contains(&7)); // Four spaces
    assert!(!flagged_lines.contains(&11)); // Five spaces
    assert!(!flagged_lines.contains(&15)); // Tab indented
}

#[test]
fn test_md033_mixed_indented_content_in_code_blocks() {
    let content = r#"# Test Document

Regular HTML: <div>flagged</div>

Mixed indented content (all in code block):

    <div>HTML in code</div>
    Regular text in code
    More content in code
    <p>More HTML in code</p>

Back to regular: <span>flagged</span>"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD033NoInlineHtml::default();
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the regular HTML outside code blocks
    let flagged_lines: Vec<usize> = warnings.iter().map(|w| w.line).collect();

    assert!(flagged_lines.contains(&3)); // Regular HTML
    assert!(flagged_lines.contains(&12)); // Back to regular

    // Should NOT flag any of the indented content
    assert!(!flagged_lines.contains(&7)); // <div>HTML in code</div>
    assert!(!flagged_lines.contains(&10)); // <p>More HTML in code</p>
}

#[test]
fn test_md033_indented_code_with_blank_lines() {
    let content = r#"# Test Document

Regular: <div>flagged</div>

Indented code block with blank lines:

    <div>first block</div>

    <p>second block after blank line</p>

    <span>third block</span>

Regular again: <em>flagged</em>"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD033NoInlineHtml::default();
    let warnings = rule.check(&ctx).unwrap();

    let flagged_lines: Vec<usize> = warnings.iter().map(|w| w.line).collect();

    // Should flag regular HTML
    assert!(flagged_lines.contains(&3)); // Regular
    assert!(flagged_lines.contains(&13)); // Regular again

    // Should NOT flag indented HTML (even with blank lines between)
    assert!(!flagged_lines.contains(&7)); // first block
    assert!(!flagged_lines.contains(&9)); // second block
    assert!(!flagged_lines.contains(&11)); // third block
}

#[test]
fn test_md033_fenced_vs_indented_code_blocks() {
    let content = r#"# Test Document

Regular: <div>flagged</div>

Fenced code block:
```html
<div>not flagged in fenced</div>
<p>also not flagged</p>
```

Indented code block:

    <div>not flagged in indented</div>
    <p>also not flagged</p>

Regular: <span>flagged</span>"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD033NoInlineHtml::default();
    let warnings = rule.check(&ctx).unwrap();

    let flagged_lines: Vec<usize> = warnings.iter().map(|w| w.line).collect();

    // Should flag regular HTML
    assert!(flagged_lines.contains(&3)); // Regular
    assert!(flagged_lines.contains(&16)); // Regular

    // Should NOT flag HTML in either type of code block
    assert!(!flagged_lines.contains(&7)); // fenced <div>
    assert!(!flagged_lines.contains(&8)); // fenced <p>
    assert!(!flagged_lines.contains(&13)); // indented <div>
    assert!(!flagged_lines.contains(&14)); // indented <p>
}

#[test]
fn test_md033_complex_html_in_indented_blocks() {
    let content = r#"# Test Document

Regular: <div class="test">flagged</div>

Complex indented HTML:

    <div class="container">
        <p id="test">Nested HTML</p>
        <span data-value="123">More content</span>
        <!-- Comment in code -->
        <img src="test.jpg" alt="image" />
    </div>

Self-closing in regular: <br />

More indented:

    <table>
        <tr>
            <td>Cell content</td>
        </tr>
    </table>

Regular: <em>flagged</em>"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD033NoInlineHtml::default();
    let warnings = rule.check(&ctx).unwrap();

    let flagged_lines: Vec<usize> = warnings.iter().map(|w| w.line).collect();

    // Should flag only regular HTML (opening tags only), not indented HTML
    assert_eq!(warnings.len(), 3); // 1 tag on line 3, 1 on line 14, 1 on line 24 (opening tags only)
    assert!(flagged_lines.contains(&3)); // Regular div (opening tag)
    assert!(flagged_lines.contains(&14)); // Self-closing br
    assert!(flagged_lines.contains(&24)); // Regular em (opening tag)
}

#[test]
fn test_md033_edge_cases_indentation() {
    // Per CommonMark spec, an indented code block requires:
    // 1. A blank line before it (or start of document)
    // 2. 4+ spaces of indentation
    // If a non-indented line appears, it breaks the code block
    let content = r#"# Test Document

Regular: <div>flagged</div>

Mixed indentation levels:

    <div>4 spaces - code block</div>
        <p>8 spaces - still code block</p>
   <span>3 spaces - NOT code block</span>
    <em>4 spaces - but code block was broken by line 9</em>

Regular: <strong>flagged</strong>"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD033NoInlineHtml::default();
    let warnings = rule.check(&ctx).unwrap();

    let flagged_lines: Vec<usize> = warnings.iter().map(|w| w.line).collect();

    // Should flag regular HTML, 3-space indented, and line 10 (code block broken by line 9)
    assert!(flagged_lines.contains(&3)); // Regular
    assert!(flagged_lines.contains(&9)); // 3 spaces breaks code block
    assert!(flagged_lines.contains(&10)); // 4 spaces but code block was broken
    assert!(flagged_lines.contains(&12)); // Regular

    // Should NOT flag 4+ space indented within code block
    assert!(!flagged_lines.contains(&7)); // 4 spaces - starts code block
    assert!(!flagged_lines.contains(&8)); // 8 spaces - continues code block
}

#[test]
fn test_md033_indented_html_in_lists() {
    let content = r#"# Test Document

Regular: <div>flagged</div>

1. List item with indented code:

       <div>should NOT be flagged</div>
       <p>also should NOT be flagged</p>

2. Another item: <span>should be flagged</span>

   But this indented HTML:

       <em>should NOT be flagged</em>

Regular: <strong>flagged</strong>"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD033NoInlineHtml::default();
    let warnings = rule.check(&ctx).unwrap();

    let flagged_lines: Vec<usize> = warnings.iter().map(|w| w.line).collect();

    // Should flag regular HTML and HTML in list items (not indented enough)
    assert!(flagged_lines.contains(&3)); // Regular
    assert!(flagged_lines.contains(&10)); // List item HTML
    assert!(flagged_lines.contains(&16)); // Regular

    // Should NOT flag properly indented HTML (even in lists)
    assert!(!flagged_lines.contains(&7)); // Indented in list
    assert!(!flagged_lines.contains(&8)); // Indented in list
    assert!(!flagged_lines.contains(&14)); // Indented in list
}

#[test]
fn test_md033_regression_original_behavior_preserved() {
    let content = r#"# Test Document

Regular HTML tags should be flagged:
<div>flagged</div>
<p>flagged</p>
<span>flagged</span>

But indented code blocks should be ignored:

    <div>not flagged</div>
    <p>not flagged</p>
    <span>not flagged</span>

And fenced code blocks should be ignored:
```html
<div>not flagged</div>
<p>not flagged</p>
```

Back to regular: <em>flagged</em>"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD033NoInlineHtml::default();
    let warnings = rule.check(&ctx).unwrap();

    // Should flag all regular HTML but none in code blocks
    let flagged_lines: Vec<usize> = warnings.iter().map(|w| w.line).collect();

    // Regular HTML should be flagged
    assert!(flagged_lines.contains(&4)); // <div>
    assert!(flagged_lines.contains(&5)); // <p>
    assert!(flagged_lines.contains(&6)); // <span>
    assert!(flagged_lines.contains(&20)); // <em>

    // Code block HTML should NOT be flagged
    assert!(!flagged_lines.contains(&10)); // indented <div>
    assert!(!flagged_lines.contains(&11)); // indented <p>
    assert!(!flagged_lines.contains(&12)); // indented <span>
    assert!(!flagged_lines.contains(&16)); // fenced <div>
    assert!(!flagged_lines.contains(&17)); // fenced <p>

    // Verify we have the expected number of warnings
    assert_eq!(warnings.len(), 4); // 4 opening tags
}

#[test]
fn test_html_inside_html_comments_should_not_be_flagged() {
    let rule = MD033NoInlineHtml::default();

    // Test case from BACKERS.md - HTML inside HTML comment should NOT be flagged
    let content = r#"# Backers

<!--
<table>
  <tr>
    <td align="center">
      <a href="[PROFILE_URL]">
        <img src="[PROFILE_IMG_SRC]" width="50" />
      </a>
    </td>
  </tr>
</table>
-->

This should be flagged: <div>real HTML</div>

<!-- Another comment with <span>HTML</span> inside -->

More real HTML: <p>flagged</p>"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag HTML outside comments (opening tags only)
    assert_eq!(result.len(), 2); // Only <div> and <p>

    let flagged_lines: Vec<usize> = result.iter().map(|w| w.line).collect();

    // Should flag HTML outside comments
    assert!(flagged_lines.contains(&15)); // <div>real HTML</div>
    assert!(flagged_lines.contains(&19)); // <p>flagged</p>

    // Should NOT flag HTML inside comments
    assert!(!flagged_lines.contains(&4)); // <table> inside comment
    assert!(!flagged_lines.contains(&5)); // <tr> inside comment
    assert!(!flagged_lines.contains(&6)); // <td> inside comment
    assert!(!flagged_lines.contains(&7)); // <a> inside comment
    assert!(!flagged_lines.contains(&8)); // <img> inside comment
    assert!(!flagged_lines.contains(&17)); // <span> inside comment
}
