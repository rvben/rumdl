use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};

/// Rule MD070: Nested code fence collision detection
///
/// Detects when a fenced code block contains fence markers that would cause
/// premature closure. Suggests using longer fences to avoid this issue.
///
/// Only checks markdown-related blocks (empty language, "markdown", "md")
/// since other languages don't use fence syntax.
///
/// See [docs/md070.md](../../docs/md070.md) for full documentation.
#[derive(Clone, Default)]
pub struct MD070NestedCodeFence;

impl MD070NestedCodeFence {
    pub fn new() -> Self {
        Self
    }

    /// Check if the given language should be checked for nested fences.
    /// Only markdown-related blocks can have fence collisions.
    fn should_check_language(lang: &str) -> bool {
        lang.is_empty() || lang.eq_ignore_ascii_case("markdown") || lang.eq_ignore_ascii_case("md")
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
    fn is_closing_fence(line: &str, fence_char: char, min_length: usize, max_indent: usize) -> bool {
        let indent = line.len() - line.trim_start().len();
        if indent > max_indent {
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
        let content = ctx.content;
        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];

            // Try to parse as opening fence
            if let Some((indent, fence_char, fence_length, info_string)) = Self::parse_fence_line(line) {
                let block_start = i;

                // Extract the language (first word of info string)
                let language = info_string.split_whitespace().next().unwrap_or("");

                // Find the closing fence
                let mut block_end = None;
                for (j, line_j) in lines.iter().enumerate().skip(i + 1) {
                    if Self::is_closing_fence(line_j, fence_char, fence_length, indent) {
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
                            let safe_length = Self::find_safe_fence_length(&block_content, fence_char) + 1;
                            let suggested_fence: String = std::iter::repeat_n(fence_char, safe_length).collect();
                            let current_fence: String = std::iter::repeat_n(fence_char, fence_length).collect();

                            let collision_line_num = block_start + 1 + collision_line_offset + 1; // 1-indexed

                            // Single warning with clear message
                            // Format matches other rules: "Problem description — solution"
                            warnings.push(LintWarning {
                                rule_name: Some(self.name().to_string()),
                                message: format!(
                                    "Nested {current_fence} at line {collision_line_num} closes block prematurely — use {suggested_fence} for outer fence"
                                ),
                                line: block_start + 1,
                                column: 1,
                                end_line: end_line + 1, // Span includes both fences
                                end_column: lines[end_line].len() + 1,
                                severity: Severity::Warning,
                                fix: None, // Fix is handled by the fix() method which updates both fences
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
        let content = ctx.content;
        let mut result = String::new();
        let lines: Vec<&str> = content.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];

            // Try to parse as opening fence
            if let Some((indent, fence_char, fence_length, info_string)) = Self::parse_fence_line(line) {
                let block_start = i;

                // Extract the language
                let language = info_string.split_whitespace().next().unwrap_or("");

                // Find the first closing fence (what CommonMark sees)
                let mut first_close = None;
                for (j, line_j) in lines.iter().enumerate().skip(i + 1) {
                    if Self::is_closing_fence(line_j, fence_char, fence_length, indent) {
                        first_close = Some(j);
                        break;
                    }
                }

                if let Some(end_line) = first_close {
                    // Check if we should fix this block
                    if Self::should_check_language(language) {
                        // Get the content between fences
                        let block_content: String = if block_start + 1 < end_line {
                            lines[(block_start + 1)..end_line].join("\n")
                        } else {
                            String::new()
                        };

                        // Check for fence collision
                        if Self::find_fence_collision(&block_content, fence_char, fence_length).is_some() {
                            // When there's a collision, find the INTENDED closing fence
                            // This is the last matching closing fence at similar indentation
                            let mut intended_close = end_line;
                            for (j, line_j) in lines.iter().enumerate().skip(end_line + 1) {
                                if Self::is_closing_fence(line_j, fence_char, fence_length, indent) {
                                    intended_close = j;
                                    // Don't break - we want the last one in a reasonable range
                                    // But stop if we hit another opening fence at same indent
                                } else if Self::parse_fence_line(line_j).is_some_and(|(ind, ch, _, info)| {
                                    ind <= indent && ch == fence_char && !info.is_empty()
                                }) {
                                    break; // Hit a new block, stop looking
                                }
                            }

                            // Get content between opening and intended close
                            let full_block_content: String = if block_start + 1 < intended_close {
                                lines[(block_start + 1)..intended_close].join("\n")
                            } else {
                                String::new()
                            };

                            let safe_length = Self::find_safe_fence_length(&full_block_content, fence_char) + 1;
                            let suggested_fence: String = std::iter::repeat_n(fence_char, safe_length).collect();

                            // Write fixed opening fence
                            let opening_indent = " ".repeat(indent);
                            result.push_str(&format!("{opening_indent}{suggested_fence}{info_string}\n"));

                            // Write content
                            for line_content in &lines[(block_start + 1)..intended_close] {
                                result.push_str(line_content);
                                result.push('\n');
                            }

                            // Write fixed closing fence
                            let closing_line = lines[intended_close];
                            let closing_indent = closing_line.len() - closing_line.trim_start().len();
                            let closing_indent_str = " ".repeat(closing_indent);
                            result.push_str(&format!("{closing_indent_str}{suggested_fence}\n"));

                            i = intended_close + 1;
                            continue;
                        }
                    }

                    // No collision or not a checked language - preserve as-is
                    for line_content in &lines[block_start..=end_line] {
                        result.push_str(line_content);
                        result.push('\n');
                    }
                    i = end_line + 1;
                    continue;
                }
            }

            // Not a fence line, preserve as-is
            result.push_str(line);
            result.push('\n');
            i += 1;
        }

        // Remove trailing newline if original didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
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
    fn test_no_collision_non_doc_language() {
        // Python is not checked for nested fences
        let content = "```python\n```bash\necho hello\n```\n```\n";
        let result = run_check(content).unwrap();
        assert!(result.is_empty(), "Non-doc language should not be checked");
    }

    #[test]
    fn test_collision_markdown_language() {
        let content = "```markdown\n```python\ncode()\n```\n```\n";
        let result = run_check(content).unwrap();
        assert_eq!(result.len(), 1, "Should emit single warning for collision");
        assert!(result[0].message.contains("Nested"));
        assert!(result[0].message.contains("closes block prematurely"));
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
        // This test demonstrates the cascading effect of nested fence collisions.
        // The markdown block has a collision, so its closing ``` becomes the opening
        // of a new empty-language block, which ALSO has a collision.
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
        // We get 2 warnings (one per collision block):
        // - 1 for the markdown block
        // - 1 for the orphan empty-language block that starts after premature close
        assert_eq!(result.len(), 2, "Both collision blocks should trigger");
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
}
