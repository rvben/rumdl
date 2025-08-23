use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD033NoInlineHtml;

#[test]
fn test_complex_html_detection() {
    let rule = MD033NoInlineHtml::default();

    // Test case with various HTML tags and edge cases
    let content = r#"# Heading with <span>inline HTML</span>

Normal paragraph with <strong>bold HTML</strong> and <em>emphasis</em>.

<div>
  Block HTML with nested tags like <span class="highlight">this</span>
</div>

<hr/>

<img src="image.png" alt="Image" />

HTML with attributes: <a href="https://example.com" target="_blank" rel="noopener">Link</a>

<!-- HTML comments should be ignored -->

```html
<div>HTML in code blocks should be ignored</div>
<span>Another tag in code block</span>
```

`<span>HTML in inline code should be ignored</span>`

This is a [link with angle brackets](<https://example.com>)
"#;

    let result = rule.check(content);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Count all the HTML tags not in code blocks or comments
    assert_eq!(warnings.len(), 9);

    // Check that code blocks and inline code are properly ignored
    let fixed = rule.fix(content).unwrap();
    assert!(fixed.contains("```html\n<div>HTML in code blocks should be ignored</div>"));
    assert!(fixed.contains("`<span>HTML in inline code should be ignored</span>`"));
    assert!(fixed.contains("[link with angle brackets](<https://example.com>)"));
    assert!(!fixed.contains("<span>inline HTML</span>"));
    assert!(!fixed.contains("<strong>bold HTML</strong>"));
}

#[test]
fn test_allowed_tags() {
    let rule = MD033NoInlineHtml::with_allowed(vec!["span".to_string(), "em".to_string()]);

    // Test with a mix of allowed and disallowed tags
    let content = r#"# Heading with <span>allowed tag</span>

This has <em>another allowed</em> tag.

But this <strong>should be flagged</strong> as disallowed.

And <div>this block</div> should also be flagged.

This <span class="highlight">has attributes</span> but is still allowed.

This has <em>allowed</em> and <strong>disallowed</strong> tags together.
"#;

    let result = rule.check(content);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should only flag the disallowed tags
    assert_eq!(warnings.len(), 3);

    // Verify the fix only removes disallowed tags
    let fixed = rule.fix(content).unwrap();
    assert!(fixed.contains("<span>allowed tag</span>"));
    assert!(fixed.contains("<em>another allowed</em>"));
    assert!(fixed.contains("<span class=\"highlight\">has attributes</span>"));
    assert!(fixed.contains("<em>allowed</em>"));
    assert!(!fixed.contains("<strong>should be flagged</strong>"));
    assert!(!fixed.contains("<div>this block</div>"));
    assert!(!fixed.contains("<strong>disallowed</strong>"));
}

#[test]
fn test_complex_nested_structures() {
    let rule = MD033NoInlineHtml::default();

    // Test with complex nesting of code, HTML, and markdown
    let content = r#"# Complex document

Normal text with `inline code containing <span>html</span>` should work.

```javascript
function example() {
    // Code with HTML comments <!-- comment -->
    let html = '<div>Some HTML in string</div>';
    return `<span>${html}</span>`;
}
```

<div>
  <h2>A nested structure</h2>
  <ul>
    <li>Item with `inline code inside <span>html tag</span>`</li>
    <li>Another item with **markdown** formatting</li>
    <li>Item with <code>code tag</code> HTML</li>
  </ul>
</div>

This is a paragraph with <!-- HTML comment --> in the middle.

And this has a [link](<http://example.com/with/<angle>brackets>).
"#;

    let result = rule.check(content);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should detect the HTML tags outside code blocks
    assert!(warnings.len() >= 8);

    // Verify the fix preserves code blocks and code spans
    let fixed = rule.fix(content).unwrap();
    assert!(fixed.contains("inline code containing <span>html</span>"));
    assert!(fixed.contains("function example() {"));
    assert!(fixed.contains("let html = '<div>Some HTML in string</div>';"));
    assert!(fixed.contains("return `<span>${html}</span>`;"));
    assert!(!fixed.contains("<div>\n  <h2>A nested structure</h2>"));
    assert!(fixed.contains("[link](<http://example.com/with/<angle>brackets>)"));
}

#[test]
fn test_self_closing_tags() {
    let rule = MD033NoInlineHtml::default();

    // Test with self-closing tags
    let content = r#"# Self-closing tags

<hr/>

<img src="image.jpg" alt="Image"/>

<br>

<input type="text" />

<meta charset="utf-8">

This is a normal paragraph with <br/> a line break.

```html
<img src="code-block.jpg"/> should be ignored
```
"#;

    let result = rule.check(content);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should detect all self-closing tags outside code blocks
    assert_eq!(warnings.len(), 6);

    // Verify the fix handles self-closing tags properly
    let fixed = rule.fix(content).unwrap();
    assert!(!fixed.contains("<hr/>"));
    assert!(!fixed.contains("<img src=\"image.jpg\" alt=\"Image\"/>"));
    assert!(!fixed.contains("<br>"));
    assert!(!fixed.contains("<input type=\"text\" />"));
    assert!(!fixed.contains("<meta charset=\"utf-8\">"));
    assert!(!fixed.contains("normal paragraph with <br/> a line"));
    assert!(fixed.contains("<img src=\"code-block.jpg\"/> should be ignored"));
}

#[test]
fn test_html_with_markdown_inside() {
    let rule = MD033NoInlineHtml::default();

    // Test HTML tags with markdown content inside
    let content = r#"# HTML with Markdown inside

<div>

  ## This is a markdown heading inside HTML

  This is a paragraph with **bold** and *italic* text.

  - List item 1
  - List item 2

</div>

<span>This has **bold markdown** inside HTML</span>

<custom-element>
  ```
  This is a code block inside HTML
  ```
</custom-element>
"#;

    let result = rule.check(content);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should detect the HTML tags
    assert_eq!(warnings.len(), 4);

    // Verify the fix preserves markdown content
    let fixed = rule.fix(content).unwrap();
    assert!(!fixed.contains("<div>"));
    assert!(!fixed.contains("</div>"));
    assert!(!fixed.contains("<span>"));
    assert!(!fixed.contains("<custom-element>"));

    // But preserves the markdown inside
    assert!(fixed.contains("## This is a markdown heading inside HTML"));
    assert!(fixed.contains("paragraph with **bold** and *italic* text"));
    assert!(fixed.contains("- List item 1\n  - List item 2"));
    assert!(fixed.contains("This has **bold markdown** inside HTML"));
    assert!(fixed.contains("```\n  This is a code block inside HTML\n  ```"));
}

#[test]
fn test_html_edge_cases() {
    let rule = MD033NoInlineHtml::default();

    // Test edge cases that might be confused with HTML
    let content = r#"# Edge cases

The < and > characters should not be detected as HTML.

We can use 2 < 3 > 1 in math.

The tag <not-really-a-tag is missing its closing >.

A bracket at end of line <
continued on next line.

An equation like a<sub>i</sub> should be detected.

A [link](<https://example.com/?query=test&param=value>) with URL parameters.

`<span>This is in code span</span>` so it's fine.

3 <= 5 and 7 >= 6 are not HTML.
"#;

    let result = rule.check(content);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should only detect the actual HTML tag
    assert_eq!(warnings.len(), 1);

    // The one detected warning should be for the sub tag
    assert!(warnings[0].message.contains("sub"));

    // Verify the fix preserves non-HTML constructs
    let fixed = rule.fix(content).unwrap();
    assert!(fixed.contains("The < and > characters"));
    assert!(fixed.contains("We can use 2 < 3 > 1 in math"));
    assert!(fixed.contains("The tag <not-really-a-tag is missing its closing >"));
    assert!(fixed.contains("A bracket at end of line <\ncontinued on next line"));
    assert!(!fixed.contains("<sub>"));
    assert!(fixed.contains("[link](<https://example.com/?query=test&param=value>)"));
    assert!(fixed.contains("`<span>This is in code span</span>`"));
    assert!(fixed.contains("3 <= 5 and 7 >= 6 are not HTML"));
}

#[test]
fn test_html_comments() {
    let rule = MD033NoInlineHtml::default();

    // Test HTML comments in various contexts
    let content = r#"# HTML Comments

<!-- This is an HTML comment -->

Normal text.

Text with <!-- inline comment --> in the middle.

<!--
  Multi-line
  HTML comment
-->

```html
<!-- Comment in code block should be ignored -->
```

`<!-- Comment in inline code should be ignored -->`
"#;

    let result = rule.check(content);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // HTML comments should not trigger warnings
    assert_eq!(warnings.len(), 0);

    // Fix should preserve HTML comments
    let fixed = rule.fix(content).unwrap();
    assert_eq!(fixed, content);
}

#[test]
fn test_binary_search_optimization() {
    let rule = MD033NoInlineHtml::default();

    // Create content with many code spans to trigger binary search optimization
    let mut content = String::from("# Binary search test\n\n");

    // Add 20 code spans with HTML inside to trigger binary search
    for i in 1..21 {
        content.push_str(&format!("`<span>HTML in code span {}</span>` ", i));
        if i % 5 == 0 {
            content.push_str("\n\n");
        }
    }

    // Add some actual HTML that should be detected
    content.push_str("<div>This should be detected</div>\n\n");

    let result = rule.check(&content);
    assert!(result.is_ok());
    let warnings = result.unwrap();

    // Should only detect the real HTML, not the ones in code spans
    assert_eq!(warnings.len(), 1);

    // Verify the fix preserves code spans
    let fixed = rule.fix(&content).unwrap();
    assert!(fixed.contains("`<span>HTML in code span 1</span>`"));
    assert!(fixed.contains("`<span>HTML in code span 20</span>`"));
    assert!(!fixed.contains("<div>This should be detected</div>"));
}
