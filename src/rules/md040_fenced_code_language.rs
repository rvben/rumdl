use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::range_utils::calculate_line_range;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};

/// Rule MD040: Fenced code blocks should have a language
///
/// See [docs/md040.md](../../docs/md040.md) for full documentation, configuration, and examples.
struct FencedCodeBlock {
    /// 0-indexed line number where the code block starts
    line_idx: usize,
    /// The language/info string (empty if no language specified)
    language: String,
    /// The fence marker used (``` or ~~~)
    fence_marker: String,
}

#[derive(Debug, Default, Clone)]
pub struct MD040FencedCodeLanguage;

impl Rule for MD040FencedCodeLanguage {
    fn name(&self) -> &'static str {
        "MD040"
    }

    fn description(&self) -> &'static str {
        "Code blocks should have a language specified"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let mut warnings = Vec::new();

        // Use pulldown-cmark to detect fenced code blocks with language info
        let fenced_blocks = detect_fenced_code_blocks(content, &ctx.line_offsets);

        // Pre-compute disabled ranges for efficient lookup
        let disabled_ranges = compute_disabled_ranges(content, self.name());

        for block in fenced_blocks {
            // Skip if this line is in a disabled range
            if is_line_disabled(&disabled_ranges, block.line_idx) {
                continue;
            }

            // Get the actual line content for additional checks
            let line = content.lines().nth(block.line_idx).unwrap_or("");
            let trimmed = line.trim();
            let after_fence = trimmed.strip_prefix(&block.fence_marker).unwrap_or("").trim();

            // Check if it has MkDocs title attribute but no language
            let has_title_only =
                ctx.flavor == crate::config::MarkdownFlavor::MkDocs && after_fence.starts_with("title=");

            // Check for Quarto/RMarkdown code chunk syntax: {language} or {language, options}
            let has_quarto_syntax = ctx.flavor == crate::config::MarkdownFlavor::Quarto
                && after_fence.starts_with('{')
                && after_fence.contains('}');

            // Warn if no language and not using special syntax
            if (block.language.is_empty() || has_title_only) && !has_quarto_syntax {
                let (start_line, start_col, end_line, end_col) = calculate_line_range(block.line_idx + 1, line);

                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: "Code block (```) missing language".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: {
                            let trimmed_start = line.len() - line.trim_start().len();
                            let fence_len = block.fence_marker.len();
                            let line_start_byte = ctx.line_offsets.get(block.line_idx).copied().unwrap_or(0);
                            let fence_start_byte = line_start_byte + trimmed_start;
                            let fence_end_byte = fence_start_byte + fence_len;
                            fence_start_byte..fence_end_byte
                        },
                        replacement: format!("{}text", block.fence_marker),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Use pulldown-cmark to detect fenced code blocks
        let fenced_blocks = detect_fenced_code_blocks(content, &ctx.line_offsets);

        // Pre-compute disabled ranges
        let disabled_ranges = compute_disabled_ranges(content, self.name());

        // Build a set of line indices that need fixing
        let mut lines_to_fix: std::collections::HashMap<usize, (&str, bool)> = std::collections::HashMap::new();

        for block in &fenced_blocks {
            if is_line_disabled(&disabled_ranges, block.line_idx) {
                continue;
            }

            let line = content.lines().nth(block.line_idx).unwrap_or("");
            let trimmed = line.trim();
            let after_fence = trimmed.strip_prefix(&block.fence_marker).unwrap_or("").trim();

            let has_title_only =
                ctx.flavor == crate::config::MarkdownFlavor::MkDocs && after_fence.starts_with("title=");

            let has_quarto_syntax = ctx.flavor == crate::config::MarkdownFlavor::Quarto
                && after_fence.starts_with('{')
                && after_fence.contains('}');

            if (block.language.is_empty() || has_title_only) && !has_quarto_syntax {
                lines_to_fix.insert(block.line_idx, (&block.fence_marker, has_title_only));
            }
        }

        // Build the result by iterating through lines
        let mut result = String::new();
        for (i, line) in content.lines().enumerate() {
            if let Some(&(fence_marker, has_title_only)) = lines_to_fix.get(&i) {
                let indent = &line[..line.len() - line.trim_start().len()];
                let trimmed = line.trim();
                let after_fence = trimmed.strip_prefix(fence_marker).unwrap_or("").trim();

                if has_title_only {
                    result.push_str(&format!("{indent}{fence_marker}text {after_fence}\n"));
                } else {
                    result.push_str(&format!("{indent}{fence_marker}text\n"));
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        // Remove trailing newline if the original content didn't have one
        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || (!ctx.likely_has_code() && !ctx.has_char('~'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(_config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        Box::new(MD040FencedCodeLanguage)
    }
}

/// Detect fenced code blocks using pulldown-cmark, returning info about each block's opening fence
fn detect_fenced_code_blocks(content: &str, line_offsets: &[usize]) -> Vec<FencedCodeBlock> {
    let mut blocks = Vec::new();
    let options = Options::all();
    let parser = Parser::new_ext(content, options).into_offset_iter();

    for (event, range) in parser {
        if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event {
            // Find the line index for this byte offset
            let line_idx = line_idx_from_offset(line_offsets, range.start);

            // Determine fence marker from the actual line content
            let line_start = line_offsets.get(line_idx).copied().unwrap_or(0);
            let line_end = line_offsets.get(line_idx + 1).copied().unwrap_or(content.len());
            let line = content.get(line_start..line_end).unwrap_or("");
            let trimmed = line.trim();
            let fence_marker = if trimmed.starts_with('`') {
                let count = trimmed.chars().take_while(|&c| c == '`').count();
                "`".repeat(count)
            } else if trimmed.starts_with('~') {
                let count = trimmed.chars().take_while(|&c| c == '~').count();
                "~".repeat(count)
            } else {
                "```".to_string() // Fallback
            };

            // Extract just the language (first word of info string)
            let language = info.split_whitespace().next().unwrap_or("").to_string();

            blocks.push(FencedCodeBlock {
                line_idx,
                language,
                fence_marker,
            });
        }
    }

    blocks
}

#[inline]
fn line_idx_from_offset(line_offsets: &[usize], offset: usize) -> usize {
    match line_offsets.binary_search(&offset) {
        Ok(idx) => idx,
        Err(idx) => idx.saturating_sub(1),
    }
}

/// Compute disabled line ranges from disable/enable comments
fn compute_disabled_ranges(content: &str, rule_name: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut disabled_start: Option<usize> = None;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if let Some(rules) = crate::rule::parse_disable_comment(trimmed)
            && (rules.is_empty() || rules.contains(&rule_name))
            && disabled_start.is_none()
        {
            disabled_start = Some(i);
        }

        if let Some(rules) = crate::rule::parse_enable_comment(trimmed)
            && (rules.is_empty() || rules.contains(&rule_name))
            && let Some(start) = disabled_start.take()
        {
            ranges.push((start, i));
        }
    }

    // Handle unclosed disable
    if let Some(start) = disabled_start {
        ranges.push((start, usize::MAX));
    }

    ranges
}

/// Check if a line index is within a disabled range
fn is_line_disabled(ranges: &[(usize, usize)], line_idx: usize) -> bool {
    ranges.iter().any(|&(start, end)| line_idx >= start && line_idx < end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    fn run_check(content: &str) -> LintResult {
        let rule = MD040FencedCodeLanguage;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.check(&ctx)
    }

    fn run_fix(content: &str) -> Result<String, LintError> {
        let rule = MD040FencedCodeLanguage;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.fix(&ctx)
    }

    #[test]
    fn test_code_blocks_with_language_specified() {
        // Basic test with language
        let content = r#"# Test

```python
print("Hello, world!")
```

```javascript
console.log("Hello!");
```
"#;
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "No warnings expected for code blocks with language");
    }

    #[test]
    fn test_code_blocks_without_language() {
        let content = r#"# Test

```
print("Hello, world!")
```
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Code block (```) missing language");
        assert_eq!(result[0].line, 3);
    }

    #[test]
    fn test_code_blocks_with_empty_language() {
        // Test with spaces after the fence
        let content = r#"# Test

```
print("Hello, world!")
```
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Code block (```) missing language");
    }

    #[test]
    fn test_indented_code_blocks_should_be_ignored() {
        // Indented code blocks (4 spaces) should not trigger the rule
        let content = r#"# Test

    This is an indented code block
    It should not trigger MD040
"#;
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Indented code blocks should be ignored");
    }

    #[test]
    fn test_inline_code_spans_should_be_ignored() {
        let content = r#"# Test

This is `inline code` and should not trigger warnings.

Use the `print()` function.
"#;
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Inline code spans should be ignored");
    }

    #[test]
    fn test_tildes_vs_backticks_for_fences() {
        // Test tilde fences without language
        let content_tildes_no_lang = r#"# Test

~~~
code here
~~~
"#;
        let result = run_check(content_tildes_no_lang).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].message, "Code block (```) missing language");

        // Test tilde fences with language
        let content_tildes_with_lang = r#"# Test

~~~python
code here
~~~
"#;
        let result = run_check(content_tildes_with_lang).unwrap();
        assert!(result.is_empty());

        // Mixed fences
        let content_mixed = r#"# Test

```python
code here
```

~~~javascript
more code
~~~

```
no language
```

~~~
also no language
~~~
"#;
        let result = run_check(content_mixed).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_language_with_additional_parameters() {
        let content = r#"# Test

```python {highlight=[1,2]}
print("Line 1")
print("Line 2")
```

```javascript {.line-numbers startFrom="10"}
console.log("Hello");
```

```ruby {data-line="1,3-4"}
puts "Hello"
puts "World"
puts "!"
```
"#;
        let result = run_check(content).unwrap();
        assert!(
            result.is_empty(),
            "Code blocks with language and parameters should pass"
        );
    }

    #[test]
    fn test_multiple_code_blocks_in_document() {
        let content = r#"# Test Document

First block without language:
```
code here
```

Second block with language:
```python
print("hello")
```

Third block without language:
```
more code
```

Fourth block with language:
```javascript
console.log("test");
```
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 4);
        assert_eq!(result[1].line, 14);
    }

    #[test]
    fn test_nested_code_blocks_in_lists() {
        let content = r#"# Test

- Item 1
  ```python
  print("nested with language")
  ```

- Item 2
  ```
  nested without language
  ```

- Item 3
  - Nested item
    ```javascript
    console.log("deeply nested");
    ```

  - Another nested
    ```
    no language
    ```
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 2);
        // Check that it detects the blocks without language
        assert_eq!(result[0].line, 9);
        assert_eq!(result[1].line, 20);
    }

    #[test]
    fn test_issue_257_list_indented_code_block_with_language() {
        // Issue #257: MD040 incorrectly flagged closing fence as needing language
        // when code block was inside a list item
        let content = r#"- Sample code:
- ```java
      List<Map<String,String>> inputs = new List<Map<String,String>>();
  ```
"#;
        // Should produce NO warnings - the code block has a language
        let result = run_check(content).unwrap();
        assert!(
            result.is_empty(),
            "List-indented code block with language should not trigger MD040. Got: {result:?}",
        );

        // Fix should NOT modify the content at all
        let fixed = run_fix(content).unwrap();
        assert_eq!(
            fixed, content,
            "Fix should not modify code blocks that already have a language"
        );
        // Specifically verify no `text` was added to closing fence
        assert!(
            !fixed.contains("```text"),
            "Fix should not add 'text' to closing fence of code block with language"
        );
    }

    #[test]
    fn test_issue_257_multiple_list_indented_blocks() {
        // Extended test for issue #257 with multiple scenarios
        let content = r#"# Document

1. Step one
   ```python
   print("hello")
   ```
2. Step two

- Item with nested code:
  ```bash
  echo "test"
  ```

- Another item:
  ```javascript
  console.log("test");
  ```
"#;
        // All blocks have languages, so no warnings
        let result = run_check(content).unwrap();
        assert!(
            result.is_empty(),
            "All list-indented code blocks have languages. Got: {result:?}",
        );

        // Fix should not modify anything
        let fixed = run_fix(content).unwrap();
        assert_eq!(
            fixed, content,
            "Fix should not modify content when all blocks have languages"
        );
    }

    #[test]
    fn test_code_blocks_in_blockquotes() {
        let content = r#"# Test

> This is a blockquote
> ```python
> print("with language")
> ```

> Another blockquote
> ```
> without language
> ```
"#;
        let result = run_check(content).unwrap();
        // Code blocks inside blockquotes ARE detected (pulldown-cmark handles nested structures)
        // The second code block has no language, so 1 warning expected
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_fix_method_adds_text_language() {
        let content = r#"# Test

```
code without language
```

```python
already has language
```

```
another block without
```
"#;
        let fixed = run_fix(content).unwrap();
        assert!(fixed.contains("```text"));
        assert!(fixed.contains("```python"));
        assert_eq!(fixed.matches("```text").count(), 2);
    }

    #[test]
    fn test_fix_preserves_indentation() {
        let content = r#"# Test

- List item
  ```
  indented code block
  ```
"#;
        let fixed = run_fix(content).unwrap();
        // Should preserve indentation for list items
        assert!(fixed.contains("  ```text"));
        assert!(fixed.contains("  indented code block"));
    }

    #[test]
    fn test_fix_preserves_indentation_numbered_list() {
        // Test case from issue #122
        let content = r#"1. Step 1

    ```
    foo
    bar
    ```
"#;
        let fixed = run_fix(content).unwrap();
        // Should preserve 4-space indentation for numbered list content
        assert!(fixed.contains("    ```text"));
        assert!(fixed.contains("    foo"));
        assert!(fixed.contains("    bar"));
        // Should not remove indentation
        assert!(!fixed.contains("\n```text\n"));
    }

    #[test]
    fn test_fix_preserves_all_indentation() {
        let content = r#"# Test

Top-level code block:
```
top level
```

1. List item

    ```
    nested in list
    ```

Indented by 2 spaces:
  ```
  content
  ```
"#;
        let fixed = run_fix(content).unwrap();

        // All indentation should be preserved exactly as-is
        assert!(
            fixed.contains("```text\ntop level"),
            "Top-level code block indentation preserved"
        );
        assert!(
            fixed.contains("    ```text\n    nested in list"),
            "List item code block indentation preserved"
        );
        assert!(
            fixed.contains("  ```text\n  content"),
            "2-space indented code block indentation preserved"
        );
    }

    #[test]
    fn test_fix_with_tilde_fences() {
        let content = r#"# Test

~~~
code with tildes
~~~
"#;
        let fixed = run_fix(content).unwrap();
        assert!(fixed.contains("~~~text"));
    }

    #[test]
    fn test_longer_fence_markers() {
        let content = r#"# Test

````
code with four backticks
````

`````python
code with five backticks and language
`````

~~~~~~
code with six tildes
~~~~~~
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 2);

        let fixed = run_fix(content).unwrap();
        assert!(fixed.contains("````text"));
        assert!(fixed.contains("~~~~~~text"));
        assert!(fixed.contains("`````python"));
    }

    #[test]
    fn test_nested_code_blocks_different_markers() {
        let content = r#"# Test

````markdown
This is a markdown block

```python
# This is nested code
print("hello")
```

More markdown
````
"#;
        let result = run_check(content).unwrap();
        assert!(
            result.is_empty(),
            "Nested code blocks with different markers should not trigger warnings"
        );
    }

    #[test]
    fn test_disable_enable_comments() {
        let content = r#"# Test

<!-- rumdl-disable MD040 -->
```
this should not trigger warning
```
<!-- rumdl-enable MD040 -->

```
this should trigger warning
```
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 9);
    }

    #[test]
    fn test_fence_with_language_only_on_closing() {
        // Edge case: language on closing fence should not be interpreted
        let content = r#"# Test

```
code
```python
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_incomplete_code_blocks() {
        // Test unclosed code block
        let content = r#"# Test

```python
this code block is not closed"#;
        let result = run_check(content).unwrap();
        assert!(
            result.is_empty(),
            "Unclosed code blocks with language should not trigger warnings"
        );

        // Test unclosed code block without language
        let content_no_lang = r#"# Test

```
this code block is not closed"#;
        let result = run_check(content_no_lang).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_fix_preserves_original_formatting() {
        let content = r#"# Test

```
code
```

No newline at end"#;
        let fixed = run_fix(content).unwrap();
        assert!(!fixed.ends_with('\n'), "Fix should preserve lack of trailing newline");

        let content_with_newline = "# Test\n\n```\ncode\n```\n";
        let fixed = run_fix(content_with_newline).unwrap();
        assert!(fixed.ends_with('\n'), "Fix should preserve trailing newline");
    }

    #[test]
    fn test_edge_case_backticks_in_content() {
        let content = r#"# Test

```javascript
console.log(`template string with backticks`);
// This line has ``` in a comment
```
"#;
        let result = run_check(content).unwrap();
        assert!(
            result.is_empty(),
            "Backticks inside code blocks should not affect parsing"
        );
    }

    #[test]
    fn test_empty_document() {
        let content = "";
        let result = run_check(content).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_should_skip_optimization() {
        let rule = MD040FencedCodeLanguage;

        // Document without code fences should skip
        let ctx = LintContext::new(
            "# Just a header\n\nSome text",
            crate::config::MarkdownFlavor::Standard,
            None,
        );
        assert!(rule.should_skip(&ctx));

        // Document with backtick fences should not skip
        let ctx = LintContext::new("```\ncode\n```", crate::config::MarkdownFlavor::Standard, None);
        assert!(!rule.should_skip(&ctx));

        // Document with tilde fences should not skip
        let ctx = LintContext::new("~~~\ncode\n~~~", crate::config::MarkdownFlavor::Standard, None);
        assert!(!rule.should_skip(&ctx));

        // Empty document should skip
        let ctx = LintContext::new("", crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.should_skip(&ctx));
    }

    #[test]
    fn test_quarto_code_chunk_syntax() {
        let rule = MD040FencedCodeLanguage;

        // Test Quarto {r} syntax - should NOT trigger warning
        let content = r#"# Test

```{r}
x <- 1
```

```{python}
x = 1
```

```{r, echo=FALSE}
plot(x)
```
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Quarto code chunks with {{language}} syntax should not trigger warnings"
        );

        // Test that missing language DOES trigger warning for Quarto
        let content_no_lang = r#"# Test

```
code without language
```
"#;
        let ctx = LintContext::new(content_no_lang, crate::config::MarkdownFlavor::Quarto, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Quarto files without language should trigger warning");

        // Test that standard flavor still requires standard language syntax
        let content_standard = r#"# Test

```{python}
code
```
"#;
        let ctx = LintContext::new(content_standard, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // In standard flavor, {python} is considered "after_fence" content, so it's valid
        // The fence marker is "```" and after_fence is "{python}", which is non-empty
        assert!(
            result.is_empty(),
            "Standard flavor should accept any non-empty after_fence content"
        );
    }
}
