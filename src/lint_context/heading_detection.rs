use crate::config::MarkdownFlavor;
use crate::utils::code_block_utils::CodeBlockUtils;
use crate::utils::skip_context::SkipRegions;
use crate::utils::table_utils::TableUtils;
use std::cell::LazyCell;
use std::sync::LazyLock;

use super::line_computation::spanned_lines;
use super::list_blocks::{column_after, column_at};
use super::types::*;

/// Source slices of an ATX opener. Marker recognition needs no capture storage.
#[derive(Debug, PartialEq, Eq)]
struct AtxMarker<'a> {
    indent: usize,
    hashes: &'a str,
    whitespace: usize,
    rest: &'a str,
}

/// What a line's opening run of `#`s makes of it.
#[derive(Debug, PartialEq, Eq)]
enum AtxOpening<'a> {
    /// CommonMark 4.2: one to six `#`s followed by a space, a tab or the end of
    /// the line open an ATX heading.
    Heading(AtxMarker<'a>),
    /// One to six `#`s with text right after them, as in `#Title` or `#tag`.
    /// CommonMark reads the line as paragraph text; only the ATX-spacing rules
    /// see a heading missing its space in it.
    MissingSpace { level: u8 },
}

/// The ATX opening of a line, or `None` when it has none (including a run of
/// seven or more `#`s, which is paragraph text with no heading to repair).
fn atx_opening(line: &str) -> Option<AtxOpening<'_>> {
    let bytes = line.as_bytes();
    let indent = bytes.iter().take_while(|&&byte| matches!(byte, b' ' | b'\t')).count();
    if bytes.get(indent) != Some(&b'#') {
        return None;
    }
    let level = bytes[indent..].iter().take_while(|&&byte| byte == b'#').count();
    if level > 6 {
        return None;
    }
    let marker_end = indent + level;
    let after = &line[marker_end..];
    if after.is_empty() || after.starts_with([' ', '\t']) {
        let whitespace = after.bytes().take_while(|&byte| matches!(byte, b' ' | b'\t')).count();
        Some(AtxOpening::Heading(AtxMarker {
            indent,
            hashes: &line[indent..marker_end],
            whitespace,
            rest: &after[whitespace..],
        }))
    } else {
        Some(AtxOpening::MissingSpace { level: level as u8 })
    }
}

/// The label opening a footnote definition, `[^id]:`. It is read only on lines
/// the parser places inside a definition, which settles what the label may hold,
/// apart from a backslash, which escapes the character after it (`[^a\]b]:`).
static FOOTNOTE_LABEL_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[ \t]*\[\^(?:[^\]\\]|\\.)+\]:").unwrap());

/// CommonMark 5.2: an ordered list marker is "one to nine digits". A longer run
/// is ordinary paragraph text, and the marker pattern does not say so.
const MAX_ORDERED_MARKER_DIGITS: usize = 9;

/// Whether `text` holds nothing but spaces and tabs, the only whitespace
/// CommonMark's block structure recognises. A no-break space is text.
fn is_cm_blank(text: &str) -> bool {
    text.trim_matches([' ', '\t']).is_empty()
}

/// A list item a line opens.
#[derive(Debug, PartialEq, Eq)]
struct OpenedItem {
    /// Where the text after the marker and its whitespace begins.
    text_start: usize,
    /// Where the marker itself ends.
    marker_end: usize,
}

/// The list item a line opens, if it does. A continuation line that does not
/// reach the item's content column is lazy.
///
/// `interrupting` says a paragraph flows into this line from the one above.
/// CommonMark 5.2 lets a list interrupt a paragraph only when its item does not
/// start with a blank line, and an ordered one only when it starts at 1. So `2.`
/// or a bare `*` written under prose is part of the sentence rather than a
/// marker, a bare `-` is the underline of that prose, and the line opens no item
/// at all.
fn list_item_content_column(line: &str, interrupting: bool) -> Option<OpenedItem> {
    let bytes = line.as_bytes();
    let start = bytes.iter().take_while(|&&byte| matches!(byte, b' ' | b'\t')).count();
    let marker_end = match bytes.get(start)? {
        b'*' | b'+' | b'-' => start + 1,
        b'0'..=b'9' => {
            let digits = bytes[start..].iter().take_while(|byte| byte.is_ascii_digit()).count();
            if digits > MAX_ORDERED_MARKER_DIGITS || !matches!(bytes.get(start + digits), Some(b'.' | b')')) {
                return None;
            }
            let number = &line[start..start + digits];
            if interrupting && number.trim_start_matches('0') != "1" {
                return None;
            }
            start + digits + 1
        }
        _ => return None,
    };
    if bytes.get(marker_end).is_some_and(|byte| !matches!(byte, b' ' | b'\t')) {
        return None;
    }
    let text_start = marker_end
        + bytes[marker_end..]
            .iter()
            .take_while(|&&byte| matches!(byte, b' ' | b'\t'))
            .count();
    if interrupting && is_cm_blank(&line[text_start..]) {
        return None;
    }
    Some(OpenedItem { text_start, marker_end })
}

/// CommonMark 4.3: a setext underline "can be indented up to three spaces" past
/// the edge of the container it is written in. One more makes it paragraph text.
const MAX_SETEXT_UNDERLINE_INDENT: usize = 3;

/// CommonMark 5.2: up to four columns of whitespace after a list marker belong
/// to the marker. With more, the item's content starts one column past the
/// marker and the rest is indented code.
const MAX_ITEM_CONTENT_GAP: usize = 4;

/// CommonMark 5.2 and 5.1: a list marker or a `>` is one only when it is written
/// no further than three columns past the edge of the container it is in. Four
/// make the line indented code, or the text of the paragraph running into it.
const MAX_MARKER_INDENT: usize = 3;

/// A footnote definition's body starts four columns past the edge of the
/// container holding the definition, wherever on its line the label starts.
const FOOTNOTE_BODY_INDENT: usize = 4;

/// One container of the stack a line has to re-enter to be written inside it.
///
/// The two kinds are re-entered differently - a blockquote wants its `>`
/// repeated, a list item wants the line indented to the column its content
/// starts at - so which order they nest in decides what a line carrying only one
/// of the two re-enters. `- > quote` and `> - item` are both two deep, and a
/// following `> more` re-enters the second while closing the first. The stack
/// therefore keeps the order rather than counting each kind.
///
/// A `Quote` is one `>`, so `> > quote` is two of them: a following `> more`
/// re-enters the outer quote only and closes the inner one.
///
/// A `Footnote` is a definition's body, re-entered by indentation the way an
/// item is, so `[^a]: intro` followed by `===` leaves the underline lazy.
#[derive(Clone, Copy, PartialEq)]
enum Marker {
    Quote,
    /// A list item: the column its content starts at where it was opened, and
    /// how many columns that is past the edge of the container holding it.
    Item(usize, usize),
    /// A footnote body, described the same way.
    Footnote(usize, usize),
}

/// One blockquote marker opening `line`: the text from its `>` on, and the text
/// after the `>` and the single space or tab that belongs to it, which is how
/// `parse_blockquote_prefix` splits a level.
fn strip_quote_marker(line: &str) -> Option<(&str, &str)> {
    let marker = line.trim_start_matches([' ', '\t']);
    let after_marker = marker.strip_prefix('>')?;
    Some((marker, after_marker.strip_prefix([' ', '\t']).unwrap_or(after_marker)))
}

/// Where a list item a line opens begins, which the `Marker` alone does not say.
#[derive(Clone, Copy)]
struct ItemSpan {
    /// The column the marker starts on.
    marker_column: usize,
    /// Nothing but whitespace follows the marker on the line.
    is_empty: bool,
}

/// What a line did with the container open above it.
struct Entered<'a> {
    /// How many of the open markers the line re-entered, outermost first. Fewer
    /// than the stack holds means the line closed the rest.
    matched: usize,
    /// The text the line holds, with every marker stripped away.
    content: &'a str,
    /// How far the text is indented past the edge of the innermost container the
    /// line re-entered or opened: the indentation CommonMark limits a setext
    /// underline to.
    indent: usize,
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
///
/// `in_footnote_definition` says the parser places the line inside a footnote
/// definition, which is what lets a `[^id]:` label on it open one.
///
/// `limit_marker_indent` applies CommonMark's three-column limit on how far a
/// marker may be indented past its container's edge. It is off for lines inside
/// a block whose own margin the pass does not track, where the indentation says
/// nothing about the edge.
///
/// Every position is a column, with a tab reaching the next multiple of four,
/// since that is how CommonMark measures both re-entry and indentation.
fn enter<'a>(
    line: &'a str,
    open: &[Marker],
    paragraph: bool,
    in_footnote_definition: bool,
    limit_marker_indent: bool,
    opened: &mut Vec<Marker>,
    item_spans: &mut Vec<ItemSpan>,
) -> Entered<'a> {
    opened.clear();
    item_spans.clear();
    // The column the tail of the line starting at `slice` sits on.
    // Once a footnote label is read, the text after it is measured as if it started
    // at the body's edge and its tab stops counted from the start of the text: the parser drops the
    // label and the gap, and puts the content edge of a container opened beside the
    // label that far in from the body's edge. Holds the byte the text starts at and
    // the column it is placed on.
    let relocated = std::cell::Cell::new(None::<(usize, usize)>);
    let column = |slice: &str| {
        let offset = line.len() - slice.len();
        match relocated.get() {
            Some((from, origin)) if offset >= from => origin + line[from..offset].chars().fold(0, column_after),
            _ => column_at(line, offset),
        }
    };
    // The column a blockquote's content starts at. The `>` takes one column of
    // the space or tab after it, so a tab reaching further leaves its remaining
    // columns as indentation of the content.
    let quote_edge = |marker: &str, content: &str| column(content).min(column(marker) + 2);
    // The column of the first non-blank character of `slice`. Consecutive open
    // items are tested against the same `rest`, so the last answer is reused
    // rather than rescanning the line once per open ancestor.
    let last_indent = std::cell::Cell::new(None::<(usize, usize)>);
    let indent_column = |slice: &str| match last_indent.get() {
        Some((len, col)) if len == slice.len() => col,
        _ => {
            let col = column(slice.trim_start_matches([' ', '\t']));
            last_indent.set(Some((slice.len(), col)));
            col
        }
    };
    let mut rest = line;
    let mut matched = 0;
    // The column the innermost container entered so far starts its content at.
    let mut edge = 0;
    while matched < open.len() {
        match open[matched] {
            Marker::Quote => match strip_quote_marker(rest).filter(|_| {
                !limit_marker_indent || column(rest.trim_start_matches([' ', '\t'])) <= edge + MAX_MARKER_INDENT
            }) {
                Some((marker, content)) => {
                    edge = quote_edge(marker, content);
                    rest = content;
                }
                None => break,
            },
            // Nothing is consumed here: the body is re-entered by indentation,
            // which is where the next marker or the text itself begins.
            // The body is as far in as it was past the edge of the container
            // holding it, measured from that container's edge on this line: a
            // `>` re-entered further in than where it was opened moves the
            // item's content with it, and indentation written before the `>`
            // does not count towards the item.
            Marker::Item(_, width) | Marker::Footnote(_, width) if indent_column(rest) >= edge + width => {
                edge += width;
            }
            Marker::Item(..) | Marker::Footnote(..) => break,
        }
        matched += 1;
    }
    loop {
        // Further in than a marker may be, the line holds text and opens nothing.
        if limit_marker_indent && column(rest.trim_start_matches([' ', '\t'])) > edge + MAX_MARKER_INDENT {
            break;
        }
        if let Some((marker, content)) = strip_quote_marker(rest) {
            opened.push(Marker::Quote);
            edge = quote_edge(marker, content);
            rest = content;
            continue;
        }
        // CommonMark 5.2: "When both a thematic break and a list item are
        // possible interpretations of a line, the thematic break takes
        // precedence", so `* * *` is a break rather than an item holding `* *`.
        if is_horizontal_rule_content(rest.trim()) {
            break;
        }
        // A label written inside a definition's body starts a definition whose
        // body has the same edge, so the body already open serves for both.
        let in_footnote_body = open[..matched]
            .iter()
            .chain(opened.iter())
            .any(|marker| matches!(marker, Marker::Footnote(..)));
        if in_footnote_definition
            && !in_footnote_body
            && let Some(label) = FOOTNOTE_LABEL_REGEX.find(rest)
        {
            opened.push(Marker::Footnote(edge + FOOTNOTE_BODY_INDENT, FOOTNOTE_BODY_INDENT));
            rest = &rest[label.end()..];
            edge += FOOTNOTE_BODY_INDENT;
            // The parser drops however much whitespace follows the label.
            rest = rest.trim_start_matches([' ', '\t']);
            relocated.set(Some((line.len() - rest.len(), edge)));
            continue;
        }
        let interrupting = paragraph && matched == open.len() && opened.is_empty();
        match list_item_content_column(rest, interrupting) {
            Some(item) => {
                // CommonMark 5.2: an item starting with a blank line has its
                // content one column past the marker, however much whitespace
                // follows it.
                let blank_start = is_cm_blank(&rest[item.text_start..]);
                let marker_column = column(rest.trim_start_matches([' ', '\t']));
                let after_marker = column(&rest[item.marker_end..]);
                let parent_edge = edge;
                let content_start = column(&rest[item.text_start..]);
                rest = &rest[item.text_start..];
                // Where the marker left off: the column a continuation of this
                // item's content has to reach. Five or more columns of
                // whitespace after the marker open indented code instead, and
                // the item's content then starts one column past the marker.
                edge = if blank_start || content_start - after_marker > MAX_ITEM_CONTENT_GAP {
                    after_marker + 1
                } else {
                    content_start
                };
                opened.push(Marker::Item(edge, edge - parent_edge));
                item_spans.push(ItemSpan {
                    marker_column,
                    is_empty: blank_start,
                });
            }
            None => break,
        }
    }
    Entered {
        matched,
        content: rest.trim_matches([' ', '\t']),
        indent: column(rest.trim_start_matches([' ', '\t'])).saturating_sub(edge),
    }
}

/// Whether the text a line holds is paragraph text, so that a paragraph running
/// into the line runs on out of it.
///
/// Blank lines and code fences end a paragraph too; the pass settles those before
/// asking, a blank line from the containers the line re-entered and a fence from
/// the line flags. The
/// ATX test is the same scanner the heading detection below runs, so the two agree
/// on `#hashtag` and other shapes CommonMark would call paragraph text.
///
/// A `=` run is paragraph text here. It is a setext underline only under a
/// paragraph it is written inside, and the pass answers that from the lines it
/// has already read rather than from the run itself; a `-` run long enough to be
/// a thematic break ends the paragraph under either reading and is rejected.
fn may_hold_open_paragraph(content: &str) -> bool {
    // An empty container holds no paragraph. A line that is nothing but the
    // markers of containers it opens (`* `, `1. `, `> - `) opens them empty; a
    // line holding nothing past the containers it re-entered is a blank line,
    // which the pass settles before asking.
    if content.is_empty() {
        return false;
    }
    !(is_horizontal_rule_content(content)
        || matches!(atx_opening(content), Some(AtxOpening::Heading(_)))
        || crate::utils::html_block::parse_html_block_start(content).is_some()
        || crate::utils::html_block::opens_untagged_html_block(content))
}

/// Whether a line begins with a tag that only start condition 7 makes an HTML
/// block: a complete tag of an element no other condition names (`<audio>`,
/// `<video src="x">`, `</audio>`), or the closing tag of an element whose opening
/// tag starts condition 1 (`</pre>`, `</script>`, `</style>`, `</textarea>`).
///
/// Such a tag cannot interrupt a paragraph, so under a running paragraph the line
/// is text of it. Where no paragraph runs into it the line does open a block, and
/// the parser's own block (`in_html_block`) says so; a line the parser did not
/// open a block on is the first line of a paragraph.
fn starts_only_through_condition_7(content: &str) -> bool {
    use crate::utils::html_block::NON_SPEC_BLOCK_ELEMENTS;
    const RAW_TEXT_ELEMENTS: [&str; 4] = ["pre", "script", "style", "textarea"];
    crate::utils::html_block::parse_html_block_start(content).is_some_and(|(tag, closing)| {
        NON_SPEC_BLOCK_ELEMENTS.contains(&tag.as_str()) || (closing && RAW_TEXT_ELEMENTS.contains(&tag.as_str()))
    })
}

/// Whether a line is paragraph text: the shape a setext underline needs above
/// it, and the shape that keeps a container's paragraph open below it.
pub(crate) fn is_paragraph_text_line(line: &str) -> bool {
    may_hold_open_paragraph(enter(line, &[], false, false, false, &mut Vec::new(), &mut Vec::new()).content)
}

/// The structural blocks a line sits inside.
///
/// No paragraph spans a boundary between two of these, so the pass ends the
/// paragraph and the table running into a line whose blocks differ from the line
/// above it. The containers around the boundary are settled the way every line
/// settles them, by what the line re-enters: a code block or a div written inside
/// a list item leaves the item open below it, and a line written outside the
/// item closes it. Comparing the two lines rather than testing one keeps a
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
///
/// `in_mdx_flow` says the line holds MDX flow syntax: an expression standing
/// as a block of its own, or a JSX flow element's own tags rather than its
/// children, which are Markdown blocks read like any other. A JSX text element
/// or an expression inside a line of text is inline content of the paragraph
/// holding it and ends nothing, so `Heading <span>x</span>` above `---` is
/// still a heading, while `<Card />` or `{x}` on a line of its own ends the
/// paragraph above it.
fn structural_blocks(
    line: &LineInfo,
    flavor: MarkdownFlavor,
    in_mdx_flow: bool,
    in_mdx_literal_body: bool,
) -> [bool; 17] {
    [
        line.in_code_block,
        line.in_front_matter,
        line.in_html_block,
        line.in_html_comment,
        line.in_math_block,
        in_mdx_literal_body,
        line.in_obsidian_comment,
        line.in_mkdocstrings,
        line.in_esm_block,
        in_mdx_flow,
        line.in_pandoc_div,
        // A `:::` fence opens a div only in Pandoc and Quarto. The flavors that
        // give `:::` another meaning mark it with a flag of their own above, and
        // everywhere else the line is paragraph text.
        line.is_div_marker && flavor.is_pandoc_compatible(),
        line.in_admonition,
        line.in_content_tab,
        line.in_pymdown_block,
        line.in_myst_directive,
        // myst-parser ends the quoted paragraph on a `%` comment, so
        // `> quote\n% c\nSetup\n=====` really is a heading in MyST.
        line.is_myst_comment,
    ]
}

/// Whether a line sits in a block whose body is not Markdown, so nothing
/// written there can be a heading.
///
/// This is the opaque subset of `structural_blocks`: LaTeX in display math,
/// comment text in an Obsidian or MDX comment, JavaScript on MDX ESM lines,
/// YAML in a mkdocstrings options block. `x` above `=` is an equation, a
/// comment or a mapping there, never a Setext heading.
///
/// The rest of `structural_blocks` is deliberately absent, because those
/// containers hold Markdown and a heading written inside one is a heading:
/// Pandoc divs, MkDocs admonitions and content tabs, PyMdown blocks, MyST
/// directives, and JSX components (MDX renders a component's children as
/// Markdown). Code blocks, front matter and HTML blocks are checked
/// separately by the caller, and an HTML comment is settled by byte range
/// rather than by this flag, since a comment can open mid-line.
fn is_opaque_body(line: &LineInfo) -> bool {
    is_opaque_heading_body(0, line, None)
}

fn is_opaque_heading_body(index: usize, line: &LineInfo, mdx_literal_bodies: Option<&[bool]>) -> bool {
    line.in_math_block
        || line.in_obsidian_comment
        || mdx_literal_bodies.map_or(line.in_mdx_comment, |bodies| bodies[index])
        || line.in_esm_block
        || line.in_mkdocstrings
}

/// Headings cannot occur inside expression or comment bodies. An inline MDX
/// region beside visible heading text does not make the entire line literal.
fn mdx_literal_bodies(content_lines: &[&str], lines: &[LineInfo], ranges: [&[(usize, usize)]; 2]) -> Vec<bool> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if !line.in_mdx_comment && !line.in_jsx_expression {
                return false;
            }
            let content = content_lines[index];
            let start = line.byte_offset + content.len() - content.trim_start_matches([' ', '\t']).len();
            let end = line.byte_offset + content.trim_end_matches([' ', '\t', '\r']).len();
            ranges.into_iter().any(|ranges| {
                let first = ranges.partition_point(|&(_, range_end)| range_end <= start);
                ranges
                    .get(first)
                    .is_some_and(|&(range_start, range_end)| range_start <= start && end <= range_end)
            })
        })
        .collect()
}

/// Whether a line's indentation belongs to a container the pass does not track,
/// so that it says nothing about how far a setext underline is indented.
///
/// MkDocs admonitions and content tabs hold their body indented by four columns.
/// MDX turns indented code off, and markdown-rs lifts the underline's limit with
/// it, since the limit exists only to leave room for indented code.
fn underline_indent_is_unbounded(line: &LineInfo, flavor: MarkdownFlavor) -> bool {
    flavor == MarkdownFlavor::MDX || line.in_admonition || line.in_content_tab
}

/// Where the link reference definitions of a document are, for the pass to tell
/// a definition's own lines from the text of the paragraph that follows them.
#[derive(Clone, Copy)]
pub(super) enum Definitions<'a> {
    /// One entry per line: the line belongs to a definition.
    Lines(&'a [bool]),
    /// The lines, ascending, on which the parser found text, a table or a
    /// heading starting. The definitions a paragraph opens with are the lines
    /// of it before the first of these.
    TextStarts(&'a [usize]),
}

/// What the pass settled about one line.
#[derive(Clone, Copy, Default)]
struct Trailing<'a> {
    /// The paragraph the line is the setext underline of, as the index of that
    /// paragraph's first text line: a `=`/`-` run written inside the container
    /// of the paragraph running into it, indented no further past the
    /// container's edge than an underline may be. The heading's text is every
    /// line of that paragraph past the link reference definitions it opens
    /// with, down to the line above the underline.
    ///
    /// CommonMark 4.3: "The setext heading underline cannot be a lazy
    /// continuation line." Where the paragraph hangs off a blockquote or a list
    /// item, the same run written outside that container is ordinary paragraph
    /// text and the whole construct stays one paragraph.
    underlines: Option<usize>,
    /// The text the line holds past the markers of the containers it entered,
    /// trimmed: a heading line's share of the heading's text.
    text: &'a str,
    /// How many blockquotes hold the paragraph running out of the line.
    quote_depth: usize,
    /// Whether the line opens a list item, so that its text sits inside the
    /// body the marker opens.
    carries_item: bool,
    /// Whether the line opens a footnote definition, whose body the pass does
    /// not model well enough to take a heading from.
    carries_footnote: bool,
    /// The byte offset in the line where `text` begins: past every marker the
    /// line opened and the whitespace after it.
    text_start: usize,
    /// Whether the line ends with the backslash of a hard line break, which
    /// renders as the break rather than as text.
    hard_break: bool,
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
///
/// `html_blocks` are the byte ranges of the HTML blocks the CommonMark parser
/// reported, whose lines hold no paragraph in whatever container they sit.
/// `code_blocks` are the byte ranges of the code blocks, which say where one
/// code block ends and the next begins when no other line comes between them.
///
/// `mdx_flow_lines` marks the lines holding MDX flow syntax where the MDX parse
/// produced them. Without that parse, `in_jsx_block` is the only evidence.
#[allow(clippy::too_many_arguments)]
fn trailing_state<'a>(
    content_lines: &[&'a str],
    lines: &[LineInfo],
    flavor: MarkdownFlavor,
    html_blocks: &[(usize, usize)],
    code_blocks: &[(usize, usize)],
    code_spans: &[(usize, usize)],
    mdx_flow_lines: Option<&[bool]>,
    mdx_literal_bodies: Option<&[bool]>,
    definitions: Option<Definitions<'_>>,
) -> Pass<'a> {
    let blocks = |index: usize| {
        let in_mdx_flow = mdx_flow_lines.map_or(lines[index].in_jsx_block, |flow| flow[index]);
        structural_blocks(
            &lines[index],
            flavor,
            in_mdx_flow,
            mdx_literal_bodies.map_or(lines[index].in_mdx_comment, |bodies| bodies[index]),
        )
    };
    let mut states = Vec::with_capacity(lines.len());
    // Whether each line is an ATX shape the pass reads as paragraph text.
    let mut atx_is_text = vec![false; lines.len()];
    // Whether each line is written too far into the paragraph running into it
    // to start a block.
    let mut is_indented_text = vec![false; lines.len()];
    // Whether each line is indented text that cannot be a table row either.
    let mut is_lazy_table_text = vec![false; lines.len()];
    // The container each line is read against: `containers[index]` is what the
    // lines above `index` left open, so a line's own container, once it has been
    // read, is `containers[index + 1]`.
    let mut containers = Vec::with_capacity(lines.len() + 1);
    // How many containers were open just after a line opened a list item with
    // nothing on it, while that is still the last thing read.
    let mut empty_item: Option<usize> = None;
    // The container the lines read so far left open, outermost first, and a
    // scratch buffer for the containers each line opens of its own.
    let mut open: Vec<Marker> = Vec::new();
    let mut opened: Vec<Marker> = Vec::new();
    let mut item_spans: Vec<ItemSpan> = Vec::new();
    // The list items each line opens, for the lines that open one.
    let mut item_chains: Vec<Vec<CommonMarkItem>> = vec![Vec::new(); lines.len()];
    // The paragraph the lines read so far left open, as the index of its first
    // line.
    let mut paragraph: Option<usize> = None;
    // Whether the paragraph running into the next line holds anything but link
    // reference definitions so far.
    let mut paragraph_has_text = false;
    let mut in_table = false;
    let mut header_cells = None;
    // MDX reads a tag as JSX, whose lines hold markdown, so only the other
    // flavors take the lines of the parser's HTML blocks for raw HTML, less
    // those `parser_html_block_is_text` rules out.
    let mut in_html_block = super::scratch::LineFlags::new(lines.len());
    // The line each code block and HTML block starts on, which opens a block of
    // its own even where the line above it closes another.
    let mut opens_raw_block = super::scratch::LineFlags::new(lines.len());
    if flavor != MarkdownFlavor::MDX {
        for &(start, end) in html_blocks {
            let spanned = spanned_lines(lines, start, end);
            if lines.get(spanned.start).is_none_or(parser_html_block_is_text) {
                continue;
            }
            opens_raw_block[spanned.start] = true;
            in_html_block[spanned].fill(true);
        }
    }
    for &(start, end) in code_blocks {
        let spanned = spanned_lines(lines, start, end);
        if spanned.start < spanned.end {
            opens_raw_block[spanned.start] = true;
        }
    }
    // Whether a line is the text of a block whose body is not Markdown.
    let raw = |index: usize| {
        let line = &lines[index];
        line.in_code_block
            || line.in_front_matter
            || line.in_html_comment
            || in_html_block[index]
            || is_opaque_heading_body(index, line, mdx_literal_bodies)
    };

    for index in 0..lines.len() {
        containers.push(container_of(&open));
        let pending_empty_item = empty_item.take();
        // A paragraph and a table end where a structural block starts or ends.
        let boundary = index > 0 && blocks(index) != blocks(index - 1);
        if boundary {
            paragraph = None;
            in_table = false;
            header_cells = None;
        }

        let entered = enter(
            content_lines[index],
            &open,
            paragraph.is_some(),
            lines[index].in_footnote_definition,
            !has_untracked_margin(&lines[index]),
            &mut opened,
            &mut item_spans,
        );
        if opened.is_empty() && entered.content.is_empty() {
            // A line holding nothing past the containers it re-entered is a blank
            // line inside them, as `>` alone is inside a blockquote. It ends a
            // paragraph and a table. A blockquote is entered by repeating its
            // `>`, which the blank line does not do for the ones it did not
            // re-enter, so it closes those and everything written inside them;
            // CommonMark 5.2 lets a list item or a footnote hold several blocks,
            // so the ones outside them go on holding their content.
            if let Some(quote) = open[entered.matched..]
                .iter()
                .position(|marker| *marker == Marker::Quote)
            {
                open.truncate(entered.matched + quote);
            }
            // CommonMark 5.2: "A list item can begin with at most one blank
            // line", so a blank line right under an item that opened empty
            // closes it, and what follows is written in the container around it.
            if pending_empty_item == Some(open.len()) {
                open.pop();
            }
            paragraph = None;
            in_table = false;
            header_cells = None;
            states.push(Trailing::default());
            continue;
        }
        // Past the line opening it, a raw block's text is content however it
        // reads: `- a` in a code block or an HTML block opens no list item. Only
        // the opening line can enter containers of its own, as `- ```` does, and
        // the lines below it stay inside the ones they re-enter.
        if !boundary && index > 0 && raw(index) && raw(index - 1) && !opens_raw_block[index] {
            open.truncate(entered.matched);
            paragraph = None;
            in_table = false;
            header_cells = None;
            states.push(Trailing::default());
            continue;
        }
        let carries_item = opened.iter().any(|marker| matches!(marker, Marker::Item(..)));
        let carries_footnote = opened.iter().any(|marker| matches!(marker, Marker::Footnote(..)));
        let text_start = if entered.content.is_empty() {
            content_lines[index].len()
        } else {
            entered.content.as_ptr() as usize - content_lines[index].as_ptr() as usize
        };
        // A line of a code block or an HTML block is code or raw HTML however it
        // reads, and its text cannot say so alone: `    x` is indented code and
        // `<span>` opens a block only where no paragraph runs into it, and either
        // block runs on through the lines below. A container's marker is
        // structure the same way: `!!! note` opens the admonition whose body is
        // the lines below it, and holds no paragraph of its own for them to
        // continue.
        //
        // Indented four columns past the edge of the container it is written in, a
        // line cannot start a block, so under a running paragraph whatever it
        // reads as (an ATX shape, a thematic break, a marker) is text that
        // paragraph continues.
        //
        // A footnote definition's body is tracked the same way (`enter` re-enters it
        // by its four columns), so while the pass has one open a line measures its
        // indentation from the edge of the containers it did re-enter, a lazy
        // continuation included.
        let in_footnote_body =
            lines[index].in_footnote_definition && open.iter().any(|marker| matches!(marker, Marker::Footnote(..)));
        let indented_text = paragraph.is_some()
            && (in_footnote_body || !in_untracked_container(&lines[index]))
            && !has_untracked_margin(&lines[index])
            && opened.is_empty()
            && entered.indent > MAX_MARKER_INDENT;
        // The link reference definitions a paragraph opens with are not its text
        // (CommonMark 4.7), so while the paragraph holds nothing else a row
        // written that far in, which is no part of a definition, is the first
        // text of the paragraph that follows them and may head a table.
        let line_in_definition = matches!(definitions, Some(Definitions::Lines(flags)) if flags[index]);
        let only_definitions = match (definitions, paragraph) {
            (Some(Definitions::Lines(flags)), Some(_)) if indented_text => !paragraph_has_text && !flags[index],
            (Some(Definitions::TextStarts(starts)), Some(first)) if indented_text => {
                starts.get(starts.partition_point(|&start| start < first)) == Some(&index)
            }
            _ => false,
        };
        // MDX turns indented code off, so a line indented that far still opens
        // the block it reads as.
        let indented_text_is_text = indented_text && flavor != MarkdownFlavor::MDX;
        is_lazy_table_text[index] = indented_text_is_text && !only_definitions;
        is_indented_text[index] = indented_text_is_text;
        atx_is_text[index] =
            indented_text_is_text && matches!(atx_opening(entered.content), Some(AtxOpening::Heading(_)));
        let holds_paragraph = !lines[index].in_code_block
            && !in_html_block[index]
            && !lines[index].is_container_marker
            && (indented_text
                || may_hold_open_paragraph(entered.content)
                || (flavor != MarkdownFlavor::MDX && !in_table && starts_only_through_condition_7(entered.content)));
        // Whether the line's text is written inside the open container: it
        // re-entered the whole of it and opened none of its own.
        let inside = entered.matched == open.len() && opened.is_empty();

        // GFM matches a table's two opening rows cell for cell, so a delimiter
        // row opens a table only under a header row holding as many cells, and
        // only where that row started its own paragraph: a paragraph already
        // running into it holds it as text. A row indented too far to start a
        // block is that text, whatever it looks like.
        // Cell splitting allocates, so only a delimiter under a possible header
        // needs it. Ordinary prose and rows of an already open table do not.
        in_table |= !in_table
            && !indented_text_is_text
            && header_cells.is_some_and(|cells| {
                TableUtils::is_delimiter_row(entered.content)
                    && cells == TableUtils::count_cells_with_flavor(entered.content, flavor)
            });
        // GFM breaks a table at the first line that starts another block-level
        // structure: a container of its own, or any shape paragraph text is not.
        in_table &= inside && holds_paragraph;

        // CommonMark 4.3 forbids a lazy underline, so a `=`/`-` run underlines
        // the paragraph running into it only when it is written inside the same
        // container, and is a paragraph line of its own otherwise. Indented past
        // the container's edge, it is a continuation line of that paragraph.
        let underlines = paragraph.filter(|_| {
            inside
                && is_setext_underline_content(entered.content)
                && (entered.indent <= MAX_SETEXT_UNDERLINE_INDENT
                    || underline_indent_is_unbounded(&lines[index], flavor))
        });
        // The link reference definitions a paragraph opens with are not its
        // text (CommonMark 4.7), so the heading starts below them. A run under
        // definitions alone underlines nothing: it is the text of the paragraph
        // they open, however it reads, and that paragraph runs on below it.
        let underlines = underlines.map(|first| {
            let texts: Vec<&str> = states[first..index].iter().map(|state| state.text).collect();
            first + super::link_parser::leading_reference_definition_lines(&texts)
        });
        let hard_break = ends_with_hard_break(content_lines[index], lines[index].byte_offset, code_spans);
        if underlines == Some(index) {
            header_cells = None;
            states.push(Trailing {
                underlines: None,
                text: entered.content,
                quote_depth: open.iter().filter(|marker| **marker == Marker::Quote).count(),
                carries_item,
                carries_footnote,
                text_start,
                hard_break,
            });
            continue;
        }
        if in_table || underlines.is_some() {
            // No row of a table is paragraph text, and a run that underlines the
            // paragraph above it ends that paragraph: either way this line
            // leaves nothing open for the lines below to continue.
            paragraph = None;
            header_cells = None;
            states.push(Trailing {
                underlines,
                text: entered.content,
                quote_depth: 0,
                carries_item,
                carries_footnote,
                text_start,
                hard_break,
            });
            continue;
        }

        // Whether the paragraph running into this line runs on out of it, as its
        // own text or as a lazy continuation. A line that enters a container of
        // its own starts a paragraph there instead.
        let continues = paragraph.is_some() && holds_paragraph && opened.is_empty();
        // The text opening the paragraph that follows definitions is its first
        // line, which is what a table's header row has to be.
        let starts_text = !continues || only_definitions;
        // Where the paragraph leaving this line hangs off. Only a line continuing
        // the paragraph running into it leaves the container where it was, which
        // is what makes its own reading the lazy one; every other line is written
        // where it re-entered, so the containers it did not re-enter close and
        // the ones it opened take their place, and the paragraph leaving the line
        // is the line's own, where it holds one. A line already inside the whole
        // of the open container re-seats it onto itself, so it needs no test here.
        if !continues {
            // The items this line opens, when they sit directly in list items
            // alone: a blockquote or a footnote between them would make the
            // columns relative to a margin the pass does not follow.
            if open[..entered.matched]
                .iter()
                .all(|marker| matches!(marker, Marker::Item(..)))
            {
                item_chains[index] = opened
                    .iter()
                    .zip(&item_spans)
                    .map_while(|(marker, span)| match marker {
                        Marker::Item(content_column, _) => Some(CommonMarkItem {
                            marker_column: span.marker_column,
                            content_column: *content_column,
                            is_empty: span.is_empty,
                        }),
                        _ => None,
                    })
                    .collect();
            }
            open.truncate(entered.matched);
            open.extend_from_slice(&opened);
            paragraph = holds_paragraph.then_some(index);
            paragraph_has_text = !line_in_definition;
            empty_item =
                (entered.content.is_empty() && matches!(opened.last(), Some(Marker::Item(..)))).then_some(open.len());
        } else {
            paragraph_has_text |= !line_in_definition;
        }
        // The header row of a table: a line that starts its own paragraph and
        // has cells for a delimiter row below to match.
        header_cells =
            (starts_text && holds_paragraph && TableUtils::is_potential_table_row_with_flavor(entered.content, flavor))
                .then(|| TableUtils::count_cells_with_flavor(entered.content, flavor));
        states.push(Trailing {
            underlines: None,
            text: entered.content,
            quote_depth: open.iter().filter(|marker| **marker == Marker::Quote).count(),
            carries_item,
            carries_footnote,
            text_start,
            hard_break,
        });
    }
    containers.push(container_of(&open));
    Pass {
        states,
        containers,
        atx_is_text,
        indented_text: is_indented_text,
        lazy_table_text: is_lazy_table_text,
        item_chains,
    }
}

/// Whether the whitespace a container prefix ends with is wide enough for the
/// line after it to be indented text instead of a block opener: a tab, or three
/// spaces together with the marker's own space.
pub(super) fn prefix_may_hide_block(prefix: &str) -> bool {
    prefix.contains('\t') || prefix.contains("   ")
}

/// One entry per line: the line is indented text that the paragraph running into
/// it continues, so it cannot be a table row (see `Pass::lazy_table_text`).
#[allow(clippy::too_many_arguments)]
pub(super) fn indented_text_lines(
    content_lines: &[&str],
    lines: &[LineInfo],
    flavor: MarkdownFlavor,
    html_blocks: &[(usize, usize)],
    code_blocks: &[(usize, usize)],
    code_spans: &[(usize, usize)],
    mdx_flow_lines: Option<&[bool]>,
    mdx_literal_ranges: Option<[&[(usize, usize)]; 2]>,
    definitions: Option<Definitions<'_>>,
) -> Vec<bool> {
    let literal_bodies = mdx_literal_ranges
        .filter(|ranges| ranges.iter().any(|ranges| !ranges.is_empty()))
        .map(|ranges| mdx_literal_bodies(content_lines, lines, ranges));
    trailing_state(
        content_lines,
        lines,
        flavor,
        html_blocks,
        code_blocks,
        code_spans,
        mdx_flow_lines,
        literal_bodies.as_deref(),
        definitions,
    )
    .lazy_table_text
}

/// What reading the document once left behind.
struct Pass<'a> {
    states: Vec<Trailing<'a>>,
    /// See `trailing_state`: one entry per line, and one more for the end.
    containers: Vec<HeadingContainer>,
    /// One entry per line: the line has the shape of an ATX heading but is indented
    /// too far into the paragraph running into it to be one.
    atx_is_text: Vec<bool>,
    /// One entry per line: the line is indented four or more columns past the edge
    /// of its container while a paragraph runs into it, so whatever it looks like
    /// it is text that paragraph continues.
    indented_text: Vec<bool>,
    /// One entry per line: `indented_text`, except for a row that opens the
    /// paragraph following the link reference definitions it sits under.
    lazy_table_text: Vec<bool>,
    /// One entry per line: the list items the line opens, outermost first, when
    /// the pass reads it as opening any.
    item_chains: Vec<Vec<CommonMarkItem>>,
}

/// The container a heading written in `open` belongs to: the innermost list item
/// when only list items are open, and nothing the pass can name otherwise.
fn container_of(open: &[Marker]) -> HeadingContainer {
    let mut container = HeadingContainer::Outside;
    for marker in open {
        match marker {
            Marker::Item(content_col, _) => {
                container = HeadingContainer::Item {
                    content_col: *content_col,
                }
            }
            Marker::Quote | Marker::Footnote(..) => return HeadingContainer::Unknown,
        }
    }
    container
}

/// Whether a line sits in a flavor's block whose body is indented by a margin of
/// its own, which the container pass does not account for.
fn in_untracked_container(line: &LineInfo) -> bool {
    has_untracked_margin(line) || line.in_footnote_definition
}

/// [`in_untracked_container`] less footnote definitions, whose body the pass does
/// track.
fn has_untracked_margin(line: &LineInfo) -> bool {
    line.in_pandoc_div
        || line.in_admonition
        || line.in_content_tab
        || line.in_pymdown_block
        || line.in_myst_directive
        || line.in_mkdocs_html_markdown
        || line.in_definition_list
        || line.in_kramdown_extension_block
}

/// The container to record on a heading's lines: the one the pass settled, unless
/// a block it does not model also holds one of the lines.
fn heading_container_of(container: HeadingContainer, lines: &[LineInfo]) -> HeadingContainer {
    match container {
        HeadingContainer::Item { .. } if lines.iter().any(in_untracked_container) => HeadingContainer::Unknown,
        other => other,
    }
}

/// The text of the paragraph `states` are the lines of, on one line: the lines
/// joined by the space each soft line break renders as. A backslash ending a
/// line before the last as a hard line break renders as the break rather than
/// as text, so it goes with the line ending it marks.
fn paragraph_text(states: &[Trailing]) -> String {
    let mut text = String::new();
    for (index, state) in states.iter().enumerate() {
        if index > 0 {
            text.push(' ');
        }
        text.push_str(if index + 1 < states.len() && state.hard_break {
            state.text.strip_suffix('\\').unwrap_or(state.text)
        } else {
            state.text
        });
    }
    text
}

/// Whether a line ends with the backslash of a hard line break: the last of an
/// odd run of backslashes, the pairs before it being escaped backslashes that
/// are text, written outside a code span, where a backslash is code. The line
/// starts at `byte_offset` of the document `code_spans` are the byte ranges of.
pub(super) fn ends_with_hard_break(line: &str, byte_offset: usize, code_spans: &[(usize, usize)]) -> bool {
    let backslashes = line.len() - line.trim_end_matches('\\').len();
    backslashes % 2 == 1 && !CodeBlockUtils::is_in_code_block(code_spans, byte_offset + line.len() - 1)
}

/// The heading a setext underline makes of the paragraph above it.
///
/// `raw_text` is the paragraph's text on one line and `text_lines` how many
/// source lines it spans. `attribute_id` is the ID of a standalone attribute
/// list written under the underline, which names the heading when its text
/// carries no ID of its own.
fn setext_heading_info(
    raw_text: &str,
    text_lines: usize,
    underline: &str,
    marker_column: usize,
    content_column: usize,
    attribute_id: Option<String>,
) -> HeadingInfo {
    let underline = trim_setext_underline_start(underline);
    let (level, style) = if underline.starts_with('=') {
        (1, HeadingStyle::Setext1)
    } else {
        (2, HeadingStyle::Setext2)
    };
    let heading_text = crate::utils::header_id_utils::extract_heading_text(raw_text);
    HeadingInfo {
        level,
        style,
        marker: underline.to_string(),
        marker_column,
        content_column,
        text: heading_text.text,
        slug_text: heading_text.slug_text,
        custom_id: heading_text.custom_id.or(attribute_id),
        raw_text: raw_text.to_string(),
        text_lines,
        has_closing_sequence: false,
        closing_sequence: String::new(),
    }
}

/// The ATX heading an opening describes, for text that starts `base` bytes into
/// line `i`: past the markers of the list items the line opens, if it opens any.
fn atx_heading_info(
    marker: &AtxMarker<'_>,
    base: usize,
    content_lines: &[&str],
    lines: &[LineInfo],
    i: usize,
) -> HeadingInfo {
    let AtxMarker {
        indent,
        hashes,
        whitespace,
        rest,
    } = *marker;

    let marker_column = base + indent;

    // Check for closing sequence, but handle custom IDs that might come after
    let (text, has_closing, closing_seq) = parse_atx_remainder(rest);

    let content_column = marker_column + hashes.len() + whitespace;

    let raw_text = text.trim_matches([' ', '\t']).to_string();
    // The display fields read the text a reader sees, without the invisible
    // Unicode whitespace around it that `raw_text` keeps.
    let heading_text = crate::utils::header_id_utils::extract_heading_text(raw_text.trim());
    let mut custom_id = heading_text.custom_id;

    if custom_id.is_none() && i + 1 < content_lines.len() && i + 1 < lines.len() {
        let next_line = content_lines[i + 1];
        if !lines[i + 1].in_code_block
            && crate::utils::header_id_utils::is_standalone_attr_list(next_line)
            && let Some(next_line_id) = crate::utils::header_id_utils::extract_standalone_attr_list_id(next_line)
        {
            custom_id = Some(next_line_id);
        }
    }

    HeadingInfo {
        level: hashes.len() as u8,
        style: HeadingStyle::ATX,
        marker: hashes.to_string(),
        marker_column,
        content_column,
        text: heading_text.text,
        slug_text: heading_text.slug_text,
        custom_id,
        raw_text,
        text_lines: 1,
        has_closing_sequence: has_closing,
        closing_sequence: closing_seq,
    }
}

/// Whether a line opens a list item that begins with `#`, so that the line may
/// hold an ATX heading after its marker: `- # a`, `1. ## a`, `- - # a`. A
/// blockquote marker may come between the markers (`- > # a`), and the pass the
/// caller consults settles what the line really opens.
fn begins_item_with_hash(line: &str) -> bool {
    let mut rest = line.trim_start_matches([' ', '\t', '>']);
    let mut markers = 0;
    loop {
        let marker_len = match rest.bytes().next() {
            Some(b'-' | b'+' | b'*') => 1,
            Some(b'0'..=b'9') => {
                let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
                if !matches!(rest.as_bytes().get(digits), Some(b'.' | b')')) {
                    return false;
                }
                digits + 1
            }
            _ => return false,
        };
        let after = &rest[marker_len..];
        if !after.is_empty() && !after.starts_with([' ', '\t']) {
            return false;
        }
        markers += 1;
        rest = after.trim_start_matches([' ', '\t', '>']);
        if rest.starts_with('#') {
            return markers > 0;
        }
    }
}

/// Heading records and reusable facts from the optional container pass.
pub(super) struct HeadingAnalysis {
    pub(super) blockquote_headings: Vec<Option<Box<HeadingInfo>>>,
    /// Valid for the table reading when reference definitions need no special
    /// treatment. Definition-aware table classification keeps its own pass.
    pub(super) lazy_table_text: Option<Vec<bool>>,
}

/// Detect headings and blockquotes (called after HTML block detection)
#[allow(clippy::too_many_arguments)]
pub(super) fn detect_headings_and_blockquotes(
    content_lines: &[&str],
    lines: &mut [LineInfo],
    flavor: MarkdownFlavor,
    html_comment_ranges: &[crate::utils::skip_context::ByteRange],
    html_blocks: &[(usize, usize)],
    code_blocks: &[(usize, usize)],
    code_spans: &[(usize, usize)],
    link_byte_ranges: &[(usize, usize)],
    front_matter_end: usize,
    mdx_flow_lines: Option<&[bool]>,
    mdx_literal_ranges: Option<[&[(usize, usize)]; 2]>,
) -> HeadingAnalysis {
    // Most documents need no link membership queries. Index only when an ATX
    // candidate asks, preserving strict endpoints and overlapping link ranges.
    let link_regions = LazyCell::new(|| SkipRegions::new(link_byte_ranges.to_vec()));
    // Only a `=`/`-` run under a line of text asks what paragraph is open, and
    // most documents hold none, so the pass runs on the first one that does.
    let mut trailing: Option<Pass> = None;
    let literal_bodies = mdx_literal_ranges
        .filter(|ranges| ranges.iter().any(|ranges| !ranges.is_empty()))
        .map(|ranges| mdx_literal_bodies(content_lines, lines, ranges));
    let literal_bodies = literal_bodies.as_deref();

    // The column an open HTML block's opener starts at. A line of the block
    // indented that far is HTML text, so a `>` starting it (the end of a tag
    // broken across lines) opens no blockquote. The body of an element with a
    // `markdown` attribute is Markdown, and keeps its blockquotes.
    let mut html_open_col = 0;

    // The ATX headings written after the marker of a list item inside a
    // blockquote, which are recorded with the headings of the blockquote.
    let mut quoted_item_headings: Vec<(usize, HeadingInfo)> = Vec::new();

    // Detect headings (including Setext which needs look-ahead) and blockquotes
    for i in 0..lines.len() {
        let line = content_lines[i];

        let html_block_body = lines[i].in_html_block && i > 0 && lines[i - 1].in_html_block;
        if lines[i].in_html_block && !html_block_body {
            html_open_col = lines[i].visual_indent;
        }
        let html_text = html_block_body && lines[i].visual_indent >= html_open_col && !lines[i].in_mkdocs_html_markdown;

        let in_front_matter = front_matter_end > 0 && i < front_matter_end;

        // Detect blockquotes FIRST, before any skip conditions.
        if !in_front_matter
            && !html_text
            && let Some(bq) = crate::utils::blockquote::parse_blockquote_prefix(line)
        {
            let nesting_level = bq.nesting_level;
            let marker_column = bq.indent.len();
            let content_leading_ws_len = bq.content.len() - bq.content.trim_start_matches([' ', '\t']).len();
            let full_prefix = format!("{}{}", bq.prefix, &bq.content[..content_leading_ws_len]);
            let normalized_content = &bq.content[content_leading_ws_len..];

            let indent_may_hide_block = prefix_may_hide_block(&full_prefix);

            let has_multiple_spaces = bq.spaces_after_marker.chars().filter(|&c| c == ' ').count() > 1;

            lines[i].blockquote = Some(Box::new(BlockquoteInfo {
                nesting_level,
                marker_column,
                prefix: full_prefix,
                content: normalized_content.to_string(),
                has_multiple_spaces_after_marker: has_multiple_spaces,
            }));

            // Update is_horizontal_rule for blockquote content. A run indented
            // too far past the quote's marker to start a block is text the
            // paragraph above it continues, which only the pass can tell.
            if !lines[i].in_code_block && is_horizontal_rule_content(normalized_content.trim_matches([' ', '\t', '\r']))
            {
                let lazy_text = indent_may_hide_block
                    && trailing
                        .get_or_insert_with(|| {
                            trailing_state(
                                content_lines,
                                lines,
                                flavor,
                                html_blocks,
                                code_blocks,
                                code_spans,
                                mdx_flow_lines,
                                literal_bodies,
                                None,
                            )
                        })
                        .indented_text[i];
                lines[i].is_horizontal_rule = !lazy_text;
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

        if is_opaque_heading_body(i, &lines[i], literal_bodies) {
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

        let atx = if is_snippet_line { None } else { atx_opening(line) };
        let line_offset = lines[i].byte_offset;
        // Only a line opening with an ATX marker needs the range scan.
        let in_html_comment_or_link = atx.is_some()
            && (crate::utils::skip_context::is_in_html_comment_ranges(html_comment_ranges, line_offset)
                || link_regions.contains_interior(line_offset));

        if let Some(AtxOpening::MissingSpace { level }) = atx
            && !in_html_comment_or_link
        {
            // Paragraph text, so it may still be the text of a setext heading
            // below; that heading clears this record when it claims the line.
            lines[i].atx_missing_space = Some(AtxMissingSpace { level });
        }

        if let Some(AtxOpening::Heading(caps)) = atx {
            if in_html_comment_or_link {
                continue;
            }
            let heading = atx_heading_info(&caps, 0, content_lines, lines, i);
            let marker_column = heading.marker_column;

            // An ATX heading cannot be a lazy continuation line, so one written
            // at the left edge is at the top level; any indentation may be a
            // container's or paragraph text, and the pass settles which.
            let container = if marker_column == 0 {
                HeadingContainer::Outside
            } else {
                let pass = trailing.get_or_insert_with(|| {
                    trailing_state(
                        content_lines,
                        lines,
                        flavor,
                        html_blocks,
                        code_blocks,
                        code_spans,
                        mdx_flow_lines,
                        literal_bodies,
                        None,
                    )
                });
                if pass.atx_is_text[i] {
                    continue;
                }
                heading_container_of(pass.containers[i + 1], &lines[i..=i])
            };
            // Inside an item the four-column limit counts from the item's content
            // column: further in, the line is indented code or a paragraph
            // continuation.
            if let HeadingContainer::Item { content_col } = container
                && lines[i].visual_indent >= content_col + 4
            {
                continue;
            }
            lines[i].heading = Some(Box::new(heading));
            lines[i].heading_container = Some(container);
            continue;
        }

        // An ATX heading written after the marker of a list item the line opens
        // (`- # a`). It is the item's own first block, so the pass says which
        // item that is and whether the text is far enough in to be code.
        if atx.is_none() && !is_snippet_line && begins_item_with_hash(line) {
            let pass = trailing.get_or_insert_with(|| {
                trailing_state(
                    content_lines,
                    lines,
                    flavor,
                    html_blocks,
                    code_blocks,
                    code_spans,
                    mdx_flow_lines,
                    literal_bodies,
                    None,
                )
            });
            let state = pass.states[i];
            if state.carries_item
                && !crate::utils::skip_context::is_in_html_comment_ranges(html_comment_ranges, line_offset)
                && !link_regions.contains_interior(line_offset)
                && let Some(AtxOpening::Heading(caps)) = atx_opening(state.text)
            {
                let heading = atx_heading_info(&caps, state.text_start, content_lines, lines, i);
                if state.quote_depth == 0 {
                    lines[i].heading_container = Some(heading_container_of(pass.containers[i + 1], &lines[i..=i]));
                    lines[i].heading = Some(Box::new(heading));
                } else {
                    quoted_item_headings.push((i, heading));
                }
                continue;
            }
        }

        // Check for Setext headings (need to look at next line)
        if i + 1 < content_lines.len() && i + 1 < lines.len() {
            let next_line = content_lines[i + 1];
            if !lines[i + 1].in_code_block && is_setext_underline_content(next_line) {
                if front_matter_end > 0 && i < front_matter_end {
                    continue;
                }

                if crate::utils::skip_context::is_in_html_comment_ranges(html_comment_ranges, lines[i].byte_offset) {
                    continue;
                }

                // Whether the line is paragraph text at all - rather than a list
                // item, a thematic break, an ATX heading, an HTML block or a table
                // row - and whether the run below is written where it can
                // underline that paragraph are one question about the containers
                // and blocks above, and the pass answers it for every line.
                let pass = trailing.get_or_insert_with(|| {
                    trailing_state(
                        content_lines,
                        lines,
                        flavor,
                        html_blocks,
                        code_blocks,
                        code_spans,
                        mdx_flow_lines,
                        literal_bodies,
                        None,
                    )
                });
                let states = &pass.states;
                // The heading is the whole paragraph the underline ends, recorded
                // on the paragraph's last line. Its text starts where its first
                // line's own text does, which is past the marker of the list item
                // that line opens. A footnote label is not modeled: the body it
                // opens is read against margins the pass does not track.
                let Some(first) = states[i + 1].underlines else {
                    continue;
                };
                if states[first].carries_footnote {
                    continue;
                }

                let attribute_id = content_lines
                    .get(i + 2)
                    .filter(|attr_line| {
                        lines.get(i + 2).is_some_and(|attr_info| !attr_info.in_code_block)
                            && crate::utils::header_id_utils::is_standalone_attr_list(attr_line)
                    })
                    .and_then(|attr_line| crate::utils::header_id_utils::extract_standalone_attr_list_id(attr_line));

                let heading = setext_heading_info(
                    &paragraph_text(&states[first..=i]),
                    i + 1 - first,
                    next_line,
                    next_line.len() - trim_setext_underline_start(next_line).len(),
                    if states[first].carries_item {
                        states[first].text_start
                    } else {
                        lines[first].indent
                    },
                    attribute_id,
                );
                // The paragraph's container is the one open when the underline is
                // read, the same for every text line however lazy, and for the
                // underline itself.
                let container = heading_container_of(pass.containers[i + 1], &lines[first..=i + 1]);
                for text_line in &mut lines[first..=i] {
                    text_line.is_setext_heading_text = true;
                    // A `#` opening the text is heading text, not an ATX marker.
                    text_line.atx_missing_space = None;
                    text_line.heading_container = Some(container);
                }
                lines[i + 1].heading_container = Some(container);
                lines[i].heading = Some(Box::new(heading));
            }
        }
    }

    let mut blockquote_headings: Vec<Option<Box<HeadingInfo>>> = lines
        .iter()
        .enumerate()
        .map(|(line_index, line)| {
            detect_blockquote_atx_heading(
                line_index,
                line,
                flavor,
                html_comment_ranges,
                front_matter_end,
                literal_bodies,
            )
        })
        .collect();

    // Written four columns past the quote's edge under a running paragraph, an
    // ATX shape is text that paragraph continues, as it is at the top level. So
    // is a line that only looks like a nested quote, its marker that far in.
    for (line_index, heading) in blockquote_headings.iter_mut().enumerate() {
        if heading.is_none() {
            continue;
        }
        let pass = trailing.get_or_insert_with(|| {
            trailing_state(
                content_lines,
                lines,
                flavor,
                html_blocks,
                code_blocks,
                code_spans,
                mdx_flow_lines,
                literal_bodies,
                None,
            )
        });
        if pass.atx_is_text[line_index] || pass.indented_text[line_index] {
            *heading = None;
        }
    }

    for (line_index, heading) in quoted_item_headings {
        if blockquote_headings[line_index].is_none() {
            blockquote_headings[line_index] = Some(Box::new(heading));
        }
    }

    // A setext heading inside a blockquote. Its underline repeats the `>`, so it
    // is found only once every line's blockquote is known.
    for underline_index in 1..lines.len() {
        let text_index = underline_index - 1;
        if blockquote_headings[text_index].is_some() || lines[underline_index].in_code_block {
            continue;
        }
        let Some(underline) = lines[underline_index].blockquote.as_deref() else {
            continue;
        };
        // The underline is the whole of the line's text after its last `>`.
        if !is_setext_underline_content(&underline.content) {
            continue;
        }
        let states = &trailing
            .get_or_insert_with(|| {
                trailing_state(
                    content_lines,
                    lines,
                    flavor,
                    html_blocks,
                    code_blocks,
                    code_spans,
                    mdx_flow_lines,
                    literal_bodies,
                    None,
                )
            })
            .states;
        let (underline_text, underline_column) = (underline.content.as_str(), underline.prefix.len());
        // The heading is the whole paragraph the underline ends, recorded on the
        // paragraph's last line, and starts its text at its first line's own
        // text, past the `>` and the marker of the list item that line opens. A
        // lazy continuation line carries fewer `>` than the paragraph it
        // continues sits in, and a heading is reported at the depth its line
        // carries, so the text has to be written at the paragraph's own depth.
        let Some(first) = states[underline_index].underlines else {
            continue;
        };
        if states[first].carries_footnote {
            continue;
        }
        // A list item opened on the first line may hold the blockquote
        // (`- > text`), which then has no `>` at the line's start for the
        // blockquote record to describe.
        let quoted_item = states[first].carries_item;
        let line_allowed = |index: usize| {
            quote_heading_line_allowed(
                index,
                &lines[index],
                html_comment_ranges,
                front_matter_end,
                literal_bodies,
            )
        };
        let text_quote = blockquote_heading_container(
            text_index,
            &lines[text_index],
            flavor,
            html_comment_ranges,
            front_matter_end,
            literal_bodies,
        );
        if text_quote.is_none() && !(quoted_item && text_index == first && line_allowed(text_index)) {
            continue;
        }
        // The text line must write every `>` of the blockquotes holding the
        // paragraph, which a lazy continuation line does not.
        let quotes_written = content_lines[text_index][..states[text_index].text_start]
            .bytes()
            .filter(|&byte| byte == b'>')
            .count();
        if quotes_written != states[text_index].quote_depth {
            continue;
        }
        let content_column = if quoted_item {
            if !line_allowed(first) {
                continue;
            }
            states[first].text_start
        } else {
            let Some(first_quote) = blockquote_heading_container(
                first,
                &lines[first],
                flavor,
                html_comment_ranges,
                front_matter_end,
                literal_bodies,
            ) else {
                continue;
            };
            first_quote.prefix.len()
        };
        blockquote_headings[text_index] = Some(Box::new(setext_heading_info(
            &paragraph_text(&states[first..=text_index]),
            text_index + 1 - first,
            underline_text,
            underline_column,
            content_column,
            None,
        )));
    }

    // The items each list item line opens, as CommonMark places them.
    if lines.iter().any(|line| line.list_item.is_some()) {
        let pass = trailing.get_or_insert_with(|| {
            trailing_state(
                content_lines,
                lines,
                flavor,
                html_blocks,
                code_blocks,
                code_spans,
                mdx_flow_lines,
                literal_bodies,
                None,
            )
        });
        let chains = std::mem::take(&mut pass.item_chains);
        for (line, chain) in lines.iter_mut().zip(chains) {
            if let Some(item) = line.list_item.as_deref_mut() {
                item.commonmark_items = chain;
            }
        }
    }

    HeadingAnalysis {
        blockquote_headings,
        lazy_table_text: trailing.map(|pass| pass.lazy_table_text),
    }
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

    // CommonMark 4.2: only spaces and tabs separate the closing sequence from the
    // text and from the end of the line; any other whitespace is text.
    let trimmed_rest = rest_without_id.trim_end_matches([' ', '\t']);
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
        && (char_idx == 0 || matches!(char_positions[char_idx - 1].1, ' ' | '\t'));
    if !is_closing {
        return (rest.to_string(), false, String::new());
    }

    let text = if custom_id_part.is_empty() {
        trimmed_rest[..start_of_hashes]
            .trim_end_matches([' ', '\t'])
            .to_string()
    } else {
        format!(
            "{}{}",
            trimmed_rest[..start_of_hashes].trim_end_matches([' ', '\t']),
            custom_id_part
        )
    };
    (text, true, potential_closing.to_string())
}

/// Read the display text of an already-confirmed root ATX heading with the
/// same source helpers used by `atx_heading_info`. Source proofs must inspect
/// this text too: an anchor or custom ID can hide punctuation at the end.
pub(crate) fn atx_display_text_for_proof(line: &str) -> Option<String> {
    let AtxOpening::Heading(marker) = atx_opening(line)? else {
        return None;
    };
    let (text, _, _) = parse_atx_remainder(marker.rest);
    Some(crate::utils::header_id_utils::extract_heading_text(text.trim()).text)
}

/// Whether a line is written where a heading inside a blockquote can be: not in
/// a code block, raw HTML, front matter, a comment or a body that is not
/// Markdown.
fn quote_heading_line_allowed(
    line_index: usize,
    line: &LineInfo,
    html_comment_ranges: &[crate::utils::skip_context::ByteRange],
    front_matter_end: usize,
    mdx_literal_bodies: Option<&[bool]>,
) -> bool {
    !(line.in_code_block
        || (line.in_html_block && !line.in_mkdocs_html_markdown)
        || line.in_kramdown_extension_block
        || is_opaque_heading_body(line_index, line, mdx_literal_bodies)
        || (front_matter_end > 0 && line_index < front_matter_end)
        || crate::utils::skip_context::is_in_html_comment_ranges(html_comment_ranges, line.byte_offset))
}

/// The blockquote a line sits in, when a heading written there renders as one.
fn blockquote_heading_container<'a>(
    line_index: usize,
    line: &'a LineInfo,
    flavor: MarkdownFlavor,
    html_comment_ranges: &[crate::utils::skip_context::ByteRange],
    front_matter_end: usize,
    mdx_literal_bodies: Option<&[bool]>,
) -> Option<&'a BlockquoteInfo> {
    if !quote_heading_line_allowed(
        line_index,
        line,
        html_comment_ranges,
        front_matter_end,
        mdx_literal_bodies,
    ) {
        return None;
    }
    let blockquote = line.blockquote.as_deref()?;
    let content = blockquote.content.as_str();
    if flavor == MarkdownFlavor::MkDocs
        && (crate::utils::mkdocs_snippets::is_snippet_section_start(content)
            || crate::utils::mkdocs_snippets::is_snippet_section_end(content))
    {
        return None;
    }
    Some(blockquote)
}

fn detect_blockquote_atx_heading(
    line_index: usize,
    line: &LineInfo,
    flavor: MarkdownFlavor,
    html_comment_ranges: &[crate::utils::skip_context::ByteRange],
    front_matter_end: usize,
    mdx_literal_bodies: Option<&[bool]>,
) -> Option<Box<HeadingInfo>> {
    let blockquote = blockquote_heading_container(
        line_index,
        line,
        flavor,
        html_comment_ranges,
        front_matter_end,
        mdx_literal_bodies,
    )?;
    let content = blockquote.content.as_str();

    let marker_len = content.bytes().take_while(|&byte| byte == b'#').count();
    if !(1..=6).contains(&marker_len) {
        return None;
    }
    let after_marker = &content[marker_len..];
    let spaces_len = after_marker
        .bytes()
        .take_while(|&byte| matches!(byte, b' ' | b'\t'))
        .count();
    // CommonMark 4.2: the `#`s are followed by a space, a tab or the end of the line
    if spaces_len == 0 && !after_marker.is_empty() {
        return None;
    }

    let rest = &after_marker[spaces_len..];
    let (text, has_closing_sequence, closing_sequence) = parse_atx_remainder(rest);
    let raw_text = text.trim_matches([' ', '\t']).to_string();
    let heading_text = crate::utils::header_id_utils::extract_heading_text(raw_text.trim());
    Some(Box::new(HeadingInfo {
        level: marker_len as u8,
        style: HeadingStyle::ATX,
        marker: content[..marker_len].to_string(),
        marker_column: blockquote.prefix.len(),
        content_column: blockquote.prefix.len() + marker_len + spaces_len,
        text: heading_text.text,
        slug_text: heading_text.slug_text,
        custom_id: heading_text.custom_id,
        raw_text,
        text_lines: 1,
        has_closing_sequence,
        closing_sequence,
    }))
}

/// Whether an HTML block the CommonMark parser opened on this line is really
/// the text of a block that is not Markdown. The parser reads front matter,
/// code of a flavor's own fences and opaque bodies such as `%%` comments as
/// Markdown, so a tag written in one of them opens no block over the lines it
/// runs on into.
fn parser_html_block_is_text(opener: &LineInfo) -> bool {
    opener.in_front_matter || opener.in_code_block || is_opaque_body(opener)
}

/// Mark the HTML blocks CommonMark's parser found that `detect_html_blocks`
/// does not recognise: a complete tag alone on a line (start condition 7,
/// `<math display="block">`, `<span>`) and block-level names outside
/// [`crate::utils::html_block::BLOCK_ELEMENTS`]. `blocks` are the byte ranges of
/// pulldown-cmark's HTML blocks.
///
/// Runs once every non-Markdown body is marked, which is what
/// [`parser_html_block_is_text`] reads. Comments, processing instructions and
/// declarations are tracked as comments elsewhere, MDX reads a tag alone on a
/// line as JSX, and a block opened after a container marker would hide that
/// marker's line from the rules, so none of those is marked. A block opened inside
/// a quote is marked below its opening line.
pub(super) fn mark_parser_html_blocks(
    content: &str,
    lines: &mut [LineInfo],
    blocks: &[(usize, usize)],
    flavor: MarkdownFlavor,
) {
    if flavor.supports_jsx() {
        return;
    }
    for &(start, end) in blocks {
        let spanned = spanned_lines(lines, start, end);
        let Some(opener) = lines.get(spanned.start) else {
            continue;
        };
        let before_tag = &content[opener.byte_offset..start];
        if opener.in_html_block
            || parser_html_block_is_text(opener)
            || content[start..].starts_with("<!")
            || content[start..].starts_with("<?")
        {
            continue;
        }
        let mut spanned = spanned;
        if !before_tag.trim().is_empty() {
            // A block opened inside a quote (which may itself sit in list items) keeps
            // its opening line visible to the rules, which read the quote marker there;
            // the lines below it are raw HTML. Any other container marker leaves the
            // whole block unmarked.
            if !before_tag.contains('>')
                || !before_tag
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '>' | ' ' | '\t' | '-' | '+' | '*' | '.' | ')'))
            {
                continue;
            }
            spanned.start += 1;
        }
        for line in &mut lines[spanned] {
            line.in_html_block = true;
            // A line holding only `>` is HTML text here, not an empty quote line.
            line.is_blank &= line.content(content).trim().is_empty();
        }
    }
}

/// Every HTML block CommonMark's parser found, as half-open source byte ranges
/// in document order. Unlike the `in_html_block` flag this includes
/// comments, processing instructions, declarations and blocks opened after a
/// container marker, so it answers whether a line is HTML rather than
/// paragraph text. A block the flavor reads as text or JSX is left out, as in
/// [`mark_parser_html_blocks`].
pub(super) fn parser_html_blocks(
    lines: &[LineInfo],
    blocks: &[(usize, usize)],
    flavor: MarkdownFlavor,
) -> (Vec<(usize, usize)>, Vec<std::ops::Range<usize>>) {
    let mut ranges = Vec::new();
    let mut line_runs = Vec::new();
    if !flavor.supports_jsx() {
        for &(start, end) in blocks {
            let spanned = spanned_lines(lines, start, end);
            if lines
                .get(spanned.start)
                .is_some_and(|opener| !parser_html_block_is_text(opener))
            {
                ranges.push((start, end));
                line_runs.push(spanned);
            }
        }
    }
    debug_assert!(ranges.windows(2).all(|pair| pair[0].1 <= pair[1].0));
    (ranges, line_runs)
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

        // A closing tag opens a block too, one that runs to a blank line like
        // every block a Type-1 start tag does not open.
        let closing_tag = format!("</{tag_name}>");
        let allow_blank_lines = !is_closing && TYPE_1_BLOCK_ELEMENTS.contains(&tag_name.as_str());

        // Only a Type-1 block ends at its end tag; any other block runs on to
        // the first blank line, also when its opening line closes the tag.
        if allow_blank_lines && lines[i].content(content).contains(&closing_tag) {
            i += 1;
            continue;
        }
        // A line indented to the opener's column belongs to the block, so a `>`
        // starting it is HTML text (the end of a tag broken across lines), not a
        // blockquote marker, and only an empty line ends the block. A line
        // indented less may have left the container the block sits in.
        let open_col = lines[i].visual_indent;
        let in_block_text =
            |line: &LineInfo| line.visual_indent >= open_col && !line.content(content).trim().is_empty();
        let ends_block = |line: &LineInfo| !in_block_text(line) && line.is_blank;
        let mut j = i + 1;
        let mut found_closing_tag = false;
        while j < lines.len() {
            if !allow_blank_lines && ends_block(&lines[j]) {
                break;
            }

            lines[j].in_html_block = true;
            if in_block_text(&lines[j]) {
                lines[j].is_blank = false;
            }

            if lines[j].content(content).contains(&closing_tag) {
                found_closing_tag = true;
            }

            if found_closing_tag {
                j += 1;
                while j < lines.len() {
                    if ends_block(&lines[j]) {
                        break;
                    }
                    lines[j].in_html_block = true;
                    if in_block_text(&lines[j]) {
                        lines[j].is_blank = false;
                    }
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

#[cfg(test)]
mod marker_line_tests {
    use super::begins_item_with_hash;

    #[test]
    fn begins_item_with_hash_accepts_only_marker_chains_ending_in_a_hash() {
        for line in [
            "- # a", "1. ## a", "- - # a", "- > # a", "> - # a", "1) # a", "-\t# a", "- #",
        ] {
            assert!(begins_item_with_hash(line), "{line:?}");
        }
        for line in [
            "# a", "> # a", "- a # b", "-# a", "1.# a", "text # a", "a - # b", "- ", "",
        ] {
            assert!(!begins_item_with_hash(line), "{line:?}");
        }
    }
}

#[cfg(test)]
mod marker_tests {
    use super::*;
    use proptest::prelude::*;

    fn regex_atx(line: &str) -> Option<AtxOpening<'_>> {
        static PATTERN: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"^([ \t]*)(#{1,6})([ \t]*)(.*)$").unwrap());
        let caps = PATTERN.captures(line)?;
        let hashes = caps.get(2).unwrap();
        let after = &line[hashes.end()..];
        if after.is_empty() || after.starts_with([' ', '\t']) {
            Some(AtxOpening::Heading(AtxMarker {
                indent: caps[1].len(),
                hashes: hashes.as_str(),
                whitespace: caps[3].len(),
                rest: caps.get(4).unwrap().as_str(),
            }))
        } else if after.starts_with('#') {
            None
        } else {
            Some(AtxOpening::MissingSpace {
                level: hashes.len() as u8,
            })
        }
    }

    fn regex_item(line: &str, interrupting: bool) -> Option<OpenedItem> {
        static BULLET: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"^([ \t]*)([*+-])([ \t]+|$)").unwrap());
        static ORDERED: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"^([ \t]*)([0-9]+)([.)])([ \t]+|$)").unwrap());
        let (marker_end, text_start) = if let Some(caps) = BULLET.captures(line) {
            (caps.get(2)?.end(), caps.get(0)?.end())
        } else {
            let caps = ORDERED.captures(line)?;
            let number = caps.get(2)?.as_str();
            if number.len() > 9 || (interrupting && number.trim_start_matches('0') != "1") {
                return None;
            }
            (caps.get(3)?.end(), caps.get(0)?.end())
        };
        if interrupting && is_cm_blank(&line[text_start..]) {
            return None;
        }
        Some(OpenedItem { text_start, marker_end })
    }

    proptest! {
        // The scanners consume individual source lines. Unicode whitespace is
        // text here; only ASCII spaces and tabs belong to a marker.
        #[test]
        fn atx_scanner_matches_capture_reading(line in "[ \\t]{0,5}#{0,10}[ \\t]{0,5}[^\\n]{0,40}") {
            prop_assert_eq!(atx_opening(&line), regex_atx(&line));
        }

        #[test]
        fn item_scanner_matches_capture_reading(
            line in "[ \\t]{0,5}([*+\\-]|[0-9]{1,12}[.)]?)[ \\t]{0,5}[^\\n]{0,40}",
            interrupting in any::<bool>(),
        ) {
            prop_assert_eq!(list_item_content_column(&line, interrupting), regex_item(&line, interrupting));
        }
    }
}
