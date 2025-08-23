use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD042NoEmptyLinks;

#[test]
fn test_valid_links() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[Link text](https://example.com)\n[Another link](./local/path)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_link_text() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[](https://example.com)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[Link text](https://example.com)");
}

#[test]
fn test_empty_link_url() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[Link text]()";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[Link text](https://example.com)");
}

#[test]
fn test_empty_link_both() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[]()";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[Link text](https://example.com)");
}

#[test]
fn test_multiple_empty_links() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[Link]() and []() and [](url)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 3);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "[Link](https://example.com) and [Link text](https://example.com) and [Link text](url)"
    );
}

#[test]
fn test_whitespace_only_links() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[ ](  )";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, "[Link text](https://example.com)");
}

#[test]
fn test_mixed_valid_and_empty_links() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[Valid](https://example.com) and []() and [Another](./path)";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "[Valid](https://example.com) and [Link text](https://example.com) and [Another](./path)"
    );
}

// REGRESSION TESTS: Ensure MD042 properly ignores links in code blocks and code spans

#[test]
fn test_md042_ignores_links_in_fenced_code_blocks() {
    let content = r#"# Test Document

Regular empty link: [](https://example.com)

Fenced code block with empty links:

```markdown
[empty link]()
[another empty]()
```

Another regular empty link: [empty text]()"#;

    let ctx = LintContext::new(content);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the two regular empty links, not the ones in the code block
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 3); // Regular empty link
    assert_eq!(warnings[1].line, 12); // Another regular empty link
}

#[test]
fn test_md042_ignores_links_in_indented_code_blocks() {
    let content = r#"# Test Document

Regular empty link: [](https://example.com)

Indented code block with empty links:

    [empty link]()
    [another empty]()

Another regular empty link: [empty text]()"#;

    let ctx = LintContext::new(content);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the two regular empty links, not the ones in the indented code block
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 3); // Regular empty link
    assert_eq!(warnings[1].line, 10); // Another regular empty link
}

#[test]
fn test_md042_ignores_links_in_code_spans() {
    let content = r#"# Test Document

Regular empty link: [](https://example.com)

Inline code with empty link: `[empty link]()`

Another regular empty link: [empty text]()

More inline code: `Check this [empty]() link`"#;

    let ctx = LintContext::new(content);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the two regular empty links, not the ones in code spans
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 3); // Regular empty link
    assert_eq!(warnings[1].line, 7); // Another regular empty link
}

#[test]
fn test_md042_complex_nested_scenarios() {
    let content = r#"# Test Document

Regular empty link: [](https://example.com)

## Code blocks in lists

1. First item with code:

    ```markdown
    [empty in fenced]()
    ```

2. Second item with indented code:

        [empty in indented]()

3. Third item with inline code: `[empty in span]()`

## Blockquotes with code

> Regular empty link in quote: [empty]()
>
> Code in quote:
> ```
> [empty in quoted code]()
> ```

Final empty link: []()"#;

    let ctx = LintContext::new(content);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should flag: line 3 (regular), line 21 (in blockquote), line 28 (final)
    // Should NOT flag: lines 10, 15, 17, 25 (all in code)
    // Note: We correctly detect code blocks in blockquotes, unlike markdownlint
    assert_eq!(warnings.len(), 3);
    assert_eq!(warnings[0].line, 3); // Regular empty link
    assert_eq!(warnings[1].line, 21); // Empty link in blockquote
    assert_eq!(warnings[2].line, 28); // Final empty link
}

#[test]
fn test_md042_mixed_code_block_types() {
    let content = r#"# Test Document

Empty link: [](https://example.com)

Fenced with language:
```javascript
// [empty link]() in comment
```

Fenced without language:
```
[empty link]()
```

Indented code:

    [empty link]()

Tilde fenced:
~~~
[empty link]()
~~~

Final empty: [empty text]()"#;

    let ctx = LintContext::new(content);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the first and last empty links
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 3); // Empty link
    assert_eq!(warnings[1].line, 24); // Final empty
}

#[test]
fn test_md042_reference_links_in_code() {
    let content = r#"# Test Document

Regular empty reference: [][empty-ref]

Code block with reference:
```
[][empty-ref]
[text][empty-ref]
```

Inline code: `[][empty-ref]`

[empty-ref]: "#;

    let ctx = LintContext::new(content);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the regular empty reference link, not the ones in code
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 3); // Regular empty reference
}

#[test]
fn test_md042_edge_cases_with_code() {
    let content = r#"# Test Document

Empty link: []()

Mixed content:
```markdown
Valid: [text](url)
Empty: []()
```

    Indented empty: []()

Inline: `[]()` and more text

Final: []()"#;

    let ctx = LintContext::new(content);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should flag lines 3 and 15 only
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 3); // Empty link
    assert_eq!(warnings[1].line, 15); // Final
}

#[test]
fn test_md042_reference_links() {
    let rule = MD042NoEmptyLinks::new();

    // Test valid reference link
    let content = "[text][ref]\n\n[ref]: https://example.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test empty text reference link
    let content = "[][ref]\n\n[ref]: https://example.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    // Test reference link with missing definition
    let content = "[text][missing]";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Empty URL due to missing reference

    // Test empty text with implicit reference
    let content = "[text][]\n\n[text]: https://example.com";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Valid implicit reference

    // Test both text and URL empty
    let content = "[][]";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Empty text and no matching reference
}
