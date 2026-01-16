/// Rule MD031: Blank lines around fenced code blocks
///
/// See [docs/md031.md](../../docs/md031.md) for full documentation, configuration, and examples.
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use crate::utils::element_cache::ElementCache;
use crate::utils::kramdown_utils::is_kramdown_block_attribute;
use crate::utils::mkdocs_admonitions;
use crate::utils::quarto_divs;
use crate::utils::range_utils::calculate_line_range;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

/// Configuration for MD031 rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct MD031Config {
    /// Whether to require blank lines around code blocks in lists
    #[serde(default = "default_list_items")]
    pub list_items: bool,
}

impl Default for MD031Config {
    fn default() -> Self {
        Self {
            list_items: default_list_items(),
        }
    }
}

fn default_list_items() -> bool {
    true
}

impl RuleConfig for MD031Config {
    const RULE_NAME: &'static str = "MD031";
}

/// Rule MD031: Fenced code blocks should be surrounded by blank lines
#[derive(Clone, Default)]
pub struct MD031BlanksAroundFences {
    config: MD031Config,
}

impl MD031BlanksAroundFences {
    pub fn new(list_items: bool) -> Self {
        Self {
            config: MD031Config { list_items },
        }
    }

    pub fn from_config_struct(config: MD031Config) -> Self {
        Self { config }
    }

    /// Check if a line is effectively empty (blank or an empty blockquote line like ">")
    /// Uses the pre-computed blockquote info from LintContext for accurate detection
    fn is_effectively_empty_line(line_idx: usize, lines: &[&str], ctx: &crate::lint_context::LintContext) -> bool {
        let line = lines.get(line_idx).unwrap_or(&"");

        // First check if it's a regular blank line
        if line.trim().is_empty() {
            return true;
        }

        // Check if this is an empty blockquote line (like ">", "> ", ">>", etc.)
        if let Some(line_info) = ctx.lines.get(line_idx)
            && let Some(ref bq) = line_info.blockquote
        {
            // If the blockquote content is empty, this is effectively a blank line
            return bq.content.trim().is_empty();
        }

        false
    }

    /// Check if a line is inside a list item
    fn is_in_list(&self, line_index: usize, lines: &[&str]) -> bool {
        // Look backwards to find if we're in a list item
        for i in (0..=line_index).rev() {
            let line = lines[i];
            let trimmed = line.trim_start();

            // If we hit a blank line, we're no longer in a list
            if trimmed.is_empty() {
                return false;
            }

            // Check for ordered list (number followed by . or ))
            if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                let mut chars = trimmed.chars().skip_while(|c| c.is_ascii_digit());
                if let Some(next) = chars.next()
                    && (next == '.' || next == ')')
                    && chars.next() == Some(' ')
                {
                    return true;
                }
            }

            // Check for unordered list
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
                return true;
            }

            // If this line is indented (3+ columns), it might be a continuation of a list item
            let is_indented = ElementCache::calculate_indentation_width_default(line) >= 3;
            if is_indented {
                continue; // Keep looking backwards for the list marker
            }

            // If we reach here and haven't found a list marker, and we're not at an indented line,
            // then we're not in a list
            return false;
        }

        false
    }

    /// Check if blank line should be required based on configuration
    fn should_require_blank_line(&self, line_index: usize, lines: &[&str]) -> bool {
        if self.config.list_items {
            // Always require blank lines when list_items is true
            true
        } else {
            // Don't require blank lines inside lists when list_items is false
            !self.is_in_list(line_index, lines)
        }
    }

    /// Check if the current line is immediately after frontmatter (prev line is closing delimiter)
    fn is_right_after_frontmatter(line_index: usize, ctx: &crate::lint_context::LintContext) -> bool {
        line_index > 0
            && ctx.lines.get(line_index - 1).is_some_and(|info| info.in_front_matter)
            && ctx.lines.get(line_index).is_some_and(|info| !info.in_front_matter)
    }

    /// Detect fenced code blocks using pulldown-cmark (handles list-indented fences correctly)
    ///
    /// Returns a vector of (opening_line_idx, closing_line_idx) for each fenced code block.
    /// The indices are 0-based line numbers.
    fn detect_fenced_code_blocks_pulldown(
        content: &str,
        line_offsets: &[usize],
        lines: &[&str],
    ) -> Vec<(usize, usize)> {
        let mut fenced_blocks = Vec::new();
        let options = Options::all();
        let parser = Parser::new_ext(content, options).into_offset_iter();

        let mut current_block_start: Option<usize> = None;

        // Helper to convert byte offset to line index
        let byte_to_line = |byte_offset: usize| -> usize {
            line_offsets
                .iter()
                .enumerate()
                .rev()
                .find(|&(_, &offset)| offset <= byte_offset)
                .map(|(idx, _)| idx)
                .unwrap_or(0)
        };

        for (event, range) in parser {
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) => {
                    let line_idx = byte_to_line(range.start);
                    current_block_start = Some(line_idx);
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(start_line) = current_block_start.take() {
                        // Find the closing fence line
                        // The range.end points just past the closing fence
                        // We need to find the line that contains the actual closing fence
                        let end_byte = if range.end > 0 { range.end - 1 } else { 0 };
                        let end_line = byte_to_line(end_byte);

                        // Verify this is actually a closing fence line (not just end of content)
                        // For properly closed fences, the end line should contain a fence marker
                        let end_line_content = lines.get(end_line).unwrap_or(&"");
                        // Strip blockquote prefix before checking for fence markers
                        let trimmed = end_line_content.trim();
                        let content_after_bq = if trimmed.starts_with('>') {
                            trimmed.trim_start_matches(['>', ' ']).trim()
                        } else {
                            trimmed
                        };
                        let is_closing_fence = (content_after_bq.starts_with("```")
                            || content_after_bq.starts_with("~~~"))
                            && content_after_bq
                                .chars()
                                .skip_while(|&c| c == '`' || c == '~')
                                .all(|c| c.is_whitespace());

                        if is_closing_fence {
                            fenced_blocks.push((start_line, end_line));
                        } else {
                            // Unclosed code block - extends to end of document
                            // We still record it but the end_line will be the last line
                            fenced_blocks.push((start_line, lines.len().saturating_sub(1)));
                        }
                    }
                }
                _ => {}
            }
        }

        // Handle any unclosed block
        if let Some(start_line) = current_block_start {
            fenced_blocks.push((start_line, lines.len().saturating_sub(1)));
        }

        fenced_blocks
    }
}

impl Rule for MD031BlanksAroundFences {
    fn name(&self) -> &'static str {
        "MD031"
    }

    fn description(&self) -> &'static str {
        "Fenced code blocks should be surrounded by blank lines"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        let line_index = &ctx.line_index;

        let mut warnings = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let is_mkdocs = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;
        let is_quarto = ctx.flavor == crate::config::MarkdownFlavor::Quarto;

        // Detect fenced code blocks using pulldown-cmark (handles list-indented fences correctly)
        let fenced_blocks = Self::detect_fenced_code_blocks_pulldown(content, &ctx.line_offsets, &lines);

        // Helper to check if a line is a Quarto div marker (opening or closing)
        let is_quarto_div_marker =
            |line: &str| -> bool { is_quarto && (quarto_divs::is_div_open(line) || quarto_divs::is_div_close(line)) };

        // Check blank lines around each fenced code block
        for (opening_line, closing_line) in &fenced_blocks {
            // Check for blank line before opening fence
            // Skip if right after frontmatter
            // Skip if right after Quarto div marker (Quarto flavor)
            // Use is_effectively_empty_line to handle blockquote blank lines (issue #284)
            let prev_line_is_quarto_marker = *opening_line > 0 && is_quarto_div_marker(lines[*opening_line - 1]);
            if *opening_line > 0
                && !Self::is_effectively_empty_line(*opening_line - 1, &lines, ctx)
                && !Self::is_right_after_frontmatter(*opening_line, ctx)
                && !prev_line_is_quarto_marker
                && self.should_require_blank_line(*opening_line, &lines)
            {
                let (start_line, start_col, end_line, end_col) =
                    calculate_line_range(*opening_line + 1, lines[*opening_line]);

                let bq_prefix = ctx.blockquote_prefix_for_blank_line(*opening_line);
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: "No blank line before fenced code block".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range_with_length(*opening_line + 1, 1, 0),
                        replacement: format!("{bq_prefix}\n"),
                    }),
                });
            }

            // Check for blank line after closing fence
            // Allow Kramdown block attributes if configured
            // Skip if followed by Quarto div marker (Quarto flavor)
            // Use is_effectively_empty_line to handle blockquote blank lines (issue #284)
            let next_line_is_quarto_marker =
                *closing_line + 1 < lines.len() && is_quarto_div_marker(lines[*closing_line + 1]);
            if *closing_line + 1 < lines.len()
                && !Self::is_effectively_empty_line(*closing_line + 1, &lines, ctx)
                && !is_kramdown_block_attribute(lines[*closing_line + 1])
                && !next_line_is_quarto_marker
                && self.should_require_blank_line(*closing_line, &lines)
            {
                let (start_line, start_col, end_line, end_col) =
                    calculate_line_range(*closing_line + 1, lines[*closing_line]);

                let bq_prefix = ctx.blockquote_prefix_for_blank_line(*closing_line);
                warnings.push(LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: start_line,
                    column: start_col,
                    end_line,
                    end_column: end_col,
                    message: "No blank line after fenced code block".to_string(),
                    severity: Severity::Warning,
                    fix: Some(Fix {
                        range: line_index.line_col_to_byte_range_with_length(
                            *closing_line + 1,
                            lines[*closing_line].len() + 1,
                            0,
                        ),
                        replacement: format!("{bq_prefix}\n"),
                    }),
                });
            }
        }

        // Handle MkDocs admonitions separately
        if is_mkdocs {
            let mut in_admonition = false;
            let mut admonition_indent = 0;
            let mut i = 0;

            while i < lines.len() {
                let line = lines[i];

                // Skip if this line is inside a fenced code block
                let in_fenced_block = fenced_blocks.iter().any(|(start, end)| i >= *start && i <= *end);
                if in_fenced_block {
                    i += 1;
                    continue;
                }

                // Check for MkDocs admonition start
                if mkdocs_admonitions::is_admonition_start(line) {
                    // Check for blank line before admonition
                    if i > 0
                        && !Self::is_effectively_empty_line(i - 1, &lines, ctx)
                        && !Self::is_right_after_frontmatter(i, ctx)
                        && self.should_require_blank_line(i, &lines)
                    {
                        let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, lines[i]);

                        let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "No blank line before admonition block".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range_with_length(i + 1, 1, 0),
                                replacement: format!("{bq_prefix}\n"),
                            }),
                        });
                    }

                    in_admonition = true;
                    admonition_indent = mkdocs_admonitions::get_admonition_indent(line).unwrap_or(0);
                    i += 1;
                    continue;
                }

                // Check if we're exiting an admonition
                if in_admonition
                    && !line.trim().is_empty()
                    && !mkdocs_admonitions::is_admonition_content(line, admonition_indent)
                {
                    in_admonition = false;

                    // Check for blank line after admonition
                    // We need a blank line between the admonition content and the current line
                    // Check if the previous line (i-1) is a blank line separator
                    if i > 0
                        && !Self::is_effectively_empty_line(i - 1, &lines, ctx)
                        && self.should_require_blank_line(i - 1, &lines)
                    {
                        let (start_line, start_col, end_line, end_col) = calculate_line_range(i + 1, lines[i]);

                        let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                        warnings.push(LintWarning {
                            rule_name: Some(self.name().to_string()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message: "No blank line after admonition block".to_string(),
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: line_index.line_col_to_byte_range_with_length(i, 0, 0),
                                replacement: format!("{bq_prefix}\n"),
                            }),
                        });
                    }

                    admonition_indent = 0;
                }

                i += 1;
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let content = ctx.content;

        // Check if original content ended with newline
        let had_trailing_newline = content.ends_with('\n');

        let lines: Vec<&str> = content.lines().collect();
        let is_quarto = ctx.flavor == crate::config::MarkdownFlavor::Quarto;

        // Helper to check if a line is a Quarto div marker (opening or closing)
        let is_quarto_div_marker =
            |line: &str| -> bool { is_quarto && (quarto_divs::is_div_open(line) || quarto_divs::is_div_close(line)) };

        // Detect fenced code blocks using pulldown-cmark (handles list-indented fences correctly)
        let fenced_blocks = Self::detect_fenced_code_blocks_pulldown(content, &ctx.line_offsets, &lines);

        // Collect lines that need blank lines before/after
        let mut needs_blank_before: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut needs_blank_after: std::collections::HashSet<usize> = std::collections::HashSet::new();

        for (opening_line, closing_line) in &fenced_blocks {
            // Check if needs blank line before opening fence
            // Skip if right after Quarto div marker (Quarto flavor)
            // Use is_effectively_empty_line to handle blockquote blank lines
            let prev_line_is_quarto_marker = *opening_line > 0 && is_quarto_div_marker(lines[*opening_line - 1]);
            if *opening_line > 0
                && !Self::is_effectively_empty_line(*opening_line - 1, &lines, ctx)
                && !Self::is_right_after_frontmatter(*opening_line, ctx)
                && !prev_line_is_quarto_marker
                && self.should_require_blank_line(*opening_line, &lines)
            {
                needs_blank_before.insert(*opening_line);
            }

            // Check if needs blank line after closing fence
            // Skip if followed by Quarto div marker (Quarto flavor)
            // Use is_effectively_empty_line to handle blockquote blank lines
            let next_line_is_quarto_marker =
                *closing_line + 1 < lines.len() && is_quarto_div_marker(lines[*closing_line + 1]);
            if *closing_line + 1 < lines.len()
                && !Self::is_effectively_empty_line(*closing_line + 1, &lines, ctx)
                && !is_kramdown_block_attribute(lines[*closing_line + 1])
                && !next_line_is_quarto_marker
                && self.should_require_blank_line(*closing_line, &lines)
            {
                needs_blank_after.insert(*closing_line);
            }
        }

        // Build result with blank lines inserted as needed
        let mut result = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            // Add blank line before this line if needed
            if needs_blank_before.contains(&i) {
                let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                result.push(bq_prefix);
            }

            result.push((*line).to_string());

            // Add blank line after this line if needed
            if needs_blank_after.contains(&i) {
                let bq_prefix = ctx.blockquote_prefix_for_blank_line(i);
                result.push(bq_prefix);
            }
        }

        let fixed = result.join("\n");

        // Preserve original trailing newline if it existed
        let final_result = if had_trailing_newline && !fixed.ends_with('\n') {
            format!("{fixed}\n")
        } else {
            fixed
        };

        Ok(final_result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::CodeBlock
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Check for fenced code blocks (backticks or tildes)
        ctx.content.is_empty() || (!ctx.likely_has_code() && !ctx.has_char('~'))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD031Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;
        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD031Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD031Config>(config);
        Box::new(MD031BlanksAroundFences::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_basic_functionality() {
        let rule = MD031BlanksAroundFences::default();

        // Test with properly formatted code blocks
        let content = "# Test Code Blocks\n\n```rust\nfn main() {}\n```\n\nSome text here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Expected no warnings for properly formatted code blocks"
        );

        // Test with missing blank line before
        let content = "# Test Code Blocks\n```rust\nfn main() {}\n```\n\nSome text here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Expected 1 warning for missing blank line before");
        assert_eq!(warnings[0].line, 2, "Warning should be on line 2");
        assert!(
            warnings[0].message.contains("before"),
            "Warning should be about blank line before"
        );

        // Test with missing blank line after
        let content = "# Test Code Blocks\n\n```rust\nfn main() {}\n```\nSome text here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Expected 1 warning for missing blank line after");
        assert_eq!(warnings[0].line, 5, "Warning should be on line 5");
        assert!(
            warnings[0].message.contains("after"),
            "Warning should be about blank line after"
        );

        // Test with missing blank lines both before and after
        let content = "# Test Code Blocks\n```rust\nfn main() {}\n```\nSome text here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Expected 2 warnings for missing blank lines before and after"
        );
    }

    #[test]
    fn test_nested_code_blocks() {
        let rule = MD031BlanksAroundFences::default();

        // Test that nested code blocks are not flagged
        let content = r#"````markdown
```
content
```
````"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0, "Should not flag nested code blocks");

        // Test that fixes don't corrupt nested blocks
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Fix should not modify nested code blocks");
    }

    #[test]
    fn test_nested_code_blocks_complex() {
        let rule = MD031BlanksAroundFences::default();

        // Test documentation example with nested code blocks
        let content = r#"# Documentation

## Examples

````markdown
```python
def hello():
    print("Hello, world!")
```

```javascript
console.log("Hello, world!");
```
````

More text here."#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            0,
            "Should not flag any issues in properly formatted nested code blocks"
        );

        // Test with 5-backtick outer block
        let content_5 = r#"`````markdown
````python
```bash
echo "nested"
```
````
`````"#;

        let ctx_5 = LintContext::new(content_5, crate::config::MarkdownFlavor::Standard, None);
        let warnings_5 = rule.check(&ctx_5).unwrap();
        assert_eq!(warnings_5.len(), 0, "Should handle deeply nested code blocks");
    }

    #[test]
    fn test_fix_preserves_trailing_newline() {
        let rule = MD031BlanksAroundFences::default();

        // Test content with trailing newline
        let content = "Some text\n```\ncode\n```\nMore text\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should preserve the trailing newline
        assert!(fixed.ends_with('\n'), "Fix should preserve trailing newline");
        assert_eq!(fixed, "Some text\n\n```\ncode\n```\n\nMore text\n");
    }

    #[test]
    fn test_fix_preserves_no_trailing_newline() {
        let rule = MD031BlanksAroundFences::default();

        // Test content without trailing newline
        let content = "Some text\n```\ncode\n```\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should not add trailing newline if original didn't have one
        assert!(
            !fixed.ends_with('\n'),
            "Fix should not add trailing newline if original didn't have one"
        );
        assert_eq!(fixed, "Some text\n\n```\ncode\n```\n\nMore text");
    }

    #[test]
    fn test_list_items_config_true() {
        // Test with list_items: true (default) - should require blank lines even in lists
        let rule = MD031BlanksAroundFences::new(true);

        let content = "1. First item\n   ```python\n   code_in_list()\n   ```\n2. Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should flag missing blank lines before and after code block in list
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].message.contains("before"));
        assert!(warnings[1].message.contains("after"));
    }

    #[test]
    fn test_list_items_config_false() {
        // Test with list_items: false - should NOT require blank lines in lists
        let rule = MD031BlanksAroundFences::new(false);

        let content = "1. First item\n   ```python\n   code_in_list()\n   ```\n2. Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should not flag missing blank lines inside lists
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_list_items_config_false_outside_list() {
        // Test with list_items: false - should still require blank lines outside lists
        let rule = MD031BlanksAroundFences::new(false);

        let content = "Some text\n```python\ncode_outside_list()\n```\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should still flag missing blank lines outside lists
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].message.contains("before"));
        assert!(warnings[1].message.contains("after"));
    }

    #[test]
    fn test_default_config_section() {
        let rule = MD031BlanksAroundFences::default();
        let config_section = rule.default_config_section();

        assert!(config_section.is_some());
        let (name, value) = config_section.unwrap();
        assert_eq!(name, "MD031");

        // Should contain the list_items option with default value true
        if let toml::Value::Table(table) = value {
            assert!(table.contains_key("list-items"));
            assert_eq!(table["list-items"], toml::Value::Boolean(true));
        } else {
            panic!("Expected TOML table");
        }
    }

    #[test]
    fn test_fix_list_items_config_false() {
        // Test that fix respects list_items: false configuration
        let rule = MD031BlanksAroundFences::new(false);

        let content = "1. First item\n   ```python\n   code()\n   ```\n2. Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should not add blank lines when list_items is false
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_fix_list_items_config_true() {
        // Test that fix respects list_items: true configuration
        let rule = MD031BlanksAroundFences::new(true);

        let content = "1. First item\n   ```python\n   code()\n   ```\n2. Second item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should add blank lines when list_items is true
        let expected = "1. First item\n\n   ```python\n   code()\n   ```\n\n2. Second item";
        assert_eq!(fixed, expected);
    }

    #[test]
    fn test_no_warning_after_frontmatter() {
        // Code block immediately after frontmatter should not trigger MD031
        // This matches markdownlint behavior
        let rule = MD031BlanksAroundFences::default();

        let content = "---\ntitle: Test\n---\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should not flag missing blank line before code block after frontmatter
        assert!(
            warnings.is_empty(),
            "Expected no warnings for code block after frontmatter, got: {warnings:?}"
        );
    }

    #[test]
    fn test_fix_does_not_add_blank_after_frontmatter() {
        // Fix should not add blank line between frontmatter and code block
        let rule = MD031BlanksAroundFences::default();

        let content = "---\ntitle: Test\n---\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should not add blank line after frontmatter
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_frontmatter_with_blank_line_before_code() {
        // If there's already a blank line between frontmatter and code, that's fine
        let rule = MD031BlanksAroundFences::default();

        let content = "---\ntitle: Test\n---\n\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(warnings.is_empty());
    }

    #[test]
    fn test_no_warning_for_admonition_after_frontmatter() {
        // Admonition immediately after frontmatter should not trigger MD031
        let rule = MD031BlanksAroundFences::default();

        let content = "---\ntitle: Test\n---\n!!! note\n    This is a note";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "Expected no warnings for admonition after frontmatter, got: {warnings:?}"
        );
    }

    #[test]
    fn test_toml_frontmatter_before_code() {
        // TOML frontmatter should also be handled
        let rule = MD031BlanksAroundFences::default();

        let content = "+++\ntitle = \"Test\"\n+++\n```\ncode\n```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        assert!(
            warnings.is_empty(),
            "Expected no warnings for code block after TOML frontmatter, got: {warnings:?}"
        );
    }

    #[test]
    fn test_fenced_code_in_list_with_4_space_indent_issue_276() {
        // Issue #276: Fenced code blocks inside lists with 4+ space indentation
        // were not being detected because of the old 0-3 space CommonMark limit.
        // Now we use pulldown-cmark which correctly handles list-indented fences.
        let rule = MD031BlanksAroundFences::new(true);

        // 4-space indented fenced code block in list (was not detected before fix)
        let content =
            "1. First item\n2. Second item with code:\n    ```python\n    print(\"Hello\")\n    ```\n3. Third item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should detect missing blank lines around the code block
        assert_eq!(
            warnings.len(),
            2,
            "Should detect fenced code in list with 4-space indent, got: {warnings:?}"
        );
        assert!(warnings[0].message.contains("before"));
        assert!(warnings[1].message.contains("after"));

        // Test the fix adds blank lines
        let fixed = rule.fix(&ctx).unwrap();
        let expected =
            "1. First item\n2. Second item with code:\n\n    ```python\n    print(\"Hello\")\n    ```\n\n3. Third item";
        assert_eq!(
            fixed, expected,
            "Fix should add blank lines around list-indented fenced code"
        );
    }

    #[test]
    fn test_fenced_code_in_list_with_mixed_indentation() {
        // Test both 3-space and 4-space indented fenced code blocks in same document
        let rule = MD031BlanksAroundFences::new(true);

        let content = r#"# Test

3-space indent:
1. First item
   ```python
   code
   ```
2. Second item

4-space indent:
1. First item
    ```python
    code
    ```
2. Second item"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();

        // Should detect all 4 missing blank lines (2 per code block)
        assert_eq!(
            warnings.len(),
            4,
            "Should detect all fenced code blocks regardless of indentation, got: {warnings:?}"
        );
    }

    #[test]
    fn test_fix_preserves_blockquote_prefix_before_fence() {
        // Issue #268: Fix should insert blockquote-prefixed blank lines inside blockquotes
        let rule = MD031BlanksAroundFences::default();

        let content = "> Text before
> ```
> code
> ```";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The blank line inserted before the fence should have the blockquote prefix
        let expected = "> Text before
>
> ```
> code
> ```";
        assert_eq!(
            fixed, expected,
            "Fix should insert '>' blank line, not plain blank line"
        );
    }

    #[test]
    fn test_fix_preserves_blockquote_prefix_after_fence() {
        // Issue #268: Fix should insert blockquote-prefixed blank lines inside blockquotes
        let rule = MD031BlanksAroundFences::default();

        let content = "> ```
> code
> ```
> Text after";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // The blank line inserted after the fence should have the blockquote prefix
        let expected = "> ```
> code
> ```
>
> Text after";
        assert_eq!(
            fixed, expected,
            "Fix should insert '>' blank line after fence, not plain blank line"
        );
    }

    #[test]
    fn test_fix_preserves_nested_blockquote_prefix() {
        // Nested blockquotes should preserve the full prefix (e.g., ">>")
        let rule = MD031BlanksAroundFences::default();

        let content = ">> Nested quote
>> ```
>> code
>> ```
>> More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        // Should insert ">>" blank lines, not ">" or plain
        let expected = ">> Nested quote
>>
>> ```
>> code
>> ```
>>
>> More text";
        assert_eq!(fixed, expected, "Fix should preserve nested blockquote prefix '>>'");
    }

    #[test]
    fn test_fix_preserves_triple_nested_blockquote_prefix() {
        // Triple-nested blockquotes should preserve full prefix
        let rule = MD031BlanksAroundFences::default();

        let content = ">>> Triple nested
>>> ```
>>> code
>>> ```
>>> More text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();

        let expected = ">>> Triple nested
>>>
>>> ```
>>> code
>>> ```
>>>
>>> More text";
        assert_eq!(
            fixed, expected,
            "Fix should preserve triple-nested blockquote prefix '>>>'"
        );
    }

    // ==================== Quarto Flavor Tests ====================

    #[test]
    fn test_quarto_code_block_after_div_open() {
        // Code block immediately after Quarto div opening should not require blank line
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\n```python\ncode\n```\n:::";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Should not require blank line after Quarto div opening: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_code_block_before_div_close() {
        // Code block immediately before Quarto div closing should not require blank line
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\nSome text\n```python\ncode\n```\n:::";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        // Should only warn about the blank before the code block (after "Some text"), not after
        assert!(
            warnings.len() <= 1,
            "Should not require blank line before Quarto div closing: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_code_block_outside_div_still_requires_blanks() {
        // Code block outside Quarto div should still require blank lines
        let rule = MD031BlanksAroundFences::default();
        let content = "Some text\n```python\ncode\n```\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(
            warnings.len(),
            2,
            "Should still require blank lines around code blocks outside divs"
        );
    }

    #[test]
    fn test_quarto_code_block_with_callout_note() {
        // Code block inside callout-note should work without blank lines at boundaries
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\n```r\n1 + 1\n```\n:::\n\nMore text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Callout note with code block should have no warnings: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_nested_divs_with_code() {
        // Nested divs with code blocks
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.outer}\n::: {.inner}\n```python\ncode\n```\n:::\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(
            warnings.is_empty(),
            "Nested divs with code blocks should have no warnings: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_div_markers_in_standard_flavor() {
        // In standard flavor, ::: is not special, so normal rules apply
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\n```python\ncode\n```\n:::\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        // In standard flavor, both before and after the code block need blank lines
        // (unless the ":::" lines are treated as text and thus need blanks)
        assert!(
            !warnings.is_empty(),
            "Standard flavor should require blanks around code blocks: {warnings:?}"
        );
    }

    #[test]
    fn test_quarto_fix_does_not_add_blanks_at_div_boundaries() {
        // Fix should not add blank lines at div boundaries
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\n```python\ncode\n```\n:::";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let fixed = rule.fix(&ctx).unwrap();
        // Should remain unchanged - no blanks needed
        assert_eq!(fixed, content, "Fix should not add blanks at Quarto div boundaries");
    }

    #[test]
    fn test_quarto_code_block_with_content_before() {
        // Code block with content before it (inside div) needs blank
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\nHere is some code:\n```python\ncode\n```\n:::";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        // Should warn about missing blank before code block (after "Here is some code:")
        assert_eq!(
            warnings.len(),
            1,
            "Should require blank before code block inside div: {warnings:?}"
        );
        assert!(warnings[0].message.contains("before"));
    }

    #[test]
    fn test_quarto_code_block_with_content_after() {
        // Code block with content after it (inside div) needs blank
        let rule = MD031BlanksAroundFences::default();
        let content = "::: {.callout-note}\n```python\ncode\n```\nMore content here.\n:::";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Quarto, None);
        let warnings = rule.check(&ctx).unwrap();
        // Should warn about missing blank after code block (before "More content here.")
        assert_eq!(
            warnings.len(),
            1,
            "Should require blank after code block inside div: {warnings:?}"
        );
        assert!(warnings[0].message.contains("after"));
    }
}
