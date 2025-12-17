use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD042NoEmptyLinks;

#[test]
fn test_valid_links() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[Link text](https://example.com)\n[Another link](./local/path)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_link_text() {
    // MD042 only flags empty URLs, not empty text
    // Empty text with valid URL is an accessibility concern, not an "empty link"
    let rule = MD042NoEmptyLinks::new();
    let content = "[](https://example.com)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty(), "Empty text with valid URL should not be flagged");
}

#[test]
fn test_empty_link_url() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[Link text]()";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    // Non-URL text with empty URL is not fixable - we can't guess the URL
    assert!(result[0].fix.is_none(), "Non-URL text should not have auto-fix");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "Should not modify unfixable links");
}

#[test]
fn test_empty_link_both() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[]()";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    // Both empty is not fixable - we can't guess either
    assert!(result[0].fix.is_none(), "Both empty should not have auto-fix");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "Should not modify unfixable links");
}

#[test]
fn test_multiple_empty_links() {
    // MD042 only flags empty URLs, not empty text
    let rule = MD042NoEmptyLinks::new();
    let content = "[Link]() and []() and [](url)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    // [Link]() - flagged (empty URL)
    // []() - flagged (empty URL)
    // [](url) - NOT flagged (has URL, empty text is not an error)
    assert_eq!(result.len(), 2, "Should only flag links with empty URLs");
}

#[test]
fn test_whitespace_only_links() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[ ](  )";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    // Both empty (whitespace is trimmed) - not fixable
    assert!(result[0].fix.is_none(), "Whitespace-only should not have auto-fix");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "Should not modify unfixable links");
}

#[test]
fn test_mixed_valid_and_empty_links() {
    let rule = MD042NoEmptyLinks::new();
    let content = "[Valid](https://example.com) and []() and [Another](./path)";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    // []() is not fixable (both empty)
    assert!(result[0].fix.is_none(), "Both empty should not have auto-fix");
    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(fixed, content, "Should not modify unfixable links");
}

// REGRESSION TESTS: Ensure MD042 properly ignores links in code blocks and code spans

#[test]
fn test_md042_ignores_links_in_fenced_code_blocks() {
    // MD042 only flags empty URLs, not empty text
    let content = r#"# Test Document

Regular empty link: [empty text]()

Fenced code block with empty links:

```markdown
[empty link]()
[another empty]()
```

Another regular empty link: [empty text]()"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the two regular empty links (empty URLs), not the ones in the code block
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 3); // Regular empty link
    assert_eq!(warnings[1].line, 12); // Another regular empty link
}

#[test]
fn test_md042_ignores_links_in_indented_code_blocks() {
    // MD042 only flags empty URLs, not empty text
    let content = r#"# Test Document

Regular empty link: [empty text]()

Indented code block with empty links:

    [empty link]()
    [another empty]()

Another regular empty link: [empty text]()"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the two regular empty links (empty URLs), not the ones in the indented code block
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 3); // Regular empty link
    assert_eq!(warnings[1].line, 10); // Another regular empty link
}

#[test]
fn test_md042_ignores_links_in_code_spans() {
    // MD042 only flags empty URLs, not empty text
    let content = r#"# Test Document

Regular empty link: [empty text]()

Inline code with empty link: `[empty link]()`

Another regular empty link: [empty text]()

More inline code: `Check this [empty]() link`"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the two regular empty links (empty URLs), not the ones in code spans
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 3); // Regular empty link
    assert_eq!(warnings[1].line, 7); // Another regular empty link
}

#[test]
fn test_md042_complex_nested_scenarios() {
    // MD042 only flags empty URLs, not empty text
    let content = r#"# Test Document

Regular empty link: [empty text]()

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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should flag: line 3 (empty URL), line 21 (empty URL in blockquote), line 28 (empty URL)
    // Should NOT flag: lines 10, 15, 17, 25 (all in code blocks)
    assert_eq!(warnings.len(), 3);
    assert_eq!(warnings[0].line, 3); // Regular empty link
    assert_eq!(warnings[1].line, 21); // Empty link in blockquote
    assert_eq!(warnings[2].line, 28); // Final empty link
}

#[test]
fn test_md042_mixed_code_block_types() {
    // MD042 only flags empty URLs, not empty text
    let content = r#"# Test Document

Empty link: [empty text]()

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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should only flag the first and last empty links (both have empty URLs)
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
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

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let rule = MD042NoEmptyLinks::new();
    let warnings = rule.check(&ctx).unwrap();

    // Should flag lines 3 and 15 only
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].line, 3); // Empty link
    assert_eq!(warnings[1].line, 15); // Final
}

#[test]
fn test_md042_reference_links() {
    // MD042 only flags empty URLs, not empty text
    let rule = MD042NoEmptyLinks::new();

    // Test valid reference link
    let content = "[text][ref]\n\n[ref]: https://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());

    // Test empty text reference link with valid URL - NOT flagged (empty text is not an error)
    let content = "[][ref]\n\n[ref]: https://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Empty text with valid reference should not be flagged"
    );

    // Test reference link with missing definition - handled by MD052, not MD042
    let content = "[text][missing]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Undefined references are handled by MD052, not MD042"
    );

    // Test empty text with implicit reference
    let content = "[text][]\n\n[text]: https://example.com";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty()); // Valid implicit reference

    // Test both text and URL empty
    let content = "[][]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1); // Empty URL (no matching reference)
}

#[test]
fn test_md042_mkdocs_backtick_wrapped_auto_references() {
    // Test for issue #97 - backtick-wrapped references should be recognized as MkDocs auto-references
    let rule = MD042NoEmptyLinks::new();

    // Module.Class pattern with backticks - should not flag
    let content = "[`module.Class`][]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag [`module.Class`][] as empty in MkDocs mode (issue #97). Got: {result:?}"
    );

    // Single-word backtick-wrapped identifiers should also work (the actual issue #97 example)
    let content = "[`str`][]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag [`str`][] as empty in MkDocs mode (issue #97 example). Got: {result:?}"
    );

    // Multiple single-word backtick-wrapped identifiers
    let content = "See [`str`][], [`int`][], and [`bool`][] for details.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag single-word backtick-wrapped identifiers in MkDocs mode (issue #97). Got: {result:?}"
    );

    // Plain single words without backticks - undefined reference handled by MD052
    let content = "[str][]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Undefined references are handled by MD052, not MD042. Got: {result:?}"
    );

    // Undefined references in standard mode are handled by MD052, not MD042
    let content = "[`str`][]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Undefined references are handled by MD052, not MD042. Got: {result:?}"
    );

    // Test with text in reference ID position
    let content = "[`module.func`][`module.func`]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::MkDocs, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag explicit backtick-wrapped reference IDs in MkDocs mode. Got: {result:?}"
    );
}

#[test]
fn test_url_in_text_with_empty_destination() {
    // Issue #104: When link text is a URL and destination is empty, use text as destination
    let rule = MD042NoEmptyLinks::new();
    let content = "[https://github.com/user/repo]()";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1, "Should flag empty URL");

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed, "[https://github.com/user/repo](https://github.com/user/repo)",
        "Should use URL text as destination instead of example.com"
    );
}

#[test]
fn test_url_variants_in_text_with_empty_destination() {
    let rule = MD042NoEmptyLinks::new();

    // Test various URL protocols
    let test_cases = vec![
        ("[https://example.com]()", "[https://example.com](https://example.com)"),
        ("[http://example.com]()", "[http://example.com](http://example.com)"),
        ("[ftp://example.com]()", "[ftp://example.com](ftp://example.com)"),
        ("[ftps://example.com]()", "[ftps://example.com](ftps://example.com)"),
    ];

    for (input, expected) in test_cases {
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag empty URL in: {input}");
        assert!(result[0].fix.is_some(), "URL text should be fixable: {input}");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, expected, "Failed for input: {input}");
    }

    // Test non-URL text - should NOT be fixable
    let non_url_cases = vec!["[Click here]()", "[Some text]()", "[Learn more]()"];

    for input in non_url_cases {
        let ctx = LintContext::new(input, rumdl_lib::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should flag empty URL in: {input}");
        assert!(result[0].fix.is_none(), "Non-URL text should NOT be fixable: {input}");

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, input, "Should not modify non-URL text: {input}");
    }
}

#[test]
fn test_issue_104_regression() {
    // Full regression test for issue #104
    let rule = MD042NoEmptyLinks::new();
    let content = "check it out in its new repository at [https://github.com/pfeif/hx-complete-generator]().";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);

    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);

    let fixed = rule.fix(&ctx).unwrap();
    assert_eq!(
        fixed,
        "check it out in its new repository at [https://github.com/pfeif/hx-complete-generator](https://github.com/pfeif/hx-complete-generator).",
        "Should use the URL from text as the destination"
    );
}
#[test]
fn test_autolinks_not_flagged() {
    let rule = MD042NoEmptyLinks::new();
    let content = "Visit <https://example.com> and <https://github.com/user/repo>.
Also email me at <user@example.com>.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 0, "Autolinks should not be flagged as empty links");
}

#[test]
fn test_brackets_in_html_href_not_flagged() {
    // MD042 only flags empty URLs, not empty text
    let rule = MD042NoEmptyLinks::new();
    // Regression test for false positive: MD042 should not flag brackets in HTML href attributes
    // Found in free-programming-books repository (334k stars)
    let content = r#"Check this out:
<a href="https://example.com?p[images][0]=test&title=Example">Share on Example</a>

This is a real markdown link that should be flagged (empty URL):
[empty text]()"#;
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should only flag the actual markdown empty link (empty URL), not the brackets in HTML href
    assert_eq!(
        result.len(),
        1,
        "Should only flag the real markdown empty link, not brackets in HTML href attributes"
    );

    // Verify it's the markdown link that was flagged, not the HTML
    assert!(
        result[0].line > 3,
        "Should flag the markdown link on line 5, not the HTML on line 2"
    );
}

#[test]
fn test_obsidian_block_references() {
    let rule = MD042NoEmptyLinks::new();

    // Test block reference in current file: [[#^block-id]]
    let content = "This paragraph has a block reference. ^my-block\n\nLink to it: [[#^my-block]]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag Obsidian block references in current file. Got: {result:?}"
    );

    // Test block reference in other file: [[Note#^block-id]]
    let content = "Reference block in another file: [[OtherNote#^block-id]]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag Obsidian block references in other files. Got: {result:?}"
    );

    // Test with nested path: [[folder/Note#^block-id]]
    let content = "Reference with path: [[docs/guide#^important-note]]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag block references with file paths. Got: {result:?}"
    );

    // Note: [[]] (empty wiki-link) is NOT parsed as a link by pulldown-cmark, so we skip this test case

    // Wiki-links with heading anchors are valid - they link to a heading in another page
    // [[Note#heading]] is valid wiki syntax and should NOT be flagged
    let content = "Regular anchor: [[Note#heading]]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Wiki-links with heading anchors [[Note#heading]] are valid and should not be flagged. Got: {result:?}"
    );
}

#[test]
fn test_wiki_style_links_not_flagged() {
    // Discussion #153: Wiki-style links [[Page Name]] should not be flagged as empty links
    let rule = MD042NoEmptyLinks::new();

    // Basic wiki link
    let content = "[[Example]]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag wiki-style links [[Example]]. Got: {result:?}"
    );

    // Wiki link with spaces in page name
    let content = "[[Page Name]]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag wiki-style links with spaces [[Page Name]]. Got: {result:?}"
    );

    // Wiki link with path
    let content = "[[Folder/Page]]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag wiki-style links with paths [[Folder/Page]]. Got: {result:?}"
    );

    // Wiki link with display text (Obsidian/Notion syntax)
    let content = "[[Page|Display Text]]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag wiki-style links with display text [[Page|Display Text]]. Got: {result:?}"
    );

    // Multiple wiki links in paragraph
    let content = "Check out [[Page One]] and [[Page Two]] for details.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag multiple wiki-style links. Got: {result:?}"
    );

    // Wiki link with block reference
    let content = "[[#^block-id]]";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "Should not flag wiki-style block references. Got: {result:?}"
    );
}
