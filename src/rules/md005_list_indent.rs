//!
//! Rule MD005: Inconsistent indentation for list items at the same level
//!
//! See [docs/md005.md](../../docs/md005.md) for full documentation, configuration, and examples.

use crate::utils::range_utils::calculate_match_range;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
// No regex patterns needed for this rule
use std::collections::HashMap;
use toml;

/// Rule MD005: Inconsistent indentation for list items at the same level
#[derive(Clone, Default)]
pub struct MD005ListIndent {
    /// Expected indentation for top-level lists (from MD007 config)
    top_level_indent: usize,
    /// Expected indentation increment for nested lists (from MD007 config)
    md007_indent: usize,
}

impl MD005ListIndent {
    /// Group related list blocks that should be treated as one logical list structure
    fn group_related_list_blocks<'a>(
        &self,
        list_blocks: &'a [crate::lint_context::ListBlock],
    ) -> Vec<Vec<&'a crate::lint_context::ListBlock>> {
        if list_blocks.is_empty() {
            return Vec::new();
        }

        let mut groups = Vec::new();
        let mut current_group = vec![&list_blocks[0]];

        for i in 1..list_blocks.len() {
            let prev_block = &list_blocks[i - 1];
            let current_block = &list_blocks[i];

            // Check if blocks are consecutive (no significant gap between them)
            let line_gap = current_block.start_line.saturating_sub(prev_block.end_line);

            // Group blocks if they are close together (within 2 lines)
            // This handles cases where mixed list types are split but should be treated together
            if line_gap <= 2 {
                current_group.push(current_block);
            } else {
                // Start a new group
                groups.push(current_group);
                current_group = vec![current_block];
            }
        }
        groups.push(current_group);

        groups
    }

    /// Check if a list item is continuation content of a parent list item
    fn is_continuation_content(
        &self,
        ctx: &crate::lint_context::LintContext,
        list_line: usize,
        list_indent: usize,
    ) -> bool {
        // Look backward to find the true parent list item (not just immediate previous)
        for line_num in (1..list_line).rev() {
            if let Some(line_info) = ctx.line_info(line_num) {
                if let Some(parent_list_item) = &line_info.list_item {
                    let parent_marker_column = parent_list_item.marker_column;
                    let parent_content_column = parent_list_item.content_column;

                    // Skip list items at the same or greater indentation - we want the true parent
                    if parent_marker_column >= list_indent {
                        continue;
                    }

                    // Found a potential parent list item at a shallower indentation
                    // Check if there are continuation lines between parent and current list
                    let continuation_indent =
                        self.find_continuation_indent_between(ctx, line_num + 1, list_line - 1, parent_content_column);

                    if let Some(cont_indent) = continuation_indent {
                        // If the current list's indent matches the continuation content indent,
                        // OR if it's at the standard continuation list indentation (parent_content + 2),
                        // it's continuation content
                        let is_standard_continuation = list_indent == parent_content_column + 2;
                        let matches_content_indent = list_indent == cont_indent;

                        if matches_content_indent || is_standard_continuation {
                            return true;
                        }
                    }

                    // Special case: if this list item is at the same indentation as previous
                    // continuation lists, it might be part of the same continuation block
                    if list_indent > parent_marker_column {
                        // Check if previous list items at this indentation are also continuation
                        if self.has_continuation_list_at_indent(
                            ctx,
                            line_num,
                            list_line,
                            list_indent,
                            parent_content_column,
                        ) {
                            return true;
                        }

                        // Also check if there are any continuation text blocks between the parent
                        // and this list (even if there are other lists in between)
                        if self.has_any_continuation_content_after_parent(
                            ctx,
                            line_num,
                            list_line,
                            parent_content_column,
                        ) {
                            return true;
                        }
                    }

                    // If no continuation lines, this might still be a child list
                    // but not continuation content, so continue looking for a parent
                } else if !line_info.content.trim().is_empty() {
                    // Found non-list content - only stop if it's at the left margin
                    // (which would indicate we've moved out of any potential parent structure)
                    let content = line_info.content.trim_start();
                    let line_indent = line_info.content.len() - content.len();

                    if line_indent == 0 {
                        break;
                    }
                }
            }
        }
        false
    }

    /// Check if there are continuation lists at the same indentation after a parent
    fn has_continuation_list_at_indent(
        &self,
        ctx: &crate::lint_context::LintContext,
        parent_line: usize,
        current_line: usize,
        list_indent: usize,
        parent_content_column: usize,
    ) -> bool {
        // Look for list items between parent and current that are at the same indentation
        // and are part of continuation content
        for line_num in (parent_line + 1)..current_line {
            if let Some(line_info) = ctx.line_info(line_num)
                && let Some(list_item) = &line_info.list_item
                && list_item.marker_column == list_indent
            {
                // Found a list at same indentation - check if it has continuation content before it
                if self
                    .find_continuation_indent_between(ctx, parent_line + 1, line_num - 1, parent_content_column)
                    .is_some()
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if there are any continuation content blocks after a parent (anywhere between parent and current)
    fn has_any_continuation_content_after_parent(
        &self,
        ctx: &crate::lint_context::LintContext,
        parent_line: usize,
        current_line: usize,
        parent_content_column: usize,
    ) -> bool {
        // Look for any continuation content between parent and current line
        for line_num in (parent_line + 1)..current_line {
            if let Some(line_info) = ctx.line_info(line_num) {
                let content = line_info.content.trim_start();

                // Skip empty lines and list items
                if content.is_empty() || line_info.list_item.is_some() {
                    continue;
                }

                // Calculate indentation of this line
                let line_indent = line_info.content.len() - content.len();

                // If this line is indented more than the parent's content column,
                // it's continuation content
                if line_indent > parent_content_column {
                    return true;
                }
            }
        }
        false
    }

    /// Find the indentation level used for continuation content between two line numbers
    fn find_continuation_indent_between(
        &self,
        ctx: &crate::lint_context::LintContext,
        start_line: usize,
        end_line: usize,
        parent_content_column: usize,
    ) -> Option<usize> {
        if start_line > end_line {
            return None;
        }

        for line_num in start_line..=end_line {
            if let Some(line_info) = ctx.line_info(line_num) {
                let content = line_info.content.trim_start();

                // Skip empty lines
                if content.is_empty() {
                    continue;
                }

                // Skip list items
                if line_info.list_item.is_some() {
                    continue;
                }

                // Calculate indentation of this line
                let line_indent = line_info.content.len() - content.len();

                // If this line is indented more than the parent's content column,
                // it's continuation content - return its indentation level
                if line_indent > parent_content_column {
                    return Some(line_indent);
                }
            }
        }
        None
    }

    /// Check a group of related list blocks as one logical list structure
    fn check_list_block_group(
        &self,
        ctx: &crate::lint_context::LintContext,
        group: &[&crate::lint_context::ListBlock],
        warnings: &mut Vec<LintWarning>,
    ) -> Result<(), LintError> {
        // Use ctx.line_offsets instead of creating new LineIndex for better performance

        // Collect all list items from all blocks in the group
        let mut all_list_items = Vec::new();

        for list_block in group {
            for &item_line in &list_block.item_lines {
                if let Some(line_info) = ctx.line_info(item_line)
                    && let Some(list_item) = &line_info.list_item
                {
                    // Calculate the effective indentation (considering blockquotes)
                    let effective_indent = if let Some(blockquote) = &line_info.blockquote {
                        // For blockquoted lists, use relative indentation within the blockquote
                        list_item.marker_column.saturating_sub(blockquote.nesting_level * 2)
                    } else {
                        // For normal lists, use the marker column directly
                        list_item.marker_column
                    };

                    // Skip list items that are continuation content
                    if self.is_continuation_content(ctx, item_line, effective_indent) {
                        continue;
                    }

                    all_list_items.push((item_line, effective_indent, line_info, list_item));
                }
            }
        }

        if all_list_items.is_empty() {
            return Ok(());
        }

        // Sort by line number to process in order
        all_list_items.sort_by_key(|(line_num, _, _, _)| *line_num);

        // Build level mapping based on hierarchical structure
        // Key insight: We need to identify which items are meant to be at the same level
        // even if they have slightly different indentations (inconsistent formatting)
        let mut level_map: HashMap<usize, usize> = HashMap::new();
        let mut level_indents: HashMap<usize, Vec<usize>> = HashMap::new(); // Track all indents seen at each level

        // Process items in order to build the level hierarchy
        for i in 0..all_list_items.len() {
            let (line_num, indent, _, _) = &all_list_items[i];

            let level = if i == 0 {
                // First item establishes level 1
                level_indents.entry(1).or_default().push(*indent);
                1
            } else {
                // Find the appropriate level for this item
                let mut determined_level = 0;

                // First, check if this indent matches any existing level exactly
                for (lvl, indents) in &level_indents {
                    if indents.contains(indent) {
                        determined_level = *lvl;
                        break;
                    }
                }

                if determined_level == 0 {
                    // No exact match - determine level based on hierarchy
                    // Look for the most recent item with clearly less indentation (parent)
                    for j in (0..i).rev() {
                        let (prev_line, prev_indent, _, _) = &all_list_items[j];
                        let prev_level = level_map[prev_line];

                        // A clear parent has at least 2 spaces less indentation
                        if *prev_indent + 2 <= *indent {
                            // This is a child of prev_item
                            determined_level = prev_level + 1;
                            break;
                        } else if (*prev_indent as i32 - *indent as i32).abs() <= 1 {
                            // Within 1 space - likely meant to be same level but inconsistent
                            determined_level = prev_level;
                            break;
                        } else if *prev_indent < *indent {
                            // Less than 2 space difference but more than 1
                            // This is ambiguous - could be same level or child
                            // Look at the pattern: if prev_level already has items with similar indent,
                            // this is probably meant to be at the same level
                            if let Some(level_indents_list) = level_indents.get(&prev_level) {
                                // Check if any indent at prev_level is close to this indent
                                for &lvl_indent in level_indents_list {
                                    if (lvl_indent as i32 - *indent as i32).abs() <= 1 {
                                        // Close to an existing indent at prev_level
                                        determined_level = prev_level;
                                        break;
                                    }
                                }
                            }
                            if determined_level == 0 {
                                // Still not determined - treat as child since it has more indent
                                determined_level = prev_level + 1;
                            }
                            break;
                        }
                    }

                    // If still not determined, default to level 1
                    if determined_level == 0 {
                        determined_level = 1;
                    }

                    // Record this indent for the level
                    level_indents.entry(determined_level).or_default().push(*indent);
                }

                determined_level
            };

            level_map.insert(*line_num, level);
        }

        // Now group items by their level
        let mut level_groups: HashMap<usize, Vec<(usize, usize, &crate::lint_context::LineInfo)>> = HashMap::new();
        for (line_num, indent, line_info, _) in &all_list_items {
            let level = level_map[line_num];
            level_groups
                .entry(level)
                .or_default()
                .push((*line_num, *indent, *line_info));
        }

        // For each level, check consistency
        for (level, group) in level_groups {
            // For level 1 (top-level), even single items should start at column 0
            // For other levels, we need at least 2 items to check consistency
            if level != 1 && group.len() < 2 {
                continue;
            }

            // Sort by line number
            let mut group = group;
            group.sort_by_key(|(line_num, _, _)| *line_num);

            // Check if all items at this level have the same indentation
            let indents: std::collections::HashSet<usize> = group.iter().map(|(_, indent, _)| *indent).collect();

            // For level 1, check if any item doesn't match expected top-level indentation
            // For other levels, check for inconsistent indentation
            let has_issue = if level == 1 {
                // Top-level items should have the configured indentation
                indents.iter().any(|&indent| indent != self.top_level_indent)
            } else {
                // Other levels need consistency
                indents.len() > 1
            };

            if has_issue {
                // Inconsistent indentation at this level!
                // Determine what the correct indentation should be

                // For level 1, it should be the configured top-level indent
                // For other levels, we need to look at parent alignment or use the most common indent
                let expected_indent = if level == 1 {
                    self.top_level_indent
                } else {
                    // For non-top-level items, determine the expected indent
                    // If MD007 is configured with fixed indentation, use that
                    if self.md007_indent > 0 {
                        // When MD007 indent is configured, use fixed indentation
                        // Each level should be indented by md007_indent * (level - 1)
                        (level - 1) * self.md007_indent
                    } else {
                        // No MD007 config, determine based on existing patterns
                        let mut indent_counts: HashMap<usize, usize> = HashMap::new();
                        for (_, indent, _) in &group {
                            *indent_counts.entry(*indent).or_insert(0) += 1;
                        }

                        if indent_counts.len() == 1 {
                            // All items have the same indent already
                            *indent_counts.keys().next().unwrap()
                        } else {
                            // Multiple indents - pick the most common one
                            // When counts are equal, prefer the smaller indentation
                            // This handles cases where one item has correct indentation and another is wrong
                            indent_counts
                                .iter()
                                .max_by(|(indent_a, count_a), (indent_b, count_b)| {
                                    // First compare by count, then by preferring smaller indent
                                    count_a.cmp(count_b).then(indent_b.cmp(indent_a))
                                })
                                .map(|(indent, _)| *indent)
                                .unwrap()
                        }
                    }
                };

                // Flag all items that don't match the expected indentation
                for (line_num, indent, line_info) in &group {
                    if *indent != expected_indent {
                        let message = format!(
                            "Expected indentation of {} {}, found {}",
                            expected_indent,
                            if expected_indent == 1 { "space" } else { "spaces" },
                            indent
                        );

                        let (start_line, start_col, end_line, end_col) = if *indent > 0 {
                            calculate_match_range(*line_num, &line_info.content, 0, *indent)
                        } else {
                            calculate_match_range(*line_num, &line_info.content, 0, 1)
                        };

                        let fix_range = if *indent > 0 {
                            let start_byte = ctx.line_offsets.get(line_num - 1).copied().unwrap_or(0);
                            let end_byte = start_byte + *indent;
                            start_byte..end_byte
                        } else {
                            let byte_pos = ctx.line_offsets.get(line_num - 1).copied().unwrap_or(0);
                            byte_pos..byte_pos
                        };

                        let replacement = if expected_indent > 0 {
                            " ".repeat(expected_indent)
                        } else {
                            String::new()
                        };

                        warnings.push(LintWarning {
                            rule_name: Some(self.name()),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            message,
                            severity: Severity::Warning,
                            fix: Some(Fix {
                                range: fix_range,
                                replacement,
                            }),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Migrated to use centralized list blocks for better performance and accuracy
    fn check_optimized(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early returns for common cases
        if content.is_empty() {
            return Ok(Vec::new());
        }

        // Quick check for any list blocks before processing
        if ctx.list_blocks.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();

        // Group consecutive list blocks that should be treated as one logical structure
        // This is needed because mixed list types (ordered/unordered) get split into separate blocks
        let block_groups = self.group_related_list_blocks(&ctx.list_blocks);

        for group in block_groups {
            self.check_list_block_group(ctx, &group, &mut warnings)?;
        }

        Ok(warnings)
    }
}

impl Rule for MD005ListIndent {
    fn name(&self) -> &'static str {
        "MD005"
    }

    fn description(&self) -> &'static str {
        "List indentation should be consistent"
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Use optimized version
        self.check_optimized(ctx)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        let warnings = self.check(ctx)?;
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Sort warnings by position (descending) to apply from end to start
        let mut warnings_with_fixes: Vec<_> = warnings
            .into_iter()
            .filter_map(|w| w.fix.clone().map(|fix| (w, fix)))
            .collect();
        warnings_with_fixes.sort_by_key(|(_, fix)| std::cmp::Reverse(fix.range.start));

        // Apply fixes to content
        let mut content = ctx.content.to_string();
        for (_, fix) in warnings_with_fixes {
            if fix.range.start <= content.len() && fix.range.end <= content.len() {
                content.replace_range(fix.range, &fix.replacement);
            }
        }

        Ok(content)
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::List
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if content is empty or has no list items
        ctx.content.is_empty() || !ctx.lines.iter().any(|line| line.list_item.is_some())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        None
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        // Check MD007 configuration to understand expected list indentation
        let mut top_level_indent = 0;
        let mut md007_indent = 2; // Default to 2 if not specified

        // Try to get MD007 configuration
        if let Some(md007_config) = config.rules.get("MD007") {
            // Check for start_indented setting
            if let Some(start_indented) = md007_config.values.get("start-indented")
                && let Some(start_indented_bool) = start_indented.as_bool()
                && start_indented_bool
            {
                // If start_indented is true, check for start_indent value
                if let Some(start_indent) = md007_config.values.get("start-indent") {
                    if let Some(indent_value) = start_indent.as_integer() {
                        top_level_indent = indent_value as usize;
                    }
                } else {
                    // Default start_indent when start_indented is true
                    top_level_indent = 2;
                }
            }

            // Also check 'indent' setting - this is the expected increment for nested lists
            if let Some(indent) = md007_config.values.get("indent")
                && let Some(indent_value) = indent.as_integer()
            {
                md007_indent = indent_value as usize;
            }
        }

        Box::new(MD005ListIndent {
            top_level_indent,
            md007_indent,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_valid_unordered_list() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1
* Item 2
  * Nested 1
  * Nested 2
* Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_valid_ordered_list() {
        let rule = MD005ListIndent::default();
        let content = "\
1. Item 1
2. Item 2
   1. Nested 1
   2. Nested 2
3. Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // With dynamic alignment, nested items should align with parent's text content
        // Ordered items starting with "1. " have text at column 3, so nested items need 3 spaces
        assert!(result.is_empty());
    }

    #[test]
    fn test_invalid_unordered_indent() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1
 * Item 2
   * Nested 1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // With dynamic alignment, line 3 correctly aligns with line 2's text position
        // Only line 2 is incorrectly indented
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "* Item 1\n* Item 2\n   * Nested 1");
    }

    #[test]
    fn test_invalid_ordered_indent() {
        let rule = MD005ListIndent::default();
        let content = "\
1. Item 1
 2. Item 2
    1. Nested 1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        // With dynamic alignment, ordered items align with parent's text content
        // Line 1 text starts at col 3, so line 2 should have 3 spaces
        // Line 3 already correctly aligns with line 2's text position
        assert_eq!(fixed, "1. Item 1\n2. Item 2\n    1. Nested 1");
    }

    #[test]
    fn test_mixed_list_types() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1
  1. Nested ordered
  * Nested unordered
* Item 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_multiple_levels() {
        let rule = MD005ListIndent::default();
        let content = "\
* Level 1
   * Level 2
      * Level 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // MD005 should now accept consistent 3-space increments
        assert!(result.is_empty(), "MD005 should accept consistent indentation pattern");
    }

    #[test]
    fn test_empty_lines() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1

  * Nested 1

* Item 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_no_lists() {
        let rule = MD005ListIndent::default();
        let content = "\
Just some text
More text
Even more text";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_complex_nesting() {
        let rule = MD005ListIndent::default();
        let content = "\
* Level 1
  * Level 2
    * Level 3
  * Back to 2
    1. Ordered 3
    2. Still 3
* Back to 1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_invalid_complex_nesting() {
        let rule = MD005ListIndent::default();
        let content = "\
* Level 1
   * Level 2
     * Level 3
   * Back to 2
      1. Ordered 3
     2. Still 3
* Back to 1";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Lines 5-6 have inconsistent indentation (6 vs 5 spaces) for the same level
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.contains("Expected indentation of 5 spaces, found 6")
                || result[0].message.contains("Expected indentation of 6 spaces, found 5")
        );
    }

    #[test]
    fn test_with_lint_context() {
        let rule = MD005ListIndent::default();

        // Test with consistent list indentation
        let content = "* Item 1\n* Item 2\n  * Nested item\n  * Another nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with inconsistent list indentation
        let content = "* Item 1\n* Item 2\n * Nested item\n  * Another nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty()); // Should have at least one warning

        // Test with different level indentation issues
        let content = "* Item 1\n  * Nested item\n * Another nested item with wrong indent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty()); // Should have at least one warning
    }

    // Additional comprehensive tests
    #[test]
    fn test_list_with_continuations() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1
  This is a continuation
  of the first item
  * Nested item
    with its own continuation
* Item 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_in_blockquote() {
        let rule = MD005ListIndent::default();
        let content = "\
> * Item 1
>   * Nested 1
>   * Nested 2
> * Item 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Blockquoted lists should have correct indentation within the blockquote context
        assert!(
            result.is_empty(),
            "Expected no warnings for correctly indented blockquote list, got: {result:?}"
        );
    }

    #[test]
    fn test_list_with_code_blocks() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1
  ```
  code block
  ```
  * Nested item
* Item 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_with_tabs() {
        let rule = MD005ListIndent::default();
        let content = "* Item 1\n\t* Tab indented\n  * Space indented";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Should detect inconsistent indentation
        assert!(!result.is_empty());
    }

    #[test]
    fn test_inconsistent_at_same_level() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1
  * Nested 1
  * Nested 2
   * Wrong indent for same level
  * Nested 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty());
        // Should flag the inconsistent item
        assert!(result.iter().any(|w| w.line == 4));
    }

    #[test]
    fn test_zero_indent_top_level() {
        let rule = MD005ListIndent::default();
        // Use concat to preserve the leading space
        let content = concat!(" * Wrong indent\n", "* Correct\n", "  * Nested");
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // Should flag the indented top-level item
        assert!(!result.is_empty());
        assert!(result.iter().any(|w| w.line == 1));
    }

    #[test]
    fn test_fix_preserves_content() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item with **bold** and *italic*
 * Wrong indent with `code`
   * Also wrong with [link](url)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(fixed.contains("**bold**"));
        assert!(fixed.contains("*italic*"));
        assert!(fixed.contains("`code`"));
        assert!(fixed.contains("[link](url)"));
    }

    #[test]
    fn test_deeply_nested_lists() {
        let rule = MD005ListIndent::default();
        let content = "\
* L1
  * L2
    * L3
      * L4
        * L5
          * L6";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_fix_multiple_issues() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1
 * Wrong 1
   * Wrong 2
    * Wrong 3
  * Correct
   * Wrong 4";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let fixed = rule.fix(&ctx).unwrap();
        // Should fix to consistent indentation
        let lines: Vec<&str> = fixed.lines().collect();
        assert_eq!(lines[0], "* Item 1");
        // All level 2 items should have same indent
        assert!(lines[1].starts_with("  * ") || lines[1].starts_with("* "));
    }

    #[test]
    fn test_performance_large_document() {
        let rule = MD005ListIndent::default();
        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!("* Item {i}\n"));
            content.push_str(&format!("  * Nested {i}\n"));
        }
        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_column_positions() {
        let rule = MD005ListIndent::default();
        let content = " * Wrong indent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].column, 1, "Expected column 1, got {}", result[0].column);
        assert_eq!(
            result[0].end_column, 2,
            "Expected end_column 2, got {}",
            result[0].end_column
        );
    }

    #[test]
    fn test_should_skip() {
        let rule = MD005ListIndent::default();

        // Empty content should skip
        let ctx = LintContext::new("", crate::config::MarkdownFlavor::Standard);
        assert!(rule.should_skip(&ctx));

        // Content without lists should skip
        let ctx = LintContext::new("Just plain text", crate::config::MarkdownFlavor::Standard);
        assert!(rule.should_skip(&ctx));

        // Content with lists should not skip
        let ctx = LintContext::new("* List item", crate::config::MarkdownFlavor::Standard);
        assert!(!rule.should_skip(&ctx));

        let ctx = LintContext::new("1. Ordered list", crate::config::MarkdownFlavor::Standard);
        assert!(!rule.should_skip(&ctx));
    }

    #[test]
    fn test_should_skip_validation() {
        let rule = MD005ListIndent::default();
        let content = "* List item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        assert!(!rule.should_skip(&ctx));

        let content = "No lists here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        assert!(rule.should_skip(&ctx));
    }

    #[test]
    fn test_edge_case_single_space_indent() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1
 * Single space - wrong
  * Two spaces - correct";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Both the single space and two space items get warnings
        // because they establish inconsistent indentation at the same level
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|w| w.line == 2 && w.message.contains("found 1")));
    }

    #[test]
    fn test_edge_case_three_space_indent() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1
   * Three spaces - wrong
  * Two spaces - correct";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Should flag the item with 3 spaces as inconsistent (2 spaces is correct)
        assert_eq!(result.len(), 1);
        assert!(result.iter().any(|w| w.line == 2 && w.message.contains("found 3")));
    }

    #[test]
    fn test_nested_bullets_under_numbered_items() {
        let rule = MD005ListIndent::default();
        let content = "\
1. **Active Directory/LDAP**
   - User authentication and directory services
   - LDAP for user information and validation

2. **Oracle Unified Directory (OUD)**
   - Extended user directory services
   - Verification of project account presence and changes";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Should have no warnings - 3 spaces is correct for bullets under numbered items
        assert!(
            result.is_empty(),
            "Expected no warnings for bullets with 3 spaces under numbered items, got: {result:?}"
        );
    }

    #[test]
    fn test_nested_bullets_under_numbered_items_wrong_indent() {
        let rule = MD005ListIndent::default();
        let content = "\
1. **Active Directory/LDAP**
  - Wrong: only 2 spaces
   - Correct: 3 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Should flag one of them as inconsistent
        assert_eq!(
            result.len(),
            1,
            "Expected 1 warning, got {}. Warnings: {:?}",
            result.len(),
            result
        );
        // Either line 2 or line 3 should be flagged for inconsistency
        assert!(
            result
                .iter()
                .any(|w| (w.line == 2 && w.message.contains("found 2"))
                    || (w.line == 3 && w.message.contains("found 3")))
        );
    }

    #[test]
    fn test_regular_nested_bullets_still_work() {
        let rule = MD005ListIndent::default();
        let content = "\
* Top level
  * Second level (2 spaces is correct for bullets under bullets)
    * Third level (4 spaces)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // Should have no warnings - regular bullet nesting still uses 2-space increments
        assert!(
            result.is_empty(),
            "Expected no warnings for regular bullet nesting, got: {result:?}"
        );
    }

    #[test]
    fn test_fix_range_accuracy() {
        let rule = MD005ListIndent::default();
        let content = " * Wrong indent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);

        let fix = result[0].fix.as_ref().unwrap();
        // Fix should replace the single space with nothing (0 indent for level 1)
        assert_eq!(fix.replacement, "");
    }

    #[test]
    fn test_four_space_indent_pattern() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1
    * Item 2 with 4 spaces
        * Item 3 with 8 spaces
    * Item 4 with 4 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();
        // MD005 should accept consistent 4-space pattern
        assert!(
            result.is_empty(),
            "MD005 should accept consistent 4-space indentation pattern, got {} warnings",
            result.len()
        );
    }

    #[test]
    fn test_issue_64_scenario() {
        // Test the exact scenario from issue #64
        let rule = MD005ListIndent::default();
        let content = "\
* Top level item
    * Sub item with 4 spaces (as configured in MD007)
        * Nested sub item with 8 spaces
    * Another sub item with 4 spaces
* Another top level";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);
        let result = rule.check(&ctx).unwrap();

        // MD005 should accept consistent 4-space pattern
        assert!(
            result.is_empty(),
            "MD005 should accept 4-space indentation when that's the pattern being used. Got {} warnings",
            result.len()
        );
    }

    #[test]
    fn test_continuation_content_scenario() {
        let rule = MD005ListIndent::default();
        let content = "\
- **Changes to how the Python version is inferred** ([#16319](example))

    In previous versions of Ruff, you could specify your Python version with:

    - The `target-version` option in a `ruff.toml` file
    - The `project.requires-python` field in a `pyproject.toml` file";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let result = rule.check(&ctx).unwrap();

        // Should not flag continuation content lists as inconsistent
        assert!(
            result.is_empty(),
            "MD005 should not flag continuation content lists, got {} warnings: {:?}",
            result.len(),
            result
        );
    }

    #[test]
    fn test_multiple_continuation_lists_scenario() {
        let rule = MD005ListIndent::default();
        let content = "\
- **Changes to how the Python version is inferred** ([#16319](example))

    In previous versions of Ruff, you could specify your Python version with:

    - The `target-version` option in a `ruff.toml` file
    - The `project.requires-python` field in a `pyproject.toml` file

    In v0.10, config discovery has been updated to address this issue:

    - If Ruff finds a `ruff.toml` file without a `target-version`, it will check
    - If Ruff finds a user-level configuration, the `requires-python` field will take precedence
    - If there is no config file, Ruff will search for the closest `pyproject.toml`";

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard);

        let result = rule.check(&ctx).unwrap();

        // Should not flag continuation content lists as inconsistent
        assert!(
            result.is_empty(),
            "MD005 should not flag continuation content lists, got {} warnings: {:?}",
            result.len(),
            result
        );
    }
}
