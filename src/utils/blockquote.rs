//! Blockquote-related utilities for rumdl.
//!
//! Provides functions for working with blockquote-prefixed lines, including
//! calculating effective indentation within blockquote context.

/// Parsed blockquote prefix components.
///
/// This parser supports both compact (`>> text`) and spaced (`> > text`) nesting.
/// It preserves at most one optional space/tab after the final marker in `prefix`;
/// additional whitespace is left in `content`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedBlockquotePrefix<'a> {
    /// Leading indentation before the first marker.
    pub indent: &'a str,
    /// Prefix up to (and including) the optional single space/tab after final `>`.
    pub prefix: &'a str,
    /// Content after `prefix` (may begin with additional spaces/tabs).
    pub content: &'a str,
    /// Number of blockquote markers at the start of the line.
    pub nesting_level: usize,
    /// All whitespace after the final `>` (including the optional first space/tab).
    pub spaces_after_marker: &'a str,
}

#[inline]
fn is_space_or_tab(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// Parse a blockquote prefix from a line.
///
/// Returns `None` when the line does not start (after leading indentation) with
/// a blockquote marker.
#[inline]
pub fn parse_blockquote_prefix(line: &str) -> Option<ParsedBlockquotePrefix<'_>> {
    let bytes = line.as_bytes();
    let mut pos = 0;

    // Leading indentation before the first marker.
    while pos < bytes.len() && is_space_or_tab(bytes[pos]) {
        pos += 1;
    }
    let indent_end = pos;

    if pos >= bytes.len() || bytes[pos] != b'>' {
        return None;
    }

    let mut nesting_level = 0;
    let mut prefix_end = pos;
    let mut spaces_after_marker_start = pos;
    let mut spaces_after_marker_end = pos;

    loop {
        if pos >= bytes.len() || bytes[pos] != b'>' {
            break;
        }

        nesting_level += 1;
        pos += 1; // past '>'
        let marker_end = pos;

        // Include at most one optional space/tab in the preserved prefix.
        if pos < bytes.len() && is_space_or_tab(bytes[pos]) {
            pos += 1;
        }
        let content_start_candidate = pos;

        // Consume any additional whitespace.
        while pos < bytes.len() && is_space_or_tab(bytes[pos]) {
            pos += 1;
        }

        // If another marker follows, all consumed whitespace belongs to nesting.
        if pos < bytes.len() && bytes[pos] == b'>' {
            continue;
        }

        // Final marker.
        prefix_end = content_start_candidate;
        spaces_after_marker_start = marker_end;
        spaces_after_marker_end = pos;
        break;
    }

    Some(ParsedBlockquotePrefix {
        indent: &line[..indent_end],
        prefix: &line[..prefix_end],
        content: &line[prefix_end..],
        nesting_level,
        spaces_after_marker: &line[spaces_after_marker_start..spaces_after_marker_end],
    })
}

/// Calculate the effective indentation of a line within a blockquote context.
///
/// For lines inside blockquotes, the "raw" leading whitespace (before `>`) is always 0,
/// but the semantically meaningful indent is the whitespace *after* the blockquote markers.
///
/// # Arguments
///
/// * `line_content` - The full line content including any blockquote markers
/// * `expected_bq_level` - The blockquote nesting level to match (0 for no blockquote)
/// * `fallback_indent` - The indent to return if blockquote levels don't match or if
///   `expected_bq_level` is 0
///
/// # Returns
///
/// The effective indentation:
/// - If `expected_bq_level` is 0: returns `fallback_indent`
/// - If line's blockquote level matches `expected_bq_level`: returns indent after stripping markers
/// - If blockquote levels don't match: returns `fallback_indent`
///
/// # Examples
///
/// ```
/// use rumdl_lib::utils::blockquote::effective_indent_in_blockquote;
///
/// // Regular line (no blockquote context)
/// assert_eq!(effective_indent_in_blockquote("   text", 0, 3), 3);
///
/// // Blockquote with 2 spaces after marker (first space consumed as marker syntax)
/// assert_eq!(effective_indent_in_blockquote(">  text", 1, 0), 1);
///
/// // Nested blockquote with 3 spaces after last marker (first space consumed as marker syntax)
/// assert_eq!(effective_indent_in_blockquote("> >   text", 2, 0), 2);
///
/// // Mismatched blockquote level - returns fallback
/// assert_eq!(effective_indent_in_blockquote("> text", 2, 5), 5);
/// ```
pub fn effective_indent_in_blockquote(line_content: &str, expected_bq_level: usize, fallback_indent: usize) -> usize {
    if expected_bq_level == 0 {
        return fallback_indent;
    }

    // Count blockquote markers at the start of the line
    // Markers can be separated by whitespace: "> > text" or ">> text"
    let line_bq_level = line_content
        .chars()
        .take_while(|c| *c == '>' || c.is_whitespace())
        .filter(|&c| c == '>')
        .count();

    if line_bq_level != expected_bq_level {
        return fallback_indent;
    }

    // Strip blockquote markers and compute indent within the blockquote context
    let mut pos = 0;
    let mut found_markers = 0;
    for c in line_content.chars() {
        pos += c.len_utf8();
        if c == '>' {
            found_markers += 1;
            if found_markers == line_bq_level {
                // Skip optional space after final >
                if line_content.get(pos..pos + 1) == Some(" ") {
                    pos += 1;
                }
                break;
            }
        }
    }

    let after_bq = &line_content[pos..];
    after_bq.len() - after_bq.trim_start().len()
}

/// Count the number of blockquote markers (`>`) at the start of a line.
///
/// Handles both compact (`>>text`) and spaced (`> > text`) blockquote syntax.
fn count_blockquote_level(line_content: &str) -> usize {
    line_content
        .chars()
        .take_while(|c| *c == '>' || c.is_whitespace())
        .filter(|&c| c == '>')
        .count()
}

/// Extract the content after blockquote markers.
///
/// Returns the portion of the line after all blockquote markers and the
/// optional space following the last marker.
///
/// # Examples
///
/// ```
/// use rumdl_lib::utils::blockquote::content_after_blockquote;
///
/// assert_eq!(content_after_blockquote("> text", 1), "text");
/// assert_eq!(content_after_blockquote(">  indented", 1), " indented");
/// assert_eq!(content_after_blockquote("> > nested", 2), "nested");
/// assert_eq!(content_after_blockquote("no quote", 0), "no quote");
/// ```
pub fn content_after_blockquote(line_content: &str, expected_bq_level: usize) -> &str {
    if expected_bq_level == 0 {
        return line_content;
    }

    // First, verify the line has the expected blockquote level
    let actual_level = count_blockquote_level(line_content);
    if actual_level != expected_bq_level {
        return line_content;
    }

    let mut pos = 0;
    let mut found_markers = 0;
    for c in line_content.chars() {
        pos += c.len_utf8();
        if c == '>' {
            found_markers += 1;
            if found_markers == expected_bq_level {
                // Skip optional space after final >
                if line_content.get(pos..pos + 1) == Some(" ") {
                    pos += 1;
                }
                break;
            }
        }
    }

    &line_content[pos..]
}

/// Strip all blockquote markers from a line, returning the content after them.
///
/// Unlike [`content_after_blockquote`], this does not require knowing the
/// expected nesting level — it strips all `>` markers and their trailing spaces.
/// Returns the original line unchanged if it has no blockquote markers.
///
/// # Examples
///
/// ```
/// use rumdl_lib::utils::blockquote::strip_blockquote_prefix;
///
/// assert_eq!(strip_blockquote_prefix("> text"), "text");
/// assert_eq!(strip_blockquote_prefix("> > nested"), "nested");
/// assert_eq!(strip_blockquote_prefix(">>compact"), "compact");
/// assert_eq!(strip_blockquote_prefix("  > indented"), "indented");
/// assert_eq!(strip_blockquote_prefix("no quote"), "no quote");
/// ```
pub fn strip_blockquote_prefix(line: &str) -> &str {
    match parse_blockquote_prefix(line) {
        Some(parsed) => parsed.content,
        None => line,
    }
}

/// The columns of whitespace `line` starts with, a tab running to the next multiple of four.
fn indent_columns(line: &str) -> usize {
    let mut column = 0;
    for c in line.chars() {
        match c {
            ' ' => column += 1,
            '\t' => column += 4 - column % 4,
            _ => break,
        }
    }
    column
}

/// The length of the list marker `text` opens with (`-`, `+`, `*`, or up to nine digits
/// then `.` or `)`), when a space, a tab or the end of the text follows it.
fn list_marker_len(text: &str) -> Option<usize> {
    let token = text.split([' ', '\t']).next().unwrap_or("");
    let digits = token.trim_end_matches(['.', ')']);
    let is_marker = matches!(token, "-" | "+" | "*")
        || (token.len() == digits.len() + 1
            && (1..=9).contains(&digits.len())
            && digits.bytes().all(|b| b.is_ascii_digit()));
    is_marker.then_some(token.len())
}

/// The visual column of byte `offset` in `line`, a tab running to the next multiple of four.
fn column_at(line: &str, offset: usize) -> usize {
    line[..offset].chars().fold(0, |column, c| match c {
        '\t' => column + 4 - column % 4,
        _ => column + 1,
    })
}

/// For a line that opens list items from `from` on (the byte offset after any quote
/// markers), the byte offset where the text of a quote opened inside them begins
/// (`- > text`, `1. - > text`, after the marker and its optional space), or `None` when
/// the items' content is not a quote. A list marker followed by five or more columns of
/// whitespace holds indented code, and a quote marker written four columns past the
/// marker before it is text.
pub fn quote_text_start_in_items(line: &str, from: usize) -> Option<usize> {
    let mut offset = from;
    let mut opened_item = false;
    // The visual column of `offset`, carried forward so tabs are measured at their real
    // tab stops without recounting the line for every nested marker.
    let mut column = column_at(line, from);
    loop {
        let rest = &line[offset..];
        let text = rest.trim_start_matches([' ', '\t']);
        let text_column = rest[..rest.len() - text.len()].chars().fold(column, |column, c| {
            if c == '\t' { column + 4 - column % 4 } else { column + 1 }
        });
        // The first marker may be indented up to three columns; the content of an item
        // starts within four columns after its marker, or is code.
        if text_column - column >= if opened_item { 5 } else { 4 } {
            return None;
        }
        if let Some(marker_len) = list_marker_len(text).filter(|&len| text.len() > len) {
            offset += rest.len() - text.len() + marker_len;
            column = text_column + marker_len;
            opened_item = true;
            continue;
        }
        if !opened_item {
            return None;
        }
        let parsed = parse_blockquote_prefix(rest)?;
        if !quote_markers_are_real(rest) {
            return None;
        }
        return Some(line.len() - parsed.content.len());
    }
}

/// The content columns of the list items an item line opens when `line` carries no
/// chain from the container pass (an item opened in a quote): the item's own, then
/// each item written on the same line inside it (`> - - x`).
fn written_item_columns(line: &str, content_column: usize, bare: bool) -> Vec<usize> {
    let mut columns = vec![content_column + usize::from(bare)];
    if bare {
        return columns;
    }
    // One forward pass: the visual column is carried along instead of recounted from the
    // start of the line for every offset.
    let mut position = 0;
    let mut column = 0;
    let mut column_at_forward = |target: usize| -> usize {
        for c in line[position..target].chars() {
            column += if c == '\t' { 4 - column % 4 } else { 1 };
        }
        position = target;
        column
    };
    let Some(mut offset) = line
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(line.len()))
        .find(|&index| column_at_forward(index) >= content_column)
    else {
        return columns;
    };
    while let Some(marker_len) =
        list_marker_len(&line[offset..]).filter(|_| !crate::utils::thematic_break::is_thematic_break(&line[offset..]))
    {
        let after_marker = offset + marker_len;
        let spaces = line[after_marker..].len() - line[after_marker..].trim_start_matches([' ', '\t']).len();
        let marker_end = column_at_forward(after_marker);
        let rest_blank = line[after_marker + spaces..].trim().is_empty();
        let spaces_columns = column_at_forward(after_marker + spaces) - marker_end;
        if rest_blank || spaces_columns >= 5 {
            columns.push(marker_end + 1);
            break;
        }
        columns.push(marker_end + spaces_columns);
        offset = after_marker + spaces;
    }
    columns
}

/// Whether the quote markers of `line` (zero-based `line_num`) are text continuing the
/// paragraph above rather than markers: the line sits four columns or more past the edge
/// of the container it is written in (`edges`, from `container_edges`), straight under a
/// line of text.
pub fn quote_markers_are_paragraph_text(
    ctx: &crate::lint_context::LintContext,
    edges: &[usize],
    line_num: usize,
    line: &str,
) -> bool {
    line_num > 0 && !ctx.lines[line_num - 1].is_blank && indent_columns(line) >= edges[line_num] + 4
}

/// The column each line's container starts its content at, indexed by zero-based line:
/// zero outside a list, and the content column of the innermost list item still open at
/// the line inside one. An item stays open through blank lines and ends at the first line
/// written left of its content column, or at a sibling or outer item marker. For a line
/// that opens an item, the edge is that of the container the item is written in.
///
/// An item opened in a quote is measured against the text after the quote markers on
/// quoted lines, and ends at the first unquoted line.
pub fn container_edges(ctx: &crate::lint_context::LintContext) -> Vec<usize> {
    container_edges_with_margins(ctx)
        .into_iter()
        .map(|(edge, _)| edge)
        .collect()
}

/// `container_edges`, with for each line the width of the quote markers (and the optional
/// space after them) in front of the line that opened the innermost open item. The edge
/// less this margin is the item's content column measured from the text after the
/// markers, which is how a quoted line is compared when its markers are written with a
/// different width (`>> - x` then `> > y`).
///
/// The margin is zero, meaning the edge is not to be trusted past the quote markers, for
/// an item opened outside a quote or in front of a tab, for a line inside a quote nested
/// deeper than the item's (its markers already consume the item's indentation), and after
/// a quoted line that ended inner items while continuing a paragraph (a possible lazy
/// continuation).
pub fn container_edges_with_margins(ctx: &crate::lint_context::LintContext) -> Vec<(usize, usize)> {
    // Open items, outermost first: content column, whether the item opened in a quote
    // the width of the quote markers in front of it and how many quotes deep it opened.
    let mut open: Vec<(usize, bool, usize, usize)> = Vec::new();
    let mut edges = Vec::with_capacity(ctx.lines.len());
    // Set when a quoted line ended inner items while continuing a paragraph: it may be a lazy
    // continuation of one of them, so the surviving edge no longer says where later lines belong.
    let mut lazy_taint = false;
    for (index, info) in ctx.lines.iter().enumerate() {
        let depth = info.blockquote.as_ref().map_or(0, |quote| quote.nesting_level);
        if !info.is_blank {
            let line = info.content(ctx.content);
            let quoted = info.blockquote.is_some();
            let margin = match parse_blockquote_prefix(line) {
                // A tab on the opener line makes its columns depend on the tab stop, so the
                // margin is not known and the item is not measured past its quote.
                Some(parsed) if quoted && !line.contains('\t') => column_at(line, parsed.prefix.len()),
                _ => 0,
            };
            // The column a line is measured at against an open item: past the quote
            // markers when both the item and the line are quoted.
            let measured = |opened_quoted: bool,
                            opened_margin: usize,
                            opened_depth: usize,
                            content: usize,
                            column: usize,
                            raw_column: usize|
             -> bool {
                if opened_quoted && quoted {
                    // Leaving the quote an item was opened in ends the item.
                    depth < opened_depth || content.saturating_sub(opened_margin) > column.saturating_sub(margin)
                } else {
                    content > raw_column
                }
            };
            match info.list_item.as_deref() {
                Some(item) => {
                    while open.last().is_some_and(|&(content, oq, om, od)| {
                        measured(oq, om, od, content, item.marker_column, item.marker_column)
                    }) {
                        open.pop();
                    }
                    edges.push(open.last().map_or((0, 0), |&(content, _, om, od)| {
                        (content, if depth > od || lazy_taint { 0 } else { om })
                    }));
                    // An item with nothing after its marker takes its content from the
                    // lines below, which have to clear the marker and the space after it.
                    let bare = line.trim_end().chars().count() <= item.content_column;
                    let columns = if item.commonmark_items.is_empty() {
                        written_item_columns(line, item.content_column, bare)
                    } else {
                        item.commonmark_items.iter().map(|item| item.content_column).collect()
                    };
                    // An item inside an unquoted item is compared by raw indentation: its quote
                    // markers sit in the outer item's content, so margins do not line up.
                    let relative = quoted && open.iter().all(|&(_, oq, _, _)| oq);
                    open.extend(columns.into_iter().map(|content| (content, relative, margin, depth)));
                    lazy_taint = false;
                    continue;
                }
                None => {
                    let column = match parse_blockquote_prefix(line) {
                        Some(parsed) if quoted => column_at(line, parsed.prefix.len()) + indent_columns(parsed.content),
                        _ => indent_columns(line),
                    };
                    let before = open.len();
                    while open.last().is_some_and(|&(content, oq, om, od)| {
                        measured(oq, om, od, content, column, indent_columns(line))
                    }) {
                        open.pop();
                    }
                    if index > 0 && !ctx.lines[index - 1].is_blank && open.len() < before {
                        lazy_taint = true;
                    }
                    if open.is_empty() {
                        lazy_taint = false;
                    }
                }
            }
        }
        edges.push(open.last().map_or((0, 0), |&(content, _, om, od)| {
            (content, if depth > od || lazy_taint { 0 } else { om })
        }));
    }
    edges
}

/// Whether the unquoted `line` at zero-based `line_num` is indented four columns or more
/// past the edge of its container (`container_edges`), which makes it indented code rather
/// than a heading. `10. x` followed by a heading at column 6 is still a heading.
pub fn is_indented_past_edge(edges: &[usize], line_num: usize, line: &str) -> bool {
    indent_columns(line) >= edges[line_num] + 4
}

/// Whether every quote marker opening `line` opens a quote: a marker written
/// four columns or more past the optional space after the one before it is text
/// of the quote around it. Tabs advance to the next multiple of four.
pub fn quote_markers_are_real(line: &str) -> bool {
    let mut column = 0;
    // Column just past the previous marker, once a marker has been seen.
    let mut after_marker: Option<usize> = None;
    for c in line.chars() {
        match c {
            ' ' => column += 1,
            '\t' => column += 4 - column % 4,
            '>' => {
                if after_marker.is_some_and(|end| column >= end + 1 + 4) {
                    return false;
                }
                column += 1;
                after_marker = Some(column);
            }
            _ => break,
        }
    }
    true
}

/// The length of the quote markers opening `line`, up to `depth` of them, with
/// the one optional space or tab after the last. Markers past `depth` are text
/// of the heading (`>     > x` in a setext paragraph), not quotes. `None` for a
/// line without any marker.
pub fn quote_margin_len(line: &str, depth: usize) -> Option<usize> {
    let mut pos = 0;
    let mut markers = 0;
    while markers < depth {
        let rest = &line[pos..];
        let next = rest.trim_start_matches([' ', '\t']);
        if !next.starts_with('>') {
            break;
        }
        pos += rest.len() - next.len() + 1;
        markers += 1;
    }
    if markers == 0 {
        return None;
    }
    Some(pos + usize::from(line[pos..].starts_with([' ', '\t'])))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // parse_blockquote_prefix tests
    // ==========================================================================

    #[test]
    fn test_parse_blockquote_prefix_compact_nested() {
        let parsed = parse_blockquote_prefix(">> text").expect("should parse compact nested blockquote");
        assert_eq!(parsed.indent, "");
        assert_eq!(parsed.prefix, ">> ");
        assert_eq!(parsed.content, "text");
        assert_eq!(parsed.nesting_level, 2);
        assert_eq!(parsed.spaces_after_marker, " ");
    }

    #[test]
    fn test_parse_blockquote_prefix_spaced_nested() {
        let parsed = parse_blockquote_prefix("> >  text").expect("should parse spaced nested blockquote");
        assert_eq!(parsed.indent, "");
        assert_eq!(parsed.prefix, "> > ");
        assert_eq!(parsed.content, " text");
        assert_eq!(parsed.nesting_level, 2);
        assert_eq!(parsed.spaces_after_marker, "  ");
    }

    #[test]
    fn test_parse_blockquote_prefix_with_indent() {
        let parsed = parse_blockquote_prefix("   > quote").expect("should parse indented blockquote");
        assert_eq!(parsed.indent, "   ");
        assert_eq!(parsed.prefix, "   > ");
        assert_eq!(parsed.content, "quote");
        assert_eq!(parsed.nesting_level, 1);
        assert_eq!(parsed.spaces_after_marker, " ");
    }

    #[test]
    fn test_parse_blockquote_prefix_non_blockquote() {
        assert!(parse_blockquote_prefix("plain text").is_none());
        assert!(parse_blockquote_prefix("  plain text").is_none());
    }

    // ==========================================================================
    // effective_indent_in_blockquote tests
    // ==========================================================================

    #[test]
    fn test_effective_indent_no_blockquote_context() {
        // When expected_bq_level is 0, always return fallback
        assert_eq!(effective_indent_in_blockquote("text", 0, 0), 0);
        assert_eq!(effective_indent_in_blockquote("   text", 0, 3), 3);
        assert_eq!(effective_indent_in_blockquote("> text", 0, 5), 5);
    }

    #[test]
    fn test_effective_indent_single_level_blockquote() {
        // Single > with various indents after
        assert_eq!(effective_indent_in_blockquote("> text", 1, 99), 0);
        assert_eq!(effective_indent_in_blockquote(">  text", 1, 99), 1);
        assert_eq!(effective_indent_in_blockquote(">   text", 1, 99), 2);
        assert_eq!(effective_indent_in_blockquote(">    text", 1, 99), 3);
    }

    #[test]
    fn test_effective_indent_no_space_after_marker() {
        // >text (no space after >) - should have 0 effective indent
        assert_eq!(effective_indent_in_blockquote(">text", 1, 99), 0);
        assert_eq!(effective_indent_in_blockquote(">>text", 2, 99), 0);
    }

    #[test]
    fn test_effective_indent_nested_blockquote_compact() {
        // Compact nested: >>text, >> text, >>  text
        assert_eq!(effective_indent_in_blockquote(">> text", 2, 99), 0);
        assert_eq!(effective_indent_in_blockquote(">>  text", 2, 99), 1);
        assert_eq!(effective_indent_in_blockquote(">>   text", 2, 99), 2);
    }

    #[test]
    fn test_effective_indent_nested_blockquote_spaced() {
        // Spaced nested: > > text, > >  text
        assert_eq!(effective_indent_in_blockquote("> > text", 2, 99), 0);
        assert_eq!(effective_indent_in_blockquote("> >  text", 2, 99), 1);
        assert_eq!(effective_indent_in_blockquote("> >   text", 2, 99), 2);
    }

    #[test]
    fn test_effective_indent_mismatched_level() {
        // Line has different blockquote level than expected - return fallback
        assert_eq!(effective_indent_in_blockquote("> text", 2, 42), 42);
        assert_eq!(effective_indent_in_blockquote(">> text", 1, 42), 42);
        assert_eq!(effective_indent_in_blockquote("text", 1, 42), 42);
    }

    #[test]
    fn test_effective_indent_empty_blockquote() {
        // Empty blockquote lines
        assert_eq!(effective_indent_in_blockquote(">", 1, 99), 0);
        assert_eq!(effective_indent_in_blockquote("> ", 1, 99), 0);
        assert_eq!(effective_indent_in_blockquote(">  ", 1, 99), 1);
    }

    #[test]
    fn test_effective_indent_issue_268_case() {
        // The exact pattern from issue #268:
        // ">   text" where we expect 2 spaces of indent (list continuation)
        assert_eq!(effective_indent_in_blockquote(">   Opening the app", 1, 0), 2);
        assert_eq!(
            effective_indent_in_blockquote(">   [**See preview here!**](https://example.com)", 1, 0),
            2
        );
    }

    #[test]
    fn test_effective_indent_triple_nested() {
        // Triple nested blockquotes
        assert_eq!(effective_indent_in_blockquote("> > > text", 3, 99), 0);
        assert_eq!(effective_indent_in_blockquote("> > >  text", 3, 99), 1);
        assert_eq!(effective_indent_in_blockquote(">>> text", 3, 99), 0);
        assert_eq!(effective_indent_in_blockquote(">>>  text", 3, 99), 1);
    }

    // ==========================================================================
    // count_blockquote_level tests
    // ==========================================================================

    #[test]
    fn test_count_blockquote_level_none() {
        assert_eq!(count_blockquote_level("regular text"), 0);
        assert_eq!(count_blockquote_level("   indented text"), 0);
        assert_eq!(count_blockquote_level(""), 0);
    }

    #[test]
    fn test_count_blockquote_level_single() {
        assert_eq!(count_blockquote_level("> text"), 1);
        assert_eq!(count_blockquote_level(">text"), 1);
        assert_eq!(count_blockquote_level(">"), 1);
    }

    #[test]
    fn test_count_blockquote_level_nested() {
        assert_eq!(count_blockquote_level(">> text"), 2);
        assert_eq!(count_blockquote_level("> > text"), 2);
        assert_eq!(count_blockquote_level(">>> text"), 3);
        assert_eq!(count_blockquote_level("> > > text"), 3);
    }

    // ==========================================================================
    // content_after_blockquote tests
    // ==========================================================================

    #[test]
    fn test_content_after_blockquote_no_quote() {
        assert_eq!(content_after_blockquote("text", 0), "text");
        assert_eq!(content_after_blockquote("   indented", 0), "   indented");
    }

    #[test]
    fn test_content_after_blockquote_single() {
        assert_eq!(content_after_blockquote("> text", 1), "text");
        assert_eq!(content_after_blockquote(">text", 1), "text");
        assert_eq!(content_after_blockquote(">  indented", 1), " indented");
    }

    #[test]
    fn test_content_after_blockquote_nested() {
        assert_eq!(content_after_blockquote(">> text", 2), "text");
        assert_eq!(content_after_blockquote("> > text", 2), "text");
        assert_eq!(content_after_blockquote("> >  indented", 2), " indented");
    }

    #[test]
    fn test_content_after_blockquote_mismatched_level() {
        // If level doesn't match, return original
        assert_eq!(content_after_blockquote("> text", 2), "> text");
        assert_eq!(content_after_blockquote(">> text", 1), ">> text");
    }
}
