/// Rule MD007: Unordered list indentation
///
/// See [docs/md007.md](../../docs/md007.md) for full documentation, configuration, and examples.
use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;

pub mod md007_config;
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

    /// Pop list-stack entries that a content line at (`bq_depth`, `visual_indent`)
    /// has closed. An open item still contains the line only when the line stays
    /// in the item's blockquote context (or a deeper one) and begins at or past
    /// the item's content column; otherwise the item has ended. Keeping the stack
    /// accurate prevents a later list from being mistaken for a sublist of an item
    /// that already closed (which would, e.g., wrongly extend the ordered-ancestor
    /// exemption past a terminating paragraph, blockquote, or code block).
    /// Visual indentation of a line measured in the same coordinate space the
    /// stack uses for `content_col`: for a blockquoted line that is the width of
    /// the leading whitespace *after* the `>` prefix(es); for any other line it is
    /// the absolute `visual_indent`. Comparing a blockquoted line's absolute indent
    /// (which counts the `>` markers) against a blockquote-relative content column
    /// would otherwise treat in-quote content as if it had dedented out of the item.
    /// Measure the line's indentation in the coordinate space of a blockquote at the
    /// given nesting `depth`: strip exactly `depth` `>` markers (each with one optional
    /// following space or tab) and return the leading whitespace of the remainder as
    /// visual columns. At depth 0 this is the line's own visual indent.
    ///
    /// The remainder may itself begin with deeper `>` markers; the whitespace measured
    /// is whatever precedes them, so an interrupting deeper quote reports the column at
    /// which its `>` begins inside the shallower container. That lets a closed-item check
    /// compare the line against an item using the item's own quote coordinate space,
    /// avoiding any relative-vs-absolute mismatch.
    fn indent_relative_to_depth(
        ctx: &crate::lint_context::LintContext,
        line_info: &crate::lint_context::LineInfo,
        depth: usize,
    ) -> usize {
        if depth == 0 {
            return line_info.visual_indent;
        }
        // The blockquote's pre-parsed `content` has its leading whitespace stripped,
        // so it cannot report the in-quote indentation. Walk the `>` prefix(es) on the
        // raw line (mirroring the list-item indent calculation) and measure the
        // whitespace that follows, which is the indent inside the quote container.
        let line_content = line_info.content(ctx.content);
        let mut remaining = line_content;
        let mut content_start = 0;
        let mut stripped_levels = 0;
        while stripped_levels < depth {
            let trimmed = remaining.trim_start();
            if !trimmed.starts_with('>') {
                break;
            }
            content_start += remaining.len() - trimmed.len();
            content_start += 1;
            let after_gt = &trimmed[1..];
            if let Some(stripped) = after_gt.strip_prefix(' ') {
                content_start += 1;
                remaining = stripped;
            } else if let Some(stripped) = after_gt.strip_prefix('\t') {
                content_start += 1;
                remaining = stripped;
            } else {
                remaining = after_gt;
            }
            stripped_levels += 1;
        }
        let content_after_prefix = &line_content[content_start..];
        let ws_chars = content_after_prefix
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();
        Self::char_pos_to_visual_column(content_after_prefix, ws_chars)
    }

    fn terminate_closed_items(
        ctx: &crate::lint_context::LintContext,
        line_info: &crate::lint_context::LineInfo,
        list_stack: &mut Vec<(usize, usize, bool, usize, usize, bool)>,
        line_bq_depth: usize,
    ) {
        while let Some(&(_, _, _, content_col, item_bq_depth, _)) = list_stack.last() {
            let closed = match item_bq_depth.cmp(&line_bq_depth) {
                // The line has exited a deeper blockquote the item lived in.
                std::cmp::Ordering::Greater => true,
                // The line is in the same or a deeper blockquote than the item.
                // Measure the line's indent in the item's own quote coordinate space
                // and close the item when the line begins left of the item's content.
                // For a same-depth line this is the in-container indent; for a deeper
                // interrupting quote it is the column where that quote's `>` begins
                // inside the item's container, so a `> > quote` left of the item's
                // content (e.g. interrupting `> 1. ordered`) closes it, while a quote
                // indented into the item's content keeps it open.
                std::cmp::Ordering::Equal | std::cmp::Ordering::Less => {
                    content_col > Self::indent_relative_to_depth(ctx, line_info, item_bq_depth)
                }
            };
            if closed {
                list_stack.pop();
            } else {
                break;
            }
        }
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
        if self.config.indent_explicit {
            match parent_info {
                Some((true, parent_content_col)) => {
                    // Parent is ordered: return text-aligned as primary expected value.
                    // The caller also accepts the fixed indent as an alternative.
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
        let mut list_stack: Vec<(usize, usize, bool, usize, usize, bool)> = Vec::new(); // Stack of (marker_visual_col, line_num, is_ordered, content_visual_col, blockquote_depth, exempt) for tracking nesting. `exempt` marks an unordered item that inherited the ordered-ancestor MD007 exemption.

        for (line_idx, line_info) in ctx.lines.iter().enumerate() {
            // Skip if this line is in a code block, front matter, or mkdocstrings
            let is_skipped_region = |info: &crate::lint_context::LineInfo| {
                info.in_code_block || info.in_front_matter || info.in_mkdocstrings || info.in_footnote_definition
            };
            // Exception: a fenced code block can open on a list-marker line
            // (e.g. "- ```"). Such a line is flagged `in_code_block` but is
            // genuinely a list item, so it must fall through to the list-item
            // handling below to be pushed onto the ancestor stack; otherwise its
            // descendants resolve one nesting level too shallow and get wrongly
            // flagged (and "fixed") as over-indented. The line must ITSELF open a
            // backtick/tilde fence: a list-like line interior to a code construct
            // that pulldown-cmark does not parse (e.g. an Azure `:::` block) is also
            // `in_code_block` with a `list_item`, but it is opaque code, not a list
            // item, so it stays skipped. The other skipped regions (front matter,
            // mkdocstrings, footnote definitions) genuinely contain their list
            // items, so those are still skipped.
            let opens_fence_on_marker_line = line_info
                .list_item
                .as_ref()
                .and_then(|item| line_info.content(ctx.content).get(item.content_column..))
                .is_some_and(|after_marker| {
                    let after_marker = after_marker.trim_start();
                    after_marker.starts_with("```") || after_marker.starts_with("~~~")
                });
            let fence_opening_marker_line = opens_fence_on_marker_line
                && line_info.in_code_block
                && !line_info.in_front_matter
                && !line_info.in_mkdocstrings
                && !line_info.in_footnote_definition;
            if is_skipped_region(line_info) && !fence_opening_marker_line {
                // The opening line of such a region (e.g. an unindented code fence)
                // breaks out of any open list just like a paragraph would, so the
                // stale list stack must be cleared even though the region's lines
                // are otherwise skipped. Interior lines (code contents, etc.) are
                // immaterial: only act on the region's first non-blank line, using
                // its indentation to decide which items it closed.
                let region_start = line_idx == 0 || !is_skipped_region(&ctx.lines[line_idx - 1]);
                if region_start && !line_info.is_blank {
                    let bq_depth = line_info.blockquote.as_ref().map_or(0, |bq| bq.nesting_level);
                    Self::terminate_closed_items(ctx, line_info, &mut list_stack, bq_depth);
                }
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

                // Determine blockquote depth for this line
                let bq_depth = line_info.blockquote.as_ref().map_or(0, |bq| bq.nesting_level);

                // Clean up stack - remove items at same or deeper indentation,
                // but only consider items at the same blockquote depth
                while let Some(&(indent, _, _, _, item_bq_depth, _)) = list_stack.last() {
                    if item_bq_depth == bq_depth && indent >= visual_marker_for_nesting {
                        list_stack.pop();
                    } else if item_bq_depth > bq_depth {
                        // Pop items from deeper blockquote contexts that we've left
                        list_stack.pop();
                    } else {
                        break;
                    }
                }

                // The loop above only reconciles items at the same (or deeper)
                // blockquote depth. A list item that enters a deeper blockquote than an
                // ancestor (e.g. `> > - item` after `> 1. ordered`, or `> - item` after
                // a top-level `1. ordered`) starts a separate container when that quote
                // begins left of the ancestor's content. Measured in the ancestor's own
                // quote coordinate space, the deeper quote's marker is then to the left
                // of the ancestor's content column, so the ancestor is closed. Pop it
                // here, otherwise a closed ordered ancestor would linger and wrongly
                // extend its exemption to a later, separately indented unordered list.
                // A deeper quote indented into the ancestor's content is part of that
                // item and keeps it open. Same-depth nesting and items already inside a
                // blockquote are left to the loop above and the exemption check below.
                while let Some(&(_, _, _, content_col, item_bq_depth, _)) = list_stack.last() {
                    if item_bq_depth < bq_depth
                        && content_col > Self::indent_relative_to_depth(ctx, line_info, item_bq_depth)
                    {
                        list_stack.pop();
                    } else {
                        break;
                    }
                }

                // For ordered list items, just track them in the stack
                if list_item.is_ordered {
                    // For ordered lists, we don't check indentation but we need to track for text-aligned children
                    // Use the actual positions since we don't enforce indentation for ordered lists
                    list_stack.push((
                        visual_marker_column,
                        line_idx,
                        true,
                        visual_content_column,
                        bq_depth,
                        false,
                    ));
                    continue;
                }

                // At this point, we know this is an unordered list item.
                //
                // markdownlint applies MD007 to a sublist only if its parent lists
                // are all also unordered. An unordered item that is genuinely nested
                // under an ordered ancestor is therefore exempt from the indentation
                // check, at any depth. Two conditions must both hold:
                //
                //   1. threshold: an ordered ancestor at this blockquote depth has its
                //      content column at or left of this bullet's marker, so the bullet
                //      is indented far enough to be that ordered item's sublist. A
                //      bullet indented less than the ordered content column is a new
                //      top-level list, which markdownlint still checks.
                //   2. chain: the nearest same-depth ancestor is itself ordered, or is
                //      an unordered item that already inherited the exemption. This
                //      stops the exemption from leaking past a non-nested unordered
                //      parent to its children. For `100. ordered` / `   - parent` /
                //      `     - child`, the parent is left of the ordered content column
                //      (not nested, not exempt), so the child resolves against the real
                //      unordered layout and is still checked.
                //
                // The MkDocs flavor is excluded: it deliberately enforces
                // Python-Markdown's stricter continuation indent under ordered parents
                // (insufficient indent there is a real rendering bug, not a style nit).
                let threshold_ok = list_stack
                    .iter()
                    .any(|item| item.4 == bq_depth && item.2 && item.3 <= visual_marker_column);
                let chain_ok = list_stack
                    .iter()
                    .rev()
                    .find(|item| item.4 == bq_depth)
                    .is_some_and(|item| item.2 || item.5);
                if ctx.flavor != crate::config::MarkdownFlavor::MkDocs && threshold_ok && chain_ok {
                    list_stack.push((
                        visual_marker_column,
                        line_idx,
                        false,
                        visual_content_column,
                        bq_depth,
                        true,
                    ));
                    continue;
                }

                // Count only items at the same blockquote depth for nesting level
                let nesting_level = list_stack.iter().filter(|item| item.4 == bq_depth).count();

                // Get parent info for per-parent calculation (only from same blockquote depth)
                let parent_info = list_stack
                    .iter()
                    .rev()
                    .find(|item| item.4 == bq_depth)
                    .map(|&(_, _, is_ordered, content_col, _, _)| (is_ordered, content_col));

                // Calculate expected indent using per-parent logic
                // When start_indented is true, only depth-0 items use the start_indent value.
                // For nested items (depth >= 1), the parent's actual position in the stack
                // already reflects the start_indent shift, so calculate_expected_indent
                // naturally produces the correct result.
                let mut expected_indent = if self.config.start_indented && nesting_level == 0 {
                    self.config.start_indent.get() as usize
                } else {
                    self.calculate_expected_indent(nesting_level, parent_info)
                };

                // When indent is explicitly set and parent is ordered, also accept
                // the fixed indent value (nesting_level * indent). This lets users
                // choose either text-aligned or their configured indent under ordered lists.
                let also_acceptable =
                    if self.config.indent_explicit && parent_info.is_some_and(|(is_ordered, _)| is_ordered) {
                        Some(nesting_level * self.config.indent.get() as usize)
                    } else {
                        None
                    };

                // MkDocs (Python-Markdown) uses 4-space-tab continuation for list items.
                // Under an ordered list item, Python-Markdown requires at least
                // marker_column + 4 spaces for continuation content to be recognized.
                if ctx.flavor == crate::config::MarkdownFlavor::MkDocs
                    && let Some(&(parent_marker_col, _, true, _, _, _)) =
                        list_stack.iter().rev().find(|item| item.4 == bq_depth && item.2)
                {
                    expected_indent = expected_indent.max(parent_marker_col + 4);
                }

                // Add current item to stack
                // Use actual marker position for cleanup logic
                // For text-aligned children, store the EXPECTED content position after fix
                // (not the actual position) to prevent error cascade
                // When accepted via also_acceptable, use that indent for content col
                let accepted_indent = if also_acceptable.is_some_and(|alt| visual_marker_column == alt) {
                    visual_marker_column
                } else {
                    expected_indent
                };
                // Store the content column the item will have *after* its indent is
                // fixed: the corrected marker column plus this marker's actual width
                // (marker char + the spaces after it). Using the real width rather than a
                // hard-coded 2 keeps a child aligned to a parent whose marker was widened
                // by a non-default MD030 (e.g. `-   ` under `ul-multi = 3`, content column
                // 4); hard-coding 2 stored column 2 and flagged the correctly-nested child
                // as over-indented, then "fixed" it to a column where it detaches into a
                // sibling. For the common single-space marker the width is 2, so the
                // stored value is unchanged.
                let marker_width = visual_content_column.saturating_sub(visual_marker_column);
                let expected_content_visual_col = accepted_indent + marker_width;
                list_stack.push((
                    visual_marker_column,
                    line_idx,
                    false,
                    expected_content_visual_col,
                    bq_depth,
                    false,
                ));

                // A top-level item (depth 0) is expected at column 0 when start_indented
                // is false. Column 0 is already correct, so skip it; any other column
                // (1, 2, or 3) is a misindented top-level list and must be flagged with
                // "Expected 0". Four or more leading spaces form an indented code block,
                // not a list, so such lines never reach here as list items.
                if !self.config.start_indented && nesting_level == 0 && visual_marker_column == 0 {
                    continue;
                }

                if visual_marker_column != expected_indent && also_acceptable != Some(visual_marker_column) {
                    // Use the fixed indent as the suggested value when the alternative was available
                    if let Some(alt) = also_acceptable {
                        expected_indent = alt;
                    }
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

                        Some(crate::rule::Fix::new(start_byte..end_byte, replacement))
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
            } else if !line_info.is_blank {
                // A non-blank, non-list content line that breaks out of the open
                // list terminates every list item whose content begins to its
                // right: an item's children must be indented past its content
                // column, so a line indented less cannot belong to it. Popping
                // these closed items keeps list_stack accurate, so a later list
                // is not mistaken for a sublist of one that has already ended
                // (e.g. a top-level paragraph closing an ordered list, after
                // which a separately indented bullet is a new top-level list).
                //
                // A CommonMark lazy continuation line is the exception: plain
                // paragraph text that immediately follows the item (no blank line
                // between) continues the item's open paragraph and so keeps the
                // list open. Constructs that interrupt a paragraph (ATX heading,
                // thematic break, fenced code, HTML block, HTML comment, div block)
                // end the list even without a blank line, matching markdownlint. A
                // line beginning with
                // a list marker is likewise not lazy paragraph text - it would start
                // a new list item - so it must still terminate stale ancestors (e.g.
                // a deeper bullet that pulldown-cmark treats as item content rather
                // than a sublist).
                //
                // Blockquotes need container awareness: a continuation in the *same*
                // quote (`> text` after `> 1. item`) is lazy, but newly entering a
                // quote (`> text` after a non-quoted item) interrupts the paragraph
                // and ends the list. So compare the previous line's quote depth, and
                // examine the marker on the quote-stripped content.
                let bq_depth = line_info.blockquote.as_ref().map_or(0, |bq| bq.nesting_level);
                let prev_line = line_idx.checked_sub(1).map(|i| &ctx.lines[i]);
                let prev_blank = prev_line.is_none_or(|p| p.is_blank);
                let prev_bq_depth = prev_line
                    .and_then(|p| p.blockquote.as_ref())
                    .map_or(0, |bq| bq.nesting_level);
                let same_container = prev_bq_depth == bq_depth;
                let text = line_info
                    .blockquote
                    .as_ref()
                    .map_or_else(|| line_info.content(ctx.content), |bq| bq.content.as_str());
                let trimmed = text.trim_start();
                let starts_like_list_marker = match trimmed.as_bytes().first() {
                    Some(b'-' | b'*' | b'+') => {
                        matches!(trimmed.as_bytes().get(1), Some(b' ' | b'\t'))
                    }
                    Some(c) if c.is_ascii_digit() => {
                        // CommonMark allows at most 9 digits in an ordered list marker.
                        // A longer digit run is not a marker, so the line can be lazy
                        // paragraph text rather than a list-interrupting item.
                        let after_digits = trimmed.trim_start_matches(|ch: char| ch.is_ascii_digit());
                        let num_digits = trimmed.len() - after_digits.len();
                        let mut rest = after_digits.chars();
                        (1..=9).contains(&num_digits)
                            && matches!(rest.next(), Some('.' | ')'))
                            && matches!(rest.next(), Some(' ' | '\t') | None)
                    }
                    _ => false,
                };
                // Lazy continuation only extends an OPEN paragraph. The previous line
                // must itself be paragraph text (or a list-item line whose paragraph the
                // current line continues), not a closed block such as a fenced code
                // block, heading, thematic break, HTML block/comment, or div marker.
                // After such a block, an unindented line starts a new paragraph and
                // closes the list instead of lazily continuing it.
                let prev_is_open_paragraph = prev_line.is_some_and(|p| {
                    !p.is_blank
                        && !p.in_code_block
                        && p.heading.is_none()
                        && !p.is_horizontal_rule
                        && !p.in_html_block
                        && !p.in_html_comment
                        && !p.is_div_marker
                });
                let is_lazy_paragraph_continuation = !prev_blank
                    && prev_is_open_paragraph
                    && same_container
                    && !starts_like_list_marker
                    && line_info.heading.is_none()
                    && !line_info.is_horizontal_rule
                    && !line_info.in_code_block
                    && !line_info.in_html_block
                    && !line_info.in_html_comment
                    && !line_info.is_div_marker;
                if is_lazy_paragraph_continuation {
                    // Lazy continuation: the list stays open, leave the stack intact.
                    continue;
                }
                Self::terminate_closed_items(ctx, line_info, &mut list_stack, bq_depth);
            }
        }
        Ok(warnings)
    }

    /// Optimized check using document structure
    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // Get all warnings with their fixes
        let warnings = self.check(ctx)?;
        let warnings =
            crate::utils::fix_utils::filter_warnings_by_inline_config(warnings, ctx.inline_config(), self.name());

        // If no warnings, return original content
        if warnings.is_empty() {
            return Ok(ctx.content.to_string());
        }

        // Collect all fixes and sort by range start (descending) to apply from end to beginning
        let mut fixes: Vec<_> = warnings
            .iter()
            .filter_map(|w| w.fix.as_ref().map(|f| (f.range.start, f.range.end, &f.replacement)))
            .collect();
        fixes.sort_by_key(|f| std::cmp::Reverse(f.0));

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

        // MkDocs/Python-Markdown requires 4-space indentation for nested list content.
        // Enforce indent=4 and style=fixed regardless of user config.
        if config.markdown_flavor() == crate::config::MarkdownFlavor::MkDocs {
            if rule_config.indent_explicit && rule_config.indent.get() < 4 {
                eprintln!(
                    "\x1b[33m[config warning]\x1b[0m MD007: MkDocs flavor requires indent >= 4 \
                     (Python-Markdown enforces 4-space indentation). \
                     Overriding indent={} to indent=4.",
                    rule_config.indent.get()
                );
            }
            if rule_config.style_explicit && rule_config.style == md007_config::IndentStyle::TextAligned {
                eprintln!(
                    "\x1b[33m[config warning]\x1b[0m MD007: MkDocs flavor requires style=\"fixed\" \
                     (Python-Markdown uses fixed 4-space indentation). \
                     Overriding style=\"text-aligned\" to style=\"fixed\"."
                );
            }
            if rule_config.indent.get() < 4 {
                rule_config.indent = crate::types::IndentSize::from_const(4);
            }
            rule_config.style = md007_config::IndentStyle::Fixed;
        }

        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;
    use crate::rule::Rule;
    use indoc::indoc;

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
    fn test_start_indented_false_flags_indented_first_level() {
        let rule = MD007ULIndent::default(); // start_indented is false by default

        // When start_indented is false, a top-level item is expected at column 0. A
        // top-level item indented 1-3 spaces is a misindented list and must be flagged
        // with "Expected 0", matching markdownlint-cli2 (which reports Expected: 0;
        // Actual: 3 here).
        let content = "   * Item 1"; // First level at 3 spaces
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.iter().any(|w| w.line == 1 && w.message.contains("Expected 0")),
            "a top-level item indented 3 spaces must be flagged with Expected 0, got: {result:?}"
        );

        // A correctly nested list (0/2/4 spaces) produces no warnings: these are a
        // top-level item and its properly indented descendants, not three first-level
        // items.
        let content = "* Item 1\n  * Item 2\n    * Item 3";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "a correctly nested 0/2/4-space list should produce no warnings, got: {result:?}"
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
        // Inside the blockquote the bullet is indented away from column 0, so it is a
        // misindented top-level list and is flagged with "Expected 0", matching
        // markdownlint-cli2 (which flags Expected: 0). The reported actual column
        // reflects rumdl's CommonMark tab-stop expansion rather than a raw char count.
        assert!(
            result.iter().any(|w| w.line == 1 && w.message.contains("Expected 0")),
            "an indented blockquoted top-level item must be flagged with Expected 0, got: {result:?}"
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
        // When indent is explicitly set, both text-aligned and fixed indent are accepted
        // under ordered parents, since the user wants their configured indent but
        // text-aligned is also valid for ordered list children.
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: false,
            indent_explicit: true, // User set indent=4
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // 4-space indent under "1. " should pass (matches configured indent)
        let content = "1. Ordered\n    * Bullet with 4-space indent";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "4-space indent under ordered should pass with indent=4: {result:?}"
        );

        // 3-space indent under "1. " should also pass (text-aligned with "1. ")
        let content_3 = "1. Ordered\n   * Bullet with 3-space indent";
        let ctx = LintContext::new(content_3, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "3-space indent under ordered should pass (text-aligned): {result:?}"
        );

        // 2-space indent under "1. " should be wrong (neither text-aligned nor fixed)
        let wrong_content = "1. Ordered\n  * Bullet with 2-space indent";
        let ctx = LintContext::new(wrong_content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            !result.is_empty(),
            "2-space indent under ordered list should be flagged when indent=4: {result:?}"
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
        // Level 3: bullet - text-aligned=11 (3 chars for "1. " from col 8), fixed=12
        // Both 11 (text-aligned) and 12 (fixed) should be accepted
        let content_text_aligned = r#"* Level 0
    * Level 1 (4-space indent from bullet parent)
        1. Level 2 ordered
           * Level 3 bullet (text-aligned under ordered)"#;
        let ctx = LintContext::new(content_text_aligned, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Text-aligned nesting under ordered should pass: {result:?}"
        );

        let content_fixed = r#"* Level 0
    * Level 1 (4-space indent from bullet parent)
        1. Level 2 ordered
            * Level 3 bullet (fixed indent under ordered)"#;
        let ctx = LintContext::new(content_fixed, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Fixed indent nesting under ordered should also pass: {result:?}"
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

        // "10. " = 4 chars, text-aligned = 4, fixed = 4
        let content = "10. Double digit\n    * Bullet at col 4";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Bullet under '10.' should align at column 4: {result:?}"
        );

        // Single digit "1. " = 3 chars, text-aligned = 3, fixed = 4
        // Both should be accepted under ordered parent with explicit indent
        let content_3 = "1. Single digit\n   * Bullet at col 3";
        let ctx = LintContext::new(content_3, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Bullet under '1.' with 3-space indent should pass (text-aligned): {result:?}"
        );

        let content_4 = "1. Single digit\n    * Bullet at col 4";
        let ctx = LintContext::new(content_4, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Bullet under '1.' with 4-space indent should pass (fixed): {result:?}"
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

    #[test]
    fn test_mkdocs_ordered_list_with_4_space_nested_unordered() {
        // MkDocs (Python-Markdown) requires 4-space continuation for ordered
        // list items. `1. text` has content at column 3, but Python-Markdown
        // needs marker_col + 4 = 4 spaces minimum.
        let rule = MD007ULIndent::default();
        let content = "1. text\n\n    - nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "4-space indent under ordered list should be valid in MkDocs flavor, got: {result:?}"
        );
    }

    #[test]
    fn test_standard_flavor_ordered_list_with_3_space_nested_unordered() {
        // Without MkDocs, `1. text` has content at column 3,
        // so 3-space indent is correct (text-aligned).
        let rule = MD007ULIndent::default();
        let content = "1. text\n\n   - nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "3-space indent under ordered list should be valid in Standard flavor, got: {result:?}"
        );
    }

    #[test]
    fn test_standard_flavor_ordered_list_under_ordered_is_exempt() {
        // markdownlint exempts unordered sublists of an ordered list from MD007
        // ("applies only if parent lists are all also unordered"). A 4-space bullet
        // under `1. text` (content column 3) is a genuine sublist, so it must not be
        // flagged. Verified: markdownlint-cli2 reports 0 MD007 errors here.
        let rule = MD007ULIndent::default();
        let content = "1. text\n\n    - nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "unordered sublist of an ordered list must be exempt in Standard flavor, got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_multi_digit_ordered_list() {
        // `10. text` has content at column 4, which already meets
        // the 4-space minimum (marker_col 0 + 4 = 4). No adjustment needed.
        let rule = MD007ULIndent::default();
        let content = "10. text\n\n    - nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "4-space indent under `10.` should be valid in MkDocs flavor, got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_triple_digit_ordered_list() {
        // `100. text` has content at column 5, which exceeds
        // the 4-space minimum (marker_col 0 + 4 = 4). No adjustment needed.
        let rule = MD007ULIndent::default();
        let content = "100. text\n\n     - nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "5-space indent under `100.` should be valid in MkDocs flavor, got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_insufficient_indent_under_ordered() {
        // In MkDocs, 2-space indent under `1. text` is insufficient.
        // Expected: marker_col(0) + 4 = 4, got: 2.
        let rule = MD007ULIndent::default();
        let content = "1. text\n\n  - nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "2-space indent under ordered list should warn in MkDocs flavor"
        );
        assert!(
            result[0].message.contains("Expected 4"),
            "Warning should expect 4 spaces (MkDocs minimum), got: {}",
            result[0].message
        );
    }

    #[test]
    fn test_mkdocs_deeper_nesting_under_ordered() {
        // `1. text` -> `    - sub` (4 spaces) -> `      - subsub` (6 spaces)
        // The sub-item at 4 spaces is correct for MkDocs.
        // The sub-sub-item at 6 spaces: parent is unordered at col 4 with content at col 6,
        // so 6-space indent is text-aligned (correct).
        let rule = MD007ULIndent::default();
        let content = "1. text\n\n    - sub\n      - subsub";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Deeper nesting under ordered list should be valid in MkDocs flavor, got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_fix_adjusts_to_4_spaces() {
        // Verify that auto-fix corrects 3-space indent to 4-space in MkDocs
        let rule = MD007ULIndent::default();
        let content = "1. text\n\n   - nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "3-space indent should warn in MkDocs");
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "1. text\n\n    - nested item",
            "Fix should adjust indent to 4 spaces in MkDocs"
        );
    }

    #[test]
    fn test_mkdocs_start_indented_with_ordered_parent() {
        // start_indented mode with MkDocs: the MkDocs adjustment should still apply
        // as a floor on top of the start_indented calculation.
        let config = MD007Config {
            start_indented: true,
            ..Default::default()
        };
        let rule = MD007ULIndent::from_config_struct(config);
        let content = "1. text\n\n    - nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "4-space indent under ordered list with start_indented should be valid in MkDocs, got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_ordered_at_nonzero_indent() {
        // Ordered list nested inside an unordered list, with a further unordered child.
        // `- outer` at col 0, `  1. inner` at col 2, `      - deep` at col 6.
        // For `deep`: parent is ordered at marker_col=2, so MkDocs minimum = 2+4 = 6.
        // Text-aligned: content_col of `1. inner` = 5. max(5, 6) = 6.
        let rule = MD007ULIndent::default();
        let content = "- outer\n  1. inner\n      - deep";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "6-space indent under nested ordered list should be valid in MkDocs, got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_blockquoted_ordered_list() {
        // Blockquoted ordered list in MkDocs: the indent is relative to
        // the blockquote content, so `> 1. text` with `>     - nested`
        // has 4 spaces of indent within the blockquote context.
        let rule = MD007ULIndent::default();
        let content = "> 1. text\n>\n>     - nested item";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "4-space indent under blockquoted ordered list should be valid in MkDocs, got: {result:?}"
        );
    }

    #[test]
    fn test_mkdocs_ordered_at_nonzero_indent_insufficient() {
        // Same structure but with only 5 spaces for `deep`.
        // MkDocs minimum = marker_col(2) + 4 = 6, but got 5. Should warn.
        let rule = MD007ULIndent::default();
        let content = "- outer\n  1. inner\n     - deep";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "5-space indent under nested ordered at col 2 should warn in MkDocs (needs 6)"
        );
    }

    #[test]
    fn test_issue_504_indent4_ordered_parent() {
        // Reproduction case from issue #504:
        // With indent=4, nested unordered items under ordered parent
        // should accept 4-space indentation
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: false,
            indent_explicit: true,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        let content = r#"# Things

+ An unordered list
    + An item with 4 spaces, ok.

1. A numbered list
    + A sublist with 4 spaces, not ok
        + A sub item with 4 spaces, ok
    + Why is rumdl expecting 3 spaces for a 4 space indent?
2. Item 2
3. Item 3"#;
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Issue #504: indent=4 with ordered parent should accept 4-space indent: {result:?}"
        );
    }

    #[test]
    fn test_indent2_explicit_with_ordered_parent() {
        // When indent=2 is explicit and parent is "1. " (text-aligned=3),
        // both 2 (fixed) and 3 (text-aligned) should be accepted
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(2),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: false,
            indent_explicit: true,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // 3-space indent should pass (text-aligned with "1. ")
        let content = "1. Ordered\n   * Bullet at 3 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "indent=2 under '1.' should accept text-aligned (3 spaces): {result:?}"
        );

        // 2-space indent should also pass (matches configured fixed indent)
        let content_2 = "1. Ordered\n  * Bullet at 2 spaces";
        let ctx = LintContext::new(content_2, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "indent=2 under '1.' should accept fixed indent (2 spaces): {result:?}"
        );
    }

    // Issue #638: MD007 must not fire on unordered items nested under an ordered
    // list. markdownlint: "applies to a sublist only if its parent lists are all
    // also unordered." Verified against markdownlint-cli2 v0.18.1 (0 MD007 errors).
    const ISSUE_638_INPUT: &str = "# Title\n\n1. Some text\n   - Indented text\n     - more indented\n";

    #[test]
    fn test_issue_638_unordered_under_ordered_smart_default() {
        let rule = MD007ULIndent::new(2);
        let ctx = LintContext::new(ISSUE_638_INPUT, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "smart default: unordered items under an ordered list must not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_unordered_under_ordered_indent_explicit() {
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(2),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: false,
            indent_explicit: true,
        };
        let rule = MD007ULIndent::from_config_struct(config);
        let ctx = LintContext::new(ISSUE_638_INPUT, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "indent=2 explicit: unordered items under an ordered list must not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_unordered_under_ordered_style_fixed() {
        // The reporter's exact config: indent = 2, style = "fixed".
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(2),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::Fixed,
            style_explicit: true,
            indent_explicit: true,
        };
        let rule = MD007ULIndent::from_config_struct(config);
        let ctx = LintContext::new(ISSUE_638_INPUT, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "style=fixed: unordered items under an ordered list must not be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_deeper_unordered_chain_under_ordered() {
        // Every unordered item below the ordered ancestor is exempt, at any depth.
        let rule = MD007ULIndent::new(2);
        let content = "1. Ordered\n   - child\n      - grandchild\n         - great-grandchild\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "all unordered descendants of an ordered list are exempt, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_pure_unordered_still_checked() {
        // Guard: the exemption must not leak into pure unordered lists.
        let rule = MD007ULIndent::new(2);
        let content = "- Top\n   - three spaces (wrong, expected 2)\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "pure unordered nesting must still be checked, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_exemption_not_applied_after_list_terminated_by_paragraph() {
        // A top-level paragraph terminates the ordered list. The later, separately
        // indented unordered list is NOT a sublist of the (now-closed) ordered item, so
        // the ordered-ancestor exemption must not apply: MD007 flags both the misindented
        // top-level item and its child. Verified against markdownlint-cli2, which reports
        // MD007 on the parent (Expected: 0; Actual: 3) and the child (Expected: 2;
        // Actual: 6).
        let rule = MD007ULIndent::new(2);
        let content = "1. ordered\n\nparagraph\n\n   - parent\n      - child six\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "the new top-level list following a terminated ordered list is checked at both levels, got: {result:?}"
        );
        assert!(
            result.iter().any(|w| w.line == 5 && w.message.contains("Expected 0")),
            "the misindented top-level item must be flagged with Expected 0, got: {result:?}"
        );
        assert!(
            result
                .iter()
                .any(|w| w.line == 6 && w.message.contains("Expected 2") && w.message.contains("found 6")),
            "the misindented child must be flagged with Expected 2, found 6, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_lazy_continuation_does_not_terminate_ordered_list() {
        // A non-indented paragraph line that immediately follows the ordered item
        // (no blank line between) is a CommonMark lazy continuation of that item,
        // so the ordered list stays open and its unordered sublist is exempt.
        // markdownlint-cli2 reports 0 MD007 errors here; the stale-ancestor
        // termination must not fire on a lazy continuation line.
        let rule = MD007ULIndent::new(2);
        let content = "1. ordered\nlazy continuation\n   - child\n     - grandchild\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "lazy continuation must not terminate the ordered list; sublist stays exempt, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_heading_interrupts_ordered_list_without_blank() {
        // Unlike a lazy paragraph continuation, an ATX heading interrupts the open
        // paragraph and therefore terminates the ordered list even without an
        // intervening blank line. The following bullets are then a new top-level list,
        // so both the misindented top item and its child are flagged. markdownlint-cli2
        // reports MD007 on the top item (Expected: 0; Actual: 3) and the child
        // (Expected: 2; Actual: 5).
        let rule = MD007ULIndent::new(2);
        let content = "1. ordered\n# heading\n   - child\n     - grandchild\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            2,
            "a heading terminates the ordered list, so the new top-level list and its child are both checked, got: {result:?}"
        );
        assert!(
            result.iter().any(|w| w.line == 3 && w.message.contains("Expected 0")),
            "the misindented top-level item must be flagged with Expected 0, got: {result:?}"
        );
        assert!(
            result.iter().any(|w| w.line == 4 && w.message.contains("Expected 2")),
            "the misindented child must be flagged with Expected 2, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_lazy_continuation_inside_blockquote_keeps_exemption() {
        // Inside a blockquote, a plain continuation line in the same quote is a
        // lazy paragraph continuation of the ordered item, so the list stays open
        // and its sublist remains exempt. markdownlint-cli2 reports 0 MD007 errors;
        // termination must operate in blockquote-content coordinates, not absolute.
        let rule = MD007ULIndent::new(2);
        let content = "> 1. ordered\n> continuation\n>\n>    - child\n>      - grandchild\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "a lazy continuation within the same blockquote must keep the sublist exempt, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_indented_fence_inside_blockquoted_ordered_item_keeps_exemption() {
        // A fenced code block indented to the ordered item's content column, all
        // within a blockquote, is part of that item. The list stays open and the
        // sublist remains exempt. markdownlint-cli2 reports 0 MD007 errors; the
        // skip-region termination must use blockquote-content-relative indent.
        let rule = MD007ULIndent::new(2);
        let content = "> 1. ordered\n>    ```\n>    code\n>    ```\n>    - child\n>      - grandchild\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "an indented fence inside a blockquoted ordered item must keep the sublist exempt, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_fenced_code_block_terminates_ordered_list() {
        // A top-level fenced code block (its opening fence not indented into the
        // item) terminates the ordered list. Because the rule skips code-block
        // lines, the stale ordered ancestor must still be cleared so the exemption
        // does not leak to a later list. markdownlint-cli2 flags the misindented
        // child (Expected: 2; Actual: 6).
        let rule = MD007ULIndent::new(2);
        let content = "1. ordered\n```\ncode\n```\n\n   - parent\n      - child\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.iter().any(|w| w.line == 7),
            "a top-level fenced code block terminates the ordered list; the child must be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_fenced_code_block_inside_item_keeps_exemption() {
        // A fenced code block indented into the ordered item's content column is
        // part of that item, so the list stays open and the sublist remains exempt.
        // markdownlint-cli2 reports 0 MD007 errors; termination must not over-fire
        // on the code block's interior lines.
        let rule = MD007ULIndent::new(2);
        let content = "1. ordered\n   ```\n   code\n   ```\n   - child\n     - grandchild\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "a fenced code block nested inside the item must keep the sublist exempt, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_blockquote_terminates_ordered_list() {
        // A top-level blockquote interrupts the open paragraph and terminates the
        // ordered list (it is not indented into the item's content). The later,
        // separately indented unordered list is therefore NOT a sublist of the
        // closed ordered item, so the ordered-ancestor exemption must not leak:
        // the misindented child must still be flagged. markdownlint-cli2 reports
        // MD007 on the child (Expected: 2; Actual: 6).
        let rule = MD007ULIndent::new(2);
        let content = "1. ordered\n> quote\n\n   - parent\n      - child\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.iter().any(|w| w.line == 5),
            "blockquote terminates the ordered list, so the child must still be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_blockquote_inside_item_keeps_exemption() {
        // When the blockquote is indented into the ordered item's content column it
        // is part of that item, so the list stays open and its unordered sublist
        // remains exempt. markdownlint-cli2 reports 0 MD007 errors here; the
        // termination must not over-fire on a blockquote nested inside the item.
        let rule = MD007ULIndent::new(2);
        let content = "1. ordered\n   > quote inside item\n   - child\n     - grandchild\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "a blockquote nested inside the item must keep the sublist exempt, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_exemption_requires_genuine_nesting_under_ordered() {
        // A wide ordered marker ("100. ") has its content at column 5. An unordered
        // bullet indented only 3 spaces is left of that content column, so it is NOT
        // nested under the ordered item but a new top-level list. The ordered-ancestor
        // exemption must not leak through this non-nested bullet to its child: with
        // the ordered item no longer a genuine ancestor, the misindented child must
        // still be checked. markdownlint-cli2 flags both the parent (Expected: 0) and
        // the child (Expected: 2). The exemption must not suppress the child, and the
        // fix must not flatten the child into a sibling of the parent.
        let rule = MD007ULIndent::new(2);
        let content = "100. ordered\n   - parent\n     - child\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.iter().any(|w| w.line == 3),
            "the child of a non-nested bullet must still be checked, not exempted; got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_paragraph_after_fenced_code_closes_ordered_list() {
        // A fenced code block inside an ordered item is not paragraph text, so an
        // unindented line after the closing fence is NOT a lazy paragraph continuation:
        // it closes the list. The later, separately indented bullet list is therefore a
        // new top-level list, not a sublist of the ordered item, so the ordered-ancestor
        // exemption must not leak: the misindented child must still be flagged.
        // (markdownlint-cli2 also flags the parent with Expected: 0; rumdl does not flag
        // indented top-level list items, a separate pre-existing limitation, so we assert
        // only the child here - the part this fix governs.)
        let rule = MD007ULIndent::new(2);
        let content = "1. ordered\n   ```\n   code\n   ```\nnot lazy text\n   - parent\n     - child\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.iter().any(|w| w.line == 7),
            "fenced code is not paragraph text, so the list closes and the nested child must still be checked, not exempted; got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_overlong_ordered_marker_is_lazy_continuation() {
        // CommonMark ordered list markers allow at most 9 digits. A run of 10+ digits
        // (`1234567890.`) is not a valid marker, so the line is a lazy paragraph
        // continuation of the open ordered item, which keeps the list open. The nested
        // bullets remain a sublist under the ordered item and are exempt from MD007.
        // markdownlint-cli2 reports no MD007 warnings here.
        let rule = MD007ULIndent::new(2);
        let content = "1. ordered\n1234567890. this is continuation text\n   - child\n     - grandchild\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "an overlong digit run is not a valid ordered marker, so the list stays open and the nested bullets are exempt; got: {result:?}"
        );
    }

    #[test]
    fn test_indented_top_level_list_item_is_flagged() {
        // A top-level unordered list item indented 2 or 3 spaces is a misindented list
        // (4+ spaces would be an indented code block, not a list). markdownlint-cli2
        // flags the top item with "Expected: 0". rumdl must flag it too, not only its
        // children. The default config has start_indented = false, so the expected
        // indent for a depth-0 item is column 0.
        let rule = MD007ULIndent::new(2);
        for indent in 2..=3 {
            let pad = " ".repeat(indent);
            let content = format!("{pad}- parent\n{pad}  - child\n");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.iter().any(|w| w.line == 1),
                "a top-level item indented {indent} spaces must be flagged (Expected 0); got: {result:?}"
            );
        }
    }

    #[test]
    fn test_indented_code_block_bullet_is_not_a_list_item() {
        // Four or more leading spaces at the top level form an indented code block, not a
        // list, so MD007 must not fire. Both rumdl and markdownlint-cli2 stay silent.
        let rule = MD007ULIndent::new(2);
        let content = "    - not a list, this is code\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "a 4-space-indented bullet is an indented code block, not a misindented list; got: {result:?}"
        );
    }

    #[test]
    fn test_tab_indent_expands_to_four_column_tabstop() {
        // CommonMark expands a leading tab to the next 4-column tab stop when it helps
        // define block structure. A single-tab-indented sublist therefore sits at visual
        // column 4, which is an over-indent for depth 1 (expected 2). rumdl must report
        // the expanded column (found 4), NOT a raw character count of 1. (markdownlint
        // counts the tab as a single character and reports "Actual 1"; that is incorrect
        // per the CommonMark tab-stop rule, so rumdl deliberately diverges here.)
        let rule = MD007ULIndent::new(2);
        let content = "- a\n\t- b\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        let warning = result
            .iter()
            .find(|w| w.line == 2)
            .expect("a tab-indented sublist at column 4 is over-indented for depth 1 and must be flagged");
        assert!(
            warning.message.contains("found 4"),
            "the tab must expand to the 4-column tab stop (found 4), not be counted as one character; got: {}",
            warning.message
        );
    }

    #[test]
    fn test_tab_completing_two_space_indent_to_tabstop_is_accepted() {
        // Two spaces advance to column 2; a following tab then advances to the next
        // 4-column tab stop, landing the sublist marker at column 4 - exactly the
        // expected indent for depth 2. With correct tab-stop math the line is well
        // indented and must produce no warning. (markdownlint miscounts `  \t` as three
        // characters and false-positives with "Actual 3"; rumdl correctly stays silent.)
        let rule = MD007ULIndent::new(2);
        let content = "- a\n  - b\n  \t- c\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "`  \\t` expands to column 4, the correct depth-2 indent, so no MD007 warning is expected; got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_html_comment_terminates_ordered_list() {
        // An HTML comment is a block construct that interrupts the open paragraph and
        // terminates the ordered list, just like a heading or fenced code block. The
        // later, separately indented unordered list is therefore not a sublist of the
        // closed ordered item, so the ordered-ancestor exemption must not leak: the
        // misindented child must still be flagged. markdownlint-cli2 reports MD007 on
        // the child (Expected: 2; Actual: 6).
        let rule = MD007ULIndent::new(2);
        let content = "1. ordered\n<!-- comment -->\n\n   - parent\n      - child\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.iter().any(|w| w.line == 5),
            "an HTML comment terminates the ordered list, so the child must still be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_blockquoted_list_item_terminates_ordered_list() {
        // A blockquoted list item that begins left of the ordered item's content
        // column starts a new container and terminates the ordered list (the `>` is
        // not indented into the item). The later, separately indented unordered list
        // is therefore not a sublist of the closed ordered item, so the
        // ordered-ancestor exemption must not leak: the misindented child must still
        // be flagged. markdownlint-cli2 reports MD007 on the child
        // (Expected: 2; Actual: 5).
        let rule = MD007ULIndent::new(2);
        let content = "1. ordered\n> - quote list\n\n   - parent\n     - child\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.iter().any(|w| w.line == 5),
            "a blockquoted list item terminates the ordered list, so the child must still be flagged, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_deeper_nested_quote_terminates_blockquoted_ordered_list() {
        // A blockquoted ordered item (`> 1. ordered`) is interrupted by a deeper
        // nested quote (`> > quote`). The inner `>` begins left of the ordered
        // item's content column (in the item's own quote coordinate space), so it
        // is a sibling block that closes the ordered list, not a continuation of
        // it. The unordered list that follows inside the same depth-1 quote is
        // therefore a fresh top-level list, not a sublist of the (closed) ordered
        // item, so the ordered-ancestor exemption must NOT leak to it.
        // markdownlint-cli2 (MD007 only) reports the parent (Expected: 0; Actual: 3)
        // and the child (Expected: 2; Actual: 6).
        let rule = MD007ULIndent::new(2);
        let content = "> 1. ordered\n> > quote\n>\n>    - parent\n>       - child\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.iter().any(|w| w.line == 4),
            "deeper nested quote closes the ordered list, so the misindented parent must be flagged, got: {result:?}"
        );
        assert!(
            result.iter().any(|w| w.line == 5),
            "the child of the fresh unordered list must be flagged, not exempted, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_deeper_quote_list_item_terminates_blockquoted_ordered_list() {
        // Same leak as the deeper-nested-quote case, but the interrupting deeper
        // quote is itself a list item (`> > - quote list`). Its marker begins left
        // of the ordered item's content column (in the item's coordinate space), so
        // it closes the ordered list. The unordered list that follows in the depth-1
        // quote is therefore a fresh top-level list and must not inherit the
        // ordered-ancestor exemption. markdownlint-cli2 reports the parent
        // (Expected: 0; Actual: 3) and the child (Expected: 2; Actual: 6).
        let rule = MD007ULIndent::new(2);
        let content = "> 1. ordered\n> > - quote list\n>\n>    - parent\n>       - child\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.iter().any(|w| w.line == 4),
            "a deeper-quote list item closes the ordered list, so the parent must be flagged, got: {result:?}"
        );
        assert!(
            result.iter().any(|w| w.line == 5),
            "the child of the fresh unordered list must be flagged, not exempted, got: {result:?}"
        );
    }

    #[test]
    fn test_issue_638_deeper_quote_indented_into_item_keeps_exemption() {
        // When the deeper quote is indented to (or past) the ordered item's content
        // column, the `> quote` is a child block of the item, so the ordered list
        // stays open and its unordered sublist remains exempt. The termination must
        // not over-fire. markdownlint-cli2 reports 0 MD007 errors here.
        let rule = MD007ULIndent::new(2);
        let content = "> 1. ordered\n>    > quote inside item\n>    - child\n>      - grandchild\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "a deeper quote indented into the item must keep the sublist exempt, got: {result:?}"
        );
    }

    #[test]
    fn test_indent4_explicit_with_wide_ordered_parent() {
        // When indent=4 and parent is "100. " (text-aligned=5),
        // both 4-space and 5-space indent should be accepted.
        // The list parser may recognize 4-space as valid nesting under "100."
        let config = MD007Config {
            indent: crate::types::IndentSize::from_const(4),
            start_indented: false,
            start_indent: crate::types::IndentSize::from_const(2),
            style: md007_config::IndentStyle::TextAligned,
            style_explicit: false,
            indent_explicit: true,
        };
        let rule = MD007ULIndent::from_config_struct(config);

        // 5-space indent should pass
        let content = "100. Wide ordered\n     * Bullet at 5 spaces";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "indent=4 under '100.' should accept 5-space indent: {result:?}"
        );

        // 4-space indent should also pass (matches configured indent)
        let content_4 = "100. Wide ordered\n    * Bullet at 4 spaces";
        let ctx = LintContext::new(content_4, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "indent=4 under '100.' should accept 4-space indent: {result:?}"
        );
    }

    /// Maximum list-nesting depth a real CommonMark parser sees in `md`: 1 for a
    /// flat list, 2 for a list nested inside a list item. Guards against indent
    /// "fixes" that silently flatten a child into a sibling.
    fn commonmark_max_list_depth(md: &str) -> usize {
        use pulldown_cmark::{Event, Parser, Tag, TagEnd};
        let (mut depth, mut max) = (0usize, 0usize);
        for event in Parser::new(md) {
            match event {
                Event::Start(Tag::List(_)) => {
                    depth += 1;
                    max = max.max(depth);
                }
                Event::End(TagEnd::List(_)) => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        max
    }

    #[test]
    fn test_md007_widened_parent_marker_keeps_nested_child() {
        // A non-default MD030 (e.g. `ul-multi = 3`) widens a parent bullet to `-   `,
        // moving its content column to 4. A child aligned to that column (indent 4) is
        // correctly nested in CommonMark, so MD007 must accept it instead of flagging it
        // as over-indented — the old behavior stored the parent's content column as 2 and
        // "fixed" the child to column 2, detaching it into a sibling.
        let rule = MD007ULIndent::default();
        let content = indoc! {"
            -   Parent item
                - Nested item
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "a child aligned to a widened parent's content column must not be flagged: {result:?}"
        );
        assert_eq!(commonmark_max_list_depth(content), 2, "precondition: source is nested");
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            content,
            "fix must be a no-op for an already correctly nested child"
        );
    }

    #[test]
    fn test_md007_widened_parent_aligns_child_to_content_column() {
        // A child mis-indented under a widened parent is corrected to the parent's
        // content column (4 here), not to the fixed-grid column 2 that would detach it.
        let rule = MD007ULIndent::default();
        let content = indoc! {"
            -   Parent item
                 - Nested item
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed,
            indoc! {"
                -   Parent item
                    - Nested item
            "},
            "child must align to the parent's content column 4: {fixed:?}"
        );
        assert_eq!(
            commonmark_max_list_depth(&fixed),
            2,
            "fixed child must remain nested, not flattened to a sibling:\n{fixed}"
        );
    }

    #[test]
    fn test_md007_widened_markers_nested_multiple_levels() {
        // Several levels of widened markers all stay nested: each child aligns to its
        // own parent's widened content column.
        let rule = MD007ULIndent::default();
        let content = indoc! {"
            -   Level 0
                -   Level 1
                    - Level 2
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "deeply nested widened markers must not be flagged: {result:?}"
        );
        assert_eq!(
            commonmark_max_list_depth(content),
            3,
            "three nesting levels are preserved"
        );
    }

    #[test]
    fn test_md007_default_marker_indent_still_enforced() {
        // Regression guard: the widened-marker handling must not relax the check for
        // ordinary single-space markers. An over-indented child is still flagged and
        // fixed back to the 2-space grid.
        let rule = MD007ULIndent::default();
        let content = indoc! {"
            - Parent item
                - Nested item
        "};
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "an over-indented child under a normal marker is still flagged: {result:?}"
        );
        assert_eq!(
            rule.fix(&ctx).unwrap(),
            indoc! {"
                - Parent item
                  - Nested item
            "}
        );
    }
}
