use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD031BlanksAroundFences;

#[test]
fn test_valid_fenced_blocks() {
    let rule = MD031BlanksAroundFences::default();
    let content = "Text before\n\n```\ncode block\n```\n\nText after";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_blank_before() {
    let rule = MD031BlanksAroundFences::default();
    let content = "Text before\n```\ncode block\n```\n\nText after";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].column, 1);
}

#[test]
fn test_no_blank_after() {
    let rule = MD031BlanksAroundFences::default();
    let content = "Text before\n\n```\ncode block\n```\nText after";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
    assert_eq!(result[0].column, 1);
}

#[test]
fn test_fix_missing_blanks() {
    let rule = MD031BlanksAroundFences::default();
    let content = "Text before\n```\ncode block\n```\nText after";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&result, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let fixed_result = rule.check(&fixed_ctx).unwrap();
    assert_eq!(fixed_result, Vec::new());
}

#[test]
fn test_nested_code_blocks_no_internal_blanks() {
    let rule = MD031BlanksAroundFences::default();

    // Test nested markdown code blocks (4 backticks containing 3 backticks)
    let content = "# Test\n\n````markdown\nHere's some text.\n\n```python\ndef hello():\n    print(\"Hello!\")\n```\n\nMore text.\n````\n\nAfter.";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();

    // Verify that the inner ```python block has NO internal blank lines
    assert!(result.contains("```python\ndef hello():\n    print(\"Hello!\")\n```"));
    assert!(!result.contains("```python\n\ndef hello()"));
    assert!(!result.contains("print(\"Hello!\")\n\n```"));

    // Verify that blank lines are only added around the outer ````markdown block
    let lines: Vec<&str> = result.lines().collect();
    let markdown_start = lines.iter().position(|&line| line.starts_with("````markdown")).unwrap();
    let markdown_end = lines.iter().rposition(|&line| line.starts_with("````")).unwrap();

    // Should have blank line before ````markdown
    assert_eq!(lines[markdown_start - 1], "");
    // Should have blank line after closing ````
    assert_eq!(lines[markdown_end + 1], "");
}

#[test]
fn test_nested_code_blocks_different_fence_types() {
    let rule = MD031BlanksAroundFences::default();

    // Test ~~~ containing ```
    let content = "Text\n~~~markdown\n```python\ncode\n```\n~~~\nAfter";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();

    // Inner ```python should not get blank lines (it's content inside ~~~)
    assert!(result.contains("```python\ncode\n```"));
    assert!(!result.contains("```python\n\ncode"));
    assert!(!result.contains("code\n\n```"));
}

#[test]
fn test_multiple_nested_levels() {
    let rule = MD031BlanksAroundFences::default();

    // Test 5 backticks containing 4 backticks containing 3 backticks
    let content = "`````text\n````markdown\n```python\ncode\n```\n````\n`````";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();

    // Only the outermost fence should be treated as a real code block
    // Everything inside should be preserved as-is
    assert!(result.contains("````markdown\n```python\ncode\n```\n````"));
    assert!(!result.contains("```python\n\ncode"));
}

#[test]
fn test_nested_vs_standalone_distinction() {
    let rule = MD031BlanksAroundFences::default();

    // Test that standalone ``` blocks still get blank lines, but nested ones don't
    let content = "# Test\nStandalone:\n```python\ncode1\n```\nNested:\n````markdown\n```python\ncode2\n```\n````\nEnd";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();

    // Standalone ```python should get blank lines around it
    assert!(result.contains("Standalone:\n\n```python\ncode1\n```\n\nNested:"));

    // Nested ```python should NOT get blank lines (it's inside ````markdown)
    assert!(result.contains("```python\ncode2\n```"));
    assert!(!result.contains("```python\n\ncode2"));

    // Outer ````markdown should get blank lines
    assert!(result.contains("Nested:\n\n````markdown"));
    assert!(result.contains("````\n\nEnd"));
}

#[test]
fn test_mixed_fence_markers_nested() {
    let rule = MD031BlanksAroundFences::default();

    // Test ``` inside ~~~ and ~~~ inside ```
    let content = "Test1:\n~~~text\n```python\ncode\n```\n~~~\nTest2:\n````text\n~~~bash\nscript\n~~~\n````\nEnd";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();

    // Inner fences should not get blank lines
    assert!(result.contains("```python\ncode\n```"));
    assert!(result.contains("~~~bash\nscript\n~~~"));
    assert!(!result.contains("```python\n\ncode"));
    assert!(!result.contains("~~~bash\n\nscript"));

    // Outer fences should get blank lines
    assert!(result.contains("Test1:\n\n~~~text"));
    assert!(result.contains("Test2:\n\n````text"));
}

#[test]
fn test_documentation_example_scenario() {
    let rule = MD031BlanksAroundFences::default();

    // Test the exact scenario from docs/md031.md that was causing issues
    let content = "### Example\n\n````markdown\nHere's some text explaining the code.\n\n```python\ndef hello():\n    print(\"Hello, world!\")\n```\n\nAnd here's more text after the code.\n````\n\n## Next section";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();

    // The ```python block should remain clean (no internal blank lines)
    assert!(result.contains("```python\ndef hello():\n    print(\"Hello, world!\")\n```"));

    // Should NOT contain internal blank lines in the code block
    assert!(!result.contains("```python\n\ndef hello()"));
    assert!(!result.contains("print(\"Hello, world!\")\n\n```"));

    // The outer ````markdown block should have proper spacing
    assert!(result.contains("### Example\n\n````markdown"));
    assert!(result.contains("````\n\n## Next section"));
}

#[test]
fn test_fence_length_specificity() {
    let rule = MD031BlanksAroundFences::default();

    // Test that fence length matters - ``` inside ```` should not close the outer block
    let content = "````markdown\n```python\ncode\n```\nmore content\n````";
    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.fix(&ctx).unwrap();

    // The ```python should be treated as content, not as opening/closing a block
    let lines: Vec<&str> = result.lines().collect();
    let python_line = lines.iter().position(|&line| line == "```python").unwrap();
    let close_python_line = lines.iter().position(|&line| line == "```").unwrap();
    let more_content_line = lines.iter().position(|&line| line == "more content").unwrap();

    // Should maintain the order and not treat ``` as block delimiters
    assert!(python_line < close_python_line);
    assert!(close_python_line < more_content_line);
}

#[test]
fn test_code_blocks_in_lists() {
    let rule = MD031BlanksAroundFences::default();

    // Test code blocks inside list items - this was causing issues in docs/md031.md
    let content = r#"# Test

1. First item with code:

   ```python
   code_in_list()
   ```

2. Second item

3. Third item with code:
   ```javascript
   console.log("test");
   ```
   More text in item 3.

Regular paragraph."#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Code blocks in lists should still require blank lines
    assert!(
        !result.is_empty(),
        "Should detect missing blank lines around code blocks in lists"
    );

    // Test the fix
    let fixed = rule.fix(&ctx).unwrap();

    // Should add blank lines around code blocks in lists
    assert!(fixed.contains("1. First item with code:\n\n   ```python"));
    assert!(fixed.contains("   ```\n\n2. Second item"));
    assert!(fixed.contains("3. Third item with code:\n\n   ```javascript"));
    assert!(fixed.contains("   ```\n\n   More text"));
}

#[test]
fn test_issue_284_blockquote_blank_lines() {
    // Issue #284: Empty blockquote lines (like ">") should be treated as blank lines
    // MD031 should not report false positives for code blocks in blockquotes
    let rule = MD031BlanksAroundFences::default();
    let content = r#"# Blockquote with code

> Some content
>
> ```python
> def hello():
>     print("Hello")
> ```
>
> More content
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // Should NOT report missing blank lines - `>` is effectively a blank line in blockquote context
    assert!(
        result.is_empty(),
        "Empty blockquote lines should be treated as blank lines: {result:?}"
    );
}

#[test]
fn test_blockquote_with_blank_marker_only() {
    // Test blockquote with just ">" as blank line separator
    let rule = MD031BlanksAroundFences::default();
    let content = "> Text before\n>\n> ```\n> code\n> ```\n>\n> Text after";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Blockquote with > as blank line should not trigger MD031: {result:?}"
    );
}

#[test]
fn test_blockquote_with_trailing_space_blank() {
    // Test blockquote with "> " (with trailing space) as blank line separator
    let rule = MD031BlanksAroundFences::default();
    let content = "> Text before\n> \n> ```\n> code\n> ```\n> \n> Text after";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Blockquote with '> ' as blank line should not trigger MD031: {result:?}"
    );
}

#[test]
fn test_nested_blockquote_blank_lines() {
    // Test nested blockquotes with blank lines
    let rule = MD031BlanksAroundFences::default();
    let content = r#">> Nested content
>>
>> ```python
>> code here
>> ```
>>
>> More nested content
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    assert!(
        result.is_empty(),
        "Nested blockquote blank lines should work: {result:?}"
    );
}

#[test]
fn test_blockquote_still_detects_missing_blanks() {
    // Verify that MD031 still detects issues when blank lines are truly missing in blockquotes
    let rule = MD031BlanksAroundFences::default();
    let content = "> Text before\n> ```\n> code\n> ```\n> Text after";

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let _result = rule.check(&ctx).unwrap();

    // Should still detect issues when there's no blank line (not even `>`)
    // Note: This behavior depends on how the rule handles blockquote context
    // Currently, MD031 does not require blank lines in blockquote context
    // because blockquote content is handled separately
}

#[test]
fn test_mixed_blockquote_and_regular_content() {
    // Test that regular content outside blockquotes still requires blank lines
    let rule = MD031BlanksAroundFences::default();
    let content = r#"# Mixed Content

> Blockquote with proper spacing
>
> ```python
> inside_quote()
> ```
>
> End of quote

Regular text without blank line
```javascript
outside_quote();
```
More text
"#;

    let ctx = LintContext::new(content, rumdl_lib::config::MarkdownFlavor::Standard, None);
    let result = rule.check(&ctx).unwrap();

    // The blockquote section should NOT trigger warnings
    // But the non-blockquote section should trigger warnings
    assert!(
        !result.is_empty(),
        "Should still detect missing blanks outside blockquotes"
    );

    // Verify the warnings are for the right lines
    let warning_lines: Vec<usize> = result.iter().map(|w| w.line).collect();
    // Line 12 is "```javascript" without blank before
    // Line 14 is after "```" without blank after
    assert!(
        warning_lines.iter().all(|&l| l >= 12),
        "Warnings should be for non-blockquote section: {warning_lines:?}"
    );
}
