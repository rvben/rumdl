//! Parser confirmation of the table blocks the line scanner finds.
//!
//! The scanner in `TableUtils` is lenient on purpose: MD056 and MD075 report on
//! tables whose delimiter row is wrong. Rules that rewrite a table (MD055, MD058,
//! MD060) must only touch what the Markdown parser reads as one, or a fix turns
//! code, prose or raw HTML into a table, or moves the table out of its container.
//! This pass marks each block `parser_confirmed` and trims its body to the rows the
//! parser agrees with.
//!
//! A block is confirmed when its header and delimiter row are well formed and one
//! of these holds:
//!
//! - the central parse has a table starting on the header line;
//! - the header interrupts a paragraph, which GFM allows and pulldown-cmark does
//!   not: parsing the enclosing top-level block with one blank line inserted
//!   before the header gives a table in the same containers;
//! - the header sits in a container the parser reads as code or HTML but the
//!   flavor reads as Markdown (MkDocs admonitions, tabs and `<div markdown>`,
//!   MyST directives): the same checks run on the header's own block, dedented;
//! - the flavor is MDG and the header sits at a Gherkin table indent.

use super::LineInfo;
use crate::config::MarkdownFlavor;
use crate::utils::blockquote::strip_blockquote_prefix;
use crate::utils::parser_options::rumdl_parser_options_for;
use crate::utils::table_utils::TableBlock;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

/// Largest slice re-parsed to confirm one table.
const MAX_SLICE_BYTES: usize = 32 * 1024;

/// What the central parse, the flavor carving and the native MDX parse know.
pub(super) struct Evidence<'a> {
    pub content: &'a str,
    pub lines: &'a [LineInfo],
    pub line_offsets: &'a [usize],
    pub flavor: MarkdownFlavor,
    pub table_ranges: &'a [(usize, usize)],
    pub top_level_blocks: &'a [(usize, usize)],
    pub rejected_definitions: &'a [(usize, usize)],
    /// List items as `(start, end, content start)` byte offsets
    pub list_items: &'a [(usize, usize, usize)],
    /// For each list item, the index of the item enclosing it
    pub list_item_parent: &'a [Option<usize>],
    /// Code block ranges as the parser reads them, before flavor carving
    pub parser_code_blocks: &'a [(usize, usize)],
    /// Tables the native MDX parser reads
    pub native_tables: &'a [(usize, usize)],
    /// Per line, whether raw HTML opens earlier in the carved container above it;
    /// built on first use
    pub html_above: std::cell::OnceCell<Vec<bool>>,
    /// Per line, whether it is a MkDocs container body that starts right under its
    /// opener; built on first use
    pub opener_body: std::cell::OnceCell<Vec<bool>>,
    /// Per line, whether it starts inside a rejected definition and whether it or a
    /// line of the unbroken non-blank run above it does; built on first use
    pub rejected_definition_reach: std::cell::OnceCell<Vec<(bool, bool)>>,
}

/// Set `parser_confirmed` on every block and trim bodies to the parsed table.
pub(super) fn confirm_table_blocks(blocks: Vec<TableBlock>, ev: &Evidence) -> Vec<TableBlock> {
    blocks
        .into_iter()
        .map(|mut block| {
            if let Some(end_line) = confirmed_end(&block, ev, false) {
                if !is_gherkin_table(&block, ev) {
                    trim_body(&mut block, end_line, ev);
                }
                block.parser_confirmed = true;
            } else if let Some(end_line) = confirmed_end(&block, ev, true) {
                trim_body(&mut block, end_line, ev);
                block.repairable = true;
            }
            block
        })
        .collect()
}

/// An indented Gherkin table, whose rows follow Gherkin's indentation rules
/// rather than CommonMark's.
fn is_gherkin_table(block: &TableBlock, ev: &Evidence) -> bool {
    let header = line_text(ev, block.header_line);
    ev.flavor == MarkdownFlavor::MDG && crate::utils::mdg::has_table_indent(header) && header.contains('|')
}

/// With `repair`, the delimiter row is read with the header's cell count, so a
/// block whose only defect is a delimiter row of the wrong width is judged by
/// where it sits rather than by that width.
fn confirmed_end(block: &TableBlock, ev: &Evidence, repair: bool) -> Option<usize> {
    let header = line_text(ev, block.header_line);
    let delimiter = line_text(ev, block.delimiter_line);
    let last = block.end_line;

    if is_gherkin_table(block, ev) {
        return Some(last);
    }
    let divergent = strict_structure(header, delimiter, block, ev.flavor)?;
    // A lone CR ends a line for the parser but not for the line index, so the
    // parser's table lines cannot be matched to the scanner's.
    let has_lone_cr = |line: &str| line.trim_end_matches('\r').contains('\r');
    if (block.header_line..=block.end_line).any(|index| has_lone_cr(line_text(ev, index))) {
        return None;
    }
    if touches_rejected_definition(ev, block)
        || has_unsafe_list_layout(ev, block)
        || header_is_lazy_item_text(ev, block)
    {
        return None;
    }
    if ev.flavor != MarkdownFlavor::MDX && is_carved(ev, block.header_line) {
        return confirm_carved(ev, block, divergent, repair);
    }
    if divergent {
        return confirm_in_enclosing_block(ev, block, true, repair);
    }
    if !repair && let Some(end) = primary(ev, block) {
        return Some(end.min(last));
    }
    confirm_in_enclosing_block(ev, block, false, repair)
}

fn line_text<'a>(ev: &Evidence<'a>, index: usize) -> &'a str {
    ev.lines
        .get(index)
        .map_or("", |line| line.content(ev.content).trim_end_matches('\r'))
}

fn line_of(ev: &Evidence, offset: usize) -> usize {
    ev.line_offsets
        .partition_point(|&start| start <= offset)
        .saturating_sub(1)
}

fn last_line_of(ev: &Evidence, start: usize, end: usize) -> usize {
    line_of(ev, end.saturating_sub(1).max(start))
}

fn covering(ranges: &[(usize, usize)], offset: usize) -> bool {
    let index = ranges.partition_point(|&(start, _)| start <= offset);
    index > 0 && offset < ranges[index - 1].1
}

/// Columns of leading whitespace, tabs advancing to the next multiple of four.
fn indent_columns(line: &str) -> usize {
    let mut columns = 0;
    for byte in line.bytes() {
        match byte {
            b' ' => columns += 1,
            b'\t' => columns = (columns / 4 + 1) * 4,
            _ => break,
        }
    }
    columns
}

/// The end line of a table the central parse (or, for MDX, the native parse)
/// starts on the header line.
fn primary(ev: &Evidence, block: &TableBlock) -> Option<usize> {
    let header_offset = ev.lines.get(block.header_line)?.byte_offset;
    let header_end = header_offset + ev.lines[block.header_line].byte_len;
    let starting_here = |ranges: &[(usize, usize)]| -> Option<(usize, usize)> {
        let index = ranges.partition_point(|&(start, _)| start < header_offset);
        ranges[index..]
            .iter()
            .take_while(|&&(start, _)| start <= header_end)
            .find(|&&(start, _)| line_of(ev, start) == block.header_line)
            .copied()
    };
    let (start, end) = [starting_here(ev.table_ranges), starting_here(ev.native_tables)]
        .into_iter()
        .flatten()
        .max_by_key(|&(_, end)| end)?;
    Some(last_line_of(ev, start, end))
}

/// A rejected `:warning:` definition is a container to the parser but paragraph
/// text to rumdl. A table line inside one, or in the paragraph run that touches
/// one with no blank line between, is read differently by the two, so the block
/// is left to neither.
fn touches_rejected_definition(ev: &Evidence, block: &TableBlock) -> bool {
    if ev.rejected_definitions.is_empty() {
        return false;
    }
    let reach = ev.rejected_definition_reach.get_or_init(|| {
        let total = ev.lines.len();
        let mut inside = vec![false; total];
        for &(start, end) in ev.rejected_definitions {
            let first = ev.line_offsets.partition_point(|&offset| offset < start);
            let past = ev.line_offsets.partition_point(|&offset| offset < end);
            inside[first.min(total)..past.min(total)].fill(true);
        }
        // A block that starts on a line ends the paragraph above it, so the run
        // does not reach across a heading, rule, fence or HTML block.
        let mut starts_block = vec![false; total];
        for &(start, _) in ev.top_level_blocks {
            let line = ev
                .line_offsets
                .partition_point(|&offset| offset <= start)
                .saturating_sub(1);
            if let Some(slot) = starts_block.get_mut(line) {
                *slot = true;
            }
        }
        // A thematic break is an event of its own, not a block the parser opens.
        for (line, slot) in starts_block.iter_mut().enumerate() {
            *slot = *slot || is_thematic_break(strip_blockquote_prefix(line_text(ev, line)));
        }
        let mut run_touches = false;
        (0..total)
            .map(|line| {
                run_touches =
                    !line_text(ev, line).trim().is_empty() && (inside[line] || (run_touches && !starts_block[line]));
                (inside[line], run_touches)
            })
            .collect()
    });
    let inside = |line: usize| reach.get(line).is_some_and(|&(inside, _)| inside);
    inside(block.header_line)
        || inside(block.delimiter_line)
        || block
            .header_line
            .checked_sub(1)
            .and_then(|above| reach.get(above))
            .is_some_and(|&(_, run_touches)| run_touches)
}

/// Three or more `*`, `-` or `_` of one kind, with spaces or tabs between and nothing else.
fn is_thematic_break(line: &str) -> bool {
    let mut marks = line.trim().bytes().filter(|&byte| byte != b' ' && byte != b'\t');
    let Some(first) = marks.next().filter(|byte| matches!(byte, b'*' | b'-' | b'_')) else {
        return false;
    };
    let rest: Vec<u8> = marks.collect();
    rest.len() >= 2 && rest.iter().all(|&byte| byte == first)
}

/// The header directly follows a list item's text but sits left of the item's
/// content column, which makes it lazy continuation text of the item, while the
/// delimiter row sits inside the item. The parser reads a table there that the
/// item's paragraph swallows, so a rewrite would pull the rows out of the item.
fn header_is_lazy_item_text(ev: &Evidence, block: &TableBlock) -> bool {
    let Some(above) = block.header_line.checked_sub(1) else {
        return false;
    };
    let line = line_text(ev, above);
    let text = strip_blockquote_prefix(line);
    if text.trim().is_empty() {
        return false;
    }
    // A header that opens an item of its own continues nothing.
    let header = strip_blockquote_prefix(line_text(ev, block.header_line));
    if !crate::utils::table_utils::TableUtils::extract_list_prefix(header)
        .0
        .is_empty()
    {
        return false;
    }
    let offset = ev.lines[above].byte_offset + (line.len() - text.len()) + (text.len() - text.trim_start().len());
    let Some(&(start, _, content)) = items_containing(ev, offset).next() else {
        return false;
    };
    let column = item_content_column(ev, start, content);
    let indent = |index: usize| indent_columns(strip_blockquote_prefix(line_text(ev, index)));
    indent(block.header_line) < column && indent(block.delimiter_line) >= column
}

/// The list items holding `offset`, innermost first. Items are sorted by start,
/// so the walk starts at the last item opened by `offset`; every item holding
/// `offset` is that item or one of its ancestors.
fn items_containing<'a>(ev: &'a Evidence, offset: usize) -> impl Iterator<Item = &'a (usize, usize, usize)> {
    let opened = ev.list_items.partition_point(|&(start, _, _)| start <= offset);
    std::iter::successors(opened.checked_sub(1), |&index| ev.list_item_parent[index])
        .map(|index| &ev.list_items[index])
        .filter(move |&&(_, end, _)| offset < end)
}

/// A table in a list item whose layout a rewrite cannot reproduce, so a fix would
/// move rows or the table out of the item or split rows from it:
///
/// - the scanner's content column differs from the one the parser reads for the
///   innermost list item holding the header (nested markers, tab indentation);
/// - the header shares the marker line and a gap of more than one column follows
///   the marker, which a rewrite collapses;
/// - the header shares the marker line and a row after it is indented with a tab,
///   which the scanner measures as one column.
fn has_unsafe_list_layout(ev: &Evidence, block: &TableBlock) -> bool {
    use crate::utils::table_utils::TableUtils;
    let Some(info) = ev.lines.get(block.header_line) else {
        return false;
    };
    let line = line_text(ev, block.header_line);
    let header = strip_blockquote_prefix(line);
    // Where the header's own text starts, past any blockquote prefix and indentation.
    let text_offset = info.byte_offset + (line.len() - header.len()) + (header.len() - header.trim_start().len());
    // Markers in front of the header on its own line open items that hold it too
    // (`- - |a|b|`), so the innermost one is found from where the header text starts.
    let text_offset = text_offset.max(info.byte_offset + block.header_prefix_len.unwrap_or(0));
    let parser_column = items_containing(ev, text_offset)
        .find(|&&(start, _, _)| !quote_opens_inside_item(ev, start, block.header_line))
        .map(|&(start, _, content)| item_content_column(ev, start, content));
    let scanner_column = block.list_context.as_ref().map(|list| list.content_indent);
    // A carved container or JSX element has no list item in the central parse, so
    // only a disagreement about an item the parser does read counts.
    let carved_without_item =
        parser_column.is_none() && (ev.flavor == MarkdownFlavor::MDX || is_carved(ev, block.header_line));
    // A marker the scanner takes for a list item that the parser reads as a setext
    // underline or paragraph text leaves a phantom context; a header indented exactly
    // to its content column is a table in whatever the parser holds it in, which a
    // rewrite keeps where it is.
    let phantom_context = (parser_column.is_none() || rejected_marker_above(ev, block.header_line))
        && !line[..line.len() - header.len()].contains('\t')
        && scanner_column.is_some_and(|column| {
            let indent = &header[..header.len() - header.trim_start().len()];
            column < parser_column.unwrap_or(0).saturating_add(4) && indent.len() == column && !indent.contains('\t')
        });
    if parser_column != scanner_column && !carved_without_item && !phantom_context {
        return true;
    }
    let (prefix, rest, _) = TableUtils::extract_list_prefix(header);
    if prefix.is_empty() {
        return false;
    }
    if rest.starts_with([' ', '\t']) {
        return true;
    }
    // The line after the scanned end counts when it reads as a row: the parser may
    // continue the table over a tab-indented row the scanner stops at.
    let next_row = block.end_line + 1 < ev.lines.len() && line_text(ev, block.end_line + 1).contains('|');
    (block.header_line + 1..=block.end_line + usize::from(next_row)).any(|row| {
        let text = strip_blockquote_prefix(line_text(ev, row));
        text[..text.len() - text.trim_start().len()].contains('\t')
    })
}

/// The line above `header_line` is a bare list marker (`-`, `+`, `*`, `1.`) after
/// any blockquote prefix that no list item the parser reads starts on.
fn rejected_marker_above(ev: &Evidence, header_line: usize) -> bool {
    let Some(above) = header_line.checked_sub(1) else {
        return false;
    };
    let text = strip_blockquote_prefix(line_text(ev, above)).trim();
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    let bare = matches!(text, "-" | "+" | "*") || ((1..=9).contains(&digits) && matches!(&text[digits..], "." | ")"));
    let line_start = ev.lines[above].byte_offset;
    let first = ev.list_items.partition_point(|&(start, _, _)| start < line_start);
    bare && ev
        .list_items
        .get(first)
        .is_none_or(|&(start, _, _)| line_of(ev, start) != above)
}

/// The header line sits in a blockquote that opens inside the list item opened at
/// `start`: the header is quoted more deeply than the markers in front of the
/// item's own marker (`- > |a|b|`, or a later line `> |a|b|` of the item). The
/// scanner sees no list item for it, and the rewrite keeps the whole prefix as it is.
fn quote_opens_inside_item(ev: &Evidence, start: usize, header_line: usize) -> bool {
    let marker_line = line_of(ev, start);
    let before_marker = line_text(ev, marker_line)
        .get(..start - ev.lines[marker_line].byte_offset)
        .unwrap_or("");
    let outside = before_marker.bytes().filter(|&byte| byte == b'>').count();
    crate::utils::table_utils::header_quote_depth(
        line_text(ev, header_line),
        ev.lines[header_line].byte_offset,
        ev.list_items,
    ) > outside
}

/// Column where the list item opened at `start` holds its content, by the rule a
/// parser applies to the marker line: after the marker and the one to four columns
/// following it, one column past the marker when the rest of the line is blank or
/// five or more columns follow. The first child's position is not the content
/// column, since it can sit further right.
fn item_content_column(ev: &Evidence, start: usize, content: usize) -> usize {
    use crate::utils::table_utils::TableUtils;
    let line = line_of(ev, start);
    let from_marker = start - ev.lines[line].byte_offset;
    let marker = line_text(ev, line).get(from_marker..).unwrap_or("");
    // Pad with spaces to the marker's column so the rule sees the marker where it is.
    let marker_column = content_column(ev, start);
    if marker_column == usize::MAX {
        // The marker sits inside a tab the quote prefix leaves partly consumed.
        return usize::MAX;
    }
    let padded = format!("{}{marker}", " ".repeat(marker_column));
    match TableUtils::extract_list_prefix(&padded) {
        ("", _, _) => content_column(ev, content),
        (_, _, column) => column,
    }
}

/// Column of `offset` after any blockquote prefix, tabs advancing to the next
/// multiple of four; `usize::MAX` when the offset lies inside the prefix.
fn content_column(ev: &Evidence, offset: usize) -> usize {
    let line = line_of(ev, offset);
    let text = line_text(ev, line);
    let rest = strip_blockquote_prefix(text);
    let prefix = text.len() - rest.len();
    let Some(within) = (offset - ev.lines[line].byte_offset).checked_sub(prefix) else {
        return usize::MAX;
    };
    rest.get(..within).map_or(usize::MAX, |before| {
        before.bytes().fold(0, |column, byte| match byte {
            b'\t' => (column / 4 + 1) * 4,
            _ => column + 1,
        })
    })
}

/// Cut the body at the parsed table's end and at the first row indented far
/// enough to be code beside the table.
fn trim_body(block: &mut TableBlock, parsed_end: usize, ev: &Evidence) {
    let header = strip_blockquote_prefix(line_text(ev, block.header_line));
    let limit = match &block.list_context {
        Some(list) => list.content_indent + 4,
        None => indent_columns(header) / 4 * 4 + 4,
    };
    let mut end = block.delimiter_line;
    for &row in &block.content_lines {
        if row > parsed_end || indent_columns(strip_blockquote_prefix(line_text(ev, row))) >= limit {
            break;
        }
        end = row;
    }
    block.end_line = end;
    block.content_lines.retain(|&row| row <= end);
}

// ---- Strict structure -------------------------------------------------------------

/// The delimiter row is well formed. Cell counts and blockquote depth are left to
/// the parser that confirms the table. `Some(divergent)` when it is, where
/// `divergent` says the flavor counts the header's cells differently
/// from a parser that splits on every unescaped pipe; `None` when it is not.
fn strict_structure(header: &str, delimiter: &str, block: &TableBlock, flavor: MarkdownFlavor) -> Option<bool> {
    let mut header_row = strip_blockquote_prefix(header);
    if block.list_context.is_some() {
        let (prefix, rest, _) = crate::utils::table_utils::TableUtils::extract_list_prefix(header_row);
        if !prefix.is_empty() {
            header_row = rest;
        }
    }
    let delimiter_row = strip_blockquote_prefix(delimiter);

    // A dash followed by whitespace opens a list item, so the row is not a delimiter.
    if let Some(rest) = delimiter_row.trim_start().strip_prefix('-')
        && rest.starts_with([' ', '\t'])
    {
        return None;
    }
    let delimiter_cells = split_cells(delimiter_row, MarkdownFlavor::Standard);
    if delimiter_cells.is_empty() || !delimiter_cells.iter().all(|cell| is_delimiter_cell(cell)) {
        return None;
    }
    let plain = split_cells(header_row, MarkdownFlavor::Standard).len();
    let aware = split_cells(header_row, flavor).len();
    Some(plain != aware)
}

fn is_delimiter_cell(cell: &str) -> bool {
    let cell = cell.trim_matches([' ', '\t']);
    let core = cell.strip_prefix(':').unwrap_or(cell);
    let core = core.strip_suffix(':').unwrap_or(core);
    !core.is_empty() && core.bytes().all(|byte| byte == b'-')
}

fn split_cells(row: &str, flavor: MarkdownFlavor) -> Vec<String> {
    crate::utils::table_utils::TableUtils::split_table_row_for_rendering(row, flavor)
}

// ---- Parsing slices ---------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ContainerKind {
    Quote,
    Item,
    Definition,
}

struct Structure {
    tables: Vec<(usize, usize)>,
    top_level: Vec<(usize, usize)>,
    containers: Vec<(ContainerKind, usize, usize)>,
    html_blocks: Vec<(usize, usize)>,
}

fn parse_structure(text: &str) -> Structure {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let line = |offset: usize| starts.partition_point(|&start| start <= offset).saturating_sub(1);
    let span = |range: &std::ops::Range<usize>| (line(range.start), line(range.end.saturating_sub(1).max(range.start)));
    let mut structure = Structure {
        tables: Vec::new(),
        top_level: Vec::new(),
        containers: Vec::new(),
        html_blocks: Vec::new(),
    };
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(text, rumdl_parser_options_for(text)).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                let block = !matches!(
                    tag,
                    Tag::Emphasis
                        | Tag::Strong
                        | Tag::Strikethrough
                        | Tag::Superscript
                        | Tag::Subscript
                        | Tag::Link { .. }
                        | Tag::Image { .. }
                );
                if !block {
                    continue;
                }
                let (first, last) = span(&range);
                if depth == 0 {
                    structure.top_level.push((first, last));
                }
                depth += 1;
                match tag {
                    Tag::Table(_) => structure.tables.push((first, last)),
                    Tag::BlockQuote(_) => structure.containers.push((ContainerKind::Quote, first, last)),
                    Tag::Item => structure.containers.push((ContainerKind::Item, first, last)),
                    Tag::DefinitionListDefinition => {
                        structure.containers.push((ContainerKind::Definition, first, last));
                    }
                    Tag::HtmlBlock => structure.html_blocks.push((first, last)),
                    _ => {}
                }
            }
            Event::End(end) => {
                let inline = matches!(
                    end,
                    TagEnd::Emphasis
                        | TagEnd::Strong
                        | TagEnd::Strikethrough
                        | TagEnd::Superscript
                        | TagEnd::Subscript
                        | TagEnd::Link
                        | TagEnd::Image
                );
                if !inline {
                    depth = depth.saturating_sub(1);
                }
            }
            _ => {}
        }
    }
    structure
}

/// The containers holding `line`, outermost first, as (kind, start line).
fn chain(structure: &Structure, line: usize, map_start: impl Fn(usize) -> usize) -> Vec<(ContainerKind, usize)> {
    let mut chain: Vec<(ContainerKind, usize)> = structure
        .containers
        .iter()
        .filter(|&&(_, first, last)| first <= line && line <= last)
        .map(|&(kind, first, _)| (kind, map_start(first)))
        .collect();
    chain.sort_by_key(|&(_, start)| start);
    chain
}

/// Whether the slice, parsed as a document, has a table starting on line
/// `header`; the table's last line when it does. A header directly under
/// paragraph text may instead interrupt it.
fn confirm_slice(slice: &[String], header: usize) -> Option<usize> {
    let text = join(slice);
    let original = parse_structure(&text);
    if let Some(&(_, last)) = original.tables.iter().find(|&&(first, _)| first == header) {
        return Some(last);
    }
    interrupting_table(slice, header, &original)
}

fn join(lines: &[String]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// A table GFM lets interrupt a paragraph: with one blank line inserted before
/// the header the parser reads a table there, in the same containers.
fn interrupting_table(slice: &[String], header: usize, original: &Structure) -> Option<usize> {
    if header == 0
        || slice[header - 1].trim().is_empty()
        || original
            .html_blocks
            .iter()
            .any(|&(first, last)| first <= header && header <= last)
    {
        return None;
    }
    let prefix = quote_prefix(&slice[header]);
    let mut variant: Vec<String> = slice.to_vec();
    variant.insert(header, prefix);
    let parsed = parse_structure(&join(&variant));
    let &(_, last) = parsed.tables.iter().find(|&&(first, _)| first == header + 1)?;
    let unshift = |line: usize| if line > header { line - 1 } else { line };
    let before = chain(original, header, |start| start);
    let after = chain(&parsed, header + 1, unshift);
    (before == after).then(|| unshift(last))
}

/// The blockquote markers opening `line`, trimmed: the text of a blank line that
/// stays inside the same quote.
fn quote_prefix(line: &str) -> String {
    let content = strip_blockquote_prefix(line);
    line[..line.len() - content.len()].trim_end().to_string()
}

// ---- Slices of the document -------------------------------------------------------

/// Confirm in the top-level block holding the header, up to the block's last
/// scanned line. With `neutralize`, pipes the flavor masks are replaced so the
/// parser splits the header the way the flavor does.
fn confirm_in_enclosing_block(ev: &Evidence, block: &TableBlock, neutralize: bool, repair: bool) -> Option<usize> {
    // A block starts at its first character, past any indentation.
    let line = line_text(ev, block.header_line);
    let header_offset = ev.lines.get(block.header_line)?.byte_offset + (line.len() - line.trim_start().len());
    let index = ev
        .top_level_blocks
        .partition_point(|&(start, _)| start <= header_offset);
    let &(top_start, top_end) = ev.top_level_blocks[..index].last()?;
    if header_offset >= top_end {
        return None;
    }
    let first = line_of(ev, top_start);
    let (numbers, cut) = bounded_lines(ev, first, block)?;
    let mut slice: Vec<String> = numbers.iter().map(|&line| line_text(ev, line).to_string()).collect();
    let header = numbers.iter().position(|&line| line == block.header_line)?;
    if neutralize {
        slice[header] = neutralized(&slice[header], ev.flavor);
    }
    if repair {
        repair_delimiter(&mut slice, header_cells(ev, block), header);
    }
    let last = confirm_slice(&slice, header)?;
    // The parsed table reaching the cut means it runs on to the scanned end.
    Some(if cut && last + 1 >= slice.len() {
        block.end_line
    } else {
        numbers[last]
    })
}

/// How far above a header the reparse slice reaches for its paragraph.
const CONTEXT_LINES: usize = 64;

/// The lines to reparse for the table in `block`, as document line numbers, and
/// whether the slice stops short of the table's end. Whole when it fits; otherwise
/// the container opener at `first`, the header's own paragraph and the rows up to
/// just past the delimiter, so the container context is kept at any size.
fn bounded_lines(ev: &Evidence, first: usize, block: &TableBlock) -> Option<(Vec<usize>, bool)> {
    let fits = |numbers: &[usize]| {
        let bytes: usize = numbers.iter().map(|&line| ev.lines[line].byte_len + 1).sum();
        bytes <= MAX_SLICE_BYTES
    };
    let last = block.end_line.min(ev.lines.len().checked_sub(1)?);
    // Offsets give the span's size without visiting its lines.
    let span = ev.lines[last].byte_offset + ev.lines[last].byte_len - ev.lines.get(first)?.byte_offset;
    if span <= MAX_SLICE_BYTES {
        return Some(((first..=last).collect(), false));
    }
    let tail = last.min(block.delimiter_line + 2);
    let mut start = block.header_line;
    while start > first && block.header_line - start < CONTEXT_LINES && !line_text(ev, start - 1).trim().is_empty() {
        start -= 1;
    }
    let mut numbers = Vec::new();
    if start > first {
        // The opener of every list item holding the header keeps the nesting the
        // elided lines would otherwise carry.
        let header_line = line_text(ev, block.header_line);
        let header_offset =
            ev.lines[block.header_line].byte_offset + (header_line.len() - header_line.trim_start().len());
        let mut openers: Vec<usize> = items_containing(ev, header_offset)
            .map(|&(item_start, _, _)| line_of(ev, item_start))
            .filter(|&line| line > first && line < start)
            .collect();
        openers.push(first);
        if start - 1 > first {
            openers.push(start - 1);
        }
        openers.sort_unstable();
        openers.dedup();
        numbers.extend(openers);
    }
    numbers.extend(start..=tail);
    fits(&numbers).then_some((numbers, tail < last))
}

/// The header's cell count as the table rules count it, past any list and quote prefix.
fn header_cells(ev: &Evidence, block: &TableBlock) -> usize {
    let line = line_text(ev, block.header_line);
    let row = crate::utils::table_utils::TableUtils::extract_table_row_content(line, block, 0);
    crate::utils::table_utils::TableUtils::count_cells_with_flavor(row, ev.flavor).max(1)
}

/// Replace the delimiter row below the header at `header` in `slice` with one that
/// has the header's cell count, keeping its quote markers and indentation.
fn repair_delimiter(slice: &mut [String], cells: usize, header: usize) {
    let Some(delimiter) = slice.get(header + 1) else {
        return;
    };
    let kept = delimiter.len() - strip_blockquote_prefix(delimiter).len();
    let leading = &delimiter[..kept];
    // The header's own indentation inside the item, so the row stays in the same block.
    let inner = &strip_blockquote_prefix(delimiter);
    let indent = &inner[..inner.len() - inner.trim_start().len()];
    slice[header + 1] = format!("{leading}{indent}|{}", "-|".repeat(cells));
}

fn neutralized(line: &str, flavor: MarkdownFlavor) -> String {
    let row = strip_blockquote_prefix(line);
    let offset = line.len() - row.len();
    let masked = crate::utils::table_utils::TableUtils::rendering_masked_pipes(row, flavor);
    let mut bytes = line.as_bytes().to_vec();
    for position in masked {
        bytes[offset + position] = b'x';
    }
    String::from_utf8(bytes).unwrap_or_else(|_| line.to_string())
}

// ---- Flavor containers read as code or HTML by the parser -------------------------

/// A line the parser reads as code while the flavor reads it as Markdown.
fn is_carved(ev: &Evidence, line: usize) -> bool {
    let Some(info) = ev.lines.get(line) else {
        return false;
    };
    if info.in_mkdocs_html_markdown {
        return true;
    }
    // The parser's code range starts after the indentation.
    let text = info.content(ev.content);
    let first_text = info.byte_offset + (text.len() - text.trim_start().len());
    if text.trim().is_empty() || info.in_code_block {
        return false;
    }
    covering(ev.parser_code_blocks, first_text) || continues_opener_paragraph(ev, line)
}

/// A MkDocs container body that starts on the line after its opener, with no
/// blank line between. The parser reads the opener and the indented lines as one
/// paragraph, so none of them is code, yet the flavor reads the lines as the
/// container's Markdown. A body line is indented one level (four to seven columns)
/// past its opener; further in, the first body line is code and a later one is
/// paragraph text.
fn continues_opener_paragraph(ev: &Evidence, line: usize) -> bool {
    ev.flavor == MarkdownFlavor::MkDocs
        && ev
            .opener_body
            .get_or_init(|| opener_body(ev))
            .get(line)
            .copied()
            .unwrap_or(false)
}

fn opener_body(ev: &Evidence) -> Vec<bool> {
    let mut body = vec![false; ev.lines.len()];
    // The opener's indentation while the unbroken run under it continues, and
    // whether the next line is the first one under the opener.
    let mut owner: Option<(usize, bool)> = None;
    // The raw HTML block that is open; markers inside it are plain text.
    let mut raw_html: Option<HtmlEnd> = None;
    for (index, info) in ev.lines.iter().enumerate() {
        let text = line_text(ev, index);
        let indent = indent_columns(text);
        if text.trim().is_empty() {
            owner = None;
            if raw_html == Some(HtmlEnd::Blank) {
                raw_html = None;
            }
            continue;
        }
        // The container body level this line sits at, relative to its owner.
        let body_level = owner.filter(|&(opener, _)| indent >= opener + 4);
        let on_body_level = body_level.is_some_and(|(opener, _)| indent < opener + 8);
        match raw_html {
            Some(HtmlEnd::Marker(end)) => {
                if text.to_ascii_lowercase().contains(end) {
                    raw_html = None;
                }
                // Lines of raw HTML in a container body stay marked, so the HTML
                // scan keeps its state across them.
                body[index] = body_level.is_some();
                continue;
            }
            Some(HtmlEnd::Blank) => {
                body[index] = body_level.is_some();
                continue;
            }
            None => {}
        }
        if info.in_code_block {
            owner = None;
            continue;
        }
        let relative = indent - body_level.map_or(0, |(opener, _)| opener + 4);
        if relative < 4 && !info.in_mkdocs_html_markdown {
            raw_html = html_block_open(text);
            if raw_html.is_some() {
                body[index] = on_body_level;
                if body_level.is_none() {
                    owner = None;
                } else if let Some((opener, _)) = owner {
                    // The HTML takes the first-body position; what follows it continues that block.
                    owner = Some((opener, false));
                }
                continue;
            }
        }
        // Renderers recognize an opener at the margin or exactly one level inside its parent.
        let nested = owner.is_some_and(|(opener, _)| indent == opener + 4);
        if is_valid_opener(text) && (indent == 0 || nested) {
            owner = Some((indent, true));
        } else if let Some((opener, first)) = owner {
            if indent < opener + 4 || (first && indent >= opener + 8) {
                owner = None;
            } else {
                // Only the first line under the opener starts a block of its own;
                // later lines of the unbroken run continue the opener's paragraph.
                body[index] = first && indent < opener + 8;
                owner = Some((opener, false));
            }
        }
    }
    body
}

/// An admonition or content tab marker as the renderers accept it: type words
/// with an optional quoted title, or a quoted tab label. The scanner's own
/// detectors are lenient about trailing text, which would let a plain paragraph
/// such as `!!! note garbage!` own the lines under it.
fn is_valid_opener(line: &str) -> bool {
    static OPENER: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r#"^[ \t]*(?:(?:!!!|\?\?\?\+?) ?[A-Za-z0-9_-]+(?: +[A-Za-z0-9_-]+)*(?: +"[^"]*")?|=== +"[^"]*") *$"#,
        )
        .unwrap()
    });
    OPENER.is_match(line)
}

fn is_opener(line: &str) -> bool {
    let text = line.trim_start();
    text.starts_with("!!!")
        || text.starts_with("???")
        || text.starts_with("===")
        || (text.starts_with('<') && crate::utils::mkdocs_html_markdown::mentions_markdown(text))
}

/// How a raw HTML block that is still open ends.
#[derive(Clone, PartialEq)]
enum HtmlEnd {
    /// At the next blank line (CommonMark types 6 and 7).
    Blank,
    /// At the first line holding this lowercase text (types 1 to 5).
    Marker(&'static str),
}

/// Block-level tag names that open a type 6 HTML block.
const HTML_BLOCK_TAGS: &[&str] = &[
    "address",
    "article",
    "aside",
    "base",
    "basefont",
    "blockquote",
    "body",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "nav",
    "noframes",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
    "search",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
];

/// The raw HTML block a line opens (after any list markers) that is still open at
/// the end of that line. A tag, closing tag, comment, declaration or processing
/// instruction that is not an autolink opens one; a block that closes on its own
/// line, and inline HTML such as `<em>Intro</em>`, leave nothing open.
fn html_block_open(text: &str) -> Option<HtmlEnd> {
    use crate::utils::table_utils::TableUtils;
    let mut text = text.trim_start();
    loop {
        let (prefix, rest, _) = TableUtils::extract_list_prefix(text);
        if prefix.is_empty() {
            break;
        }
        text = rest.trim_start();
    }
    let bytes = text.as_bytes();
    if bytes.len() < 2
        || bytes[0] != b'<'
        || !(bytes[1].is_ascii_alphabetic() || matches!(bytes[1], b'/' | b'!' | b'?'))
        || autolink_end(text).is_some()
    {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    let until =
        |marker: &'static str, skip: usize| (!lower[skip..].contains(marker)).then_some(HtmlEnd::Marker(marker));
    if lower.starts_with("<!--") {
        return until("-->", 4);
    }
    if lower.starts_with("<?") {
        return until("?>", 2);
    }
    if lower.starts_with("<![cdata[") {
        return until("]]>", 9);
    }
    if lower.starts_with("<!") {
        return until(">", 2);
    }
    let closing = lower.starts_with("</");
    let name_start = if closing { 2 } else { 1 };
    let name: String = lower[name_start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if !closing {
        let end = match name.as_str() {
            "pre" => Some("</pre"),
            "script" => Some("</script"),
            "style" => Some("</style"),
            "textarea" => Some("</textarea"),
            _ => None,
        };
        if let Some(end) = end {
            return until(end, 1 + name.len());
        }
    }
    if HTML_BLOCK_TAGS.contains(&name.as_str()) {
        return Some(HtmlEnd::Blank);
    }
    // Type 7: one complete tag alone on its line.
    let mut quote = None;
    for (at, c) in lower.char_indices().skip(name_start + name.len()) {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return lower[at + 1..].trim().is_empty().then_some(HtmlEnd::Blank),
            _ => {}
        }
    }
    None
}

/// Length of the autolink (`<scheme:...>` or `<user@host>`) that `text` starts
/// with, if it starts with one.
fn autolink_end(text: &str) -> Option<usize> {
    let inner = &text[1..];
    let close = inner.find('>')?;
    let body = &inner[..close];
    if body.is_empty() || body.contains(|c: char| c.is_whitespace() || c == '<') {
        return None;
    }
    let scheme_len = body.find(':');
    let is_uri = scheme_len.is_some_and(|len| {
        (2..=32).contains(&len)
            && body.as_bytes()[0].is_ascii_alphabetic()
            && body[..len]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'))
    });
    let is_email = scheme_len.is_none() && body.contains('@');
    (is_uri || is_email).then_some(close + 2)
}

/// For every line, whether the carved container above it holds something a parser
/// would open raw HTML with, looking back to the line that opens the container or
/// to the nearest line outside it. Only the line that opens a run owns it; an
/// opener-like line further in is body text, which raw HTML above it can swallow.
fn html_above(ev: &Evidence) -> Vec<bool> {
    let mut above = Vec::with_capacity(ev.lines.len());
    let mut state: Option<HtmlEnd> = None;
    for index in 0..ev.lines.len() {
        above.push(state.is_some());
        let text = line_text(ev, index);
        if text.trim().is_empty() {
            // A blank line ends a block of type 6 or 7 and nothing else.
            if state == Some(HtmlEnd::Blank) {
                state = None;
            }
            continue;
        }
        let carved = is_carved(ev, index);
        let owns_run = is_opener(text) && (index == 0 || !is_carved(ev, index - 1));
        state = if owns_run || !carved {
            if is_opener(text) {
                let trimmed = text.trim_start();
                let rest = if trimmed.starts_with('<') {
                    trimmed.split_once('>').map_or("", |(_, rest)| rest)
                } else {
                    ""
                };
                html_block_open(rest)
            } else {
                None
            }
        } else {
            match state {
                Some(HtmlEnd::Marker(end)) => {
                    (!text.to_ascii_lowercase().contains(end)).then_some(HtmlEnd::Marker(end))
                }
                Some(HtmlEnd::Blank) => Some(HtmlEnd::Blank),
                None => html_block_open(text),
            }
        };
    }
    above
}

fn confirm_carved(ev: &Evidence, block: &TableBlock, neutralize: bool, repair: bool) -> Option<usize> {
    let header = block.header_line;
    let header_line = line_text(ev, header);
    // Directive content is not indented, whereas admonition and `<div markdown>`
    // bodies are indented one level per nesting.
    let mut dedent = if ev.flavor == MarkdownFlavor::MyST {
        0
    } else {
        indent_columns(header_line) / 4 * 4
    };
    // A `<div markdown>` body is one level deep and nests nothing by itself, so
    // further indentation there is code.
    if let Some(info) = ev.lines.get(header)
        && info.in_mkdocs_html_markdown
        && !info.in_admonition
        && !info.in_content_tab
    {
        dedent = dedent.min(4);
    }

    // Raw HTML state does not survive a cut in the slice, so any HTML opener
    // earlier in the container leaves the block unconfirmed.
    if ev.html_above.get_or_init(|| html_above(ev))[header] {
        return None;
    }

    // The header's own block: back to the blank line (or opener) above it.
    let mut first = header;
    while first > 0 && header - first < CONTEXT_LINES {
        let text = line_text(ev, first - 1);
        if text.trim().is_empty() || is_opener(text) || indent_columns(text) < dedent || !is_carved(ev, first - 1) {
            break;
        }
        first -= 1;
    }

    // A table too long to reparse whole is judged by its header and first rows.
    let mut end = block.end_line;
    let bytes = |last: usize| {
        (first..=last.min(ev.lines.len() - 1))
            .map(|i| ev.lines[i].byte_len + 1)
            .sum::<usize>()
    };
    let mut cut = false;
    if bytes(end) > MAX_SLICE_BYTES {
        end = end.min(block.delimiter_line + 2);
        cut = end < block.end_line;
        if bytes(end) > MAX_SLICE_BYTES {
            return None;
        }
    }
    let mut slice = Vec::new();
    for index in first..=end {
        let text = line_text(ev, index);
        match dedented(text, dedent) {
            Some(line) => slice.push(line),
            None if index <= header => return None,
            None => {
                cut = false;
                break;
            }
        }
    }
    if neutralize {
        slice[header - first] = neutralized(&slice[header - first], ev.flavor);
    }
    if repair {
        repair_delimiter(&mut slice, header_cells(ev, block), header - first);
    }
    let last = confirm_slice(&slice, header - first)?;
    Some(if cut && last + 1 >= slice.len() {
        block.end_line
    } else {
        first + last
    })
}

/// `line` with `columns` columns of leading whitespace removed; `None` when it
/// has less than that or a tab straddles the cut.
fn dedented(line: &str, columns: usize) -> Option<String> {
    if columns == 0 {
        return Some(line.to_string());
    }
    if line.trim().is_empty() {
        return Some(String::new());
    }
    let mut width = 0;
    for (at, byte) in line.bytes().enumerate() {
        match byte {
            b' ' => width += 1,
            b'\t' => width = (width / 4 + 1) * 4,
            _ => return None,
        }
        if width == columns {
            return Some(line[at + 1..].to_string());
        }
        if width > columns {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_cells_reads_any_backslash_before_a_pipe_as_an_escape() {
        for backslashes in 1..=4 {
            let row = format!("| a{}|b | c |", "\\".repeat(backslashes));
            assert_eq!(split_cells(&row, MarkdownFlavor::Standard).len(), 2, "{row}");
        }
    }

    #[test]
    fn split_cells_pairs_backslashes_for_mkdocs() {
        for backslashes in 1..=4 {
            let row = format!("| a{}|b | c |", "\\".repeat(backslashes));
            let expected = if backslashes % 2 == 0 { 3 } else { 2 };
            assert_eq!(split_cells(&row, MarkdownFlavor::MkDocs).len(), expected, "{row}");
        }
    }
}
