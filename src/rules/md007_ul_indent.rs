/// Rule MD007: Unordered list indentation
///
/// See [docs/md007.md](../../docs/md007.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use toml;

mod md007_config;
use md007_config::MD007Config;

#[derive(Debug, Clone, Default)]
pub struct MD007ULIndent {
    config: MD007Config,
}

impl MD007ULIndent {
    pub fn new(indent: usize) -> Self {
        Self {
            config: MD007Config {
                indent,
                start_indented: false,
                start_indent: 2,
                style: md007_config::IndentStyle::TextAligned,
            },
        }
    }

    pub fn from_config_struct(config: MD007Config) -> Self {
        Self { config }
    }

    /// Convert character position to visual column (accounting for tabs)
    fn char_pos_to_visual_column(content: &str, char_pos: usize) -> usize {
        let mut visual_col = 0;

        for (current_pos, ch) in content.chars().enumerate() {
            if current_pos >= char_pos {
                break;
            }
            if ch == '\t' {
                // Tab moves to next multiple of 4
                visual_col = (visual_col / 4 + 1) * 4;
            } else {
                visual_col += 1;
            }
        }
        visual_col
    }
}

impl Rule for MD007ULIndent {
    fn name(&self) -> &'static str {
        "MD007"
    }

    fn description(&self) -> &'static str {
        "Unordered list indentation"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();
        let mut list_stack: Vec<(usize, usize, bool, usize)> = Vec::new(); // Stack of (marker_visual_col, line_num, is_ordered, content_visual_col) for tracking nesting

        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            // Skip if this line is in a code block, front matter, or mkdocstrings
            if line_info.in_code_block || line_info.in_front_matter || line_info.in_mkdocstrings {
                continue;
            }

            // Check if this line has a list item
            if let Some(list_item) = &line_info.list_item {
                // For blockquoted lists, we need to calculate indentation relative to the blockquote content
                // not the full line. This is because blockquoted lists follow the same indentation rules
                // as regular lists, just within their blockquote context.
                let (content_for_calculation, adjusted_marker_column) = if line_info.blockquote.is_some() {
                    // Find the position after the blockquote prefix (> or >> etc)
                    // We need to find where the actual content starts after all '>' characters and spaces
                    let mut content_start = 0;
                    let mut found_gt = false;

                    for (i, ch) in line_info.content.chars().enumerate() {
                        if ch == '>' {
                            found_gt = true;
                            content_start = i + 1;
                        } else if found_gt && ch == ' ' {
                            // Skip the space after '>'
                            content_start = i + 1;
                            break;
                        } else if found_gt {
                            // No space after '>', content starts here
                            break;
                        }
                    }

                    // Extract the content after the blockquote prefix
                    let content_after_prefix = &line_info.content[content_start..];
                    // Adjust the marker column to be relative to the content after the prefix
                    let adjusted_col = if list_item.marker_column >= content_start {
                        list_item.marker_column - content_start
                    } else {
                        // This shouldn't happen, but handle it gracefully
                        list_item.marker_column
                    };
                    (content_after_prefix.to_string(), adjusted_col)
                } else {
                    (line_info.content.clone(), list_item.marker_column)
                };

                // Convert marker position to visual column
                let visual_marker_column =
                    Self::char_pos_to_visual_column(&content_for_calculation, adjusted_marker_column);

                // Calculate content visual column for text-aligned style
                let visual_content_column = if line_info.blockquote.is_some() {
                    // For blockquoted content, we already have the adjusted content
                    let adjusted_content_col =
                        if list_item.content_column >= (line_info.content.len() - content_for_calculation.len()) {
                            list_item.content_column - (line_info.content.len() - content_for_calculation.len())
                        } else {
                            list_item.content_column
                        };
                    Self::char_pos_to_visual_column(&content_for_calculation, adjusted_content_col)
                } else {
                    Self::char_pos_to_visual_column(&line_info.content, list_item.content_column)
                };

                // For nesting detection, treat 1-space indent as if it's at column 0
                // because 1 space is insufficient to establish a nesting relationship
                let visual_marker_for_nesting = if visual_marker_column == 1 {
                    0
                } else {
                    visual_marker_column
                };

                // Clean up stack - remove items at same or deeper indentation
                while let Some(&(indent, _, _, _)) = list_stack.last() {
                    if indent >= visual_marker_for_nesting {
                        list_stack.pop();
                    } else {
                        break;
                    }
                }

                // For ordered list items, just track them in the stack
                if list_item.is_ordered {
                    // For ordered lists, we don't check indentation but we need to track for text-aligned children
                    // Use the actual positions since we don't enforce indentation for ordered lists
                    list_stack.push((visual_marker_column, line_idx, true, visual_content_column));
                    continue;
                }

                // Only check unordered list items
                if !list_item.is_ordered {
                    // Now stack contains only parent items
                    let nesting_level = list_stack.len();

                    // Calculate expected indent first to determine expected content position
                    let expected_indent = if self.config.start_indented {
                        self.config.start_indent + (nesting_level * self.config.indent)
                    } else {
                        match self.config.style {
                            md007_config::IndentStyle::Fixed => {
                                // Fixed style: simple multiples of indent
                                nesting_level * self.config.indent
                            }
                            md007_config::IndentStyle::TextAligned => {
                                // Text-aligned style: child's marker aligns with parent's text content
                                if nesting_level > 0 {
                                    // Check if parent is an ordered list
                                    if let Some(&(_, _parent_line_idx, _is_ordered, parent_content_visual_col)) =
                                        list_stack.get(nesting_level - 1)
                                    {
                                        // Child marker is positioned where parent's text starts
                                        parent_content_visual_col
                                    } else {
                                        // No parent at that level - for text-aligned, use standard alignment
                                        // Each level aligns with previous level's text position
                                        nesting_level * 2
                                    }
                                } else {
                                    0 // First level, no indentation needed
                                }
                            }
                        }
                    };

                    // Add current item to stack
                    // Use actual marker position for cleanup logic
                    // For text-aligned children, store the EXPECTED content position after fix
                    // (not the actual position) to prevent error cascade
                    let expected_content_visual_col = expected_indent + 2; // where content SHOULD be after fix
                    list_stack.push((visual_marker_column, line_idx, false, expected_content_visual_col));

                    // Skip first level check if start_indented is false
                    // BUT always check items with 1 space indent (insufficient for nesting)
                    if !self.config.start_indented && nesting_level == 0 && visual_marker_column != 1 {
                        continue;
                    }

                    if visual_marker_column != expected_indent {
                        // Generate fix for this list item
                        let fix = {
                            let correct_indent = " ".repeat(expected_indent);

                            // Build the replacement string - need to preserve everything before the list marker
                            // For blockquoted lines, this includes the blockquote prefix
                            let replacement = if line_info.blockquote.is_some() {
                                // Count the blockquote markers
                                let mut blockquote_count = 0;
                                for ch in line_info.content.chars() {
                                    if ch == '>' {
                                        blockquote_count += 1;
                                    } else if ch != ' ' && ch != '\t' {
                                        break;
                                    }
                                }
                                // Build the blockquote prefix (one '>' per level, with spaces between for nested)
                                let blockquote_prefix = if blockquote_count > 1 {
                                    (0..blockquote_count)
                                        .map(|_| "> ")
                                        .collect::<String>()
                                        .trim_end()
                                        .to_string()
                                } else {
                                    ">".to_string()
                                };
                                // Add correct indentation after the blockquote prefix
                                // Include one space after the blockquote marker(s) as part of the indent
                                format!("{blockquote_prefix} {correct_indent}")
                            } else {
                                correct_indent
                            };

                            // Calculate the byte positions
                            // The range should cover from start of line to the marker position
                            let start_byte = line_info.byte_offset;
                            let mut end_byte = line_info.byte_offset;

                            // Calculate where the marker starts
                            for (i, ch) in line_info.content.chars().enumerate() {
                                if i >= list_item.marker_column {
                                    break;
                                }
                                end_byte += ch.len_utf8();
                            }

                            Some(crate::rule::Fix {
                                range: start_byte..end_byte,
                                replacement,
                            })
                        };

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            message: format!(
                                "Expected {expected_indent} spaces for indent depth {nesting_level}, found {visual_marker_column}"
                            ),
                            line: line_idx + 1, // Convert to 1-indexed
                            column: 1,          // Start of line
                            end_line: line_idx + 1,
                            end_column: visual_marker_column + 1, // End of visual indentation
                            severity: Severity::Warning,
                            fix,
                        });
                    }
                }
            }
        }
        Ok(warnings)
    }

    /// Optimized check using document structure
    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Get all warnings with their fixes
        let warnings = self.check(ctx)?;

        // If no warnings, return original content
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Collect all fixes and sort by range start (descending) to apply from end to beginning
        let mut fixes: Vec<_> = warnings
            .iter()
            .filter_map(|w| w.fix.as_ref().map(|f| (f.range.start, f.range.end, &f.replacement)))
            .collect();
        fixes.sort_by(|a, b| b.0.cmp(&a.0));

        // Apply fixes from end to beginning to preserve byte offsets
        let mut result = ctx.content.to_string();
        for (start, end, replacement) in fixes {
            if start < result.len() && end <= result.len() && start <= end {
                result.replace_range(start..end, replacement);
            }
        }

        Ok(result)
    }

    /// Get the category of this rule for selective processing
    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Fast path: check if document likely has lists
        if ctx.content.is_empty() || !ctx.likely_has_lists() {
            return true;
        }
        // Verify unordered list items actually exist
        !ctx.lines
            .iter()
            .any(|line| line.list_item.as_ref().is_some_and(|item| !item.is_ordered))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD007Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD007Config::RULE_NAME.to_string(), toml::Value::Table(table)))
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
        let mut rule_config = crate::rule_config_serde::load_rule_config::<MD007Config>(config);

        // For markdownlint compatibility: if indent is explicitly configured and style is not,
        // default to "fixed" style (markdownlint behavior) instead of "text-aligned"
        if let Some(rule_cfg) = config.rules.get("MD007") {
            let has_explicit_indent = rule_cfg.values.contains_key("indent");
            let has_explicit_style = rule_cfg.values.contains_key("style");

            if has_explicit_indent && !has_explicit_style && rule_config.indent != 2 {
                // User set indent explicitly but not style, and it's not the default value
                // Use fixed style for markdownlint compatibility
                rule_config.style = md007_config::IndentStyle::Fixed;
            }
        }

        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::rule::Rule;

    #[test]
    fn test_valid_list_indent() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for valid indentation, but got {} warnings",
            result.len()
        );
    }

    #[test]
    fn test_invalid_list_indent() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n   * Item 2\n      * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2);
        assert_eq!(result[0].column, 1);
        assert_eq!(result[1].line, 3);
        assert_eq!(result[1].column, 1);
    }

    #[test]
    fn test_mixed_indentation() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n  * Item 2\n   * Item 3\n  * Item 4";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert_eq!(result[0].column, 1);
    }

    #[test]
    fn test_fix_indentation() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n   * Item 2\n      * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.fix(&ctx).unwrap();
        // With text-aligned style and non-cascade:
        // Item 2 aligns with Item 1's text (2 spaces)
        // Item 3 aligns with Item 2's expected text position (4 spaces)
        let expected = "* Item 1\n  * Item 2\n    * Item 3";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_md007_in_yaml_code_block() {
        let rule = MD007ULIndent::default();
        let content = r#"```yaml
repos:
-   repo: https://github.com/rvben/rumdl
    rev: v0.5.0
    hooks:
    -   id: rumdl-check
```"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD007 should not trigger inside a code block, but got warnings: {result:?}"
        );
    }

    #[test]
    fn test_blockquoted_list_indent() {
        let rule = MD007ULIndent::default();
        let content = "> * Item 1\n>   * Item 2\n>     * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for valid blockquoted list indentation, but got {result:?}"
        );
    }

    #[test]
    fn test_blockquoted_list_invalid_indent() {
        let rule = MD007ULIndent::default();
        let content = "> * Item 1\n>    * Item 2\n>       * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "Expected 2 warnings for invalid blockquoted list indentation, got {result:?}"
        );
        assert_eq!(result[0].line, 2);
        assert_eq!(result[1].line, 3);
    }

    #[test]
    fn test_nested_blockquote_list_indent() {
        let rule = MD007ULIndent::default();
        let content = "> > * Item 1\n> >   * Item 2\n> >     * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for valid nested blockquoted list indentation, but got {result:?}"
        );
    }

    #[test]
    fn test_blockquote_list_with_code_block() {
        let rule = MD007ULIndent::default();
        let content = "> * Item 1\n>   * Item 2\n>   ```\n>   code\n>   ```\n>   * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MD007 should not trigger inside a code block within a blockquote, but got warnings: {result:?}"
        );
    }

    #[test]
    fn test_properly_indented_lists() {
        let rule = MD007ULIndent::default();

        // Test various properly indented lists
        let test_cases = vec![
            "* Item 1\n* Item 2",
            "* Item 1\n  * Item 1.1\n    * Item 1.1.1",
            "- Item 1\n  - Item 1.1",
            "+ Item 1\n  + Item 1.1",
            "* Item 1\n  * Item 1.1\n* Item 2\n  * Item 2.1",
        ];

        for content in test_cases {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Expected no warnings for properly indented list:\n{}\nGot {} warnings",
                content,
                result.len()
            );
        }
    }

    #[test]
    fn test_under_indented_lists() {
        let rule = MD007ULIndent::default();

        let test_cases = vec![
            ("* Item 1\n * Item 1.1", 1, 2),                   // Expected 2 spaces, got 1
            ("* Item 1\n  * Item 1.1\n   * Item 1.1.1", 1, 3), // Expected 4 spaces, got 3
        ];

        for (content, expected_warnings, line) in test_cases {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.len(),
                expected_warnings,
                "Expected {expected_warnings} warnings for under-indented list:\n{content}"
            );
            if expected_warnings > 0 {
                assert_eq!(result[0].line, line);
            }
        }
    }

    #[test]
    fn test_over_indented_lists() {
        let rule = MD007ULIndent::default();

        let test_cases = vec![
            ("* Item 1\n   * Item 1.1", 1, 2),                   // Expected 2 spaces, got 3
            ("* Item 1\n    * Item 1.1", 1, 2),                  // Expected 2 spaces, got 4
            ("* Item 1\n  * Item 1.1\n     * Item 1.1.1", 1, 3), // Expected 4 spaces, got 5
        ];

        for (content, expected_warnings, line) in test_cases {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(
                result.len(),
                expected_warnings,
                "Expected {expected_warnings} warnings for over-indented list:\n{content}"
            );
            if expected_warnings > 0 {
                assert_eq!(result[0].line, line);
            }
        }
    }

    #[test]
    fn test_custom_indent_2_spaces() {
        let rule = MD007ULIndent::new(2); // Default
        let content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_custom_indent_3_spaces() {
        // Test dynamic alignment behavior (default start_indented=false)
        let rule = MD007ULIndent::new(3);

        let content = "* Item 1\n   * Item 2\n      * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // With dynamic alignment, Item 2 should align with Item 1's text (2 spaces)
        // and Item 3 should align with Item 2's text (4 spaces), not fixed increments
        assert!(!result.is_empty()); // Should have warnings due to alignment

        // Test that dynamic alignment works correctly
        // Item 3 should align with Item 2's text content (4 spaces)
        let correct_content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(correct_content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_custom_indent_4_spaces() {
        // Test dynamic alignment behavior (default start_indented=false)
        let rule = MD007ULIndent::new(4);
        let content = "* Item 1\n    * Item 2\n        * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // With dynamic alignment, should expect 2 spaces and 6 spaces, not 4 and 8
        assert!(!result.is_empty()); // Should have warnings due to alignment

        // Test correct dynamic alignment
        // Item 3 should align with Item 2's text content (4 spaces)
        let correct_content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(correct_content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_tab_indentation() {
        let rule = MD007ULIndent::default();

        // Single tab
        let content = "* Item 1\n\t* Item 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Tab indentation should trigger warning");

        // Fix should convert tab to spaces
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n  * Item 2");

        // Multiple tabs
        let content_multi = "* Item 1\n\t* Item 2\n\t\t* Item 3";
        let ctx = LintContext::new(content_multi, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        // With non-cascade: Item 2 at 2 spaces, content at 4
        // Item 3 aligns with Item 2's expected content at 4 spaces
        assert_eq!(fixed, "* Item 1\n  * Item 2\n    * Item 3");

        // Mixed tabs and spaces
        let content_mixed = "* Item 1\n \t* Item 2\n\t * Item 3";
        let ctx = LintContext::new(content_mixed, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        // With non-cascade: Item 2 at 2 spaces, content at 4
        // Item 3 aligns with Item 2's expected content at 4 spaces
        assert_eq!(fixed, "* Item 1\n  * Item 2\n    * Item 3");
    }

    #[test]
    fn test_mixed_ordered_unordered_lists() {
        let rule = MD007ULIndent::default();

        // MD007 only checks unordered lists, so ordered lists should be ignored
        // Note: 3 spaces is now correct for bullets under ordered items
        let content = r#"1. Ordered item
   * Unordered sub-item (correct - 3 spaces under ordered)
   2. Ordered sub-item
* Unordered item
  1. Ordered sub-item
  * Unordered sub-item"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 0, "All unordered list indentation should be correct");

        // No fix needed as all indentation is correct
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    #[test]
    fn test_list_markers_variety() {
        let rule = MD007ULIndent::default();

        // Test all three unordered list markers
        let content = r#"* Asterisk
  * Nested asterisk
- Hyphen
  - Nested hyphen
+ Plus
  + Nested plus"#;

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "All unordered list markers should work with proper indentation"
        );

        // Test with wrong indentation for each marker type
        let wrong_content = r#"* Asterisk
   * Wrong asterisk
- Hyphen
 - Wrong hyphen
+ Plus
    + Wrong plus"#;

        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3, "All marker types should be checked for indentation");
    }

    #[test]
    fn test_empty_list_items() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n* \n  * Item 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Empty list items should not affect indentation checks"
        );
    }

    #[test]
    fn test_list_with_code_blocks() {
        let rule = MD007ULIndent::default();
        let content = r#"* Item 1
  ```
  code
  ```
  * Item 2
    * Item 3"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_in_front_matter() {
        let rule = MD007ULIndent::default();
        let content = r#"---
tags:
  - tag1
  - tag2
---
* Item 1
  * Item 2"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Lists in YAML front matter should be ignored");
    }

    #[test]
    fn test_fix_preserves_content() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1 with **bold** and *italic*\n   * Item 2 with `code`\n     * Item 3 with [link](url)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        // With non-cascade: Item 2 at 2 spaces, content at 4
        // Item 3 aligns with Item 2's expected content at 4 spaces
        let expected = "* Item 1 with **bold** and *italic*\n  * Item 2 with `code`\n    * Item 3 with [link](url)";
        assert_eq!(fixed, expected, "Fix should only change indentation, not content");
    }

    #[test]
    fn test_start_indented_config() {
        let config = MD007Config {
            start_indented: true,
            start_indent: 4,
            indent: 2,
            style: md007_config::IndentStyle::TextAligned,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // First level should be indented by start_indent (4 spaces)
        // Level 0: 4 spaces (start_indent)
        // Level 1: 6 spaces (start_indent + indent = 4 + 2)
        // Level 2: 8 spaces (start_indent + 2*indent = 4 + 4)
        let content = "    * Item 1\n      * Item 2\n        * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings with start_indented config");

        // Wrong first level indentation
        let wrong_content = "  * Item 1\n    * Item 2";
        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].message, "Expected 4 spaces for indent depth 0, found 2");
        assert_eq!(result[1].line, 2);
        assert_eq!(result[1].message, "Expected 6 spaces for indent depth 1, found 4");

        // Fix should correct to start_indent for first level
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "    * Item 1\n      * Item 2");
    }

    #[test]
    fn test_start_indented_false_allows_any_first_level() {
        let rule = MD007ULIndent::default(); // start_indented is false by default

        // When start_indented is false, first level items at any indentation are allowed
        let content = "   * Item 1"; // First level at 3 spaces
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "First level at any indentation should be allowed when start_indented is false"
        );

        // Multiple first level items at different indentations should all be allowed
        let content = "* Item 1\n  * Item 2\n    * Item 3"; // All at level 0 (different indents)
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "All first-level items should be allowed at any indentation"
        );
    }

    #[test]
    fn test_deeply_nested_lists() {
        let rule = MD007ULIndent::default();
        let content = r#"* L1
  * L2
    * L3
      * L4
        * L5
          * L6"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with wrong deep nesting
        let wrong_content = r#"* L1
  * L2
    * L3
      * L4
         * L5
            * L6"#;
        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Deep nesting errors should be detected");
    }

    #[test]
    fn test_excessive_indentation_detected() {
        let rule = MD007ULIndent::default();

        // Test excessive indentation (5 spaces instead of 2)
        let content = "- Item 1\n     - Item 2 with 5 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should detect excessive indentation (5 instead of 2)");
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("Expected 2 spaces"));
        assert!(result[0].message.contains("found 5"));

        // Test slightly excessive indentation (3 spaces instead of 2)
        let content = "- Item 1\n   - Item 2 with 3 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Should detect slightly excessive indentation (3 instead of 2)"
        );
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("Expected 2 spaces"));
        assert!(result[0].message.contains("found 3"));

        // Test insufficient indentation (1 space is treated as level 0, should be 0)
        let content = "- Item 1\n - Item 2 with 1 space";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Should detect 1-space indent (insufficient for nesting, expected 0)"
        );
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("Expected 0 spaces"));
        assert!(result[0].message.contains("found 1"));
    }

    #[test]
    fn test_excessive_indentation_with_4_space_config() {
        let rule = MD007ULIndent::new(4);

        // Test excessive indentation (5 spaces instead of 4) - like Ruff's versioning.md
        let content = "- Formatter:\n     - The stable style changed";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Due to text-aligned style, the expected indent should be 2 (aligning with "Formatter" text)
        // But with 5 spaces, it's wrong
        assert!(
            !result.is_empty(),
            "Should detect 5 spaces when expecting proper alignment"
        );

        // Test with correct alignment
        let correct_content = "- Formatter:\n  - The stable style changed";
        let ctx = LintContext::new(correct_content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should accept correct text alignment");
    }

    #[test]
    fn test_bullets_nested_under_numbered_items() {
        let rule = MD007ULIndent::default();
        let content = "\
1. **Active Directory/LDAP**
   - User authentication and directory services
   - LDAP for user information and validation

2. **Oracle Unified Directory (OUD)**
   - Extended user directory services";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Should have no warnings - 3 spaces is correct for bullets under numbered items
        assert!(
            result.is_empty(),
            "Expected no warnings for bullets with 3 spaces under numbered items, got: {result:?}"
        );
    }

    #[test]
    fn test_bullets_nested_under_numbered_items_wrong_indent() {
        let rule = MD007ULIndent::default();
        let content = "\
1. **Active Directory/LDAP**
  - Wrong: only 2 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Should flag incorrect indentation
        assert_eq!(
            result.len(),
            1,
            "Expected warning for incorrect indentation under numbered items"
        );
        assert!(
            result
                .iter()
                .any(|w| w.line == 2 && w.message.contains("Expected 3 spaces"))
        );
    }

    #[test]
    fn test_regular_bullet_nesting_still_works() {
        let rule = MD007ULIndent::default();
        let content = "\
* Top level
  * Nested bullet (2 spaces is correct)
    * Deeply nested (4 spaces)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Should have no warnings - standard bullet nesting still uses 2-space increments
        assert!(
            result.is_empty(),
            "Expected no warnings for standard bullet nesting, got: {result:?}"
        );
    }
}
