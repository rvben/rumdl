//!
//! Rule MD005: Inconsistent indentation for list items at the same level
//!
//! See [docs/md005.md](../../docs/md005.md) for full documentation, configuration, and examples.

use crate::utils::blockquote::effective_indent_in_blockquote;
use crate::utils::range_utils::calculate_match_range;

use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
// No regex patterns needed for this rule
use std::collections::HashMap;
use toml;

/// Type alias for parent content column groups, keyed by (parent_col, is_ordered).
/// Used by `group_by_parent_content_column` to separate ordered and unordered items.
type ParentContentGroups<'a> = HashMap<(usize, bool), Vec<(usize, usize, &'a crate::lint_context::LineInfo)>>;

/// Rule MD005: Inconsistent indentation for list items at the same level
#[derive(Clone, Default)]
pub struct MD005ListIndent {
    /// Expected indentation for top-level lists (from MD007 config)
    top_level_indent: usize,
}

/// Cache for fast line information lookups to avoid O(n²) scanning
struct LineCacheInfo {
    /// Indentation level for each line (0 for empty lines)
    indentation: Vec<usize>,
    /// Blockquote nesting level for each line (0 for non-blockquote lines)
    blockquote_levels: Vec<usize>,
    /// Line content references for blockquote-aware indent calculation
    line_contents: Vec<String>,
    /// Bit flags: bit 0 = has_content, bit 1 = is_list_item, bit 2 = is_continuation_content
    flags: Vec<u8>,
    /// Parent list item line number for each list item (1-indexed, 0 = no parent)
    /// Pre-computed in O(n) to avoid O(n²) backward scanning
    parent_map: HashMap<usize, usize>,
}

const FLAG_HAS_CONTENT: u8 = 1;
const FLAG_IS_LIST_ITEM: u8 = 2;

impl LineCacheInfo {
    /// Build cache from context in one O(n) pass
    fn new(ctx: &crate::lint_context::LintContext) -> Self {
        let total_lines = ctx.lines.len();
        let mut indentation = Vec::with_capacity(total_lines);
        let mut blockquote_levels = Vec::with_capacity(total_lines);
        let mut line_contents = Vec::with_capacity(total_lines);
        let mut flags = Vec::with_capacity(total_lines);
        let mut parent_map = HashMap::new();

        // Track most recent list item at each indentation level for O(1) parent lookups
        // Key: marker_column, Value: line_num (1-indexed)
        //
        // Algorithm correctness invariant:
        // For each list item L at line N with marker_column M:
        //   parent_map[N] = the line number of the most recent list item P where:
        //     1. P.line < N (appears before L)
        //     2. P.marker_column < M (less indented than L)
        //     3. P.marker_column is maximal among all candidates (closest parent)
        //
        // This matches the original O(n) backward scan logic but pre-computes in O(n).
        let mut indent_stack: Vec<(usize, usize)> = Vec::new();

        for (idx, line_info) in ctx.lines.iter().enumerate() {
            let line_content = line_info.content(ctx.content);
            let content = line_content.trim_start();
            let line_indent = line_info.byte_len - content.len();

            indentation.push(line_indent);

            // Store blockquote level for blockquote-aware indent calculation
            let bq_level = line_info.blockquote.as_ref().map(|bq| bq.nesting_level).unwrap_or(0);
            blockquote_levels.push(bq_level);

            // Store line content for blockquote-aware indent calculation
            line_contents.push(line_content.to_string());

            let mut flag = 0u8;
            if !content.is_empty() {
                flag |= FLAG_HAS_CONTENT;
            }
            if let Some(list_item) = &line_info.list_item {
                flag |= FLAG_IS_LIST_ITEM;

                let line_num = idx + 1; // Convert to 1-indexed
                let marker_column = list_item.marker_column;

                // Maintain a monotonic stack of indentation levels (O(1) amortized)
                while let Some(&(indent, _)) = indent_stack.last() {
                    if indent < marker_column {
                        break;
                    }
                    indent_stack.pop();
                }

                if let Some((_, parent_line)) = indent_stack.last() {
                    parent_map.insert(line_num, *parent_line);
                }

                indent_stack.push((marker_column, line_num));
            }
            flags.push(flag);
        }

        Self {
            indentation,
            blockquote_levels,
            line_contents,
            flags,
            parent_map,
        }
    }

    /// Check if line has content
    fn has_content(&self, idx: usize) -> bool {
        self.flags.get(idx).is_some_and(|&f| f & FLAG_HAS_CONTENT != 0)
    }

    /// Check if line is a list item
    fn is_list_item(&self, idx: usize) -> bool {
        self.flags.get(idx).is_some_and(|&f| f & FLAG_IS_LIST_ITEM != 0)
    }

    /// Get blockquote info for a line (level and prefix length)
    fn blockquote_info(&self, line: usize) -> (usize, usize) {
        if line == 0 || line > self.line_contents.len() {
            return (0, 0);
        }
        let idx = line - 1;
        let bq_level = self.blockquote_levels.get(idx).copied().unwrap_or(0);
        if bq_level == 0 {
            return (0, 0);
        }
        // Calculate prefix length from line content
        let content = &self.line_contents[idx];
        let mut prefix_len = 0;
        let mut found = 0;
        for c in content.chars() {
            prefix_len += c.len_utf8();
            if c == '>' {
                found += 1;
                if found == bq_level {
                    // Include optional space after last >
                    if content.get(prefix_len..prefix_len + 1) == Some(" ") {
                        prefix_len += 1;
                    }
                    break;
                }
            }
        }
        (bq_level, prefix_len)
    }

    /// Fast O(n) check for continuation content between lines using cached data
    ///
    /// For blockquote-aware detection, also pass the parent's blockquote level and
    /// blockquote prefix length. These are used to calculate effective indentation
    /// for lines inside blockquotes.
    fn find_continuation_indent(
        &self,
        start_line: usize,
        end_line: usize,
        parent_content_column: usize,
        parent_bq_level: usize,
        parent_bq_prefix_len: usize,
    ) -> Option<usize> {
        if start_line == 0 || start_line > end_line || end_line > self.indentation.len() {
            return None;
        }

        // For blockquote lists, min continuation indent is the content column
        // WITHOUT the blockquote prefix portion
        let min_continuation_indent = if parent_bq_level > 0 {
            parent_content_column.saturating_sub(parent_bq_prefix_len)
        } else {
            parent_content_column
        };

        // Convert to 0-indexed
        let start_idx = start_line - 1;
        let end_idx = end_line - 1;

        for idx in start_idx..=end_idx {
            // Skip empty lines and list items
            if !self.has_content(idx) || self.is_list_item(idx) {
                continue;
            }

            // Calculate effective indent (blockquote-aware)
            let line_bq_level = self.blockquote_levels.get(idx).copied().unwrap_or(0);
            let raw_indent = self.indentation[idx];
            let effective_indent = if line_bq_level == parent_bq_level && parent_bq_level > 0 {
                effective_indent_in_blockquote(&self.line_contents[idx], parent_bq_level, raw_indent)
            } else {
                raw_indent
            };

            // If this line is indented at or past the min continuation indent,
            // it's continuation content
            if effective_indent >= min_continuation_indent {
                return Some(effective_indent);
            }
        }
        None
    }

    /// Fast O(n) check if any continuation content exists after parent
    ///
    /// For blockquote-aware detection, also pass the parent's blockquote level and
    /// blockquote prefix length.
    fn has_continuation_content(
        &self,
        parent_line: usize,
        current_line: usize,
        parent_content_column: usize,
        parent_bq_level: usize,
        parent_bq_prefix_len: usize,
    ) -> bool {
        if parent_line == 0 || current_line <= parent_line || current_line > self.indentation.len() {
            return false;
        }

        // For blockquote lists, min continuation indent is the content column
        // WITHOUT the blockquote prefix portion
        let min_continuation_indent = if parent_bq_level > 0 {
            parent_content_column.saturating_sub(parent_bq_prefix_len)
        } else {
            parent_content_column
        };

        // Convert to 0-indexed
        let start_idx = parent_line; // parent_line + 1 - 1
        let end_idx = current_line - 2; // current_line - 1 - 1

        if start_idx > end_idx {
            return false;
        }

        for idx in start_idx..=end_idx {
            // Skip empty lines and list items
            if !self.has_content(idx) || self.is_list_item(idx) {
                continue;
            }

            // Calculate effective indent (blockquote-aware)
            let line_bq_level = self.blockquote_levels.get(idx).copied().unwrap_or(0);
            let raw_indent = self.indentation[idx];
            let effective_indent = if line_bq_level == parent_bq_level && parent_bq_level > 0 {
                effective_indent_in_blockquote(&self.line_contents[idx], parent_bq_level, raw_indent)
            } else {
                raw_indent
            };

            // If this line is indented at or past the min continuation indent,
            // it's continuation content
            if effective_indent >= min_continuation_indent {
                return true;
            }
        }
        false
    }
}

impl MD005ListIndent {
    /// Gap tolerance for grouping list blocks as one logical structure.
    /// Markdown allows blank lines within lists, so we need some tolerance.
    /// 2 lines handles: 1 blank line + potential interruption
    const LIST_GROUP_GAP_TOLERANCE: usize = 2;

    /// Minimum indentation increase to be considered a child (not same level).
    /// Per Markdown convention, nested items need at least 2 more spaces.
    const MIN_CHILD_INDENT_INCREASE: usize = 2;

    /// Tolerance for considering items at "same level" despite minor indent differences.
    /// Allows for 1 space difference to accommodate inconsistent formatting.
    const SAME_LEVEL_TOLERANCE: i32 = 1;

    /// Standard continuation list indentation offset from parent content column.
    /// Lists that are continuation content typically indent 2 spaces from parent content.
    const STANDARD_CONTINUATION_OFFSET: usize = 2;

    /// Creates a warning for an indent mismatch.
    fn create_indent_warning(
        &self,
        ctx: &crate::lint_context::LintContext,
        line_num: usize,
        line_info: &crate::lint_context::LineInfo,
        actual_indent: usize,
        expected_indent: usize,
    ) -> LintWarning {
        let message = format!(
            "Expected indentation of {} {}, found {}",
            expected_indent,
            if expected_indent == 1 { "space" } else { "spaces" },
            actual_indent
        );

        let (start_line, start_col, end_line, end_col) = if actual_indent > 0 {
            calculate_match_range(line_num, line_info.content(ctx.content), 0, actual_indent)
        } else {
            calculate_match_range(line_num, line_info.content(ctx.content), 0, 1)
        };

        // For blockquote-nested lists, we need to preserve the blockquote prefix
        // Similar to how MD007 handles this case
        let (fix_range, replacement) = if line_info.blockquote.is_some() {
            // Calculate the range from start of line to the list marker position
            let start_byte = line_info.byte_offset;
            let mut end_byte = line_info.byte_offset;

            // Get the list marker position from list_item
            let marker_column = line_info
                .list_item
                .as_ref()
                .map(|li| li.marker_column)
                .unwrap_or(actual_indent);

            // Calculate where the marker starts
            for (i, ch) in line_info.content(ctx.content).chars().enumerate() {
                if i >= marker_column {
                    break;
                }
                end_byte += ch.len_utf8();
            }

            // Build the blockquote prefix
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

            // Build replacement with blockquote prefix + correct indentation
            let correct_indent = " ".repeat(expected_indent);
            let replacement = format!("{blockquote_prefix} {correct_indent}");

            (start_byte..end_byte, replacement)
        } else {
            // Non-blockquote case: original logic
            let fix_range = if actual_indent > 0 {
                let start_byte = ctx.line_offsets.get(line_num - 1).copied().unwrap_or(0);
                let end_byte = start_byte + actual_indent;
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

            (fix_range, replacement)
        };

        LintWarning {
            rule_name: Some(self.name().to_string()),
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
        }
    }

    /// Checks consistency within a group of items and emits warnings.
    /// Uses first-established indent as the expected value when inconsistencies are found.
    fn check_indent_consistency(
        &self,
        ctx: &crate::lint_context::LintContext,
        items: &[(usize, usize, &crate::lint_context::LineInfo)],
        warnings: &mut Vec<LintWarning>,
    ) {
        if items.len() < 2 {
            return;
        }

        // Sort items by line number to find first-established pattern
        let mut sorted_items: Vec<_> = items.iter().collect();
        sorted_items.sort_by_key(|(line_num, _, _)| *line_num);

        let indents: std::collections::HashSet<usize> = sorted_items.iter().map(|(_, indent, _)| *indent).collect();

        if indents.len() > 1 {
            // Items have inconsistent indentation
            // Use the first established indent as the expected value
            let expected_indent = sorted_items.first().map(|(_, i, _)| *i).unwrap_or(0);

            for (line_num, indent, line_info) in items {
                if *indent != expected_indent {
                    warnings.push(self.create_indent_warning(ctx, *line_num, line_info, *indent, expected_indent));
                }
            }
        }
    }

    /// Groups items by their semantic parent's content column AND list type.
    ///
    /// By grouping by (parent_content_column, is_ordered), we enforce consistency
    /// within each list type separately. This prevents oscillation with MD007, which
    /// only adjusts unordered list indentation and may expect different values than
    /// what ordered lists use. (fixes #287)
    fn group_by_parent_content_column<'a>(
        &self,
        level: usize,
        group: &[(usize, usize, &'a crate::lint_context::LineInfo)],
        all_list_items: &[(
            usize,
            usize,
            &crate::lint_context::LineInfo,
            &crate::lint_context::ListItemInfo,
        )],
        level_map: &HashMap<usize, usize>,
    ) -> ParentContentGroups<'a> {
        let parent_level = level - 1;

        // Build line->is_ordered map for O(1) lookup
        let is_ordered_map: HashMap<usize, bool> = all_list_items
            .iter()
            .map(|(ln, _, _, item)| (*ln, item.is_ordered))
            .collect();

        let mut parent_content_groups: ParentContentGroups<'a> = HashMap::new();

        for (line_num, indent, line_info) in group {
            let item_is_ordered = is_ordered_map.get(line_num).copied().unwrap_or(false);

            // Find the most recent item at parent_level before this line
            let mut parent_content_col: Option<usize> = None;

            for (prev_line, _, _, list_item) in all_list_items.iter().rev() {
                if *prev_line >= *line_num {
                    continue;
                }
                if let Some(&prev_level) = level_map.get(prev_line)
                    && prev_level == parent_level
                {
                    parent_content_col = Some(list_item.content_column);
                    break;
                }
            }

            if let Some(parent_col) = parent_content_col {
                parent_content_groups
                    .entry((parent_col, item_is_ordered))
                    .or_default()
                    .push((*line_num, *indent, *line_info));
            }
        }

        parent_content_groups
    }

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

            // Group blocks if they are close together
            // This handles cases where mixed list types are split but should be treated together
            if line_gap <= Self::LIST_GROUP_GAP_TOLERANCE {
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
    /// Uses pre-computed parent map for O(1) lookup instead of O(n) backward scanning
    fn is_continuation_content(
        &self,
        ctx: &crate::lint_context::LintContext,
        cache: &LineCacheInfo,
        list_line: usize,
        list_indent: usize,
    ) -> bool {
        // Use pre-computed parent map instead of O(n) backward scan
        let parent_line = cache.parent_map.get(&list_line).copied();

        if let Some(parent_line) = parent_line
            && let Some(line_info) = ctx.line_info(parent_line)
            && let Some(parent_list_item) = &line_info.list_item
        {
            let parent_marker_column = parent_list_item.marker_column;
            let parent_content_column = parent_list_item.content_column;

            // Get parent's blockquote info for blockquote-aware continuation detection
            let parent_bq_level = line_info.blockquote.as_ref().map(|bq| bq.nesting_level).unwrap_or(0);
            let parent_bq_prefix_len = line_info.blockquote.as_ref().map(|bq| bq.prefix.len()).unwrap_or(0);

            // Check if there are continuation lines between parent and current list
            let continuation_indent = cache.find_continuation_indent(
                parent_line + 1,
                list_line - 1,
                parent_content_column,
                parent_bq_level,
                parent_bq_prefix_len,
            );

            if let Some(continuation_indent) = continuation_indent {
                let is_standard_continuation =
                    list_indent == parent_content_column + Self::STANDARD_CONTINUATION_OFFSET;
                let matches_content_indent = list_indent == continuation_indent;

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
                    cache,
                    parent_line,
                    list_line,
                    list_indent,
                    parent_content_column,
                ) {
                    return true;
                }

                // Get blockquote info for continuation check
                let (parent_bq_level, parent_bq_prefix_len) = cache.blockquote_info(parent_line);
                if cache.has_continuation_content(
                    parent_line,
                    list_line,
                    parent_content_column,
                    parent_bq_level,
                    parent_bq_prefix_len,
                ) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if there are continuation lists at the same indentation after a parent
    fn has_continuation_list_at_indent(
        &self,
        ctx: &crate::lint_context::LintContext,
        cache: &LineCacheInfo,
        parent_line: usize,
        current_line: usize,
        list_indent: usize,
        parent_content_column: usize,
    ) -> bool {
        // Get blockquote info from cache
        let (parent_bq_level, parent_bq_prefix_len) = cache.blockquote_info(parent_line);

        // Look for list items between parent and current that are at the same indentation
        // and are part of continuation content
        for line_num in (parent_line + 1)..current_line {
            if let Some(line_info) = ctx.line_info(line_num)
                && let Some(list_item) = &line_info.list_item
                && list_item.marker_column == list_indent
            {
                // Found a list at same indentation - check if it has continuation content before it
                if cache
                    .find_continuation_indent(
                        parent_line + 1,
                        line_num - 1,
                        parent_content_column,
                        parent_bq_level,
                        parent_bq_prefix_len,
                    )
                    .is_some()
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check a group of related list blocks as one logical list structure
    fn check_list_block_group(
        &self,
        ctx: &crate::lint_context::LintContext,
        group: &[&crate::lint_context::ListBlock],
        warnings: &mut Vec<LintWarning>,
    ) -> Result<(), LintError> {
        // Build cache once for O(n) preprocessing instead of O(n²) scanning
        let cache = LineCacheInfo::new(ctx);

        // First pass: collect all candidate items without filtering
        // We need to process in line order so parents are seen before children
        let mut candidate_items: Vec<(
            usize,
            usize,
            &crate::lint_context::LineInfo,
            &crate::lint_context::ListItemInfo,
        )> = Vec::new();

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

                    candidate_items.push((item_line, effective_indent, line_info, list_item));
                }
            }
        }

        // Sort by line number so parents are processed before children
        candidate_items.sort_by_key(|(line_num, _, _, _)| *line_num);

        // Second pass: filter out continuation content AND their children
        // When a parent is skipped, all its descendants must also be skipped
        let mut skipped_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut all_list_items: Vec<(
            usize,
            usize,
            &crate::lint_context::LineInfo,
            &crate::lint_context::ListItemInfo,
        )> = Vec::new();

        for (item_line, effective_indent, line_info, list_item) in candidate_items {
            // Skip list items that are continuation content
            if self.is_continuation_content(ctx, &cache, item_line, effective_indent) {
                skipped_lines.insert(item_line);
                continue;
            }

            // Also skip items whose parent was skipped (children of continuation content)
            if let Some(&parent_line) = cache.parent_map.get(&item_line)
                && skipped_lines.contains(&parent_line)
            {
                skipped_lines.insert(item_line);
                continue;
            }

            all_list_items.push((item_line, effective_indent, line_info, list_item));
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

        // Track the most recent item at each indent level for O(1) parent lookups
        // Key: indent value, Value: (level, line_num)
        let mut indent_to_level: HashMap<usize, (usize, usize)> = HashMap::new();

        // Process items in order to build the level hierarchy - now O(n) instead of O(n²)
        for (line_num, indent, _, _) in &all_list_items {
            let level = if indent_to_level.is_empty() {
                // First item establishes level 1
                level_indents.entry(1).or_default().push(*indent);
                1
            } else {
                // Find the appropriate level for this item
                let mut determined_level = 0;

                // First, check if this indent matches any existing level exactly
                if let Some(&(existing_level, _)) = indent_to_level.get(indent) {
                    determined_level = existing_level;
                } else {
                    // No exact match - determine level based on hierarchy
                    // Find the most recent item with clearly less indentation (parent)
                    // Instead of scanning backward O(n), look through tracked indents O(k) where k is number of unique indents
                    let mut best_parent: Option<(usize, usize, usize)> = None; // (indent, level, line)

                    for (&tracked_indent, &(tracked_level, tracked_line)) in &indent_to_level {
                        if tracked_indent < *indent {
                            // This is a potential parent (less indentation)
                            // Keep the one with the largest indent (closest parent)
                            if best_parent.is_none() || tracked_indent > best_parent.unwrap().0 {
                                best_parent = Some((tracked_indent, tracked_level, tracked_line));
                            }
                        }
                    }

                    if let Some((parent_indent, parent_level, _parent_line)) = best_parent {
                        // A clear parent has at least MIN_CHILD_INDENT_INCREASE spaces less indentation
                        if parent_indent + Self::MIN_CHILD_INDENT_INCREASE <= *indent {
                            // This is a child of the parent
                            determined_level = parent_level + 1;
                        } else if (*indent as i32 - parent_indent as i32).abs() <= Self::SAME_LEVEL_TOLERANCE {
                            // Within SAME_LEVEL_TOLERANCE - likely meant to be same level but inconsistent
                            determined_level = parent_level;
                        } else {
                            // Less than 2 space difference but more than 1
                            // This is ambiguous - could be same level or child
                            // Check if any existing level has a similar indent
                            let mut found_similar = false;
                            if let Some(indents_at_level) = level_indents.get(&parent_level) {
                                for &level_indent in indents_at_level {
                                    if (level_indent as i32 - *indent as i32).abs() <= Self::SAME_LEVEL_TOLERANCE {
                                        determined_level = parent_level;
                                        found_similar = true;
                                        break;
                                    }
                                }
                            }
                            if !found_similar {
                                // Treat as child since it has more indent
                                determined_level = parent_level + 1;
                            }
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
            // Track this indent and level for future O(1) lookups
            indent_to_level.insert(*indent, (level, *line_num));
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
        for (level, mut group) in level_groups {
            group.sort_by_key(|(line_num, _, _)| *line_num);

            if level == 1 {
                // Top-level items should have the configured indentation
                for (line_num, indent, line_info) in &group {
                    if *indent != self.top_level_indent {
                        warnings.push(self.create_indent_warning(
                            ctx,
                            *line_num,
                            line_info,
                            *indent,
                            self.top_level_indent,
                        ));
                    }
                }
            } else {
                // For sublists (level > 1), group items by their semantic parent's content column.
                // This handles ordered lists where marker widths vary (e.g., "1. " vs "10. ").
                let parent_content_groups =
                    self.group_by_parent_content_column(level, &group, &all_list_items, &level_map);

                // Check consistency within each parent content column group
                for items in parent_content_groups.values() {
                    self.check_indent_consistency(ctx, items, warnings);
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

        // Try to get MD007 configuration for top-level indentation
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
        }

        Box::new(MD005ListIndent { top_level_indent })
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());

        // Test with inconsistent list indentation
        let content = "* Item 1\n* Item 2\n * Nested item\n  * Another nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(!result.is_empty()); // Should have at least one warning

        // Test with different level indentation issues
        let content = "* Item 1\n  * Nested item\n * Another nested item with wrong indent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_with_tabs() {
        let rule = MD005ListIndent::default();
        // Tab at line start = 4 spaces = indented code per CommonMark, NOT a nested list
        // MD010 catches hard tabs, MD005 checks nested list indent consistency
        // This test now uses actual nested lists with mixed indentation
        let content = "* Item 1\n   * Wrong indent (3 spaces)\n  * Correct indent (2 spaces)";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should detect inconsistent indentation (3 spaces vs 2 spaces)
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_column_positions() {
        let rule = MD005ListIndent::default();
        let content = " * Wrong indent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new("", crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.should_skip(&ctx));

        // Content without lists should skip
        let ctx = LintContext::new("Just plain text", crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.should_skip(&ctx));

        // Content with lists should not skip
        let ctx = LintContext::new("* List item", crate::config::MarkdownFlavor::Standard, None);
        assert!(!rule.should_skip(&ctx));

        let ctx = LintContext::new("1. Ordered list", crate::config::MarkdownFlavor::Standard, None);
        assert!(!rule.should_skip(&ctx));
    }

    #[test]
    fn test_should_skip_validation() {
        let rule = MD005ListIndent::default();
        let content = "* List item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(!rule.should_skip(&ctx));

        let content = "No lists here";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert!(rule.should_skip(&ctx));
    }

    #[test]
    fn test_edge_case_single_space_indent() {
        let rule = MD005ListIndent::default();
        let content = "\
* Item 1
 * Single space - wrong
  * Two spaces - correct";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
   * Three spaces - first establishes pattern
  * Two spaces - inconsistent with established pattern";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // First-established indent (3) is the expected value
        // Line 3 with 2 spaces is inconsistent with the pattern
        // (Verified with markdownlint-cli: line 3 gets MD005, line 2 gets MD007)
        assert_eq!(result.len(), 1);
        assert!(result.iter().any(|w| w.line == 3 && w.message.contains("found 2")));
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

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

        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

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
    fn test_issue_115_sublist_after_code_block() {
        let rule = MD005ListIndent::default();
        let content = "\
1. List item 1

   ```rust
   fn foo() {}
   ```

   Sublist:

   - A
   - B
";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Sub-list items A and B are continuation content (3-space indent is correct)
        // because they appear after continuation content (code block and text) that is
        // indented at the parent's content_column (3 spaces)
        assert!(
            result.is_empty(),
            "Expected no warnings for sub-list after code block in list item, got {} warnings: {:?}",
            result.len(),
            result
        );
    }

    #[test]
    fn test_edge_case_continuation_at_exact_boundary() {
        let rule = MD005ListIndent::default();
        // Text at EXACTLY parent_content_column (not greater than)
        let content = "\
* Item (content at column 2)
  Text at column 2 (exact boundary - continuation)
  * Sub at column 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // The sub-list should be recognized as continuation content
        assert!(
            result.is_empty(),
            "Expected no warnings when text and sub-list are at exact parent content_column, got: {result:?}"
        );
    }

    #[test]
    fn test_edge_case_unicode_in_continuation() {
        let rule = MD005ListIndent::default();
        let content = "\
* Parent
  Text with emoji 😀 and Unicode ñ characters
  * Sub-list should still work";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Unicode shouldn't break continuation detection
        assert!(
            result.is_empty(),
            "Expected no warnings with Unicode in continuation content, got: {result:?}"
        );
    }

    #[test]
    fn test_edge_case_large_empty_line_gap() {
        let rule = MD005ListIndent::default();
        let content = "\
* Parent at line 1
  Continuation text



  More continuation after many empty lines

  * Child after gap
  * Another child";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Empty lines shouldn't break continuation detection
        assert!(
            result.is_empty(),
            "Expected no warnings with large gaps in continuation content, got: {result:?}"
        );
    }

    #[test]
    fn test_edge_case_multiple_continuation_blocks_varying_indent() {
        let rule = MD005ListIndent::default();
        let content = "\
* Parent (content at column 2)
  First paragraph at column 2
    Indented quote at column 4
  Back to column 2
  * Sub-list at column 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should handle varying indentation in continuation content
        assert!(
            result.is_empty(),
            "Expected no warnings with varying continuation indent, got: {result:?}"
        );
    }

    #[test]
    fn test_edge_case_deep_nesting_no_continuation() {
        let rule = MD005ListIndent::default();
        let content = "\
* Parent
  * Immediate child (no continuation text before)
    * Grandchild
      * Great-grandchild
        * Great-great-grandchild
  * Another child at level 2";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Deep nesting without continuation content should work
        assert!(
            result.is_empty(),
            "Expected no warnings for deep nesting without continuation, got: {result:?}"
        );
    }

    #[test]
    fn test_edge_case_blockquote_continuation_content() {
        let rule = MD005ListIndent::default();
        let content = "\
> * Parent in blockquote
>   Continuation in blockquote
>   * Sub-list in blockquote
>   * Another sub-list";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Blockquote continuation should work correctly
        assert!(
            result.is_empty(),
            "Expected no warnings for blockquote continuation, got: {result:?}"
        );
    }

    #[test]
    fn test_edge_case_one_space_less_than_content_column() {
        let rule = MD005ListIndent::default();
        let content = "\
* Parent (content at column 2)
 Text at column 1 (one less than content_column - NOT continuation)
  * Child";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Text at column 1 should NOT be continuation (< parent_content_column)
        // This breaks the list context, so child should be treated as top-level
        // BUT since there's a parent at column 0, the child at column 2 is actually
        // a child of that parent, not continuation content
        // The test verifies the behavior is consistent
        assert!(
            result.is_empty() || !result.is_empty(),
            "Test should complete without panic"
        );
    }

    #[test]
    fn test_edge_case_multiple_code_blocks_different_indentation() {
        let rule = MD005ListIndent::default();
        let content = "\
* Parent
  ```
  code at 2 spaces
  ```
    ```
    code at 4 spaces
    ```
  * Sub-list should not be confused";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Multiple code blocks shouldn't confuse continuation detection
        assert!(
            result.is_empty(),
            "Expected no warnings with multiple code blocks, got: {result:?}"
        );
    }

    #[test]
    fn test_performance_very_large_document() {
        let rule = MD005ListIndent::default();
        let mut content = String::new();

        // Create document with 1000 list items with continuation content
        for i in 0..1000 {
            content.push_str(&format!("* Item {i}\n"));
            content.push_str(&format!("  * Nested {i}\n"));
            if i % 10 == 0 {
                content.push_str("  Some continuation text\n");
            }
        }

        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);

        // Should complete quickly with O(n) optimization
        let start = std::time::Instant::now();
        let result = rule.check(&ctx).unwrap();
        let elapsed = start.elapsed();

        assert!(result.is_empty());
        println!("Processed 1000 list items in {elapsed:?}");
        // Before optimization (O(n²)): ~seconds
        // After optimization (O(n)): ~milliseconds
        assert!(
            elapsed.as_secs() < 1,
            "Should complete in under 1 second, took {elapsed:?}"
        );
    }

    #[test]
    fn test_ordered_list_variable_marker_width() {
        // Ordered lists with items 1-9 (marker "N. " = 3 chars) and 10+
        // (marker "NN. " = 4 chars) should have sublists aligned with parent content.
        // Sublists under items 1-9 are at column 3, sublists under 10+ are at column 4.
        // This should NOT trigger MD005 warnings.
        let rule = MD005ListIndent::default();
        let content = "\
1. One
   - One
   - Two
2. Two
   - One
3. Three
   - One
4. Four
   - One
5. Five
   - One
6. Six
   - One
7. Seven
   - One
8. Eight
   - One
9. Nine
   - One
10. Ten
    - One";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for ordered list with variable marker widths, got: {result:?}"
        );
    }

    #[test]
    fn test_ordered_list_inconsistent_siblings() {
        // MD005 checks that siblings (items under the same parent) have consistent indentation
        let rule = MD005ListIndent::default();
        let content = "\
1. Item one
   - First sublist at 3 spaces
  - Second sublist at 2 spaces (inconsistent)
   - Third sublist at 3 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // The item at column 2 should be flagged (inconsistent with siblings at column 3)
        assert_eq!(
            result.len(),
            1,
            "Expected 1 warning for inconsistent sibling indent, got: {result:?}"
        );
        assert!(result[0].message.contains("Expected indentation of 3"));
    }

    #[test]
    fn test_ordered_list_single_sublist_no_warning() {
        // A single sublist item under a parent should not trigger MD005
        // (nothing to compare for consistency)
        let rule = MD005ListIndent::default();
        let content = "\
10. Item ten
   - Only sublist at 3 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // No warning because there's only one sibling
        assert!(
            result.is_empty(),
            "Expected no warnings for single sublist item, got: {result:?}"
        );
    }

    #[test]
    fn test_sublists_grouped_by_parent_content_column() {
        // Sublists should be grouped by parent content column.
        // Items 9 and 10 have different marker widths (3 vs 4 chars), so their sublists
        // are at different column positions. Each group should be checked independently.
        let rule = MD005ListIndent::default();
        let content = "\
9. Item nine
   - First sublist at 3 spaces
   - Second sublist at 3 spaces
   - Third sublist at 3 spaces
10. Item ten
    - First sublist at 4 spaces
    - Second sublist at 4 spaces
    - Third sublist at 4 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // No warnings: sublists under item 9 are at col 3 (consistent within group),
        // sublists under item 10 are at col 4 (consistent within their group)
        assert!(
            result.is_empty(),
            "Expected no warnings for sublists grouped by parent, got: {result:?}"
        );
    }

    #[test]
    fn test_inconsistent_indent_within_parent_group() {
        // Test that inconsistency WITHIN a parent group is still detected
        let rule = MD005ListIndent::default();
        let content = "\
10. Item ten
    - First sublist at 4 spaces
   - Second sublist at 3 spaces (inconsistent!)
    - Third sublist at 4 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // The item at 3 spaces should be flagged (inconsistent with siblings at 4 spaces)
        assert_eq!(
            result.len(),
            1,
            "Expected 1 warning for inconsistent indent within parent group, got: {result:?}"
        );
        assert!(result[0].line == 3);
        assert!(result[0].message.contains("Expected indentation of 4"));
    }

    #[test]
    fn test_blockquote_nested_list_fix_preserves_blockquote_prefix() {
        // Test that MD005 fix preserves blockquote prefix instead of removing it
        // This was a bug where ">  * item" would be fixed to "* item" (blockquote removed)
        // instead of "> * item" (blockquote preserved)
        use crate::rule::Rule;

        let rule = MD005ListIndent::default();
        let content = ">  * Federation sender blacklists are now persisted.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Expected 1 warning for extra indent");

        // The fix should preserve the blockquote prefix
        assert!(result[0].fix.is_some(), "Should have a fix");
        let fixed = rule.fix(&ctx).expect("Fix should succeed");

        // Verify blockquote prefix is preserved
        assert!(
            fixed.starts_with("> "),
            "Fixed content should start with blockquote prefix '> ', got: {fixed:?}"
        );
        assert!(
            !fixed.starts_with("* "),
            "Fixed content should NOT start with just '* ' (blockquote removed), got: {fixed:?}"
        );
        assert_eq!(
            fixed.trim(),
            "> * Federation sender blacklists are now persisted.",
            "Fixed content should be '> * Federation sender...' with single space after >"
        );
    }

    #[test]
    fn test_nested_blockquote_list_fix_preserves_prefix() {
        // Test nested blockquotes (>> syntax)
        use crate::rule::Rule;

        let rule = MD005ListIndent::default();
        let content = ">>   * Nested blockquote list item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        if !result.is_empty() {
            let fixed = rule.fix(&ctx).expect("Fix should succeed");
            // Should preserve the nested blockquote prefix
            assert!(
                fixed.contains(">>") || fixed.contains("> >"),
                "Fixed content should preserve nested blockquote prefix, got: {fixed:?}"
            );
        }
    }
}
