use regex::Regex;
use std::sync::LazyLock;

use super::types::*;

/// Regex for detecting blockquote prefixes in list context
static BLOCKQUOTE_PREFIX_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^((?:\s*>\s*)+)").unwrap());

/// The column a byte offset into a line falls on, with a tab reaching the next
/// multiple of four, as CommonMark measures indent.
fn column_at(line: &str, byte_offset: usize) -> usize {
    line[..byte_offset].chars().fold(0, column_after)
}

/// The nesting level the list-block tracker assigns to a list item on `line`:
/// the column its marker sits on, at two columns per level, so a tab before
/// the marker counts for the width it spans. Every consumer that compares an
/// item against `ListBlock::nesting_level` must measure it here, or byte
/// offsets and columns get compared to each other.
pub(crate) fn list_item_nesting_level(line: &str, item: &ListItemInfo) -> usize {
    column_at(line, item.marker_column) / 2
}

/// The item lines of `block` grouped into the lists they form. Nested items
/// continue their parent's block, so `item_lines` interleaves every level in
/// source order; each group here is one list, read the way CommonMark nests
/// containers: an item that starts at or right of the open list's content
/// column is nested in its latest item and opens a list of its own; one that
/// starts left of it, but not left of the column the list itself lives in, is
/// a sibling; and one left of both closes the list and speaks to an ancestor.
/// A nested list also closes on content that belongs to an ancestor item (a
/// paragraph, fence, blockquote or other block sitting left of the nested
/// item's content column, unless it lazily continues a paragraph).
/// `- a\n  - a1\n  - a2\n- b\n  - b1` gives `[a, b]`, `[a1, a2]` and `[b1]`,
/// ordered by first line. A consumer that compares an item with its siblings
/// (spacing, alignment) reads this rather than `item_lines`, or a child breaks
/// the run of its parents.
///
/// Siblings may sit at different indents (` - a` and `  - b` are one list), so
/// the level is never derived from the marker column; a fixed columns-per-level
/// mapping would merge such a sibling with a nested list that happens to share
/// its column. A change of marker type at one level (the bullet character, or
/// an ordered marker's delimiter) starts a new list there, as CommonMark reads
/// it. The tracker keeps consecutive items of both kinds in one block, so this
/// is decided here for every level, the block's own included.
pub(crate) fn item_lines_by_list(content: &str, lines: &[LineInfo], block: &ListBlock) -> Vec<Vec<usize>> {
    struct OpenList {
        kind: u8,
        /// The content column of the list's latest item, measured from the
        /// content of the blockquote the list sits in.
        content_column: usize,
        quote_depth: usize,
        items: Vec<usize>,
    }

    // Each open list is nested in the latest item of the list before it (the
    // first sits at the block's own level), so the open lists' latest items
    // are the chain of containers a line has to continue, outermost first.
    // A line continues an item when it starts at or right of the item's
    // content column at the item's quote depth: its own text or marker at
    // that depth, or the `>` that quotes it more deeply, which then opens a
    // blockquote inside the item. The first item a line does not continue
    // ends every list nested in that item.
    let mut open: Vec<OpenList> = Vec::new();
    let mut lists: Vec<Vec<usize>> = Vec::new();
    // Whether the previous line left a paragraph open that the next line
    // could continue lazily, however far left it starts.
    let mut paragraph_open = false;
    for line_num in block.start_line..=block.end_line {
        let Some(info) = line_num.checked_sub(1).and_then(|index| lines.get(index)) else {
            break;
        };
        let line = info.content(content);
        let quote_depth = info.blockquote.as_ref().map_or(0, |bq| bq.nesting_level);
        let item = info
            .list_item
            .as_deref()
            .filter(|_| block.item_lines.binary_search(&line_num).is_ok());
        if let Some(item) = item {
            let kind = marker_kind(item);
            let content_column = column_at(line, item.content_column)
                .saturating_sub(quote_origin_column(line, quote_depth).unwrap_or(0));
            let continues = |list: &OpenList| {
                quote_depth >= list.quote_depth && column_in_quote(line, list.quote_depth) >= list.content_column
            };
            // An item that continues every open item is nested in the
            // innermost one and opens a list of its own. Otherwise it closes
            // the lists nested in the first item it does not continue; at that
            // list's own quote depth it is a sibling (the same kind joins, a
            // different kind starts a new list in the same place), and quoted
            // to another depth it starts a new list nested in the item before,
            // as a blockquote there does not carry the list on.
            let mut joins_open_list = false;
            if let Some(index) = open.iter().position(|list| !continues(list)) {
                lists.extend(open.drain(index + 1..).map(|list| list.items));
                let list = &open[index];
                joins_open_list = quote_depth == list.quote_depth && list.kind == kind;
                if !joins_open_list {
                    lists.extend(open.pop().map(|list| list.items));
                }
            }
            match open.last_mut() {
                Some(list) if joins_open_list => {
                    list.items.push(line_num);
                    list.content_column = content_column;
                }
                _ => open.push(OpenList {
                    kind,
                    content_column,
                    quote_depth,
                    items: vec![line_num],
                }),
            }
            paragraph_open = !info.in_table_block && item_opens_paragraph(line, item);
            continue;
        }
        // A line with nothing after its quote markers, or nothing at all.
        let quoted_content_blank = info
            .blockquote
            .as_ref()
            .map_or(info.is_blank, |bq| bq.content.trim().is_empty());
        // What interrupts a paragraph is what CommonMark says does: a fence,
        // a heading, a thematic break, an HTML block opener (a tag, a
        // comment, an instruction, a declaration or CDATA), a deeper
        // blockquote, and the flavor blocks rumdl reads the same way. A
        // table row counts too, and a table is whatever rumdl's table finder
        // reads as one, so this grouping and the table rules agree on where a
        // nested list ends.
        let text = info
            .blockquote
            .as_ref()
            .map_or(line, |bq| bq.content.as_str())
            .trim_start();
        let starts_a_block = info.in_code_block
            || info.heading.is_some()
            || info.is_horizontal_rule
            || info.in_html_block
            || info.in_table_block
            || crate::utils::html_block::opens_untagged_html_block(text)
            || info.in_math_block
            || info.is_div_marker;
        // A paragraph line directly after another continues that paragraph
        // wherever it sits (lazy continuation), even with fewer quote markers
        // than the item; anything else that starts left of an item's content
        // column ends that item, and with it every list nested in the item.
        // A line with fewer quote markers than the item's blockquote ends the
        // blockquote, a blank line included, so items in the next blockquote
        // are another list. A line quoted more deeply is content, not a gap:
        // its `>` opens a blockquote inside the item, or, left of the item's
        // content, beside it. Blank lines and bare `>` at the item's own
        // depth are the spacing between items and end nothing. The list at
        // the block's own level closes like any other: the tracker keeps the
        // items on both sides of a fence, an HTML block or an ended
        // blockquote in one block, and a later item then starts a new list
        // at that level rather than nesting in the item the line ended.
        let ends_item = |list: &OpenList| {
            if quote_depth < list.quote_depth {
                return !paragraph_open || starts_a_block || quoted_content_blank;
            }
            let interrupts = !paragraph_open || starts_a_block || quote_depth > list.quote_depth;
            (quote_depth > list.quote_depth || !quoted_content_blank)
                && interrupts
                && column_in_quote(line, list.quote_depth) < list.content_column
        };
        if let Some(index) = open.iter().position(ends_item) {
            lists.extend(open.drain(index..).map(|list| list.items));
        }
        // A blank line, a block opener and a bare quote marker each leave no
        // paragraph for the next line to continue; any other text is a
        // paragraph line, quoted or not, and a quoted paragraph continues
        // lazily like an unquoted one.
        paragraph_open = !starts_a_block && !quoted_content_blank;
    }
    lists.extend(open.into_iter().map(|list| list.items));
    lists.sort_by_key(|items| items[0]);
    lists
}

/// The byte that decides whether two items at one level are in the same list:
/// the bullet character, or the delimiter (`.` or `)`) of an ordered marker.
fn marker_kind(item: &ListItemInfo) -> u8 {
    item.marker.bytes().last().unwrap_or(0)
}

/// Whether a list item's own text opens a paragraph that a later line could
/// continue lazily. Nested container markers (`- - x`, `- > x`) are stepped
/// through, since the paragraph is then the innermost container's. The text
/// opens no paragraph when it is empty, when it sits five or more columns
/// past its marker (CommonMark reads that as an indented code block), or
/// when it opens a block of its own: an ATX heading, a fence, a thematic
/// break or an HTML block. The caller rules out a table, which the line info
/// carries. A setext heading and a type-7 HTML block (an inline-level tag
/// alone on the line) are not recognised here, as rumdl's parser does not
/// recognise them in an item either.
fn item_opens_paragraph(line: &str, item: &ListItemInfo) -> bool {
    let mut marker_end = item.marker_column + item.marker.len();
    let mut content_start = item.content_column;
    loop {
        let Some(text) = line.get(content_start..) else {
            return false;
        };
        let text_start = content_start + (text.len() - text.trim_start().len());
        let text = text.trim_start();
        if text.is_empty() {
            return false;
        }
        if column_at(line, text_start).saturating_sub(column_at(line, marker_end)) >= 5 {
            return false;
        }
        match container_marker_len(text) {
            Some(len) => {
                marker_end = text_start + len;
                content_start = marker_end;
            }
            None => return !text_opens_block(text),
        }
    }
}

/// The byte length of a blockquote or list marker at the start of `text`,
/// when one is there: `>`, a bullet followed by whitespace or the end of the
/// line, or up to nine digits and `.` or `)` followed by the same.
fn container_marker_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let marker_len = match bytes.first()? {
        b'>' => return Some(1),
        b'-' | b'+' | b'*' => 1,
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            if digits > 9 || !matches!(bytes.get(digits), Some(b'.' | b')')) {
                return None;
            }
            digits + 1
        }
        _ => return None,
    };
    bytes
        .get(marker_len)
        .is_none_or(|b| *b == b' ' || *b == b'\t')
        .then_some(marker_len)
}

/// Whether `text`, the first content of a list item, opens a block instead
/// of a paragraph: an ATX heading, a code fence (a backtick fence's info
/// string may not hold a backtick), a thematic break or an HTML block.
fn text_opens_block(text: &str) -> bool {
    let bytes = text.as_bytes();
    let hashes = bytes.iter().take_while(|&&b| b == b'#').count();
    let heading = (1..=6).contains(&hashes) && bytes.get(hashes).is_none_or(|b| *b == b' ' || *b == b'\t');
    let backticks = bytes.iter().take_while(|&&b| b == b'`').count();
    let fence =
        (backticks >= 3 && !text[backticks..].contains('`')) || bytes.iter().take_while(|&&b| b == b'~').count() >= 3;
    heading
        || fence
        || is_horizontal_rule_content(text.trim_end())
        || crate::utils::html_block::parse_html_block_start(text).is_some()
        || crate::utils::html_block::opens_untagged_html_block(text)
}

/// The column CommonMark measures a line's indentation from inside a
/// blockquote `quote_depth` deep: just past the quote's last `>` and the
/// optional space after it, a tab there giving one column to the prefix and
/// the rest to the indent. `None` for a line with fewer markers than that,
/// which is not inside the quote at all.
fn quote_origin_column(line: &str, quote_depth: usize) -> Option<usize> {
    let mut column = 0;
    let mut quotes = 0;
    let mut chars = line.chars().peekable();
    while quotes < quote_depth {
        match chars.next()? {
            '>' => {
                quotes += 1;
                column += 1;
            }
            c @ (' ' | '\t') => column = column_after(column, c),
            _ => return None,
        }
    }
    if quote_depth > 0 && matches!(chars.peek(), Some(' ' | '\t')) {
        column += 1;
    }
    Some(column)
}

/// The column a line's content starts at, measured from where a list quoted
/// `quote_depth` deep measures its own indentation, so `> - a` and ` >  - b`
/// stand where `- a` and ` - b` would however the `>` itself is indented. For
/// a line quoted more deeply the content is its next `>`. A line with fewer
/// markers than the list starts left of the quote's content, at zero.
fn column_in_quote(line: &str, quote_depth: usize) -> usize {
    let Some(origin) = quote_origin_column(line, quote_depth) else {
        return 0;
    };
    let mut quotes = 0;
    let mut byte = 0;
    for c in line.chars() {
        match c {
            ' ' | '\t' => {}
            '>' if quotes < quote_depth => quotes += 1,
            _ => break,
        }
        byte += c.len_utf8();
    }
    column_at(line, byte).saturating_sub(origin)
}

fn column_after(column: usize, c: char) -> usize {
    if c == '\t' { (column / 4 + 1) * 4 } else { column + 1 }
}

/// Compute the effective indentation of a line after stripping its blockquote
/// prefix, in columns.
///
/// For a line like `> >   text`, this skips past the `>` markers and the optional
/// space after the last marker, then measures the remaining leading whitespace.
/// A tab after the last marker supplies that optional space with its first
/// column and indents with the rest, as CommonMark reads `>\tfoo`. Returns
/// `None` if the line doesn't have the expected number of blockquote markers.
fn indent_after_blockquote(raw_content: &str, expected_bq_level: usize) -> Option<usize> {
    let mut pos = 0;
    let mut column = 0;
    let mut found_markers = 0;
    for c in raw_content.chars() {
        pos += c.len_utf8();
        column = if c == '\t' { (column / 4 + 1) * 4 } else { column + 1 };
        if c == '>' {
            found_markers += 1;
            if found_markers == expected_bq_level {
                break;
            }
        }
    }
    if found_markers < expected_bq_level {
        return None;
    }
    // Skip the optional space after the last marker; a tab there is one column
    // of space and the remainder indent.
    let mut prefix_column = column;
    match raw_content.get(pos..pos + 1) {
        Some(" ") => {
            pos += 1;
            column += 1;
            prefix_column = column;
        }
        Some("\t") => prefix_column += 1,
        _ => {}
    }
    let after_bq = &raw_content[pos..];
    let content_column = after_bq
        .chars()
        .take_while(|c| c.is_whitespace())
        .fold(column, |col, c| if c == '\t' { (col / 4 + 1) * 4 } else { col + 1 });
    Some(content_column - prefix_column)
}

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

        // Enhanced code block handling using Design #3's context analysis.
        //
        // Exception: a fenced code block can *open on a list-marker line*, e.g.
        // "- ```python" or "1. ```js". Such a line is flagged `in_code_block`
        // (it begins a code block) but is genuinely the start of a list item, so
        // it must fall through to the list-item handling below to be registered;
        // otherwise the whole item — and everything indented under it — would be
        // dropped from the list model. Lines *inside* the fence are never flagged
        // as list items, so this exception only ever matches the marker line.
        if line_info.in_code_block && line_info.list_item.is_none() {
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

            // Check for structural separators that break lists. Paragraph text
            // indented short of the item's content column is a lazy continuation
            // per CommonMark, however short the indent, and does not break the list.

            // Check if blockquote context changes (different prefix than current block)
            // Lines within the SAME blockquote context don't break lists
            let blockquote_prefix_changes = blockquote_prefix.trim() != block.blockquote_prefix.trim();

            let breaks_list = line_info.is_valid_heading()
                || line_content.starts_with("---")
                || line_content.starts_with("***")
                || line_content.starts_with("___")
                || (crate::utils::skip_context::is_table_line(line_content)
                    && line_info.visual_indent < min_continuation_for_tracking)
                || blockquote_prefix_changes;

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
        let effective_continuation_columns = if let Some(ref block) = current_block {
            let block_bq_level = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
            let line_content = line_info.content(content);
            let line_bq_level = line_content
                .chars()
                .take_while(|c| *c == '>' || c.is_whitespace())
                .filter(|&c| c == '>')
                .count();
            match indent_after_blockquote(line_content, line_bq_level) {
                Some(columns) if line_bq_level > 0 && line_bq_level == block_bq_level => columns,
                _ => line_info.visual_indent,
            }
        } else {
            line_info.visual_indent
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
        // Lazy continuation allows text indented short of the content column to
        // continue a list item, but NOT structural elements like headings, code
        // fences, HTML blocks, or horizontal rules. Structural checks read the
        // text after the blockquote prefix so a fence or rule inside the quote
        // is recognized.
        let inner_content = crate::utils::blockquote::parse_blockquote_prefix(line_info.content(content))
            .map_or(line_info.content(content), |parsed| parsed.content)
            .trim();
        // Whether the line reaches an item's content column is a question of
        // columns, so a tab counts for the width it spans.
        let inside_item = effective_continuation_columns >= adjusted_min_continuation_for_tracking;
        let is_structural_element =
            opens_own_block(line_info, inner_content, effective_continuation_columns, inside_item)
                || is_horizontal_rule_content(inner_content);
        let is_valid_continuation = inside_item || (!line_info.is_blank && !is_structural_element);

        if debug_list && line_info.list_item.is_none() && !line_info.is_blank {
            eprintln!(
                "[DEBUG] Line {}: checking continuation - columns={}, min_cont={}, is_valid={}, in_code_span={}, in_code_block={}, has_block={}",
                line_num,
                effective_continuation_columns,
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
            // The marker's column, so a tab before it counts for the width it
            // spans; the nesting level assumes two columns per level.
            let item_indent = column_at(line_info.content(content), list_item.marker_column);
            let nesting = list_item_nesting_level(line_info.content(content), list_item);

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
                let same_context = block.blockquote_prefix.trim() == blockquote_prefix.trim();
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
                    // End current block and start a new one. The block keeps every
                    // continuation line it collected, including lazy (column 0) ones:
                    // per CommonMark those continue the item's paragraph, so cutting
                    // them off would make MD032 insert its blank line inside the item.
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
            // For blockquote lists, compute effective indent after stripping prefix
            let block_bq_level_cont = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
            let line_raw_content = line_info.content(content);
            let line_bq_level_cont = line_raw_content
                .chars()
                .take_while(|c| *c == '>' || c.is_whitespace())
                .filter(|&c| c == '>')
                .count();

            let (effective_line_indent, min_continuation_indent) = if block_bq_level_cont > 0
                && line_bq_level_cont == block_bq_level_cont
                && let Some(columns) = indent_after_blockquote(line_raw_content, block_bq_level_cont)
            {
                let min_indent = if block.is_ordered { last_marker_width } else { 2 };
                (columns, min_indent)
            } else {
                let min_indent = if block.is_ordered {
                    current_indent_level + last_marker_width
                } else {
                    current_indent_level + 2
                };
                (line_info.visual_indent, min_indent)
            };

            if prev_line_ends_with_backslash || effective_line_indent >= min_continuation_indent {
                // Indented line or backslash continuation continues the list
                if debug_list {
                    eprintln!(
                        "[DEBUG] Line {line_num}: indented continuation (indent={effective_line_indent}, min={min_continuation_indent})",
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
                    let effective_indent = if next_bq_level_for_indent > 0
                        && next_bq_level_for_indent == block_bq_level_for_indent
                        && let Some(columns) = indent_after_blockquote(next_content, next_bq_level_for_indent)
                    {
                        columns
                    } else {
                        next_line.visual_indent
                    };
                    // Use the minimum indent needed for any ancestor list item's continuation,
                    // not just the most deeply nested. A blank line followed by text indented
                    // at the parent level is a valid list continuation paragraph.
                    let root_continuation_indent = if block.is_ordered {
                        block.nesting_level + block.max_marker_width
                    } else {
                        block.nesting_level * 2 + 2
                    };
                    let adjusted_min_continuation = if block_bq_level_for_indent > 0 {
                        if block.is_ordered { last_marker_width } else { 2 }
                    } else {
                        min_continuation_indent.min(root_continuation_indent)
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
                    if effective_indent >= adjusted_min_continuation {
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
                        if column_at(next_line.content(content), item.marker_column) == current_indent_level
                            && item.is_ordered == block.is_ordered
                            && block.blockquote_prefix.trim() == next_blockquote_prefix.trim()
                        {
                            let block_bq_level = block.blockquote_prefix.chars().filter(|&c| c == '>').count();

                            // Root-level continuation indent for the list block
                            let root_cont = if block.is_ordered {
                                block.nesting_level + block.max_marker_width
                            } else {
                                block.nesting_level * 2 + 2
                            };

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
                                        trimmed.starts_with('>') && between_bq_level != block_bq_level;
                                    // Tables indented at list continuation level are content, not separators
                                    let table_breaks = crate::utils::skip_context::is_table_line(trimmed)
                                        && between_line.visual_indent < root_cont;
                                    trimmed.starts_with("```")
                                        || trimmed.starts_with("~~~")
                                        || trimmed.starts_with("---")
                                        || trimmed.starts_with("***")
                                        || trimmed.starts_with("___")
                                        || blockquote_level_changed
                                        || table_breaks
                                        || between_line.is_valid_heading()
                                } else {
                                    false
                                }
                            });
                            found_continuation = !has_structural_separators;
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
                let mut min_required_indent = if block.is_ordered {
                    let deep = current_indent_level + last_marker_width;
                    let root = block.nesting_level + block.max_marker_width;
                    deep.min(root)
                } else {
                    let deep = current_indent_level + 2;
                    let root = block.nesting_level * 2 + 2;
                    deep.min(root)
                };

                let line_content = line_info.content(content).trim();
                // Structural checks read the text after the blockquote prefix, so a
                // fence, thematic break or table row inside the quote is recognized.
                let inner_content = crate::utils::blockquote::parse_blockquote_prefix(line_content)
                    .map_or(line_content, |parsed| parsed.content)
                    .trim();

                let looks_like_table = crate::utils::skip_context::is_table_line(inner_content);

                let block_bq_level = block.blockquote_prefix.chars().filter(|&c| c == '>').count();
                let current_bq_level = blockquote_prefix.chars().filter(|&c| c == '>').count();
                let blockquote_level_changed = line_content.starts_with('>') && current_bq_level != block_bq_level;

                // For lines in the same blockquote context, compute indent after
                // stripping the blockquote prefix and adjust min_required_indent
                let effective_columns = if block_bq_level > 0
                    && current_bq_level == block_bq_level
                    && !blockquote_level_changed
                    && let Some(columns) = indent_after_blockquote(line_info.content(content), block_bq_level)
                {
                    min_required_indent = if block.is_ordered { last_marker_width } else { 2 };
                    columns
                } else {
                    line_info.visual_indent
                };

                // Whether the line reaches an item's content column is a question
                // of columns, so a tab counts for the width it spans.
                let inside_item = effective_columns >= min_required_indent;
                let is_structural_separator = opens_own_block(line_info, inner_content, effective_columns, inside_item)
                    || inner_content.starts_with("---")
                    || inner_content.starts_with("***")
                    || inner_content.starts_with("___")
                    || blockquote_level_changed
                    || (looks_like_table && !inside_item);

                // Text indented short of the content column is a lazy continuation
                // per CommonMark, however short the indent.
                let is_lazy_continuation = !is_structural_separator && !line_info.is_blank;

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

/// Whether a line indented short of the item's content column opens a block of
/// its own instead of continuing the item's paragraph.
///
/// Lazy continuation only applies to paragraph text, so a heading CommonMark
/// accepts, a fenced code opener, or an HTML block opener ends the item however
/// short its indent. A block-level tag interrupts a paragraph and rumdl reads
/// the lines it opens as HTML rather than list text; the same tag indented to
/// the content column is the item's own content and never reaches this check.
/// `inner_content` is the trimmed text after any blockquote prefix and
/// `indent_columns` the columns before it, so a tag inside a blockquote is
/// judged the same way as one at the root. An HTML block opens only within
/// three columns of indent; a tag indented further is text and continues the
/// paragraph, whatever the line's HTML flag says, since that flag is computed
/// from the trimmed line and marks such a tag as HTML too. `inside_item` says
/// the line reaches the content column an item's marker sets (the marker and
/// one space; padding beyond that is not tracked): a tag there opens an HTML
/// block inside that item, which is the block's own content however far it
/// falls short of a nested item's column, so it ends nothing.
fn opens_own_block(line_info: &LineInfo, inner_content: &str, indent_columns: usize, inside_item: bool) -> bool {
    line_info.is_valid_heading()
        || (!inside_item
            && indent_columns <= 3
            && crate::utils::html_block::parse_html_block_start(inner_content).is_some())
        || inner_content.starts_with("```")
        || inner_content.starts_with("~~~")
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
            if line_info.is_valid_heading() {
                return true;
            }

            // Horizontal rules separate lists (---, ***, ___)
            if is_horizontal_rule_content(trimmed) {
                return true;
            }

            // Tables separate lists (unless properly indented as list content)
            if crate::utils::skip_context::is_table_line(trimmed) {
                let min_continuation_indent = if current.is_ordered {
                    current.nesting_level + current.max_marker_width
                } else {
                    current.nesting_level + 2
                };
                if line_info.visual_indent < min_continuation_indent {
                    return true;
                }
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

#[cfg(test)]
mod indent_tests {
    use super::{column_at, indent_after_blockquote};

    #[test]
    fn indent_after_blockquote_measures_columns_as_commonmark_does() {
        // A tab reaches the next multiple of four counted from the start of the
        // line, and a tab right after the last marker gives that marker its
        // optional space and indents with the rest.
        for (line, level, columns) in [
            ("> foo", 1, 0),
            (">   foo", 1, 2),
            (">\tfoo", 1, 2),
            ("> \tfoo", 1, 2),
            (">\t\tfoo", 1, 6),
            (">  \tfoo", 1, 2),
            ("> >\tfoo", 2, 0),
            (">>\tfoo", 2, 1),
            ("> > \tfoo", 2, 4),
            ("  > \tfoo", 1, 4),
        ] {
            assert_eq!(indent_after_blockquote(line, level), Some(columns), "{line:?}");
        }
        assert_eq!(indent_after_blockquote("> foo", 2), None);
    }

    #[test]
    fn column_at_expands_tabs_to_the_next_tab_stop() {
        for (line, offset, column) in [
            ("- a", 0, 0),
            ("  - a", 2, 2),
            ("\t- a", 1, 4),
            (" \t- a", 2, 4),
            ("\t\t- a", 2, 8),
            ("> \t- a", 3, 4),
            (">\t- a", 2, 4),
        ] {
            assert_eq!(column_at(line, offset), column, "{line:?}");
        }
    }
}
