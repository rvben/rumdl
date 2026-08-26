use crate::config::MarkdownFlavor;
use crate::utils::regex_cache::{ORDERED_LIST_MARKER_REGEX, UNORDERED_LIST_MARKER_REGEX};
use crate::utils::table_utils::TableUtils;
use std::sync::LazyLock;

use super::types::*;

static ATX_HEADING_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^(\s*)(#{1,6})(\s*)(.*)$").unwrap());

/// CommonMark 5.2: an ordered list marker is "one to nine digits". A longer run
/// is ordinary paragraph text, and the shared marker regex does not say so.
const MAX_ORDERED_MARKER_DIGITS: usize = 9;

/// The column a list item's content starts at. A continuation line that does not
/// reach it is lazy.
///
/// `interrupting` says a paragraph flows into this line from the one above.
/// CommonMark 5.2 lets an ordered list interrupt a paragraph only when it starts
/// at 1, so `2.` written under prose is part of the sentence rather than a marker
/// and the line opens no item at all.
fn list_item_content_column(line: &str, interrupting: bool) -> Option<usize> {
    if let Some(marker) = UNORDERED_LIST_MARKER_REGEX.find(line) {
        return Some(marker.end());
    }
    let marker = ORDERED_LIST_MARKER_REGEX.captures(line)?;
    let number = marker.get(2)?.as_str();
    if number.len() > MAX_ORDERED_MARKER_DIGITS {
        return None;
    }
    if interrupting && number.trim_start_matches('0') != "1" {
        return None;
    }
    Some(marker.get(0)?.end())
}

/// One container of the stack a line has to re-enter to be written inside it.
///
/// The two kinds are re-entered differently - a blockquote wants its `>`
/// repeated, a list item wants the line indented to the column its content
/// starts at - so which order they nest in decides what a line carrying only one
/// of the two re-enters. `- > quote` and `> - item` are both two deep, and a
/// following `> more` re-enters the second while closing the first. The stack
/// therefore keeps the order rather than counting each kind.
#[derive(Clone, Copy, PartialEq)]
enum Marker {
    Quote,
    Item(usize),
}

/// The column a line carrying no container marker of its own has to reach to be
/// written inside `open`, when such a line can be inside it at all.
///
/// A setext underline is that line: the pattern is a run of one character, so it
/// repeats no `>` and opens no item, and a blockquote anywhere in the stack puts
/// the whole container out of its reach.
fn bare_content_column(open: &[Marker]) -> Option<usize> {
    let mut column = 0;
    for marker in open {
        match marker {
            Marker::Quote => return None,
            Marker::Item(content_column) => column = *content_column,
        }
    }
    Some(column)
}

/// What a line did with the container open above it.
struct Entered<'a> {
    /// How many of the open markers the line re-entered, outermost first. Fewer
    /// than the stack holds means the line closed the rest.
    matched: usize,
    /// The text the line holds, with every marker stripped away.
    content: &'a str,
}

/// Read a line against the container open above it, recording in `opened` the
/// containers it enters past the ones it re-entered.
///
/// `paragraph` says a paragraph runs down into this line. A list marker faces
/// CommonMark 5.2's "must start with 1" only where that paragraph's own text
/// would be written, which is once the whole open container has been re-entered
/// and before this line has opened one of its own. So `2.` written under a
/// paragraph is part of the sentence, while the same line written under a list
/// item it does not indent into opens a list of its own.
fn enter<'a>(line: &'a str, open: &[Marker], paragraph: bool, opened: &mut Vec<Marker>) -> Entered<'a> {
    opened.clear();
    let mut rest = line;
    let mut matched = 0;
    while matched < open.len() {
        match open[matched] {
            Marker::Quote => match crate::utils::blockquote::parse_blockquote_prefix(rest) {
                Some(quote) => rest = quote.content,
                None => break,
            },
            // Nothing is consumed here: the item is re-entered by indentation,
            // which is where the next marker or the text itself begins.
            Marker::Item(content_column) if line.len() - rest.trim_start().len() >= content_column => {}
            Marker::Item(_) => break,
        }
        matched += 1;
    }
    loop {
        if let Some(quote) = crate::utils::blockquote::parse_blockquote_prefix(rest) {
            opened.push(Marker::Quote);
            rest = quote.content;
            continue;
        }
        // CommonMark 5.2: "When both a thematic break and a list item are
        // possible interpretations of a line, the thematic break takes
        // precedence", so `* * *` is a break rather than an item holding `* *`.
        if is_horizontal_rule_content(rest.trim()) {
            break;
        }
        let interrupting = paragraph && matched == open.len() && opened.is_empty();
        match list_item_content_column(rest, interrupting) {
            Some(end) => {
                rest = &rest[end..];
                // Where the marker left off, as a column of the whole line: what
                // a continuation of this item's content has to reach.
                opened.push(Marker::Item(line.len() - rest.len()));
            }
            None => break,
        }
    }
    Entered {
        matched,
        content: rest.trim(),
    }
}

/// Whether the text a line holds is paragraph text, so that a paragraph running
/// into the line runs on out of it.
///
/// Blank lines and code fences end a paragraph too; the pass settles those from
/// the line flags, which already speak for every line this predicate sees. The
/// ATX test is the same regex the heading detection below runs, so the two agree
/// on `#hashtag` and other shapes CommonMark would call paragraph text.
///
/// A `=` run is paragraph text here. It is a setext underline only under a
/// paragraph it is written inside, and the pass answers that from the lines it
/// has already read rather than from the run itself; a `-` run long enough to be
/// a thematic break ends the paragraph under either reading and is rejected.
fn may_hold_open_paragraph(content: &str) -> bool {
    // An empty container holds no paragraph. A line that is nothing but its
    // markers (`* `, `1. `, `> - `) opens an empty list item, and a blank line
    // never reaches here: the pass settles those from the line flags, which read
    // a `>` holding nothing as blank.
    if content.is_empty() {
        return false;
    }
    !(is_horizontal_rule_content(content)
        || ATX_HEADING_REGEX.is_match(content)
        || crate::utils::html_block::parse_html_block_start(content).is_some()
        || crate::utils::html_block::opens_untagged_html_block(content))
}

/// Whether a line is paragraph text: the shape a setext underline needs above
/// it, and the shape that keeps a container's paragraph open below it.
pub(crate) fn is_paragraph_text_line(line: &str) -> bool {
    may_hold_open_paragraph(enter(line, &[], false, &mut Vec::new()).content)
}

/// The structural blocks a line sits inside.
///
/// Nothing spans a boundary between two of these, so the pass starts over where
/// a line's blocks differ from the line above it: whatever held a paragraph or a
/// container open lies on the far side of that boundary and is closed by the time
/// the underline is read. Comparing the two lines rather than testing one keeps a
/// paragraph written INSIDE such a block reading normally, which is what lets
/// the markdown-bodied containers (Pandoc divs, admonitions, tabs, PyMdown
/// blocks, MyST directives) sit in the same list as the opaque ones: it is their
/// markers that close an outer paragraph, not their contents.
///
/// Membership is not the test, and `LineInfo::is_paragraph_context` is not the
/// list. That predicate answers "can this line be part of a paragraph?", which a
/// construct can fail while still sitting inside one: kramdown renders
/// `para\n{::comment}\nx\n{:/comment}\nSetup\n=====` as a single paragraph
/// holding every line, so an extension block interrupts nothing and belongs
/// nowhere near this list. Its block IAL is the same story from the other side:
/// `> quote\n{:.cls}\nSetup\n=====` closes the blockquote, yet kramdown still
/// renders `Setup\n=====` as a paragraph. Only a construct that ends the
/// paragraph running into it belongs here, and only on the evidence of the
/// parser that defines it.
///
/// Footnote definitions, definition lists and tables are deliberately absent for
/// the same reason. None of them can interrupt a paragraph, so a line that looks
/// like one under an open paragraph is ordinary lazy continuation text; the pass
/// settles a table that really did open from the delimiter row below.
fn structural_blocks(line: &LineInfo) -> [bool; 17] {
    [
        line.in_code_block,
        line.in_front_matter,
        line.in_html_block,
        line.in_html_comment,
        line.in_math_block,
        line.in_mdx_comment,
        line.in_obsidian_comment,
        line.in_mkdocstrings,
        line.in_esm_block,
        line.in_jsx_block,
        line.in_pandoc_div,
        line.is_div_marker,
        line.in_admonition,
        line.in_content_tab,
        line.in_pymdown_block,
        line.in_myst_directive,
        // myst-parser ends the quoted paragraph on a `%` comment, so
        // `> quote\n% c\nSetup\n=====` really is a heading in MyST.
        line.is_myst_comment,
    ]
}

/// What a line leaves behind for the line below it.
#[derive(Clone, Copy)]
struct Trailing {
    /// Whether a paragraph runs on out of the line, so that a line below can
    /// continue it and a `=`/`-` run below can underline it.
    open: bool,
    /// Whether the line is a row of a table. No row of one is paragraph text.
    in_table: bool,
    /// What a bare `=`/`-` run below has to reach to be written inside the
    /// container that paragraph hangs off: see `bare_content_column`.
    bare_column: Option<usize>,
}

/// Read the document once, recording what each line leaves open below it.
///
/// Downwards is what settles the `=`/`-` runs met on the way, and every line is
/// one: a run underlines the paragraph above it when there is one it is written
/// inside, and is a paragraph line of its own otherwise, so the reading that
/// arrives from above is the reading that decides it. The same carried paragraph
/// answers whether a marker may interrupt, and whether a table opened.
///
/// One pass rather than a walk up from each `=`/`-` run: the state a run needs
/// is the state every run needs, and a document is a list of lines either way.
fn trailing_state(content_lines: &[&str], lines: &[LineInfo], flavor: MarkdownFlavor) -> Vec<Trailing> {
    let mut states = Vec::with_capacity(lines.len());
    // The container the lines read so far left open, outermost first, and a
    // scratch buffer for the containers each line opens of its own.
    let mut open: Vec<Marker> = Vec::new();
    let mut opened: Vec<Marker> = Vec::new();
    let mut paragraph = false;
    let mut in_table = false;
    let mut header_cells = None;

    for index in 0..lines.len() {
        // Nothing crosses a boundary between structural blocks: a paragraph, a
        // table and a container all end where the block holding them does.
        if index > 0 && structural_blocks(&lines[index]) != structural_blocks(&lines[index - 1]) {
            open.clear();
            paragraph = false;
            in_table = false;
            header_cells = None;
        }

        if lines[index].is_blank {
            // A blank line ends a paragraph and a table. A blockquote is entered
            // by repeating its `>`, which a blank line does not, so the blank
            // closes every blockquote and everything written inside one;
            // CommonMark 5.2 lets a list item hold several blocks, so the items
            // outside them go on holding their content.
            if let Some(quote) = open.iter().position(|marker| *marker == Marker::Quote) {
                open.truncate(quote);
            }
            paragraph = false;
            in_table = false;
            header_cells = None;
            states.push(Trailing {
                open: false,
                in_table: false,
                bare_column: bare_content_column(&open),
            });
            continue;
        }

        let entered = enter(content_lines[index], &open, paragraph, &mut opened);
        let holds_paragraph = may_hold_open_paragraph(entered.content);
        // Whether the line's text is written inside the open container: it
        // re-entered the whole of it and opened none of its own.
        let inside = entered.matched == open.len() && opened.is_empty();

        // GFM matches a table's two opening rows cell for cell, so a delimiter
        // row opens a table only under a header row holding as many cells, and
        // only where that row started its own paragraph: a paragraph already
        // running into it holds it as text.
        in_table |= header_cells == Some(TableUtils::count_cells_with_flavor(entered.content, flavor))
            && TableUtils::is_delimiter_row(entered.content);
        // GFM breaks a table at the first line that starts another block-level
        // structure: a container of its own, or any shape paragraph text is not.
        in_table &= inside && holds_paragraph;

        // CommonMark 4.3 forbids a lazy underline, so a `=`/`-` run underlines
        // the paragraph running into it only when it is written inside the same
        // container, and is a paragraph line of its own otherwise.
        let underlines = paragraph && inside && is_setext_underline_content(entered.content);
        if in_table || underlines {
            // No row of a table is paragraph text, and a run that underlines the
            // paragraph above it ends that paragraph: either way this line
            // leaves nothing open for the lines below to continue.
            paragraph = false;
            header_cells = None;
            states.push(Trailing {
                open: false,
                in_table,
                bare_column: bare_content_column(&open),
            });
            continue;
        }

        // Whether the paragraph running into this line runs on out of it, as its
        // own text or as a lazy continuation. A line that enters a container of
        // its own starts a paragraph there instead.
        let continues = paragraph && holds_paragraph && opened.is_empty();
        // Where the paragraph leaving this line hangs off. Only a line continuing
        // the paragraph running into it leaves the container where it was, which
        // is what makes its own reading the lazy one; every other line is written
        // where it re-entered, so the containers it did not re-enter close and
        // the ones it opened take their place. A line already inside the whole of
        // the open container re-seats it onto itself, so it needs no test here.
        if !continues {
            open.truncate(entered.matched);
            open.extend_from_slice(&opened);
        }
        paragraph = holds_paragraph;
        // The header row of a table: a line that starts its own paragraph and
        // has cells for a delimiter row below to match.
        header_cells =
            (!continues && holds_paragraph && TableUtils::is_potential_table_row_with_flavor(entered.content, flavor))
                .then(|| TableUtils::count_cells_with_flavor(entered.content, flavor));
        states.push(Trailing {
            open: paragraph,
            in_table: false,
            bare_column: bare_content_column(&open),
        });
    }
    states
}

/// CommonMark 4.3: "The setext heading underline cannot be a lazy continuation
/// line." Where an open paragraph hangs off a blockquote or a list item, a
/// `=`/`-` run written outside that container is ordinary paragraph text and the
/// whole construct stays one paragraph.
fn setext_underline_is_lazy(text_line: Trailing, underline_indent: usize) -> bool {
    // The underline carries no container marker of its own: the pattern is a run
    // of one character, so it repeats no `>` and opens no item. It is written
    // inside the paragraph's container only when a line like it can be, and its
    // own indent reaches the column such a line has to reach.
    let underline_is_inside = text_line.bare_column.is_some_and(|column| underline_indent >= column);
    // A table row is not paragraph text, so an underline written as one has
    // nothing above it to underline either way.
    text_line.in_table || (text_line.open && !underline_is_inside)
}

/// Detect headings and blockquotes (called after HTML block detection)
pub(super) fn detect_headings_and_blockquotes(
    content_lines: &[&str],
    lines: &mut [LineInfo],
    flavor: MarkdownFlavor,
    html_comment_ranges: &[crate::utils::skip_context::ByteRange],
    link_byte_ranges: &[(usize, usize)],
    front_matter_end: usize,
) -> Vec<Option<Box<HeadingInfo>>> {
    // Only a `=`/`-` run under a line of text asks what paragraph is open, and
    // most documents hold none, so the pass runs on the first one that does.
    let mut trailing: Option<Vec<Trailing>> = None;

    // Detect headings (including Setext which needs look-ahead) and blockquotes
    for i in 0..lines.len() {
        let line = content_lines[i];

        // Detect blockquotes FIRST, before any skip conditions.
        if !(front_matter_end > 0 && i < front_matter_end)
            && let Some(bq) = crate::utils::blockquote::parse_blockquote_prefix(line)
        {
            let nesting_level = bq.nesting_level;
            let marker_column = bq.indent.len();
            let content_leading_ws_len = bq.content.len() - bq.content.trim_start_matches([' ', '\t']).len();
            let full_prefix = format!("{}{}", bq.prefix, &bq.content[..content_leading_ws_len]);
            let normalized_content = &bq.content[content_leading_ws_len..];

            let has_multiple_spaces = bq.spaces_after_marker.chars().filter(|&c| c == ' ').count() > 1;

            lines[i].blockquote = Some(Box::new(BlockquoteInfo {
                nesting_level,
                marker_column,
                prefix: full_prefix,
                content: normalized_content.to_string(),
                has_multiple_spaces_after_marker: has_multiple_spaces,
            }));

            // Update is_horizontal_rule for blockquote content
            if !lines[i].in_code_block && is_horizontal_rule_content(normalized_content.trim()) {
                lines[i].is_horizontal_rule = true;
            }
        }

        // Now apply skip conditions for heading detection
        if lines[i].in_code_block {
            continue;
        }

        if front_matter_end > 0 && i < front_matter_end {
            continue;
        }

        if lines[i].in_html_block {
            continue;
        }

        if lines[i].is_blank {
            continue;
        }

        // Check for ATX headings (but skip MkDocs snippet lines)
        let is_snippet_line = if flavor == MarkdownFlavor::MkDocs {
            crate::utils::mkdocs_snippets::is_snippet_section_start(line)
                || crate::utils::mkdocs_snippets::is_snippet_section_end(line)
        } else {
            false
        };

        if !is_snippet_line && let Some(caps) = ATX_HEADING_REGEX.captures(line) {
            if crate::utils::skip_context::is_in_html_comment_ranges(html_comment_ranges, lines[i].byte_offset) {
                continue;
            }
            let line_offset = lines[i].byte_offset;
            if link_byte_ranges
                .iter()
                .any(|&(start, end)| line_offset > start && line_offset < end)
            {
                continue;
            }
            let leading_spaces = caps.get(1).map_or("", |m| m.as_str());
            let hashes = caps.get(2).map_or("", |m| m.as_str());
            let spaces_after = caps.get(3).map_or("", |m| m.as_str());
            let rest = caps.get(4).map_or("", |m| m.as_str());

            let level = hashes.len() as u8;
            let marker_column = leading_spaces.len();

            // Check for closing sequence, but handle custom IDs that might come after
            let (text, has_closing, closing_seq) = parse_atx_remainder(rest);

            let content_column = marker_column + hashes.len() + spaces_after.len();

            let raw_text = text.trim().to_string();
            let (clean_text, mut custom_id) = crate::utils::header_id_utils::extract_header_id(&raw_text);

            if custom_id.is_none() && i + 1 < content_lines.len() && i + 1 < lines.len() {
                let next_line = content_lines[i + 1];
                if !lines[i + 1].in_code_block
                    && crate::utils::header_id_utils::is_standalone_attr_list(next_line)
                    && let Some(next_line_id) =
                        crate::utils::header_id_utils::extract_standalone_attr_list_id(next_line)
                {
                    custom_id = Some(next_line_id);
                }
            }

            let is_valid = !spaces_after.is_empty()
                || rest.is_empty()
                || level > 1
                || rest.trim().chars().next().is_some_and(char::is_uppercase);

            lines[i].heading = Some(Box::new(HeadingInfo {
                level,
                style: HeadingStyle::ATX,
                marker: hashes.to_string(),
                marker_column,
                content_column,
                text: clean_text,
                custom_id,
                raw_text,
                has_closing_sequence: has_closing,
                closing_sequence: closing_seq,
                is_valid,
            }));
        }
        // Check for Setext headings (need to look at next line)
        else if i + 1 < content_lines.len() && i + 1 < lines.len() {
            let next_line = content_lines[i + 1];
            if !lines[i + 1].in_code_block && is_setext_underline_content(next_line) {
                if front_matter_end > 0 && i < front_matter_end {
                    continue;
                }

                if crate::utils::skip_context::is_in_html_comment_ranges(html_comment_ranges, lines[i].byte_offset) {
                    continue;
                }

                let content_line = line.trim();

                if content_line.starts_with('-') || content_line.starts_with('*') || content_line.starts_with('+') {
                    continue;
                }

                if content_line.starts_with('_') {
                    let non_ws: String = content_line.chars().filter(|c| !c.is_whitespace()).collect();
                    if non_ws.len() >= 3 && non_ws.chars().all(|c| c == '_') {
                        continue;
                    }
                }

                if let Some(first_char) = content_line.chars().next()
                    && first_char.is_ascii_digit()
                {
                    let num_end = content_line.chars().take_while(char::is_ascii_digit).count();
                    if num_end < content_line.len() {
                        let next = content_line.chars().nth(num_end);
                        if next == Some('.') || next == Some(')') {
                            continue;
                        }
                    }
                }

                if ATX_HEADING_REGEX.is_match(line) {
                    continue;
                }

                if content_line.starts_with('>') {
                    continue;
                }

                let trimmed_start = line.trim_start();
                if trimmed_start.len() >= 3 {
                    let first_three: String = trimmed_start.chars().take(3).collect();
                    if first_three == "```" || first_three == "~~~" {
                        continue;
                    }
                }

                if content_line.starts_with('<') {
                    continue;
                }

                // Skip GFM table rows: a line that is part of a table cannot be
                // a Setext heading paragraph. A line is part of a table if:
                // - It starts with | and has a delimiter row above (body row), OR
                // - It IS a delimiter row with a pipe-containing header above (delimiter row)
                if content_line.starts_with('|') {
                    let mut is_in_table = false;

                    // Check if this line itself is a delimiter row with a header above
                    if TableUtils::is_delimiter_row(content_line)
                        && i > 0
                        && content_lines[i - 1].trim().contains('|')
                        && !lines[i - 1].in_code_block
                    {
                        is_in_table = true;
                    }

                    // Check if there's a delimiter row above (making this a body row)
                    if !is_in_table {
                        for j in (0..i).rev() {
                            let prev = content_lines[j].trim();
                            if prev.is_empty() || lines[j].in_code_block || lines[j].in_html_block {
                                break;
                            }
                            if TableUtils::is_delimiter_row(prev) {
                                is_in_table = true;
                                break;
                            }
                            if !prev.contains('|') {
                                break;
                            }
                        }
                    }

                    if is_in_table {
                        continue;
                    }
                }

                let underline_indent = next_line.len() - next_line.trim_start().len();
                let text_line = trailing.get_or_insert_with(|| trailing_state(content_lines, lines, flavor))[i];
                if setext_underline_is_lazy(text_line, underline_indent) {
                    continue;
                }

                let underline = next_line.trim();

                let level = if underline.starts_with('=') { 1 } else { 2 };
                let style = if level == 1 {
                    HeadingStyle::Setext1
                } else {
                    HeadingStyle::Setext2
                };

                let raw_text = line.trim().to_string();
                let (clean_text, mut custom_id) = crate::utils::header_id_utils::extract_header_id(&raw_text);

                if custom_id.is_none() && i + 2 < content_lines.len() && i + 2 < lines.len() {
                    let attr_line = content_lines[i + 2];
                    if !lines[i + 2].in_code_block
                        && crate::utils::header_id_utils::is_standalone_attr_list(attr_line)
                        && let Some(attr_line_id) =
                            crate::utils::header_id_utils::extract_standalone_attr_list_id(attr_line)
                    {
                        custom_id = Some(attr_line_id);
                    }
                }

                lines[i].heading = Some(Box::new(HeadingInfo {
                    level,
                    style,
                    marker: underline.to_string(),
                    marker_column: next_line.len() - next_line.trim_start().len(),
                    content_column: lines[i].indent,
                    text: clean_text,
                    custom_id,
                    raw_text,
                    has_closing_sequence: false,
                    closing_sequence: String::new(),
                    is_valid: true,
                }));
            }
        }
    }

    lines
        .iter()
        .enumerate()
        .map(|(line_index, line)| detect_blockquote_atx_heading(line_index, line, flavor, front_matter_end))
        .collect()
}

/// Parse the source after an ATX marker, preserving a trailing custom ID while
/// removing an optional CommonMark closing hash sequence.
fn parse_atx_remainder(rest: &str) -> (String, bool, String) {
    let (rest_without_id, custom_id_part) = if let Some(id_start) = rest.rfind(" {#") {
        if rest[id_start..].trim_end().ends_with('}') {
            (&rest[..id_start], &rest[id_start..])
        } else {
            (rest, "")
        }
    } else {
        (rest, "")
    };

    let trimmed_rest = rest_without_id.trim_end();
    let Some(last_hash_byte_pos) = trimmed_rest.rfind('#') else {
        return (rest.to_string(), false, String::new());
    };
    let char_positions: Vec<(usize, char)> = trimmed_rest.char_indices().collect();
    let Some(mut char_idx) = char_positions
        .iter()
        .position(|(byte_pos, _)| *byte_pos == last_hash_byte_pos)
    else {
        return (rest.to_string(), false, String::new());
    };
    while char_idx > 0 && char_positions[char_idx - 1].1 == '#' {
        char_idx -= 1;
    }
    let start_of_hashes = char_positions[char_idx].0;
    let potential_closing = &trimmed_rest[start_of_hashes..];
    let is_closing = potential_closing.chars().all(|c| c == '#')
        && (char_idx == 0 || char_positions[char_idx - 1].1.is_whitespace());
    if !is_closing {
        return (rest.to_string(), false, String::new());
    }

    let text = if custom_id_part.is_empty() {
        trimmed_rest[..start_of_hashes].trim_end().to_string()
    } else {
        format!("{}{}", trimmed_rest[..start_of_hashes].trim_end(), custom_id_part)
    };
    (text, true, potential_closing.to_string())
}

fn detect_blockquote_atx_heading(
    line_index: usize,
    line: &LineInfo,
    flavor: MarkdownFlavor,
    front_matter_end: usize,
) -> Option<Box<HeadingInfo>> {
    if line.in_code_block
        || (line.in_html_block && !line.in_mkdocs_html_markdown)
        || line.in_kramdown_extension_block
        || (front_matter_end > 0 && line_index < front_matter_end)
    {
        return None;
    }
    let blockquote = line.blockquote.as_ref()?;
    let content = blockquote.content.as_str();
    if flavor == MarkdownFlavor::MkDocs
        && (crate::utils::mkdocs_snippets::is_snippet_section_start(content)
            || crate::utils::mkdocs_snippets::is_snippet_section_end(content))
    {
        return None;
    }

    let marker_len = content.bytes().take_while(|&byte| byte == b'#').count();
    if !(1..=6).contains(&marker_len) {
        return None;
    }
    let after_marker = &content[marker_len..];
    let spaces_len = after_marker.bytes().take_while(u8::is_ascii_whitespace).count();
    if spaces_len == 0 {
        return None;
    }

    let rest = &after_marker[spaces_len..];
    let (text, has_closing_sequence, closing_sequence) = parse_atx_remainder(rest);
    let raw_text = text.trim().to_string();
    let (text, custom_id) = crate::utils::header_id_utils::extract_header_id(&raw_text);
    Some(Box::new(HeadingInfo {
        level: marker_len as u8,
        style: HeadingStyle::ATX,
        marker: content[..marker_len].to_string(),
        marker_column: blockquote.prefix.len(),
        content_column: blockquote.prefix.len() + marker_len + spaces_len,
        text,
        custom_id,
        raw_text,
        has_closing_sequence,
        closing_sequence,
        is_valid: true,
    }))
}

/// Detect HTML blocks in the content
///
/// Follows CommonMark §4.6. Type-1 blocks (`<pre>`, `<script>`, `<style>`,
/// `<textarea>`) run until their matching end tag or end of document and may
/// contain blank lines. All other recognised block elements are treated as
/// Type-6-style blocks that terminate at the first blank line.
///
/// A block start is only recognised outside an already-open block: lines inside
/// one are its content, so a `<pre>` nested in a `<table>` does not open a
/// second block that can outlive the first.
pub(super) fn detect_html_blocks(content: &str, lines: &mut [LineInfo]) {
    use crate::utils::html_block::{TYPE_1_BLOCK_ELEMENTS, parse_html_block_start};

    let mut i = 0;
    while i < lines.len() {
        if lines[i].in_code_block || lines[i].in_front_matter {
            i += 1;
            continue;
        }

        let trimmed = lines[i].content(content).trim_start();

        let Some((tag_name, is_closing)) = parse_html_block_start(trimmed) else {
            i += 1;
            continue;
        };

        lines[i].in_html_block = true;

        if is_closing {
            i += 1;
            continue;
        }

        let closing_tag = format!("</{tag_name}>");

        if lines[i].content(content).contains(&closing_tag) {
            i += 1;
            continue;
        }

        let allow_blank_lines = TYPE_1_BLOCK_ELEMENTS.contains(&tag_name.as_str());
        let mut j = i + 1;
        let mut found_closing_tag = false;
        while j < lines.len() {
            if !allow_blank_lines && lines[j].is_blank {
                break;
            }

            lines[j].in_html_block = true;

            if lines[j].content(content).contains(&closing_tag) {
                found_closing_tag = true;
            }

            if found_closing_tag {
                j += 1;
                while j < lines.len() {
                    if lines[j].is_blank {
                        break;
                    }
                    lines[j].in_html_block = true;
                    j += 1;
                }
                break;
            }
            j += 1;
        }

        // Every line the scan consumed belongs to the block it opened, so none of
        // them can open another one. Resuming at `j` is what keeps a nested
        // `<pre>` from starting a second block that outlives the first.
        i = j;
    }
}
