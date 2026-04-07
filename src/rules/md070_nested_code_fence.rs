use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

/// Rule MD070: Nested code fence collision detection
///
/// Detects when a fenced code block contains fence markers that would cause
/// premature closure. Suggests using longer fences to avoid this issue.
///
/// Checks languages where triple backtick sequences commonly appear:
/// markdown, Python, JavaScript, shell, Rust, Go, and others with multiline
/// strings, heredocs, template literals, or doc comments.
///
/// See [docs/md070.md](../../docs/md070.md) for full documentation.
#[derive(Clone, Default)]
pub struct MD070NestedCodeFence;

impl MD070NestedCodeFence {
    pub fn new() -> Self {
        Self
    }

    /// Check if the given language should be checked for nested fences.
    /// Covers languages where triple backtick sequences commonly appear in source:
    /// multiline strings with embedded markdown, heredocs, doc comments, template
    /// literals, and data formats with multiline string values.
    fn should_check_language(lang: &str) -> bool {
        let base = lang.split_whitespace().next().unwrap_or("");
        matches!(
            base.to_ascii_lowercase().as_str(),
            // Documentation / markup
            ""
                | "markdown"
                | "md"
                | "mdx"
                | "text"
                | "txt"
                | "plain"
                // Multiline strings / docstrings
                | "python"
                | "py"
                | "ruby"
                | "rb"
                | "perl"
                | "pl"
                | "php"
                | "lua"
                | "r"
                | "rmd"
                | "rmarkdown"
                // Template literals / raw strings
                | "javascript"
                | "js"
                | "jsx"
                | "mjs"
                | "cjs"
                | "typescript"
                | "ts"
                | "tsx"
                | "mts"
                | "rust"
                | "rs"
                | "go"
                | "golang"
                | "swift"
                | "kotlin"
                | "kt"
                | "kts"
                | "java"
                | "csharp"
                | "cs"
                | "c#"
                | "scala"
                // Shell heredocs
                | "shell"
                | "sh"
                | "bash"
                | "zsh"
                | "fish"
                | "powershell"
                | "ps1"
                | "pwsh"
                // Data / config formats
                | "yaml"
                | "yml"
                | "toml"
                | "json"
                | "jsonc"
                | "json5"
                // Template engines
                | "jinja"
                | "jinja2"
                | "handlebars"
                | "hbs"
                | "liquid"
                | "nunjucks"
                | "njk"
                | "ejs"
                // Terminal output
                | "console"
                | "terminal"
        )
    }

    /// Find the maximum fence length of same-character fences in the content
    /// Returns (line_offset, fence_length) of the first collision, if any
    fn find_fence_collision(content: &str, fence_char: char, outer_fence_length: usize) -> Option<(usize, usize)> {
        for (line_idx, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();

            // Check if line starts with the same fence character
            if trimmed.starts_with(fence_char) {
                let count = trimmed.chars().take_while(|&c| c == fence_char).count();

                // Collision if same char AND at least as long as outer fence
                if count >= outer_fence_length {
                    // Verify it looks like a fence line (only fence chars + optional language/whitespace)
                    let after_fence = &trimmed[count..];
                    // A fence line is: fence chars + optional language identifier + optional whitespace
                    // We detect collision if:
                    // - Line ends after fence chars (closing fence)
                    // - Line has alphanumeric after fence (opening fence with language)
                    // - Line has only whitespace after fence
                    if after_fence.is_empty()
                        || after_fence.trim().is_empty()
                        || after_fence
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_alphabetic() || c == '{')
                    {
                        return Some((line_idx, count));
                    }
                }
            }
        }
        None
    }

    /// Find the maximum fence length needed to safely contain the content
    fn find_safe_fence_length(content: &str, fence_char: char) -> usize {
        let mut max_fence = 0;

        for line in content.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with(fence_char) {
                let count = trimmed.chars().take_while(|&c| c == fence_char).count();
                if count >= 3 {
                    // Only count valid fence-like patterns
                    let after_fence = &trimmed[count..];
                    if after_fence.is_empty()
                        || after_fence.trim().is_empty()
                        || after_fence
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_alphabetic() || c == '{')
                    {
                        max_fence = max_fence.max(count);
                    }
                }
            }
        }

        max_fence
    }

    /// Find the user's intended closing fence when a collision is detected.
    /// Searches past the first (premature) closing fence for the last bare
    /// fence of the same type before hitting a new opening fence.
    fn find_intended_close(
        lines: &[&str],
        first_close: usize,
        fence_char: char,
        fence_length: usize,
        opening_indent: usize,
    ) -> usize {
        let mut intended_close = first_close;
        for (j, line_j) in lines.iter().enumerate().skip(first_close + 1) {
            if Self::is_closing_fence(line_j, fence_char, fence_length) {
                intended_close = j;
            } else if Self::parse_fence_line(line_j)
                .is_some_and(|(ind, ch, _, info)| ind <= opening_indent && ch == fence_char && !info.is_empty())
            {
                break;
            }
        }
        intended_close
    }

    /// Parse a fence marker from a line, returning (indent, fence_char, fence_length, info_string)
    fn parse_fence_line(line: &str) -> Option<(usize, char, usize, &str)> {
        let indent = line.len() - line.trim_start().len();
        // Per CommonMark, fence must have 0-3 spaces of indentation
        if indent > 3 {
            return None;
        }

        let trimmed = line.trim_start();

        if trimmed.starts_with("```") {
            let count = trimmed.chars().take_while(|&c| c == '`').count();
            if count >= 3 {
                let info = trimmed[count..].trim();
                return Some((indent, '`', count, info));
            }
        } else if trimmed.starts_with("~~~") {
            let count = trimmed.chars().take_while(|&c| c == '~').count();
            if count >= 3 {
                let info = trimmed[count..].trim();
                return Some((indent, '~', count, info));
            }
        }

        None
    }

    /// Check if a line is a valid closing fence for the given opening fence
    /// Per CommonMark, closing fences can have 0-3 spaces of indentation regardless of opening fence
    fn is_closing_fence(line: &str, fence_char: char, min_length: usize) -> bool {
        let indent = line.len() - line.trim_start().len();
        // Per CommonMark spec, closing fence can have 0-3 spaces of indentation
        if indent > 3 {
            return false;
        }

        let trimmed = line.trim_start();
        if !trimmed.starts_with(fence_char) {
            return false;
        }

        let count = trimmed.chars().take_while(|&c| c == fence_char).count();
        if count < min_length {
            return false;
        }

        // Closing fence must have only whitespace after fence chars
        trimmed[count..].trim().is_empty()
    }
}

impl Rule for MD070NestedCodeFence {
    fn name(&self) -> &'static str {
        "MD070"
    }

    fn description(&self) -> &'static str {
        "Nested code fence collision - use longer fence to avoid premature closure"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let lines = ctx.raw_lines();

        let mut i = 0;
        while i < lines.len() {
            // Skip lines in contexts that shouldn't be processed
            if let Some(line_info) = ctx.lines.get(i)
                && (line_info.in_front_matter
                    || line_info.in_html_comment
                    || line_info.in_mdx_comment
                    || line_info.in_html_block)
            {
                i += 1;
                continue;
            }

            // Skip if we're already inside a code block (check previous line).
            // This handles list-indented code blocks (4+ spaces) which our rule doesn't
            // parse directly, but the context detects correctly. If the previous line
            // is in a code block, this line is either content or a closing fence for
            // that block - not a new opening fence.
            if i > 0
                && let Some(prev_line_info) = ctx.lines.get(i - 1)
                && prev_line_info.in_code_block
            {
                i += 1;
                continue;
            }

            let line = lines[i];

            // Try to parse as opening fence
            if let Some((_indent, fence_char, fence_length, info_string)) = Self::parse_fence_line(line) {
                let block_start = i;

                // Extract the language (first word of info string)
                let language = info_string.split_whitespace().next().unwrap_or("");

                // Find the closing fence
                let mut block_end = None;
                for (j, line_j) in lines.iter().enumerate().skip(i + 1) {
                    if Self::is_closing_fence(line_j, fence_char, fence_length) {
                        block_end = Some(j);
                        break;
                    }
                }

                if let Some(end_line) = block_end {
                    // We have a complete code block from block_start to end_line
                    // Check if we should analyze this block
                    if Self::should_check_language(language) {
                        // Get the content between fences
                        let block_content: String = if block_start + 1 < end_line {
                            lines[(block_start + 1)..end_line].join("\n")
                        } else {
                            String::new()
                        };

                        // Check for fence collision
                        if let Some((collision_line_offset, _collision_length)) =
                            Self::find_fence_collision(&block_content, fence_char, fence_length)
                        {
                            let collision_line_num = block_start + 1 + collision_line_offset + 1; // 1-indexed

                            // Find the user's intended closing fence (may be past the
                            // CommonMark-visible close when inner ``` causes premature closure)
                            let indent = line.len() - line.trim_start().len();
                            let intended_close =
                                Self::find_intended_close(lines, end_line, fence_char, fence_length, indent);

                            // Compute safe fence length from the full intended content
                            let full_content: String = if block_start + 1 < intended_close {
                                lines[(block_start + 1)..intended_close].join("\n")
                            } else {
                                block_content.clone()
                            };
                            let safe_length = Self::find_safe_fence_length(&full_content, fence_char) + 1;
                            let suggested_fence: String = std::iter::repeat_n(fence_char, safe_length).collect();

                            // Build a Fix that replaces the block from opening fence
                            // through the intended closing fence. This must be safe for
                            // direct application by the LSP code action path.
                            let open_byte_start = ctx.line_index.get_line_start_byte(block_start + 1).unwrap_or(0);
                            let close_byte_end = ctx
                                .line_index
                                .get_line_start_byte(intended_close + 2)
                                .unwrap_or(ctx.content.len());

                            let indent_str = &line[..indent];
                            let closing_line = lines[intended_close];
                            let closing_indent = &closing_line[..closing_line.len() - closing_line.trim_start().len()];
                            let mut replacement = format!("{indent_str}{suggested_fence}");
                            if !info_string.is_empty() {
                                replacement.push_str(info_string);
                            }
                            replacement.push('\n');
                            for content_line in &lines[(block_start + 1)..intended_close] {
                                replacement.push_str(content_line);
                                replacement.push('\n');
                            }
                            replacement.push_str(closing_indent);
                            replacement.push_str(&suggested_fence);
                            // Only add trailing newline if the replaced range ends with one
                            if close_byte_end <= ctx.content.len() && ctx.content[..close_byte_end].ends_with('\n') {
                                replacement.push('\n');
                            }

                            warnings.push(LintWarning {
                                rule_name: Some(self.name().to_string()),
                                message: format!(
                                    "Code block contains fence markers at line {collision_line_num} that interfere with block parsing — use {suggested_fence} for outer fence"
                                ),
                                line: block_start + 1,
                                column: 1,
                                end_line: intended_close + 1,
                                end_column: lines[intended_close].len() + 1,
                                severity: Severity::Warning,
                                fix: Some(Fix {
                                    range: (open_byte_start..close_byte_end),
                                    replacement,
                                }),
                            });
                        }
                    }

                    // Move past this code block
                    i = end_line + 1;
                    continue;
                }
            }

            i += 1;
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());
        crate::utils::fix_utils::apply_warning_fixes(ctx.content, &warnings).map_err(LintError::FixFailed)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

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
        Box::new(MD070NestedCodeFence::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    fn run_check(content: &str) -> LintResult {
        let rule = MD070NestedCodeFence::new();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.check(&ctx)
    }

    fn run_fix(content: &str) -> Result<String, LintError> {
        let rule = MD070NestedCodeFence::new();
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        rule.fix(&ctx)
    }

    #[test]
    fn test_no_collision_simple() {
        let content = "```python\nprint('hello')\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Simple code block should not trigger warning");
    }

    #[test]
    fn test_no_collision_unchecked_language() {
        // C is not checked for nested fences (triple backticks don't appear in C source)
        let content = "```c\n```bash\necho hello\n```\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Unchecked language should not trigger");
    }

    #[test]
    fn test_collision_python_language() {
        // Python is checked — triple-quoted strings commonly contain markdown
        let content = "```python\n```json\n{}\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Python should be checked for nested fences");
        assert!(result[0].message.contains("````"));
    }

    #[test]
    fn test_collision_javascript_language() {
        let content = "```javascript\n```html\n<div></div>\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "JavaScript should be checked for nested fences");
    }

    #[test]
    fn test_collision_shell_language() {
        let content = "```bash\n```yaml\nkey: val\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Shell should be checked for nested fences");
    }

    #[test]
    fn test_collision_rust_language() {
        let content = "```rust\n```toml\n[dep]\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Rust should be checked for nested fences");
    }

    #[test]
    fn test_no_collision_assembly_language() {
        // Assembly, C, SQL etc. should NOT be checked
        for lang in ["asm", "c", "cpp", "sql", "css", "fortran"] {
            let content = format!("```{lang}\n```inner\ncontent\n```\n```\n");
            let result = run_check(&content).unwrap();
            assert!(result.is_empty(), "{lang} should not be checked for nested fences");
        }
    }

    #[test]
    fn test_collision_markdown_language() {
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should emit single warning for collision");
        assert!(result[0].message.contains("fence markers at line"));
        assert!(result[0].message.contains("interfere with block parsing"));
        assert!(result[0].message.contains("use ````"));
    }

    #[test]
    fn test_collision_empty_language() {
        // Empty language (no language specified) is checked
        let content = "```\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Empty language should be checked");
    }

    #[test]
    fn test_no_collision_longer_outer_fence() {
        let content = "````markdown\n```python\ncode()\n```\n````\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Longer outer fence should not trigger warning");
    }

    #[test]
    fn test_tilde_fence_ignores_backticks() {
        // Tildes and backticks don't conflict
        let content = "~~~markdown\n```python\ncode()\n```\n~~~\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Different fence types should not collide");
    }

    #[test]
    fn test_tilde_collision() {
        let content = "~~~markdown\n~~~python\ncode()\n~~~\n~~~\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Same fence type should collide");
        assert!(result[0].message.contains("~~~~"));
    }

    #[test]
    fn test_fix_increases_fence_length() {
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let fixed = run_fix(content).unwrap();
        assert!(fixed.starts_with("````markdown"), "Should increase to 4 backticks");
        assert!(
            fixed.contains("````\n") || fixed.ends_with("````"),
            "Closing should also be 4 backticks"
        );
    }

    #[test]
    fn test_fix_handles_longer_inner_fence() {
        // Inner fence has 5 backticks, so outer needs 6
        let content = "```markdown\n`````python\ncode()\n`````\n```\n";
        let fixed = run_fix(content).unwrap();
        assert!(fixed.starts_with("``````markdown"), "Should increase to 6 backticks");
    }

    #[test]
    fn test_backticks_in_code_not_fence() {
        // Template literals in JS shouldn't trigger
        let content = "```markdown\nconst x = `template`;\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Inline backticks should not be detected as fences");
    }

    #[test]
    fn test_preserves_info_string() {
        let content = "```markdown {.highlight}\n```python\ncode()\n```\n```\n";
        let fixed = run_fix(content).unwrap();
        assert!(
            fixed.contains("````markdown {.highlight}"),
            "Should preserve info string attributes"
        );
    }

    #[test]
    fn test_md_language_alias() {
        let content = "```md\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "md should be recognized as markdown");
    }

    #[test]
    fn test_real_world_docs_case() {
        // This is the actual pattern from docs/md031.md that triggered the PR
        let content = r#"```markdown
1. First item

   ```python
   code_in_list()
   ```

1. Second item

```
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should emit single warning for nested fence issue");
        assert!(result[0].message.contains("line 4")); // The nested ``` is on line 4

        let fixed = run_fix(content).unwrap();
        assert!(fixed.starts_with("````markdown"), "Should fix with longer fence");
    }

    #[test]
    fn test_empty_code_block() {
        let content = "```markdown\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Empty code block should not trigger");
    }

    #[test]
    fn test_multiple_code_blocks() {
        // The markdown block has a collision (inner ```python closes it prematurely).
        // The orphan closing fence (line 9) is NOT treated as a new opening fence
        // because the context correctly detects it as part of the markdown block.
        let content = r#"```python
safe code
```

```markdown
```python
collision
```
```

```javascript
also safe
```
"#;
        let result = run_check(content).unwrap();
        // Only 1 warning for the markdown block collision.
        // The orphan fence is correctly ignored (not parsed as new opening fence).
        assert_eq!(result.len(), 1, "Should emit single warning for collision");
        assert!(result[0].message.contains("line 6")); // The nested ```python is on line 6
    }

    #[test]
    fn test_single_collision_properly_closed() {
        // When the outer fence is properly longer, only the intended block triggers
        let content = r#"```python
safe code
```

````markdown
```python
collision
```
````

```javascript
also safe
```
"#;
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Properly fenced blocks should not trigger");
    }

    #[test]
    fn test_indented_code_block_in_list() {
        let content = r#"- List item
  ```markdown
  ```python
  nested
  ```
  ```
"#;
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should detect collision in indented block");
        assert!(result[0].message.contains("````"));
    }

    #[test]
    fn test_no_false_positive_list_indented_block() {
        // 4-space indented code blocks in list context (GFM extension) should not
        // cause false positives. The closing fence with 3-space indent should not
        // be parsed as a new opening fence.
        let content = r#"1. List item with code:

    ```json
    {"key": "value"}
    ```

2. Another item

   ```python
   code()
   ```
"#;
        let result = run_check(content).unwrap();
        // No collision - these are separate, well-formed code blocks
        assert!(
            result.is_empty(),
            "List-indented code blocks should not trigger false positives"
        );
    }

    // ==================== Comprehensive Edge Case Tests ====================

    #[test]
    fn test_case_insensitive_language() {
        // MARKDOWN, Markdown, MD should all be checked
        for lang in ["MARKDOWN", "Markdown", "MD", "Md", "mD"] {
            let content = format!("```{lang}\n```python\ncode()\n```\n```\n");
            let result = run_check(&content).unwrap();
            assert_eq!(result.len(), 1, "{lang} should be recognized as markdown");
        }
    }

    #[test]
    fn test_unclosed_outer_fence() {
        // If outer fence is never closed, no collision can be detected
        let content = "```markdown\n```python\ncode()\n```\n";
        let result = run_check(content).unwrap();
        // The outer fence finds ```python as its closing fence (premature close)
        // Then ```\n at the end becomes orphan - but context would handle this
        assert!(result.len() <= 1, "Unclosed fence should not cause issues");
    }

    #[test]
    fn test_deeply_nested_fences() {
        // Multiple levels of nesting require progressively longer fences
        let content = r#"```markdown
````markdown
```python
code()
```
````
```
"#;
        let result = run_check(content).unwrap();
        // The outer ``` sees ```` as collision (4 >= 3)
        assert_eq!(result.len(), 1, "Deep nesting should trigger warning");
        assert!(result[0].message.contains("`````")); // Needs 5 to be safe
    }

    #[test]
    fn test_very_long_fences() {
        // 10 backtick fences should work correctly
        let content = "``````````markdown\n```python\ncode()\n```\n``````````\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Very long outer fence should not trigger warning");
    }

    #[test]
    fn test_blockquote_with_fence() {
        // Fences inside blockquotes (CommonMark allows this)
        let content = "> ```markdown\n> ```python\n> code()\n> ```\n> ```\n";
        let result = run_check(content).unwrap();
        // Blockquote prefixes are part of the line, so parsing may differ
        // This documents current behavior
        assert!(result.is_empty() || result.len() == 1);
    }

    #[test]
    fn test_fence_with_attributes() {
        // Info string with attributes like {.class #id}
        let content = "```markdown {.highlight #example}\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Attributes in info string should not prevent detection"
        );

        let fixed = run_fix(content).unwrap();
        assert!(
            fixed.contains("````markdown {.highlight #example}"),
            "Attributes should be preserved in fix"
        );
    }

    #[test]
    fn test_trailing_whitespace_in_info_string() {
        let content = "```markdown   \n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Trailing whitespace should not affect detection");
    }

    #[test]
    fn test_only_closing_fence_pattern() {
        // Content that has only closing fence patterns (no language)
        let content = "```markdown\nsome text\n```\nmore text\n```\n";
        let result = run_check(content).unwrap();
        // The first ``` closes, second ``` is outside
        assert!(result.is_empty(), "Properly closed block should not trigger");
    }

    #[test]
    fn test_fence_at_end_of_file_no_newline() {
        let content = "```markdown\n```python\ncode()\n```\n```";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should detect collision even without trailing newline");

        let fixed = run_fix(content).unwrap();
        assert!(!fixed.ends_with('\n'), "Should preserve lack of trailing newline");
    }

    #[test]
    fn test_empty_lines_between_fences() {
        let content = "```markdown\n\n\n```python\n\ncode()\n\n```\n\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Empty lines should not affect collision detection");
    }

    #[test]
    fn test_tab_indented_opening_fence() {
        // Tab at start of line - CommonMark says tab = 4 spaces for indentation.
        // A 4-space indented fence is NOT a valid fenced code block per CommonMark
        // (only 0-3 spaces allowed). However, our implementation counts characters,
        // treating tab as 1 character. This means tab-indented fences ARE parsed.
        // This is intentional: consistent with other rules in rumdl and matches
        // common editor behavior where tab = 1 indent level.
        let content = "\t```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        // With tab treated as 1 char (< 3), this IS parsed as a fence and triggers collision
        assert_eq!(result.len(), 1, "Tab-indented fence is parsed (tab = 1 char)");
    }

    #[test]
    fn test_mixed_fence_types_no_collision() {
        // Backticks outer, tildes inner - should never collide
        let content = "```markdown\n~~~python\ncode()\n~~~\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Different fence chars should not collide");

        // Tildes outer, backticks inner
        let content2 = "~~~markdown\n```python\ncode()\n```\n~~~\n";
        let result2 = run_check(content2).unwrap();
        assert!(result2.is_empty(), "Different fence chars should not collide");
    }

    #[test]
    fn test_frontmatter_not_confused_with_fence() {
        // YAML frontmatter uses --- which shouldn't be confused with fences
        let content = "---\ntitle: Test\n---\n\n```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should detect collision after frontmatter");
    }

    #[test]
    fn test_html_comment_with_fence_inside() {
        // Fences inside HTML comments should be ignored
        let content = "<!-- ```markdown\n```python\ncode()\n``` -->\n\n```markdown\nreal content\n```\n";
        let result = run_check(content).unwrap();
        // The fences inside HTML comment should be skipped
        assert!(result.is_empty(), "Fences in HTML comments should be ignored");
    }

    #[test]
    fn test_consecutive_code_blocks() {
        // Multiple consecutive markdown blocks, each with collision
        let content = r#"```markdown
```python
a()
```
```

```markdown
```ruby
b()
```
```
"#;
        let result = run_check(content).unwrap();
        // Each markdown block has its own collision
        assert!(!result.is_empty(), "Should detect collision in first block");
    }

    #[test]
    fn test_numeric_info_string() {
        // Numbers after fence - some parsers treat this differently
        let content = "```123\n```456\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        // "123" is not "markdown" or "md", so should not check
        assert!(result.is_empty(), "Numeric info string is not markdown");
    }

    #[test]
    fn test_collision_at_exact_length() {
        // An empty ``` is the closing fence, not a collision.
        // For a collision, the inner fence must have content that looks like an opening fence.
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Same-length fence with language should trigger collision"
        );

        // Inner fence one shorter than outer - not a collision
        let content2 = "````markdown\n```python\ncode()\n```\n````\n";
        let result2 = run_check(content2).unwrap();
        assert!(result2.is_empty(), "Shorter inner fence should not collide");

        // Empty markdown block followed by another fence - not a collision
        let content3 = "```markdown\n```\n";
        let result3 = run_check(content3).unwrap();
        assert!(result3.is_empty(), "Empty closing fence is not a collision");
    }

    #[test]
    fn test_fix_preserves_content_exactly() {
        // Fix should not modify the content between fences
        let content = "```markdown\n```python\n  indented\n\ttabbed\nspecial: !@#$%\n```\n```\n";
        let fixed = run_fix(content).unwrap();
        assert!(fixed.contains("  indented"), "Indentation should be preserved");
        assert!(fixed.contains("\ttabbed"), "Tabs should be preserved");
        assert!(fixed.contains("special: !@#$%"), "Special chars should be preserved");
    }

    #[test]
    fn test_warning_line_numbers_accurate() {
        let content = "# Title\n\nParagraph\n\n```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5, "Warning should be on opening fence line");
        assert!(result[0].message.contains("line 6"), "Collision line should be line 6");
    }

    #[test]
    fn test_should_skip_optimization() {
        let rule = MD070NestedCodeFence::new();

        // No code-like content
        let ctx1 = LintContext::new("Just plain text", crate::config::MarkdownFlavor::Standard, None);
        assert!(
            rule.should_skip(&ctx1),
            "Should skip content without backticks or tildes"
        );

        // Has backticks
        let ctx2 = LintContext::new("Has `code`", crate::config::MarkdownFlavor::Standard, None);
        assert!(!rule.should_skip(&ctx2), "Should not skip content with backticks");

        // Has tildes
        let ctx3 = LintContext::new("Has ~~~", crate::config::MarkdownFlavor::Standard, None);
        assert!(!rule.should_skip(&ctx3), "Should not skip content with tildes");

        // Empty
        let ctx4 = LintContext::new("", crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.should_skip(&ctx4), "Should skip empty content");
    }

    #[test]
    fn test_python_triplestring_fence_collision_fix() {
        // Reproduces GitHub issue #518: Python triple-quoted strings with embedded
        // markdown cause premature fence closure
        let content = "# Test\n\n```python\ndef f():\n    text = \"\"\"\n```json\n{}\n```\n\"\"\"\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should detect collision in python block");
        assert!(result[0].fix.is_some(), "Warning should be marked as fixable");

        let fixed = run_fix(content).unwrap();
        assert!(
            fixed.contains("````python"),
            "Should upgrade opening fence to 4 backticks"
        );
        assert!(
            fixed.contains("````\n") || fixed.ends_with("````"),
            "Should upgrade closing fence to 4 backticks"
        );
        // Content between fences should be preserved
        assert!(fixed.contains("```json"), "Inner fences should be preserved as content");
    }

    #[test]
    fn test_warning_is_fixable() {
        // All MD070 warnings must have fix.is_some() so the fix coordinator calls fix()
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].fix.is_some(),
            "MD070 warnings must be marked fixable for the fix coordinator"
        );
    }

    #[test]
    fn test_fix_via_warning_struct_is_safe() {
        // The Fix on warnings is used directly by the LSP code action path.
        // It must produce valid output (not delete the fence or corrupt the file).
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);

        let fix = result[0].fix.as_ref().unwrap();
        // Apply the Fix directly (simulating LSP path)
        let mut fixed = String::new();
        fixed.push_str(&content[..fix.range.start]);
        fixed.push_str(&fix.replacement);
        fixed.push_str(&content[fix.range.end..]);

        // The fixed content should have upgraded fences
        assert!(
            fixed.contains("````markdown"),
            "Direct Fix application should upgrade opening fence, got: {fixed}"
        );
        assert!(
            fixed.contains("````\n") || fixed.ends_with("````"),
            "Direct Fix application should upgrade closing fence, got: {fixed}"
        );
        // Content should be preserved
        assert!(
            fixed.contains("```python"),
            "Inner content should be preserved, got: {fixed}"
        );
    }

    #[test]
    fn test_fix_via_warning_struct_python_block() {
        // Test the LSP code action path for a Python block where CommonMark's
        // closing fence differs from the user's intended closing fence.
        // CommonMark sees: ```python (line 1) closed by bare ``` (line 6).
        // User intended: ```python (line 1) closed by ``` (line 10).
        let content = "```python\ndef f():\n    text = \"\"\"\n```json\n{}\n```\n\"\"\"\n    print(text)\nf()\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);

        let fix = result[0].fix.as_ref().unwrap();
        let mut fixed = String::new();
        fixed.push_str(&content[..fix.range.start]);
        fixed.push_str(&fix.replacement);
        fixed.push_str(&content[fix.range.end..]);

        // The Fix must cover the full intended block (lines 1-10), not just
        // the CommonMark-visible block (lines 1-6). Verify the fixed content
        // has one code block containing ALL the Python code.
        assert!(
            fixed.starts_with("````python\n"),
            "Should upgrade opening fence, got:\n{fixed}"
        );
        assert!(
            fixed.contains("````\n") || fixed.trim_end().ends_with("````"),
            "Should upgrade closing fence, got:\n{fixed}"
        );
        // ALL Python code must be between the fences
        let fence_start = fixed.find("````python\n").unwrap();
        let after_open = fence_start + "````python\n".len();
        let close_pos = fixed[after_open..]
            .find("\n````\n")
            .or_else(|| fixed[after_open..].find("\n````"));
        assert!(
            close_pos.is_some(),
            "Should have closing fence after content, got:\n{fixed}"
        );
        let block_content = &fixed[after_open..after_open + close_pos.unwrap()];
        assert!(
            block_content.contains("print(text)"),
            "print(text) must be inside the code block, got block:\n{block_content}"
        );
        assert!(
            block_content.contains("f()"),
            "f() must be inside the code block, got block:\n{block_content}"
        );
        assert!(
            block_content.contains("```json"),
            "Inner fences must be preserved as content, got block:\n{block_content}"
        );
    }

    #[test]
    fn test_fix_via_apply_warning_fixes() {
        // End-to-end test of the LSP fix path using apply_warning_fixes
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1);

        let fixed = crate::utils::fix_utils::apply_warning_fixes(content, &result).unwrap();
        assert!(
            fixed.contains("````markdown"),
            "apply_warning_fixes should upgrade opening fence"
        );
        assert!(
            fixed.contains("````\n") || fixed.ends_with("````"),
            "apply_warning_fixes should upgrade closing fence"
        );

        // Re-check should find no issues
        let ctx2 = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD070NestedCodeFence::new();
        let result2 = rule.check(&ctx2).unwrap();
        assert!(
            result2.is_empty(),
            "Re-check after LSP fix should find no issues, got: {:?}",
            result2.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    /// Helper: run fix() then check() on the result, asserting 0 violations remain
    fn assert_fix_roundtrip(content: &str, label: &str) {
        let fixed = run_fix(content).unwrap();
        let rule = MD070NestedCodeFence::new();
        let ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let remaining = rule.check(&ctx).unwrap();
        assert!(
            remaining.is_empty(),
            "[{label}] fix() should resolve all violations, but {n} remain: {msgs:?}\nFixed content:\n{fixed}",
            n = remaining.len(),
            msgs = remaining.iter().map(|w| &w.message).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn test_fix_roundtrip_basic() {
        assert_fix_roundtrip("```markdown\n```python\ncode()\n```\n```\n", "basic collision");
    }

    #[test]
    fn test_fix_roundtrip_longer_inner_fence() {
        assert_fix_roundtrip("```markdown\n`````python\ncode()\n`````\n```\n", "longer inner fence");
    }

    #[test]
    fn test_fix_roundtrip_tilde_collision() {
        assert_fix_roundtrip("~~~markdown\n~~~python\ncode()\n~~~\n~~~\n", "tilde collision");
    }

    #[test]
    fn test_fix_roundtrip_info_string_attrs() {
        assert_fix_roundtrip(
            "```markdown {.highlight}\n```python\ncode()\n```\n```\n",
            "info string with attrs",
        );
    }

    #[test]
    fn test_fix_roundtrip_no_trailing_newline() {
        assert_fix_roundtrip("```markdown\n```python\ncode()\n```\n```", "no trailing newline");
    }

    #[test]
    fn test_fix_roundtrip_python_triple_string() {
        assert_fix_roundtrip(
            "# Test\n\n```python\ndef f():\n    text = \"\"\"\n```json\n{}\n```\n\"\"\"\n```\n",
            "python triple string",
        );
    }

    #[test]
    fn test_fix_roundtrip_deeply_nested() {
        assert_fix_roundtrip(
            "```markdown\n````markdown\n```python\ncode()\n```\n````\n```\n",
            "deeply nested fences",
        );
    }

    #[test]
    fn test_fix_roundtrip_real_world_docs() {
        let content = r#"```markdown
1. First item

   ```python
   code_in_list()
   ```

1. Second item

```
"#;
        assert_fix_roundtrip(content, "real world docs case");
    }

    #[test]
    fn test_fix_roundtrip_empty_lines() {
        assert_fix_roundtrip(
            "```markdown\n\n\n```python\n\ncode()\n\n```\n\n```\n",
            "empty lines between fences",
        );
    }

    #[test]
    fn test_fix_no_change_when_no_violations() {
        let content = "````markdown\n```python\ncode()\n```\n````\n";
        let fixed = run_fix(content).unwrap();
        assert_eq!(fixed, content, "fix() should not modify content with no violations");
    }

    #[test]
    fn test_fix_roundtrip_consecutive_collisions() {
        let content = r#"```markdown
```python
a()
```
```

```md
```ruby
b()
```
```
"#;
        // Fix and verify each collision is resolved
        let fixed = run_fix(content).unwrap();
        let rule = MD070NestedCodeFence::new();
        let ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::Standard, None);
        let remaining = rule.check(&ctx).unwrap();
        // At minimum the first block should be fixed; consecutive blocks may
        // require multiple passes but the first pass must not make things worse
        assert!(
            remaining.len() < 2,
            "fix() should resolve at least one collision, remaining: {remaining:?}",
        );
    }
}
