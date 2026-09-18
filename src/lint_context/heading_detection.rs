use crate::config::MarkdownFlavor;
use crate::utils::code_block_utils::CodeBlockUtils;
use crate::utils::regex_cache::{ORDERED_LIST_MARKER_REGEX, UNORDERED_LIST_MARKER_REGEX};
use crate::utils::table_utils::TableUtils;
use std::sync::LazyLock;

use super::line_computation::spanned_lines;
use super::list_blocks::column_at;
use super::types::*;

static ATX_HEADING_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^(\s*)(#{1,6})(\s*)(.*)$").unwrap());

/// The label opening a footnote definition, `[^id]:`. It is read only on lines
/// the parser places inside a definition, which settles what the label may hold.
static FOOTNOTE_LABEL_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[ \t]*\[\^[^\]]+\]:").unwrap());

/// CommonMark 5.2: an ordered list marker is "one to nine digits". A longer run
/// is ordinary paragraph text, and the shared marker regex does not say so.
const MAX_ORDERED_MARKER_DIGITS: usize = 9;

/// The column a list item's content starts at. A continuation line that does not
/// reach it is lazy.
///
/// `interrupting` says a paragraph flows into this line from the one above.
/// CommonMark 5.2 lets a list interrupt a paragraph only when its item does not
/// start with a blank line, and an ordered one only when it starts at 1. So `2.`
/// or a bare `*` written under prose is part of the sentence rather than a
/// marker, a bare `-` is the underline of that prose, and the line opens no item
/// at all.
fn list_item_content_column(line: &str, interrupting: bool) -> Option<usize> {
    let end = match UNORDERED_LIST_MARKER_REGEX.find(line) {
        Some(marker) => marker.end(),
        None => {
            let marker = ORDERED_LIST_MARKER_REGEX.captures(line)?;
            let number = marker.get(2)?.as_str();
            if number.len() > MAX_ORDERED_MARKER_DIGITS {
                return None;
            }
            if interrupting && number.trim_start_matches('0') != "1" {
                return None;
            }
            marker.get(0)?.end()
        }
    };
    if interrupting && line[end..].trim().is_empty() {
        return None;
    }
    Some(end)
}

/// CommonMark 4.3: a setext underline "can be indented up to three spaces" past
/// the edge of the container it is written in. One more makes it paragraph text.
const MAX_SETEXT_UNDERLINE_INDENT: usize = 3;

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
    Item(usize),
    Footnote(usize),
}

/// One blockquote marker opening `line`: the text from its `>` on, and the text
/// after the `>` and the single space or tab that belongs to it, which is how
/// `parse_blockquote_prefix` splits a level.
fn strip_quote_marker(line: &str) -> Option<(&str, &str)> {
    let marker = line.trim_start_matches([' ', '\t']);
    let after_marker = marker.strip_prefix('>')?;
    Some((marker, after_marker.strip_prefix([' ', '\t']).unwrap_or(after_marker)))
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
/// Every position is a column, with a tab reaching the next multiple of four,
/// since that is how CommonMark measures both re-entry and indentation.
fn enter<'a>(
    line: &'a str,
    open: &[Marker],
    paragraph: bool,
    in_footnote_definition: bool,
    opened: &mut Vec<Marker>,
) -> Entered<'a> {
    opened.clear();
    // The column the tail of the line starting at `slice` sits on.
    let column = |slice: &str| column_at(line, line.len() - slice.len());
    // The column a blockquote's content starts at. The `>` takes one column of
    // the space or tab after it, so a tab reaching further leaves its remaining
    // columns as indentation of the content.
    let quote_edge = |marker: &str, content: &str| column(content).min(column(marker) + 2);
    let mut rest = line;
    let mut matched = 0;
    // The column the innermost container entered so far starts its content at.
    let mut edge = 0;
    while matched < open.len() {
        match open[matched] {
            Marker::Quote => match strip_quote_marker(rest) {
                Some((marker, content)) => {
                    edge = quote_edge(marker, content);
                    rest = content;
                }
                None => break,
            },
            // Nothing is consumed here: the body is re-entered by indentation,
            // which is where the next marker or the text itself begins.
            Marker::Item(content_column) | Marker::Footnote(content_column)
                if column(rest.trim_start()) >= content_column =>
            {
                edge = content_column;
            }
            Marker::Item(_) | Marker::Footnote(_) => break,
        }
        matched += 1;
    }
    loop {
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
            .any(|marker| matches!(marker, Marker::Footnote(_)));
        if in_footnote_definition
            && !in_footnote_body
            && let Some(label) = FOOTNOTE_LABEL_REGEX.find(rest)
        {
            opened.push(Marker::Footnote(edge + FOOTNOTE_BODY_INDENT));
            rest = &rest[label.end()..];
            edge = column(rest);
            continue;
        }
        let interrupting = paragraph && matched == open.len() && opened.is_empty();
        match list_item_content_column(rest, interrupting) {
            Some(end) => {
                rest = &rest[end..];
                // Where the marker left off: the column a continuation of this
                // item's content has to reach.
                edge = column(rest);
                opened.push(Marker::Item(edge));
            }
            None => break,
        }
    }
    Entered {
        matched,
        content: rest.trim(),
        indent: column(rest.trim_start()).saturating_sub(edge),
    }
}

/// Whether the text a line holds is paragraph text, so that a paragraph running
/// into the line runs on out of it.
///
/// Blank lines and code fences end a paragraph too; the pass settles those before
/// asking, a blank line from the containers the line re-entered and a fence from
/// the line flags. The
/// ATX test is the same regex the heading detection below runs, so the two agree
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
        || ATX_HEADING_REGEX.is_match(content)
        || crate::utils::html_block::parse_html_block_start(content).is_some()
        || crate::utils::html_block::opens_untagged_html_block(content))
}

/// Whether a line is paragraph text: the shape a setext underline needs above
/// it, and the shape that keeps a container's paragraph open below it.
pub(crate) fn is_paragraph_text_line(line: &str) -> bool {
    may_hold_open_paragraph(enter(line, &[], false, false, &mut Vec::new()).content)
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
fn structural_blocks(line: &LineInfo, flavor: MarkdownFlavor, in_mdx_flow: bool) -> [bool; 17] {
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
    line.in_math_block || line.in_obsidian_comment || line.in_mdx_comment || line.in_esm_block || line.in_mkdocstrings
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
    /// Whether the line opens a list item or a footnote definition, so that
    /// its text sits inside the body the marker opens.
    carries_marker: bool,
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
fn trailing_state<'a>(
    content_lines: &[&'a str],
    lines: &[LineInfo],
    flavor: MarkdownFlavor,
    html_blocks: &[(usize, usize)],
    code_blocks: &[(usize, usize)],
    code_spans: &[(usize, usize)],
    mdx_flow_lines: Option<&[bool]>,
) -> Vec<Trailing<'a>> {
    let blocks = |index: usize| {
        let in_mdx_flow = mdx_flow_lines.map_or(lines[index].in_jsx_block, |flow| flow[index]);
        structural_blocks(&lines[index], flavor, in_mdx_flow)
    };
    let mut states = Vec::with_capacity(lines.len());
    // The container the lines read so far left open, outermost first, and a
    // scratch buffer for the containers each line opens of its own.
    let mut open: Vec<Marker> = Vec::new();
    let mut opened: Vec<Marker> = Vec::new();
    // The paragraph the lines read so far left open, as the index of its first
    // line.
    let mut paragraph: Option<usize> = None;
    let mut in_table = false;
    let mut header_cells = None;
    // MDX reads a tag as JSX, whose lines hold markdown, so only the other
    // flavors take the lines of the parser's HTML blocks for raw HTML. The
    // CommonMark parser reads front matter, code of a flavor's own fences and
    // opaque bodies such as `%%` comments as Markdown, so a block it opens on
    // one of their lines is their text, and the lines it runs on into are not
    // HTML.
    let mut in_html_block = vec![false; lines.len()];
    // The line each code block and HTML block starts on, which opens a block of
    // its own even where the line above it closes another.
    let mut opens_raw_block = vec![false; lines.len()];
    if flavor != MarkdownFlavor::MDX {
        for &(start, end) in html_blocks {
            let spanned = spanned_lines(lines, start, end);
            if lines
                .get(spanned.start)
                .is_none_or(|line| line.in_front_matter || line.in_code_block || is_opaque_body(line))
            {
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
            || is_opaque_body(line)
    };

    for index in 0..lines.len() {
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
            &mut opened,
        );
        if opened.is_empty() && entered.content.trim().is_empty() {
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
        let carries_marker = opened
            .iter()
            .any(|marker| matches!(marker, Marker::Item(_) | Marker::Footnote(_)));
        // A line of a code block or an HTML block is code or raw HTML however it
        // reads, and its text cannot say so alone: `    x` is indented code and
        // `<span>` opens a block only where no paragraph runs into it, and either
        // block runs on through the lines below. A container's marker is
        // structure the same way: `!!! note` opens the admonition whose body is
        // the lines below it, and holds no paragraph of its own for them to
        // continue.
        let holds_paragraph = !lines[index].in_code_block
            && !in_html_block[index]
            && !lines[index].is_container_marker
            && may_hold_open_paragraph(entered.content);
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
                carries_marker,
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
                carries_marker,
                hard_break,
            });
            continue;
        }

        // Whether the paragraph running into this line runs on out of it, as its
        // own text or as a lazy continuation. A line that enters a container of
        // its own starts a paragraph there instead.
        let continues = paragraph.is_some() && holds_paragraph && opened.is_empty();
        // Where the paragraph leaving this line hangs off. Only a line continuing
        // the paragraph running into it leaves the container where it was, which
        // is what makes its own reading the lazy one; every other line is written
        // where it re-entered, so the containers it did not re-enter close and
        // the ones it opened take their place, and the paragraph leaving the line
        // is the line's own, where it holds one. A line already inside the whole
        // of the open container re-seats it onto itself, so it needs no test here.
        if !continues {
            open.truncate(entered.matched);
            open.extend_from_slice(&opened);
            paragraph = holds_paragraph.then_some(index);
        }
        // The header row of a table: a line that starts its own paragraph and
        // has cells for a delimiter row below to match.
        header_cells =
            (!continues && holds_paragraph && TableUtils::is_potential_table_row_with_flavor(entered.content, flavor))
                .then(|| TableUtils::count_cells_with_flavor(entered.content, flavor));
        states.push(Trailing {
            underlines: None,
            text: entered.content,
            quote_depth: open.iter().filter(|marker| **marker == Marker::Quote).count(),
            carries_marker,
            hard_break,
        });
    }
    states
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
    let underline = underline.trim();
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
        is_valid: true,
    }
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

        if is_opaque_body(&lines[i]) {
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
            let heading_text = crate::utils::header_id_utils::extract_heading_text(&raw_text);
            let mut custom_id = heading_text.custom_id;

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
                text: heading_text.text,
                slug_text: heading_text.slug_text,
                custom_id,
                raw_text,
                text_lines: 1,
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

                // Whether the line is paragraph text at all - rather than a list
                // item, a thematic break, an ATX heading, an HTML block or a table
                // row - and whether the run below is written where it can
                // underline that paragraph are one question about the containers
                // and blocks above, and the pass answers it for every line.
                let states = trailing.get_or_insert_with(|| {
                    trailing_state(
                        content_lines,
                        lines,
                        flavor,
                        html_blocks,
                        code_blocks,
                        code_spans,
                        mdx_flow_lines,
                    )
                });
                // The heading is the whole paragraph the underline ends, recorded
                // on the paragraph's last line. A heading starts its text at its
                // first line's own left edge, so a first line carrying a list
                // marker or a footnote label, whose heading sits inside the body
                // it opens, records none, the same as `- # heading`.
                let Some(first) = states[i + 1].underlines else {
                    continue;
                };
                if states[first].carries_marker {
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
                    next_line.len() - next_line.trim_start().len(),
                    lines[first].indent,
                    attribute_id,
                );
                for text_line in &mut lines[first..=i] {
                    text_line.is_setext_heading_text = true;
                }
                lines[i].heading = Some(Box::new(heading));
            }
        }
    }

    let mut blockquote_headings: Vec<Option<Box<HeadingInfo>>> = lines
        .iter()
        .enumerate()
        .map(|(line_index, line)| {
            detect_blockquote_atx_heading(line_index, line, flavor, html_comment_ranges, front_matter_end)
        })
        .collect();

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
        if !is_setext_underline_content(&underline.content) {
            continue;
        }
        let Some(quote) = blockquote_heading_container(
            text_index,
            &lines[text_index],
            flavor,
            html_comment_ranges,
            front_matter_end,
        ) else {
            continue;
        };
        let states = trailing.get_or_insert_with(|| {
            trailing_state(
                content_lines,
                lines,
                flavor,
                html_blocks,
                code_blocks,
                code_spans,
                mdx_flow_lines,
            )
        });
        // The heading is the whole paragraph the underline ends, recorded on the
        // paragraph's last line, and starts its text at its first line's own
        // left edge, past the `>`. A lazy continuation line carries fewer `>`
        // than the paragraph it continues sits in, and a heading is reported at
        // the depth its line carries, so the text has to be written at the
        // paragraph's own depth.
        let Some(first) = states[underline_index].underlines else {
            continue;
        };
        if states[first].carries_marker || states[text_index].quote_depth != quote.nesting_level {
            continue;
        }
        let Some(first_quote) =
            blockquote_heading_container(first, &lines[first], flavor, html_comment_ranges, front_matter_end)
        else {
            continue;
        };
        blockquote_headings[text_index] = Some(Box::new(setext_heading_info(
            &paragraph_text(&states[first..=text_index]),
            text_index + 1 - first,
            &underline.content,
            underline.prefix.len(),
            first_quote.prefix.len(),
            None,
        )));
    }

    blockquote_headings
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

/// The blockquote a line sits in, when a heading written there renders as one.
fn blockquote_heading_container<'a>(
    line_index: usize,
    line: &'a LineInfo,
    flavor: MarkdownFlavor,
    html_comment_ranges: &[crate::utils::skip_context::ByteRange],
    front_matter_end: usize,
) -> Option<&'a BlockquoteInfo> {
    if line.in_code_block
        || (line.in_html_block && !line.in_mkdocs_html_markdown)
        || line.in_kramdown_extension_block
        || is_opaque_body(line)
        || (front_matter_end > 0 && line_index < front_matter_end)
        || crate::utils::skip_context::is_in_html_comment_ranges(html_comment_ranges, line.byte_offset)
    {
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
) -> Option<Box<HeadingInfo>> {
    let blockquote = blockquote_heading_container(line_index, line, flavor, html_comment_ranges, front_matter_end)?;
    let content = blockquote.content.as_str();

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
    let heading_text = crate::utils::header_id_utils::extract_heading_text(&raw_text);
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
