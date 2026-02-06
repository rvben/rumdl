use regex::Regex;
use std::sync::LazyLock;

use super::types::*;

/// Regex for detecting blockquote prefixes in list context
static BLOCKQUOTE_PREFIX_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^((?:\s*>\s*)+)").unwrap());

/// Parse all list blocks in the content (legacy line-by-line approach)
///
/// Uses a forward-scanning O(n) algorithm that tracks two variables during iteration:
/// - `has_list_breaking_content_since_last_item`: Set when encountering content that
///   terminates a list (headings, horizontal rules, tables, insufficiently indented content)
/// - `min_continuation_for_tracking`: Minimum indentation required for content to be
///   treated as list continuation (based on the list marker width)
///
/// When a new list item is encountered, we check if list-breaking content was seen
/// since the last item. If so, we start a new list block.
pub(super) fn parse_list_blocks(content: &str, lines: &[LineInfo]) -> Vec<ListBlock> {
    use crate::utils::code_block_utils::{CodeBlockContext, CodeBlockUtils};

    // Minimum indentation for unordered list continuation per CommonMark spec
    const UNORDERED_LIST_MIN_CONTINUATION_INDENT: usize = 2;

    /// Initialize or reset the forward-scanning tracking state.
    /// This helper eliminates code duplication across three initialization sites.
    #[inline]
    fn reset_tracking_state(
        list_item: &ListItemInfo,
        has_list_breaking_content: &mut bool,
        min_continuation: &mut usize,
    ) {
        *has_list_breaking_content = false;
        let marker_width = if list_item.is_ordered {
            list_item.marker.len() + 1 // Ordered markers need space after period/paren
        } else {
            list_item.marker.len()
        };
        *min_continuation = if list_item.is_ordered {
            marker_width
        } else {
            UNORDERED_LIST_MIN_CONTINUATION_INDENT
        };
    }

    // Cache debug env var to avoid repeated mutex acquisitions per line
    let debug_list = std::env::var("RUMDL_DEBUG_LIST").is_ok();

    // Pre-size based on lines that could be list items
    let mut list_blocks = Vec::with_capacity(lines.len() / 10); // Estimate ~10% of lines might start list blocks
    let mut current_block: Option<ListBlock> = None;
    let mut last_list_item_line = 0;
    let mut current_indent_level = 0;
    let mut last_marker_width = 0;

    // Track list-breaking content since last item (fixes O(n^2) bottleneck)
    let mut has_list_breaking_content_since_last_item = false;
    let mut min_continuation_for_tracking = 0;

    for (line_idx, line_info) in lines.iter().enumerate() {
        let line_num = line_idx + 1;

        // Enhanced code block handling using Design #3's context analysis
        if line_info.in_code_block {
            if let Some(ref mut block) = current_block {
                // Calculate minimum indentation for list continuation
                let min_continuation_indent =
                    CodeBlockUtils::calculate_min_continuation_indent(content, lines, line_idx);

                // Analyze code block context using the three-tier classification
                let context = CodeBlockUtils::analyze_code_block_context(lines, line_idx, min_continuation_indent);

                match context {
                    CodeBlockContext::Indented => {
                        // Code block is properly indented - continues the list
                        block.end_line = line_num;
                        continue;
                    }
                    CodeBlockContext::Standalone => {
                        // Code block separates lists - end current block
                        let completed_block = current_block.take().unwrap();
                        list_blocks.push(completed_block);
                        continue;
                    }
                    CodeBlockContext::Adjacent => {
                        // Edge case - use conservative behavior (continue list)
                        block.end_line = line_num;
                        continue;
                    }
                }
            } else {
                // No current list block - skip code block lines
                continue;
            }
        }

        // Extract blockquote prefix if any
        let blockquote_prefix = if let Some(caps) = BLOCKQUOTE_PREFIX_REGEX.captures(line_info.content(content)) {
            caps.get(0).unwrap().as_str().to_string()
        } else {
            String::new()
        };

        // Track list-breaking content for non-list, non-blank lines (O(n) replacement for nested loop)
        // Skip lines that are continuations of multi-line code spans - they're part of the previous list item
        if let Some(ref block) = current_block
            && line_info.list_item.is_none()
            && !line_info.is_blank
            && !line_info.in_code_span_continuation
        {
            let line_content = line_info.content(content).trim();

            // Check for structural separators that break lists
            // Note: Lazy continuation (indent=0) is valid in CommonMark and should NOT break lists.
            // Only lines with indent between 1 and min_continuation_for_tracking-1 break lists,
            // as they indicate improper indentation rather than lazy continuation.
            let is_lazy_continuation = line_info.indent == 0 && !line_info.is_blank;

            // Check if blockquote context changes (different prefix than current block)
            // Lines within the SAME blockquote context don't break lists
            let blockquote_prefix_changes = blockquote_prefix.trim() != block.blockquote_prefix.trim();

            let breaks_list = line_info.heading.is_some()
                || line_content.starts_with("---")
                || line_content.starts_with("***")
                || line_content.starts_with("___")
                || crate::utils::skip_context::is_table_line(line_content)
                || blockquote_prefix_changes
                || (line_info.indent > 0 && line_info.indent < min_continuation_for_tracking && !is_lazy_continuation);

            if breaks_list {
                has_list_breaking_content_since_last_item = true;
            }
        }

        // If this line is a code span continuation within an active list block,
        // extend the block's end_line to include this line (maintains list continuity)
        if line_info.in_code_span_continuation
            && line_info.list_item.is_none()
            && let Some(ref mut block) = current_block
        {
            block.end_line = line_num;
        }

        // Extend block.end_line for regular continuation lines (non-list-item, non-blank,
        // properly indented lines within the list). This ensures the workaround at line 2448
        // works correctly when there are multiple continuation lines before a nested list item.
        // Also include lazy continuation lines (indent=0) per CommonMark spec.
        // For blockquote lines, compute effective indent after stripping the prefix
        let effective_continuation_indent = if let Some(ref block) = current_block {
            let block_bq_level = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
            let line_content = line_info.content(content);
            let line_bq_level = line_content
                .chars()
                .take_while(|c| *c == '>' || c.is_whitespace())
                .filter(|&c| c == '>')
                .count();
            if line_bq_level > 0 && line_bq_level == block_bq_level {
                // Compute indent after blockquote markers
                let mut pos = 0;
                let mut found_markers = 0;
                for c in line_content.chars() {
                    pos += c.len_utf8();
                    if c == '>' {
                        found_markers += 1;
                        if found_markers == line_bq_level {
                            if line_content.get(pos..pos + 1) == Some(" ") {
                                pos += 1;
                            }
                            break;
                        }
                    }
                }
                let after_bq = &line_content[pos..];
                after_bq.len() - after_bq.trim_start().len()
            } else {
                line_info.indent
            }
        } else {
            line_info.indent
        };
        let adjusted_min_continuation_for_tracking = if let Some(ref block) = current_block {
            let block_bq_level = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
            if block_bq_level > 0 {
                if block.is_ordered { last_marker_width } else { 2 }
            } else {
                min_continuation_for_tracking
            }
        } else {
            min_continuation_for_tracking
        };
        // Lazy continuation allows unindented text to continue a list item,
        // but NOT structural elements like headings, code fences, or horizontal rules
        let is_structural_element = line_info.heading.is_some()
            || line_info.content(content).trim().starts_with("```")
            || line_info.content(content).trim().starts_with("~~~");
        let is_valid_continuation = effective_continuation_indent >= adjusted_min_continuation_for_tracking
            || (line_info.indent == 0 && !line_info.is_blank && !is_structural_element);

        if debug_list && line_info.list_item.is_none() && !line_info.is_blank {
            eprintln!(
                "[DEBUG] Line {}: checking continuation - indent={}, min_cont={}, is_valid={}, in_code_span={}, in_code_block={}, has_block={}",
                line_num,
                effective_continuation_indent,
                adjusted_min_continuation_for_tracking,
                is_valid_continuation,
                line_info.in_code_span_continuation,
                line_info.in_code_block,
                current_block.is_some()
            );
        }

        if !line_info.in_code_span_continuation
            && line_info.list_item.is_none()
            && !line_info.is_blank
            && !line_info.in_code_block
            && is_valid_continuation
            && let Some(ref mut block) = current_block
        {
            if debug_list {
                eprintln!(
                    "[DEBUG] Line {}: extending block.end_line from {} to {}",
                    line_num, block.end_line, line_num
                );
            }
            block.end_line = line_num;
        }

        // Flag to signal that current_block should be finalized after the borrow scope ends.
        // This avoids cloning the block just to push it and then set current_block to None.
        let mut finalize_current_block = false;

        // Check if this line is a list item
        if let Some(list_item) = &line_info.list_item {
            // Calculate nesting level based on indentation
            let item_indent = list_item.marker_column;
            let nesting = item_indent / 2; // Assume 2-space indentation for nesting

            if debug_list {
                eprintln!(
                    "[DEBUG] Line {}: list item found, marker={:?}, indent={}",
                    line_num, list_item.marker, item_indent
                );
            }

            if let Some(ref mut block) = current_block {
                // Check if this continues the current block
                let is_nested = nesting > block.nesting_level;
                let same_type =
                    (block.is_ordered && list_item.is_ordered) || (!block.is_ordered && !list_item.is_ordered);
                let same_context = block.blockquote_prefix == blockquote_prefix;
                // Allow one blank line after last item, or lines immediately after block content
                let reasonable_distance = line_num <= last_list_item_line + 2 || line_num == block.end_line + 1;

                // For unordered lists, also check marker consistency
                let marker_compatible =
                    block.is_ordered || block.marker.is_none() || block.marker.as_ref() == Some(&list_item.marker);

                // O(1) check: Use the tracked variable instead of O(n) nested loop
                let has_non_list_content = has_list_breaking_content_since_last_item;

                // A list continues if:
                // 1. It's a nested item (indented more than the parent), OR
                // 2. It's the same type at the same level with reasonable distance
                let mut continues_list = if is_nested {
                    // Nested items always continue the list if they're in the same context
                    same_context && reasonable_distance && !has_non_list_content
                } else {
                    // Same-level items need to match type and markers
                    same_type && same_context && reasonable_distance && marker_compatible && !has_non_list_content
                };

                if debug_list {
                    eprintln!(
                        "[DEBUG] Line {}: continues_list={}, is_nested={}, same_type={}, same_context={}, reasonable_distance={}, marker_compatible={}, has_non_list_content={}, last_item={}, block.end_line={}",
                        line_num,
                        continues_list,
                        is_nested,
                        same_type,
                        same_context,
                        reasonable_distance,
                        marker_compatible,
                        has_non_list_content,
                        last_list_item_line,
                        block.end_line
                    );
                }

                // WORKAROUND: If items are truly consecutive (no blank lines), they MUST be in the same list
                if !continues_list
                    && (is_nested || same_type)
                    && reasonable_distance
                    && line_num > 0
                    && block.end_line == line_num - 1
                {
                    continues_list = true;
                }

                if continues_list {
                    // Extend current block
                    block.end_line = line_num;
                    block.item_lines.push(line_num);

                    // Update max marker width
                    block.max_marker_width = block.max_marker_width.max(if list_item.is_ordered {
                        list_item.marker.len() + 1
                    } else {
                        list_item.marker.len()
                    });

                    // Update marker consistency for unordered lists
                    if !block.is_ordered && block.marker.is_some() && block.marker.as_ref() != Some(&list_item.marker) {
                        // Mixed markers, clear the marker field
                        block.marker = None;
                    }

                    // Reset tracked state
                    reset_tracking_state(
                        list_item,
                        &mut has_list_breaking_content_since_last_item,
                        &mut min_continuation_for_tracking,
                    );
                } else {
                    // End current block and start a new one
                    // When a different list type starts AT THE SAME LEVEL (not nested),
                    // trim back lazy continuation lines
                    if !same_type
                        && !is_nested
                        && let Some(&last_item) = block.item_lines.last()
                    {
                        block.end_line = last_item;
                    }

                    let new_block = ListBlock {
                        start_line: line_num,
                        end_line: line_num,
                        is_ordered: list_item.is_ordered,
                        marker: if list_item.is_ordered {
                            None
                        } else {
                            Some(list_item.marker.clone())
                        },
                        blockquote_prefix: blockquote_prefix.clone(),
                        item_lines: vec![line_num],
                        nesting_level: nesting,
                        max_marker_width: if list_item.is_ordered {
                            list_item.marker.len() + 1
                        } else {
                            list_item.marker.len()
                        },
                    };
                    let old_block = std::mem::replace(block, new_block);
                    list_blocks.push(old_block);

                    // Initialize tracked state for new block
                    reset_tracking_state(
                        list_item,
                        &mut has_list_breaking_content_since_last_item,
                        &mut min_continuation_for_tracking,
                    );
                }
            } else {
                // Start a new block
                current_block = Some(ListBlock {
                    start_line: line_num,
                    end_line: line_num,
                    is_ordered: list_item.is_ordered,
                    marker: if list_item.is_ordered {
                        None
                    } else {
                        Some(list_item.marker.clone())
                    },
                    blockquote_prefix,
                    item_lines: vec![line_num],
                    nesting_level: nesting,
                    max_marker_width: list_item.marker.len(),
                });

                // Initialize tracked state for new block
                reset_tracking_state(
                    list_item,
                    &mut has_list_breaking_content_since_last_item,
                    &mut min_continuation_for_tracking,
                );
            }

            last_list_item_line = line_num;
            current_indent_level = item_indent;
            last_marker_width = if list_item.is_ordered {
                list_item.marker.len() + 1 // Add 1 for the space after ordered list markers
            } else {
                list_item.marker.len()
            };
        } else if let Some(ref mut block) = current_block {
            // Not a list item - check if it continues the current block
            if debug_list {
                eprintln!(
                    "[DEBUG] Line {}: non-list-item, is_blank={}, block exists",
                    line_num, line_info.is_blank
                );
            }

            // Check if the last line in the list block ended with a backslash (hard line break)
            let prev_line_ends_with_backslash = if block.end_line > 0 && block.end_line - 1 < lines.len() {
                lines[block.end_line - 1].content(content).trim_end().ends_with('\\')
            } else {
                false
            };

            // Calculate minimum indentation for list continuation
            let min_continuation_indent = if block.is_ordered {
                current_indent_level + last_marker_width
            } else {
                current_indent_level + 2 // Unordered lists need at least 2 spaces (e.g., "- " = 2 chars)
            };

            if prev_line_ends_with_backslash || line_info.indent >= min_continuation_indent {
                // Indented line or backslash continuation continues the list
                if debug_list {
                    eprintln!(
                        "[DEBUG] Line {}: indented continuation (indent={}, min={})",
                        line_num, line_info.indent, min_continuation_indent
                    );
                }
                block.end_line = line_num;
            } else if line_info.is_blank {
                // Blank line - check if it's internal to the list or ending it
                if debug_list {
                    eprintln!("[DEBUG] Line {line_num}: entering blank line handling");
                }
                let mut check_idx = line_idx + 1;
                let mut found_continuation = false;

                // Skip additional blank lines
                while check_idx < lines.len() && lines[check_idx].is_blank {
                    check_idx += 1;
                }

                if check_idx < lines.len() {
                    let next_line = &lines[check_idx];
                    // For blockquote lines, compute indent AFTER stripping the blockquote prefix
                    let next_content = next_line.content(content);
                    let block_bq_level_for_indent = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
                    let next_bq_level_for_indent = next_content
                        .chars()
                        .take_while(|c| *c == '>' || c.is_whitespace())
                        .filter(|&c| c == '>')
                        .count();
                    let effective_indent =
                        if next_bq_level_for_indent > 0 && next_bq_level_for_indent == block_bq_level_for_indent {
                            let mut pos = 0;
                            let mut found_markers = 0;
                            for c in next_content.chars() {
                                pos += c.len_utf8();
                                if c == '>' {
                                    found_markers += 1;
                                    if found_markers == next_bq_level_for_indent {
                                        if next_content.get(pos..pos + 1) == Some(" ") {
                                            pos += 1;
                                        }
                                        break;
                                    }
                                }
                            }
                            let after_blockquote_marker = &next_content[pos..];
                            after_blockquote_marker.len() - after_blockquote_marker.trim_start().len()
                        } else {
                            next_line.indent
                        };
                    let adjusted_min_continuation = if block_bq_level_for_indent > 0 {
                        if block.is_ordered { last_marker_width } else { 2 }
                    } else {
                        min_continuation_indent
                    };
                    if debug_list {
                        eprintln!(
                            "[DEBUG] Blank line {} checking next line {}: effective_indent={}, adjusted_min={}, next_is_list={}, in_code_block={}",
                            line_num,
                            check_idx + 1,
                            effective_indent,
                            adjusted_min_continuation,
                            next_line.list_item.is_some(),
                            next_line.in_code_block
                        );
                    }
                    if !next_line.in_code_block && effective_indent >= adjusted_min_continuation {
                        found_continuation = true;
                    }
                    // Check if followed by another list item at the same level
                    else if !next_line.in_code_block
                        && next_line.list_item.is_some()
                        && let Some(item) = &next_line.list_item
                    {
                        let next_blockquote_prefix = BLOCKQUOTE_PREFIX_REGEX
                            .find(next_line.content(content))
                            .map_or(String::new(), |m| m.as_str().to_string());
                        if item.marker_column == current_indent_level
                            && item.is_ordered == block.is_ordered
                            && block.blockquote_prefix.trim() == next_blockquote_prefix.trim()
                        {
                            let block_bq_level = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
                            let _has_meaningful_content = (line_idx + 1..check_idx).any(|idx| {
                                if let Some(between_line) = lines.get(idx) {
                                    let between_content = between_line.content(content);
                                    let trimmed = between_content.trim();
                                    if trimmed.is_empty() {
                                        return false;
                                    }
                                    let line_indent = between_content.len() - between_content.trim_start().len();

                                    let between_bq_prefix = BLOCKQUOTE_PREFIX_REGEX
                                        .find(between_content)
                                        .map_or(String::new(), |m| m.as_str().to_string());
                                    let between_bq_level = between_bq_prefix.chars().filter(|&c| c == '>').count();
                                    let blockquote_level_changed =
                                        trimmed.starts_with(">") && between_bq_level != block_bq_level;

                                    if trimmed.starts_with("```")
                                        || trimmed.starts_with("~~~")
                                        || trimmed.starts_with("---")
                                        || trimmed.starts_with("***")
                                        || trimmed.starts_with("___")
                                        || blockquote_level_changed
                                        || crate::utils::skip_context::is_table_line(trimmed)
                                        || between_line.heading.is_some()
                                    {
                                        return true;
                                    }

                                    line_indent >= min_continuation_indent
                                } else {
                                    false
                                }
                            });

                            if block.is_ordered {
                                let has_structural_separators = (line_idx + 1..check_idx).any(|idx| {
                                    if let Some(between_line) = lines.get(idx) {
                                        let between_content = between_line.content(content);
                                        let trimmed = between_content.trim();
                                        if trimmed.is_empty() {
                                            return false;
                                        }
                                        let between_bq_prefix = BLOCKQUOTE_PREFIX_REGEX
                                            .find(between_content)
                                            .map_or(String::new(), |m| m.as_str().to_string());
                                        let between_bq_level = between_bq_prefix.chars().filter(|&c| c == '>').count();
                                        let blockquote_level_changed =
                                            trimmed.starts_with(">") && between_bq_level != block_bq_level;
                                        trimmed.starts_with("```")
                                            || trimmed.starts_with("~~~")
                                            || trimmed.starts_with("---")
                                            || trimmed.starts_with("***")
                                            || trimmed.starts_with("___")
                                            || blockquote_level_changed
                                            || crate::utils::skip_context::is_table_line(trimmed)
                                            || between_line.heading.is_some()
                                    } else {
                                        false
                                    }
                                });
                                found_continuation = !has_structural_separators;
                            } else {
                                let has_structural_separators = (line_idx + 1..check_idx).any(|idx| {
                                    if let Some(between_line) = lines.get(idx) {
                                        let between_content = between_line.content(content);
                                        let trimmed = between_content.trim();
                                        if trimmed.is_empty() {
                                            return false;
                                        }
                                        let between_bq_prefix = BLOCKQUOTE_PREFIX_REGEX
                                            .find(between_content)
                                            .map_or(String::new(), |m| m.as_str().to_string());
                                        let between_bq_level = between_bq_prefix.chars().filter(|&c| c == '>').count();
                                        let blockquote_level_changed =
                                            trimmed.starts_with(">") && between_bq_level != block_bq_level;
                                        trimmed.starts_with("```")
                                            || trimmed.starts_with("~~~")
                                            || trimmed.starts_with("---")
                                            || trimmed.starts_with("***")
                                            || trimmed.starts_with("___")
                                            || blockquote_level_changed
                                            || crate::utils::skip_context::is_table_line(trimmed)
                                            || between_line.heading.is_some()
                                    } else {
                                        false
                                    }
                                });
                                found_continuation = !has_structural_separators;
                            }
                        }
                    }
                }

                if debug_list {
                    eprintln!("[DEBUG] Blank line {line_num} final: found_continuation={found_continuation}");
                }
                if found_continuation {
                    // Include the blank line in the block
                    block.end_line = line_num;
                } else {
                    // Blank line ends the list - don't include it
                    finalize_current_block = true;
                }
            } else {
                // Check for lazy continuation
                let min_required_indent = if block.is_ordered {
                    current_indent_level + last_marker_width
                } else {
                    current_indent_level + 2
                };

                let line_content = line_info.content(content).trim();

                let looks_like_table = crate::utils::skip_context::is_table_line(line_content);

                let block_bq_level = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
                let current_bq_level = blockquote_prefix.chars().filter(|&c| c == '>').count();
                let blockquote_level_changed = line_content.starts_with(">") && current_bq_level != block_bq_level;

                let is_structural_separator = line_info.heading.is_some()
                    || line_content.starts_with("```")
                    || line_content.starts_with("~~~")
                    || line_content.starts_with("---")
                    || line_content.starts_with("***")
                    || line_content.starts_with("___")
                    || blockquote_level_changed
                    || looks_like_table;

                let is_lazy_continuation = !is_structural_separator
                    && !line_info.is_blank
                    && (line_info.indent == 0
                        || line_info.indent >= min_required_indent
                        || line_info.in_code_span_continuation);

                if is_lazy_continuation {
                    block.end_line = line_num;
                } else {
                    // Non-indented, non-blank line that's not a lazy continuation - end the block
                    finalize_current_block = true;
                }
            }
        }

        // Finalize the current block outside the borrow scope to avoid cloning
        if finalize_current_block && let Some(block) = current_block.take() {
            list_blocks.push(block);
        }
    }

    // Don't forget the last block
    if let Some(block) = current_block {
        list_blocks.push(block);
    }

    // Merge adjacent blocks that should be one
    merge_adjacent_list_blocks(content, &mut list_blocks, lines);

    list_blocks
}

/// Merge adjacent list blocks that should be treated as one
fn merge_adjacent_list_blocks(content: &str, list_blocks: &mut Vec<ListBlock>, lines: &[LineInfo]) {
    if list_blocks.len() < 2 {
        return;
    }

    let mut merger = ListBlockMerger::new(content, lines);
    *list_blocks = merger.merge(list_blocks);
}

/// Helper struct to manage the complex logic of merging list blocks
struct ListBlockMerger<'a> {
    content: &'a str,
    lines: &'a [LineInfo],
}

impl<'a> ListBlockMerger<'a> {
    fn new(content: &'a str, lines: &'a [LineInfo]) -> Self {
        Self { content, lines }
    }

    fn merge(&mut self, list_blocks: &[ListBlock]) -> Vec<ListBlock> {
        let mut merged = Vec::with_capacity(list_blocks.len());
        let mut current = list_blocks[0].clone();

        for next in list_blocks.iter().skip(1) {
            if self.should_merge_blocks(&current, next) {
                current = self.merge_two_blocks(current, next);
            } else {
                merged.push(current);
                current = next.clone();
            }
        }

        merged.push(current);
        merged
    }

    /// Determine if two adjacent list blocks should be merged
    fn should_merge_blocks(&self, current: &ListBlock, next: &ListBlock) -> bool {
        // Basic compatibility checks
        if !self.blocks_are_compatible(current, next) {
            return false;
        }

        // Check spacing and content between blocks
        let spacing = self.analyze_spacing_between(current, next);
        match spacing {
            BlockSpacing::Consecutive => true,
            BlockSpacing::SingleBlank => self.can_merge_with_blank_between(current, next),
            BlockSpacing::MultipleBlanks | BlockSpacing::ContentBetween => {
                self.can_merge_with_content_between(current, next)
            }
        }
    }

    /// Check if blocks have compatible structure for merging
    fn blocks_are_compatible(&self, current: &ListBlock, next: &ListBlock) -> bool {
        current.is_ordered == next.is_ordered
            && current.blockquote_prefix == next.blockquote_prefix
            && current.nesting_level == next.nesting_level
    }

    /// Analyze the spacing between two list blocks
    fn analyze_spacing_between(&self, current: &ListBlock, next: &ListBlock) -> BlockSpacing {
        let gap = next.start_line - current.end_line;

        match gap {
            1 => BlockSpacing::Consecutive,
            2 => BlockSpacing::SingleBlank,
            _ if gap > 2 => {
                if self.has_only_blank_lines_between(current, next) {
                    BlockSpacing::MultipleBlanks
                } else {
                    BlockSpacing::ContentBetween
                }
            }
            _ => BlockSpacing::Consecutive, // gap == 0, overlapping (shouldn't happen)
        }
    }

    /// Check if unordered lists can be merged with a single blank line between
    fn can_merge_with_blank_between(&self, current: &ListBlock, next: &ListBlock) -> bool {
        if has_meaningful_content_between(self.content, current, next, self.lines) {
            return false; // Structural separators prevent merging
        }

        // Only merge unordered lists with same marker across single blank
        !current.is_ordered && current.marker == next.marker
    }

    /// Check if ordered lists can be merged when there's content between them
    fn can_merge_with_content_between(&self, current: &ListBlock, next: &ListBlock) -> bool {
        if has_meaningful_content_between(self.content, current, next, self.lines) {
            return false; // Structural separators prevent merging
        }

        // Only consider merging ordered lists if there's no structural content between
        current.is_ordered && next.is_ordered
    }

    /// Check if there are only blank lines between blocks
    fn has_only_blank_lines_between(&self, current: &ListBlock, next: &ListBlock) -> bool {
        for line_num in (current.end_line + 1)..next.start_line {
            if let Some(line_info) = self.lines.get(line_num - 1)
                && !line_info.content(self.content).trim().is_empty()
            {
                return false;
            }
        }
        true
    }

    /// Merge two compatible list blocks into one
    fn merge_two_blocks(&self, mut current: ListBlock, next: &ListBlock) -> ListBlock {
        current.end_line = next.end_line;
        current.item_lines.extend_from_slice(&next.item_lines);

        // Update max marker width
        current.max_marker_width = current.max_marker_width.max(next.max_marker_width);

        // Handle marker consistency for unordered lists
        if !current.is_ordered && self.markers_differ(&current, next) {
            current.marker = None; // Mixed markers
        }

        current
    }

    /// Check if two blocks have different markers
    fn markers_differ(&self, current: &ListBlock, next: &ListBlock) -> bool {
        current.marker.is_some() && next.marker.is_some() && current.marker != next.marker
    }
}

/// Types of spacing between list blocks
#[derive(Debug, PartialEq)]
enum BlockSpacing {
    Consecutive,    // No gap between blocks
    SingleBlank,    // One blank line between blocks
    MultipleBlanks, // Multiple blank lines but no content
    ContentBetween, // Content exists between blocks
}

/// Check if there's meaningful content (not just blank lines) between two list blocks
fn has_meaningful_content_between(content: &str, current: &ListBlock, next: &ListBlock, lines: &[LineInfo]) -> bool {
    // Check lines between current.end_line and next.start_line
    for line_num in (current.end_line + 1)..next.start_line {
        if let Some(line_info) = lines.get(line_num - 1) {
            // Convert to 0-indexed
            let trimmed = line_info.content(content).trim();

            // Skip empty lines
            if trimmed.is_empty() {
                continue;
            }

            // Check for structural separators that should separate lists (CommonMark compliant)

            // Headings separate lists
            if line_info.heading.is_some() {
                return true;
            }

            // Horizontal rules separate lists (---, ***, ___)
            if is_horizontal_rule(trimmed) {
                return true;
            }

            // Tables separate lists
            if crate::utils::skip_context::is_table_line(trimmed) {
                return true;
            }

            // Blockquotes separate lists
            if trimmed.starts_with('>') {
                return true;
            }

            // Code block fences separate lists (unless properly indented as list content)
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                let line_indent = line_info.byte_len - line_info.content(content).trim_start().len();

                let min_continuation_indent = if current.is_ordered {
                    current.nesting_level + current.max_marker_width + 1 // +1 for space after marker
                } else {
                    current.nesting_level + 2
                };

                if line_indent < min_continuation_indent {
                    return true;
                }
            }

            // Check if this line has proper indentation for list continuation
            let line_indent = line_info.byte_len - line_info.content(content).trim_start().len();

            let min_indent = if current.is_ordered {
                current.nesting_level + current.max_marker_width
            } else {
                current.nesting_level + 2
            };

            if line_indent < min_indent {
                return true;
            }
        }
    }

    false
}
