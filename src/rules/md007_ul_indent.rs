/// Rule MD007: Unordered list indentation
///
/// See [docs/md007.md](../../docs/md007.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;

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
                indent: crate::types::IndentSize::from_const(indent as u8),
                start_indented: false,
                start_indent: crate::types::IndentSize::from_const(2),
                style: md007_config::IndentStyle::TextAligned,
                style_explicit: false,  // Allow auto-detection for programmatic construction
                indent_explicit: false, // Programmatic construction uses default behavior
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

    /// Calculate expected indentation for a nested list item.
    ///
    /// This uses per-parent logic rather than document-wide style selection:
    /// - When parent is **ordered**: align with parent's text (handles variable-width markers)
    /// - When parent is **unordered**: use configured indent (fixed-width markers)
    ///
    /// If user explicitly sets `style`, that choice is respected uniformly.
    /// "Do What I Mean" behavior: if user sets `indent` but not `style`, use fixed style.
    fn calculate_expected_indent(
        &self,
        nesting_level: usize,
        parent_info: Option<(bool, usize)>, // (is_ordered, content_visual_col)
    ) -> usize {
        if nesting_level == 0 {
            return 0;
        }

        // If user explicitly set style, respect their choice uniformly
        if self.config.style_explicit {
            return match self.config.style {
                md007_config::IndentStyle::Fixed => nesting_level * self.config.indent.get() as usize,
                md007_config::IndentStyle::TextAligned => {
                    parent_info.map_or(nesting_level * 2, |(_, content_col)| content_col)
                }
            };
        }

        // "Do What I Mean": if indent is explicitly set (but style is not), use fixed style
        // This is the expected behavior when users configure `indent = 4` - they want 4-space increments
        // BUT: bullets under ordered lists still need text-aligned because ordered markers have variable width
        if self.config.indent_explicit {
            match parent_info {
                Some((true, parent_content_col)) => {
                    // Parent is ordered: even with explicit indent, use text-aligned
                    // Ordered markers have variable width ("1." vs "10." vs "100.")
                    return parent_content_col;
                }
                _ => {
                    // Parent is unordered or no parent: use fixed indent
                    return nesting_level * self.config.indent.get() as usize;
                }
            }
        }

        // Smart default: per-parent type decision
        match parent_info {
            Some((true, parent_content_col)) => {
                // Parent is ordered: align with parent's text position
                // This handles variable-width markers ("1." vs "10." vs "100.")
                parent_content_col
            }
            Some((false, parent_content_col)) => {
                // Parent is unordered: check if it's at the expected fixed position
                // If yes, continue with fixed style (for pure unordered lists)
                // If no, parent is offset (e.g., inside ordered list), use text-aligned
                let parent_level = nesting_level.saturating_sub(1);
                let expected_parent_marker = parent_level * self.config.indent.get() as usize;
                // Parent's marker column is content column minus marker width (2 for "- ")
                let parent_marker_col = parent_content_col.saturating_sub(2);

                if parent_marker_col == expected_parent_marker {
                    // Parent is at expected fixed position, continue with fixed style
                    nesting_level * self.config.indent.get() as usize
                } else {
                    // Parent is offset, use text-aligned
                    parent_content_col
                }
            }
            None => {
                // No parent found (shouldn't happen at nesting_level > 0)
                nesting_level * self.config.indent.get() as usize
            }
        }
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
                    // Find the position after ALL blockquote prefixes (handles nested > > > etc)
                    let line_content = line_info.content(ctx.content);
                    let mut remaining = line_content;
                    let mut content_start = 0;

                    loop {
                        let trimmed = remaining.trim_start();
                        if !trimmed.starts_with('>') {
                            break;
                        }
                        // Account for leading whitespace
                        content_start += remaining.len() - trimmed.len();
                        // Account for '>'
                        content_start += 1;
                        let after_gt = &trimmed[1..];
                        // Handle optional whitespace after '>' (space or tab)
                        if let Some(stripped) = after_gt.strip_prefix(' ') {
                            content_start += 1;
                            remaining = stripped;
                        } else if let Some(stripped) = after_gt.strip_prefix('\t') {
                            content_start += 1;
                            remaining = stripped;
                        } else {
                            remaining = after_gt;
                        }
                    }

                    // Extract the content after the blockquote prefix
                    let content_after_prefix = &line_content[content_start..];
                    // Adjust the marker column to be relative to the content after the prefix
                    let adjusted_col = if list_item.marker_column >= content_start {
                        list_item.marker_column - content_start
                    } else {
                        // This shouldn't happen, but handle it gracefully
                        list_item.marker_column
                    };
                    (content_after_prefix.to_string(), adjusted_col)
                } else {
                    (line_info.content(ctx.content).to_string(), list_item.marker_column)
                };

                // Convert marker position to visual column
                let visual_marker_column =
                    Self::char_pos_to_visual_column(&content_for_calculation, adjusted_marker_column);

                // Calculate content visual column for text-aligned style
                let visual_content_column = if line_info.blockquote.is_some() {
                    // For blockquoted content, we already have the adjusted content
                    let adjusted_content_col =
                        if list_item.content_column >= (line_info.byte_len - content_for_calculation.len()) {
                            list_item.content_column - (line_info.byte_len - content_for_calculation.len())
                        } else {
                            list_item.content_column
                        };
                    Self::char_pos_to_visual_column(&content_for_calculation, adjusted_content_col)
                } else {
                    Self::char_pos_to_visual_column(line_info.content(ctx.content), list_item.content_column)
                };

                // For nesting detection, treat 1-space indent as if it's at column 0
                // because 1 space is insufficient to establish a nesting relationship
                // UNLESS the user has explicitly configured indent=1, in which case 1 space IS valid nesting
                let visual_marker_for_nesting = if visual_marker_column == 1 && self.config.indent.get() != 1 {
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

                // At this point, we know this is an unordered list item
                // Now stack contains only parent items
                let nesting_level = list_stack.len();

                // Get parent info for per-parent calculation
                let parent_info = list_stack
                    .get(nesting_level.wrapping_sub(1))
                    .map(|&(_, _, is_ordered, content_col)| (is_ordered, content_col));

                // Calculate expected indent using per-parent logic
                let expected_indent = if self.config.start_indented {
                    self.config.start_indent.get() as usize + (nesting_level * self.config.indent.get() as usize)
                } else {
                    self.calculate_expected_indent(nesting_level, parent_info)
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
                            for ch in line_info.content(ctx.content).chars() {
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
                        for (i, ch) in line_info.content(ctx.content).chars().enumerate() {
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
                        rule_name: Some(self.name().to_string()),
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

        // Check if style and/or indent were explicitly set in the config
        if let Some(rule_cfg) = config.rules.get("MD007") {
            rule_config.style_explicit = rule_cfg.values.contains_key("style");
            rule_config.indent_explicit = rule_cfg.values.contains_key("indent");

            // Warn if both indent and text-aligned style are explicitly set
            // This combination is contradictory: indent implies fixed increments,
            // but text-aligned ignores the indent value and aligns with parent text
            if rule_config.indent_explicit
                && rule_config.style_explicit
                && rule_config.style == md007_config::IndentStyle::TextAligned
            {
                eprintln!(
                    "\x1b[33m[config warning]\x1b[0m MD007: 'indent' has no effect when 'style = \"text-aligned\"'. \
                     Text-aligned style ignores indent and aligns nested items with parent text. \
                     To use fixed {} space increments, either remove 'style' or set 'style = \"fixed\"'.",
                    rule_config.indent.get()
                );
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3);
        assert_eq!(result[0].column, 1);
    }

    #[test]
    fn test_fix_indentation() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n   * Item 2\n      * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_custom_indent_3_spaces() {
        // With smart auto-detection, pure unordered lists with indent=3 use fixed style
        // This provides markdownlint compatibility for the common case
        let rule = MD007ULIndent::new(3);

        // Fixed style with indent=3: level 0 = 0, level 1 = 3, level 2 = 6
        let correct_content = "* Item 1\n   * Item 2\n      * Item 3";
        let ctx = LintContext::new(correct_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Fixed style expects 0, 3, 6 spaces but got: {result:?}"
        );

        // Wrong indentation (text-aligned style spacing)
        let wrong_content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty(), "Should warn: expected 3 spaces, found 2");
    }

    #[test]
    fn test_custom_indent_4_spaces() {
        // With smart auto-detection, pure unordered lists with indent=4 use fixed style
        // This provides markdownlint compatibility (fixes issue #210)
        let rule = MD007ULIndent::new(4);

        // Fixed style with indent=4: level 0 = 0, level 1 = 4, level 2 = 8
        let correct_content = "* Item 1\n    * Item 2\n        * Item 3";
        let ctx = LintContext::new(correct_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Fixed style expects 0, 4, 8 spaces but got: {result:?}"
        );

        // Wrong indentation (text-aligned style spacing)
        let wrong_content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty(), "Should warn: expected 4 spaces, found 2");
    }

    #[test]
    fn test_tab_indentation() {
        let rule = MD007ULIndent::default();

        // Note: Tab at line start = 4 spaces = indented code per CommonMark, not a list item
        // MD007 checks list indentation, so this test now checks actual nested lists
        // Hard tabs within lists should be caught by MD010, not MD007

        // Single wrong indentation (3 spaces instead of 2)
        let content = "* Item 1\n   * Item 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Wrong indentation should trigger warning");

        // Fix should correct to 2 spaces
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n  * Item 2");

        // Multiple indentation errors
        let content_multi = "* Item 1\n   * Item 2\n      * Item 3";
        let ctx = LintContext::new(content_multi, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // With non-cascade: Item 2 at 2 spaces, content at 4
        // Item 3 aligns with Item 2's expected content at 4 spaces
        assert_eq!(fixed, "* Item 1\n  * Item 2\n    * Item 3");

        // Mixed wrong indentations
        let content_mixed = "* Item 1\n   * Item 2\n     * Item 3";
        let ctx = LintContext::new(content_mixed, crate::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 3, "All marker types should be checked for indentation");
    }

    #[test]
    fn test_empty_list_items() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1\n* \n  * Item 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Lists in YAML front matter should be ignored");
    }

    #[test]
    fn test_fix_preserves_content() {
        let rule = MD007ULIndent::default();
        let content = "* Item 1 with **bold** and *italic*\n   * Item 2 with `code`\n     * Item 3 with [link](url)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
            start_indent: crate::types::IndentSize::from_const(4),
            indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: true, // Explicit style for this test
            indent_explicit: false,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // First level should be indented by start_indent (4 spaces)
        // Level 0: 4 spaces (start_indent)
        // Level 1: 6 spaces (start_indent + indent = 4 + 2)
        // Level 2: 8 spaces (start_indent + 2*indent = 4 + 4)
        let content = "    * Item 1\n      * Item 2\n        * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings with start_indented config");

        // Wrong first level indentation
        let wrong_content = "  * Item 1\n    * Item 2";
        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "First level at any indentation should be allowed when start_indented is false"
        );

        // Multiple first level items at different indentations should all be allowed
        let content = "* Item 1\n  * Item 2\n    * Item 3"; // All at level 0 (different indents)
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with wrong deep nesting
        let wrong_content = r#"* L1
  * L2
    * L3
      * L4
         * L5
            * L6"#;
        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 2, "Deep nesting errors should be detected");
    }

    #[test]
    fn test_excessive_indentation_detected() {
        let rule = MD007ULIndent::default();

        // Test excessive indentation (5 spaces instead of 2)
        let content = "- Item 1\n     - Item 2 with 5 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should detect excessive indentation (5 instead of 2)");
        assert_eq!(result[0].line, 2);
        assert!(result[0].message.contains("Expected 2 spaces"));
        assert!(result[0].message.contains("found 5"));

        // Test slightly excessive indentation (3 spaces instead of 2)
        let content = "- Item 1\n   - Item 2 with 3 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        // With smart auto-detection, pure unordered lists use fixed style
        // Fixed style with indent=4: level 0 = 0, level 1 = 4, level 2 = 8
        let rule = MD007ULIndent::new(4);

        // Test excessive indentation (5 spaces instead of 4)
        let content = "- Formatter:\n     - The stable style changed";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "Should detect 5 spaces when expecting 4 (fixed style)"
        );

        // Test with correct fixed style alignment (4 spaces for level 1)
        let correct_content = "- Formatter:\n    - The stable style changed";
        let ctx = LintContext::new(correct_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should accept correct fixed style indent (4 spaces)");
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should have no warnings - standard bullet nesting still uses 2-space increments
        assert!(
            result.is_empty(),
            "Expected no warnings for standard bullet nesting, got: {result:?}"
        );
    }

    #[test]
    fn test_blockquote_with_tab_after_marker() {
        let rule = MD007ULIndent::default();
        let content = ">\t* List item\n>\t  * Nested\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Tab after blockquote marker should be handled correctly, got: {result:?}"
        );
    }

    #[test]
    fn test_blockquote_with_space_then_tab_after_marker() {
        let rule = MD007ULIndent::default();
        let content = "> \t* List item\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // First-level list item at any indentation is allowed when start_indented=false (default)
        assert!(
            result.is_empty(),
            "First-level list item at any indentation is allowed when start_indented=false, got: {result:?}"
        );
    }

    #[test]
    fn test_blockquote_with_multiple_tabs() {
        let rule = MD007ULIndent::default();
        let content = ">\t\t* List item\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // First-level list item at any indentation is allowed when start_indented=false (default)
        assert!(
            result.is_empty(),
            "First-level list item at any indentation is allowed when start_indented=false, got: {result:?}"
        );
    }

    #[test]
    fn test_nested_blockquote_with_tab() {
        let rule = MD007ULIndent::default();
        let content = ">\t>\t* List item\n>\t>\t  * Nested\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Nested blockquotes with tabs should work correctly, got: {result:?}"
        );
    }

    // Tests for smart style auto-detection (fixes issue #210 while preserving #209 fix)

    #[test]
    fn test_smart_style_pure_unordered_uses_fixed() {
        // Issue #210: Pure unordered lists with custom indent should use fixed style
        let rule = MD007ULIndent::new(4);

        // With fixed style (auto-detected), this should be valid
        let content = "* Level 0\n    * Level 1\n        * Level 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Pure unordered with indent=4 should use fixed style (0, 4, 8), got: {result:?}"
        );
    }

    #[test]
    fn test_smart_style_mixed_lists_uses_text_aligned() {
        // Issue #209: Mixed lists should use text-aligned to avoid oscillation
        let rule = MD007ULIndent::new(4);

        // With text-aligned style (auto-detected for mixed), bullets align with parent text
        let content = "1. Ordered\n   * Bullet aligns with 'Ordered' text (3 spaces)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Mixed lists should use text-aligned style, got: {result:?}"
        );
    }

    #[test]
    fn test_smart_style_explicit_fixed_overrides() {
        // When style is explicitly set to fixed, it should be respected even for mixed lists
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::Fixed,
            style_explicit: true, // Explicit setting
            indent_explicit: false,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // With explicit fixed style, expect fixed calculations even for mixed lists
        let content = "1. Ordered\n    * Should be at 4 spaces (fixed)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // The bullet is at 4 spaces which matches fixed style level 1
        assert!(
            result.is_empty(),
            "Explicit fixed style should be respected, got: {result:?}"
        );
    }

    #[test]
    fn test_smart_style_explicit_text_aligned_overrides() {
        // When style is explicitly set to text-aligned, it should be respected
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: true, // Explicit setting
            indent_explicit: false,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // With explicit text-aligned, pure unordered should use text-aligned (not auto-switch to fixed)
        let content = "* Level 0\n  * Level 1 (aligned with 'Level 0' text)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Explicit text-aligned should be respected, got: {result:?}"
        );

        // This would be correct for fixed but wrong for text-aligned
        let fixed_style_content = "* Level 0\n    * Level 1 (4 spaces - fixed style)";
        let ctx = LintContext::new(fixed_style_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "With explicit text-aligned, 4-space indent should be wrong (expected 2)"
        );
    }

    #[test]
    fn test_smart_style_default_indent_no_autoswitch() {
        // When indent is default (2), no auto-switch happens (both styles produce same result)
        let rule = MD007ULIndent::new(2);

        let content = "* Level 0\n  * Level 1\n    * Level 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Default indent should work regardless of style, got: {result:?}"
        );
    }

    #[test]
    fn test_has_mixed_list_nesting_detection() {
        // Test the mixed list detection function directly

        // Pure unordered - no mixed nesting
        let content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !ctx.has_mixed_list_nesting(),
            "Pure unordered should not be detected as mixed"
        );

        // Pure ordered - no mixed nesting
        let content = "1. Item 1\n   2. Item 2\n      3. Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !ctx.has_mixed_list_nesting(),
            "Pure ordered should not be detected as mixed"
        );

        // Mixed: unordered under ordered
        let content = "1. Ordered\n   * Unordered child";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            ctx.has_mixed_list_nesting(),
            "Unordered under ordered should be detected as mixed"
        );

        // Mixed: ordered under unordered
        let content = "* Unordered\n  1. Ordered child";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            ctx.has_mixed_list_nesting(),
            "Ordered under unordered should be detected as mixed"
        );

        // Separate lists (not nested) - not mixed
        let content = "* Unordered\n\n1. Ordered (separate list)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !ctx.has_mixed_list_nesting(),
            "Separate lists should not be detected as mixed"
        );

        // Mixed lists inside blockquotes should be detected
        let content = "> 1. Ordered in blockquote\n>    * Unordered child";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            ctx.has_mixed_list_nesting(),
            "Mixed lists in blockquotes should be detected"
        );
    }

    #[test]
    fn test_issue_210_exact_reproduction() {
        // Exact reproduction from issue #210
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned, // Default
            style_explicit: false,                         // Not explicitly set - should auto-detect
            indent_explicit: false,                        // Not explicitly set
        };
        let rule = MD007ULIndent::from_config_struct(config);

        let content = "# Title\n\n* some\n    * list\n    * items\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Issue #210: indent=4 on pure unordered should work (auto-fixed style), got: {result:?}"
        );
    }

    #[test]
    fn test_issue_209_still_fixed() {
        // Verify issue #209 (oscillation) is still fixed when style is explicitly set
        // With issue #236 fix, explicit style must be set to get pure text-aligned behavior
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(3),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: true, // Explicit style to test text-aligned behavior
            indent_explicit: false,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // Mixed list from issue #209 - with explicit text-aligned, no oscillation
        let content = r#"# Header 1

- **Second item**:
  - **This is a nested list**:
    1. **First point**
       - First subpoint
"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert!(
            result.is_empty(),
            "Issue #209: With explicit text-aligned style, should have no issues, got: {result:?}"
        );
    }

    // Edge case tests for review findings

    #[test]
    fn test_multi_level_mixed_detection_grandparent() {
        // Test that multi-level mixed detection finds grandparent type differences
        // ordered → unordered → unordered should be detected as mixed
        // because the grandparent (ordered) is different from descendants (unordered)
        let content = "1. Ordered grandparent\n   * Unordered child\n     * Unordered grandchild";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            ctx.has_mixed_list_nesting(),
            "Should detect mixed nesting when grandparent differs in type"
        );

        // unordered → ordered → ordered should also be detected as mixed
        let content = "* Unordered grandparent\n  1. Ordered child\n     2. Ordered grandchild";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            ctx.has_mixed_list_nesting(),
            "Should detect mixed nesting for ordered descendants under unordered"
        );
    }

    #[test]
    fn test_html_comments_skipped_in_detection() {
        // Lists inside HTML comments should not affect mixed detection
        let content = r#"* Unordered list
<!-- This is a comment
  1. This ordered list is inside a comment
     * This nested bullet is also inside
-->
  * Another unordered item"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !ctx.has_mixed_list_nesting(),
            "Lists in HTML comments should be ignored in mixed detection"
        );
    }

    #[test]
    fn test_blank_lines_separate_lists() {
        // Blank lines at root level should separate lists, treating them as independent
        let content = "* First unordered list\n\n1. Second list is ordered (separate)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !ctx.has_mixed_list_nesting(),
            "Blank line at root should separate lists"
        );

        // But nested lists after blank should still be detected if mixed
        let content = "1. Ordered parent\n\n   * Still a child due to indentation";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            ctx.has_mixed_list_nesting(),
            "Indented list after blank is still nested"
        );
    }

    #[test]
    fn test_column_1_normalization() {
        // 1-space indent should be treated as column 0 (root level)
        // This creates a sibling relationship, not nesting
        let content = "* First item\n * Second item with 1 space (sibling)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let rule = MD007ULIndent::default();
        let result = rule.check(&ctx).unwrap();
        // The second item should be flagged as wrong (1 space is not valid for nesting)
        assert!(
            result.iter().any(|w| w.line == 2),
            "1-space indent should be flagged as incorrect"
        );
    }

    #[test]
    fn test_code_blocks_skipped_in_detection() {
        // Lists inside code blocks should not affect mixed detection
        let content = r#"* Unordered list
```
1. This ordered list is inside a code block
   * This nested bullet is also inside
```
  * Another unordered item"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !ctx.has_mixed_list_nesting(),
            "Lists in code blocks should be ignored in mixed detection"
        );
    }

    #[test]
    fn test_front_matter_skipped_in_detection() {
        // Lists inside YAML front matter should not affect mixed detection
        let content = r#"---
items:
  - yaml list item
  - another item
---
* Unordered list after front matter"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !ctx.has_mixed_list_nesting(),
            "Lists in front matter should be ignored in mixed detection"
        );
    }

    #[test]
    fn test_alternating_types_at_same_level() {
        // Alternating between ordered and unordered at the same nesting level
        // is NOT mixed nesting (they are siblings, not parent-child)
        let content = "* First bullet\n1. First number\n* Second bullet\n2. Second number";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !ctx.has_mixed_list_nesting(),
            "Alternating types at same level should not be detected as mixed"
        );
    }

    #[test]
    fn test_five_level_deep_mixed_nesting() {
        // Test detection at 5+ levels of nesting
        let content = "* L0\n  1. L1\n     * L2\n       1. L3\n          * L4\n            1. L5";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(ctx.has_mixed_list_nesting(), "Should detect mixed nesting at 5+ levels");
    }

    #[test]
    fn test_very_deep_pure_unordered_nesting() {
        // Test pure unordered list with 10+ levels of nesting
        let mut content = String::from("* L1");
        for level in 2..=12 {
            let indent = "  ".repeat(level - 1);
            content.push_str(&format!("\n{indent}* L{level}"));
        }

        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);

        // Should NOT be detected as mixed (all unordered)
        assert!(
            !ctx.has_mixed_list_nesting(),
            "Pure unordered deep nesting should not be detected as mixed"
        );

        // Should use fixed style with custom indent
        let rule = MD007ULIndent::new(4);
        let result = rule.check(&ctx).unwrap();
        // With text-aligned default but auto-switch to fixed for pure unordered,
        // the first nested level should be flagged (2 spaces instead of 4)
        assert!(!result.is_empty(), "Should flag incorrect indentation for fixed style");
    }

    #[test]
    fn test_interleaved_content_between_list_items() {
        // Paragraph continuation between list items should not break detection
        let content = "1. Ordered parent\n\n   Paragraph continuation\n\n   * Unordered child";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            ctx.has_mixed_list_nesting(),
            "Should detect mixed nesting even with interleaved paragraphs"
        );
    }

    #[test]
    fn test_esm_blocks_skipped_in_detection() {
        // ESM import/export blocks in MDX should be skipped
        // Note: ESM detection depends on LintContext properly setting in_esm_block
        let content = "* Unordered list\n  * Nested unordered";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !ctx.has_mixed_list_nesting(),
            "Pure unordered should not be detected as mixed"
        );
    }

    #[test]
    fn test_multiple_list_blocks_pure_then_mixed() {
        // Document with pure unordered list followed by mixed list
        // Detection should find the mixed list and return true
        let content = r#"* Pure unordered
  * Nested unordered

1. Mixed section
   * Bullet under ordered"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            ctx.has_mixed_list_nesting(),
            "Should detect mixed nesting in any part of document"
        );
    }

    #[test]
    fn test_multiple_separate_pure_lists() {
        // Multiple pure unordered lists separated by blank lines
        // Should NOT be detected as mixed
        let content = r#"* First list
  * Nested

* Second list
  * Also nested

* Third list
  * Deeply
    * Nested"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            !ctx.has_mixed_list_nesting(),
            "Multiple separate pure unordered lists should not be mixed"
        );
    }

    #[test]
    fn test_code_block_between_list_items() {
        // Code block between list items should not affect detection
        let content = r#"1. Ordered
   ```
   code
   ```
   * Still a mixed child"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(
            ctx.has_mixed_list_nesting(),
            "Code block between items should not prevent mixed detection"
        );
    }

    #[test]
    fn test_blockquoted_mixed_detection() {
        // Mixed lists inside blockquotes should be detected
        let content = "> 1. Ordered in blockquote\n>    * Mixed child";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        // Note: Detection depends on correct marker_column calculation in blockquotes
        // This test verifies the detection logic works with blockquoted content
        assert!(
            ctx.has_mixed_list_nesting(),
            "Should detect mixed nesting in blockquotes"
        );
    }

    // Tests for "Do What I Mean" behavior (issue #273)

    #[test]
    fn test_indent_explicit_uses_fixed_style() {
        // When indent is explicitly set but style is not, use fixed style automatically
        // This is the "Do What I Mean" behavior for issue #273
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned, // Default
            style_explicit: false,                         // Style NOT explicitly set
            indent_explicit: true,                         // Indent explicitly set
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // With indent_explicit=true and style_explicit=false, should use fixed style
        // Fixed style with indent=4: level 0 = 0, level 1 = 4, level 2 = 8
        let content = "* Level 0\n    * Level 1\n        * Level 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "With indent_explicit=true, should use fixed style (0, 4, 8), got: {result:?}"
        );

        // Text-aligned spacing (2 spaces per level) should now be wrong
        let wrong_content = "* Level 0\n  * Level 1\n    * Level 2";
        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "Should flag text-aligned spacing when indent_explicit=true"
        );
    }

    #[test]
    fn test_explicit_style_overrides_indent_explicit() {
        // When both indent and style are explicitly set, style wins
        // This ensures backwards compatibility and respects explicit user choice
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: true,  // Style explicitly set
            indent_explicit: true, // Indent also explicitly set (user will see warning)
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // With explicit text-aligned style, should use text-aligned even with indent_explicit
        let content = "* Level 0\n  * Level 1\n    * Level 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Explicit text-aligned style should be respected, got: {result:?}"
        );
    }

    #[test]
    fn test_no_indent_explicit_uses_smart_detection() {
        // When neither is explicitly set, use smart per-parent detection (original behavior)
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: false,
            indent_explicit: false, // Neither explicitly set - use smart detection
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // Pure unordered with neither explicit: per-parent logic applies
        // For pure unordered at expected positions, fixed style is used
        let content = "* Level 0\n    * Level 1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // This should work with smart detection for pure unordered lists
        assert!(
            result.is_empty(),
            "Smart detection should accept 4-space indent, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_273_exact_reproduction() {
        // Exact reproduction from issue #273:
        // User sets `indent = 4` without setting style, expects 4-space increments
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned, // Default (would use text-aligned)
            style_explicit: false,
            indent_explicit: true, // User explicitly set indent
        };
        let rule = MD007ULIndent::from_config_struct(config);

        let content = r#"* Item 1
    * Item 2
        * Item 3"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Issue #273: indent=4 should use 4-space increments, got: {result:?}"
        );
    }

    #[test]
    fn test_indent_explicit_with_ordered_parent() {
        // When indent is explicitly set BUT the parent is ordered,
        // bullets must still use text-aligned because ordered markers have variable width.
        // This is the critical edge case that caused the regression.
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: false,
            indent_explicit: true, // User set indent=4
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // Ordered list with bullet child - bullet MUST align with ordered text (3 spaces)
        // NOT use fixed indent (4 spaces) even though indent=4 is set
        let content = "1. Ordered\n   * Bullet aligned with ordered text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Bullet under ordered must use text-aligned (3 spaces) even with indent=4: {result:?}"
        );

        // Fixed indent (4 spaces) under ordered list should be WRONG
        let wrong_content = "1. Ordered\n    * Bullet with 4-space fixed indent";
        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "4-space indent under ordered list should be flagged"
        );
    }

    #[test]
    fn test_indent_explicit_mixed_list_deep_nesting() {
        // Deep nesting with alternating list types tests the edge case thoroughly:
        // - Bullets under bullets: use configured indent (4)
        // - Bullets under ordered: use text-aligned
        // - Ordered under bullets: N/A (MD007 only checks bullets)
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: false,
            indent_explicit: true,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // Level 0: bullet (col 0)
        // Level 1: bullet (col 4 - fixed, parent is bullet)
        // Level 2: ordered (col 8 - not checked by MD007)
        // Level 3: bullet (col 11 - text-aligned with "1. " = 3 chars from col 8)
        let content = r#"* Level 0
    * Level 1 (4-space indent from bullet parent)
        1. Level 2 ordered
           * Level 3 bullet (text-aligned under ordered)"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Mixed nesting should handle each parent type correctly: {result:?}"
        );
    }

    #[test]
    fn test_ordered_list_double_digit_markers() {
        // Ordered lists with 10+ items have wider markers ("10." vs "9.")
        // Bullets nested under these must text-align correctly
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: false,
            indent_explicit: true,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // "10. " = 4 chars, so bullet should be at column 4
        let content = "10. Double digit\n    * Bullet at col 4";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Bullet under '10.' should align at column 4: {result:?}"
        );

        // Single digit "1. " = 3 chars, bullet at column 3
        let content_single = "1. Single digit\n   * Bullet at col 3";
        let ctx = LintContext::new(content_single, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Bullet under '1.' should align at column 3: {result:?}"
        );
    }

    #[test]
    fn test_indent_explicit_pure_unordered_uses_fixed() {
        // Regression test: pure unordered lists should use fixed indent
        // when indent is explicitly configured
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: false,
            indent_explicit: true,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // Pure unordered with 4-space indent should pass
        let content = "* Level 0\n    * Level 1\n        * Level 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Pure unordered with indent=4 should use 4-space increments: {result:?}"
        );

        // Text-aligned (2-space) should fail with indent=4
        let wrong_content = "* Level 0\n  * Level 1\n    * Level 2";
        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "2-space indent should be flagged when indent=4 is configured"
        );
    }
}
