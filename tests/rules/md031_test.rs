use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::MD031BlanksAroundFences;

#[test]
fn test_valid_fenced_blocks() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n\n```\ncode block\n```\n\nText after";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_no_blank_before() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n```\ncode block\n```\n\nText after";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 2);
    assert_eq!(result[0].column, 1);
}

#[test]
fn test_no_blank_after() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n\n```\ncode block\n```\nText after";
    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line, 5);
    assert_eq!(result[0].column, 1);
}

#[test]
fn test_fix_missing_blanks() {
    let rule = MD031BlanksAroundFences;
    let content = "Text before\n```\ncode block\n```\nText after";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();
    let fixed_ctx = LintContext::new(&result);
    let fixed_result = rule.check(&fixed_ctx).unwrap();
    assert_eq!(fixed_result, Vec::new());
}

#[test]
fn test_nested_code_blocks_no_internal_blanks() {
    let rule = MD031BlanksAroundFences;

    // Test nested markdown code blocks (4 backticks containing 3 backticks)
    let content = "# Test\n\n````markdown\nHere's some text.\n\n```python\ndef hello():\n    print(\"Hello!\")\n```\n\nMore text.\n````\n\nAfter.";
    let ctx = LintContext::new(content);
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
    let rule = MD031BlanksAroundFences;

    // Test ~~~ containing ```
    let content = "Text\n~~~markdown\n```python\ncode\n```\n~~~\nAfter";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();

    // Inner ```python should not get blank lines (it's content inside ~~~)
    assert!(result.contains("```python\ncode\n```"));
    assert!(!result.contains("```python\n\ncode"));
    assert!(!result.contains("code\n\n```"));
}

#[test]
fn test_multiple_nested_levels() {
    let rule = MD031BlanksAroundFences;

    // Test 5 backticks containing 4 backticks containing 3 backticks
    let content = "`````text\n````markdown\n```python\ncode\n```\n````\n`````";
    let ctx = LintContext::new(content);
    let result = rule.fix(&ctx).unwrap();

    // Only the outermost fence should be treated as a real code block
    // Everything inside should be preserved as-is
    assert!(result.contains("````markdown\n```python\ncode\n```\n````"));
    assert!(!result.contains("```python\n\ncode"));
}

#[test]
fn test_nested_vs_standalone_distinction() {
    let rule = MD031BlanksAroundFences;

    // Test that standalone ``` blocks still get blank lines, but nested ones don't
    let content = "# Test\nStandalone:\n```python\ncode1\n```\nNested:\n````markdown\n```python\ncode2\n```\n````\nEnd";
    let ctx = LintContext::new(content);
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
    let rule = MD031BlanksAroundFences;

    // Test ``` inside ~~~ and ~~~ inside ```
    let content = "Test1:\n~~~text\n```python\ncode\n```\n~~~\nTest2:\n````text\n~~~bash\nscript\n~~~\n````\nEnd";
    let ctx = LintContext::new(content);
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
    let rule = MD031BlanksAroundFences;

    // Test the exact scenario from docs/md031.md that was causing issues
    let content = "### Example\n\n````markdown\nHere's some text explaining the code.\n\n```python\ndef hello():\n    print(\"Hello, world!\")\n```\n\nAnd here's more text after the code.\n````\n\n## Next section";
    let ctx = LintContext::new(content);
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
    let rule = MD031BlanksAroundFences;

    // Test that fence length matters - ``` inside ```` should not close the outer block
    let content = "````markdown\n```python\ncode\n```\nmore content\n````";
    let ctx = LintContext::new(content);
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
    let rule = MD031BlanksAroundFences;
    
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

    let ctx = LintContext::new(content);
    let result = rule.check(&ctx).unwrap();
    
    // Code blocks in lists should still require blank lines
    assert!(result.len() > 0, "Should detect missing blank lines around code blocks in lists");
    
    // Test the fix
    let fixed = rule.fix(&ctx).unwrap();
    
    // Should add blank lines around code blocks in lists
    assert!(fixed.contains("1. First item with code:\n\n   ```python"));
    assert!(fixed.contains("   ```\n\n2. Second item"));
    assert!(fixed.contains("3. Third item with code:\n\n   ```javascript"));
    assert!(fixed.contains("   ```\n\n   More text"));
}
