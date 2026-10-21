//! GFM: table occurs in the [flow][] content type.
//!
//! ## Grammar
//!
//! Tables form with the following BNF
//! (<small>see [construct][crate::mdx_parser::construct] for character groups</small>):
//!
//! ```bnf
//! gfm_table ::= gfm_table_head 0*(eol gfm_table_body_row)
//!
//! ; Restriction: both rows must have the same number of cells.
//! gfm_table_head ::= gfm_table_row eol gfm_table_delimiter_row
//!
//! gfm_table_row ::= ['|'] gfm_table_cell 0*('|' gfm_table_cell) ['|'] *space_or_tab
//! gfm_table_cell ::= *space_or_tab gfm_table_text *space_or_tab
//! gfm_table_text ::= 0*(line - '\\' - '|' | '\\' ['\\' | '|'])
//
//! gfm_table_delimiter_row ::= ['|'] gfm_table_delimiter_cell 0*('|' gfm_table_delimiter_cell) ['|'] *space_or_tab
//! gfm_table_delimiter_cell ::= *space_or_tab gfm_table_delimiter_value *space_or_tab
//! gfm_table_delimiter_value ::= [':'] 1*'-' [':']
//! ```
//!
//! As this construct occurs in flow, like all flow constructs, it must be
//! followed by an eol (line ending) or eof (end of file).
//!
//! The above grammar shows that basically anything can be a cell or a row.
//! The main thing that makes something a row, is that it occurs directly before
//! or after a delimiter row, or after another row.
//!
//! It is not required for a table to have a body: it can end right after the
//! delimiter row.
//!
//! Each column can be marked with an alignment.
//! The alignment marker is a colon (`:`) used before and/or after delimiter row
//! filler.
//! To illustrate:
//!
//! ```markdown
//! | none | left | right | center |
//! | ---- | :--- | ----: | :----: |
//! ```
//!
//! The number of cells in the delimiter row, is the number of columns of the
//! table.
//! Only the head row is required to have the same number of cells.
//! Body rows are not required to have a certain number of cells.
//! For body rows that have less cells than the number of columns of the table,
//! empty cells are injected.
//! When a row has more cells than the number of columns of the table, the
//! superfluous cells are dropped.
//! To illustrate:
//!
//! ```markdown
//! | a | b |
//! | - | - |
//! | c |
//! | d | e | f |
//! ```
//!
//! Yields:
//!
//! ```html
//! <table>
//! <thead>
//! <tr>
//! <th>a</th>
//! <th>b</th>
//! </tr>
//! </thead>
//! <tbody>
//! <tr>
//! <td>c</td>
//! <td></td>
//! </tr>
//! <tr>
//! <td>d</td>
//! <td>e</td>
//! </tr>
//! </tbody>
//! </table>
//! ```
//!
//! Each cell’s text is interpreted as the [text][] content type.
//! That means that it can include constructs such as [attention][attention].
//!
//! The grammar for cells prohibits the use of `|` in them.
//! To use pipes in cells, encode them as a character reference or character
//! escape: `&vert;` (or `&VerticalLine;`, `&verbar;`, `&#124;`, `&#x7c;`) or
//! `\|`.
//!
//! Escapes will typically work, but they are not supported in
//! [code (text)][raw_text] (and the math (text) extension).
//! To work around this, GitHub came up with a rather weird “trick”.
//! When inside a table cell *and* inside code, escaped pipes *are* decoded.
//! To illustrate:
//!
//! ```markdown
//! | Name | Character |
//! | - | - |
//! | Left curly brace | `{` |
//! | Pipe | `\|` |
//! | Right curly brace | `}` |
//! ```
//!
//! Yields:
//!
//! ```html
//! <table>
//! <thead>
//! <tr>
//! <th>Name</th>
//! <th>Character</th>
//! </tr>
//! </thead>
//! <tbody>
//! <tr>
//! <td>Left curly brace</td>
//! <td><code>{</code></td>
//! </tr>
//! <tr>
//! <td>Pipe</td>
//! <td><code>|</code></td>
//! </tr>
//! <tr>
//! <td>Right curly brace</td>
//! <td><code>}</code></td>
//! </tr>
//! </tbody>
//! </table>
//! ```
//!
//! > 👉 **Note**: no other character can be escaped like this.
//! > Escaping pipes in code does not work when not inside a table, either.
//!
//! ## HTML
//!
//! GFM tables relate to several HTML elements: `<table>`, `<tbody>`, `<td>`,
//! `<th>`, `<thead>`, and `<tr>`.
//! See
//! [*§ 4.9.1 The `table` element*][html_table],
//! [*§ 4.9.5 The `tbody` element*][html_tbody],
//! [*§ 4.9.9 The `td` element*][html_td],
//! [*§ 4.9.10 The `th` element*][html_th],
//! [*§ 4.9.6 The `thead` element*][html_thead], and
//! [*§ 4.9.8 The `tr` element*][html_tr]
//! in the HTML spec for more info.
//!
//! If the alignment of a column is left, right, or center, a deprecated
//! `align` attribute is added to each `<th>` and `<td>` element belonging to
//! that column.
//! That attribute is interpreted by browsers as if a CSS `text-align` property
//! was included, with its value set to that same keyword.
//!
//! ## Recommendation
//!
//! When authoring markdown with GFM tables, it’s recommended to *always* put
//! pipes around cells.
//! Without them, it can be hard to infer whether the table will work, how many
//! columns there are, and which column you are currently editing.
//!
//! It is recommended to not use many columns, as it results in very long lines,
//! making it hard to infer which column you are currently editing.
//!
//! For larger tables, particularly when cells vary in size, it is recommended
//! *not* to manually “pad” cell text.
//! While it can look better, it results in a lot of time spent realigning
//! everything when a new, longer cell is added or the longest cell removed, as
//! every row then must be changed.
//! Other than costing time, it also causes large diffs in Git.
//!
//! To illustrate, when authoring large tables, it is discouraged to pad cells
//! like this:
//!
//! ```markdown
//! | Alpha bravo charlie |              delta |
//! | ------------------- | -----------------: |
//! | Echo                | Foxtrot golf hotel |
//! ```
//!
//! Instead, use single spaces (and single filler dashes):
//!
//! ```markdown
//! | Alpha bravo charlie | delta |
//! | - | -: |
//! | Echo | Foxtrot golf hotel |
//! ```
//!
//! ## Bugs
//!
//! GitHub’s own algorithm to parse tables contains a bug.
//! This bug is not present in this project.
//! The issue relating to tables is:
//!
//! * [GFM tables: escaped escapes are incorrectly treated as escapes](https://github.com/github/cmark-gfm/issues/277)
//!
//! ## Tokens
//!
//! * [`GfmTable`][Name::GfmTable]
//! * [`GfmTableBody`][Name::GfmTableBody]
//! * [`GfmTableCell`][Name::GfmTableCell]
//! * [`GfmTableCellDivider`][Name::GfmTableCellDivider]
//! * [`GfmTableCellText`][Name::GfmTableCellText]
//! * [`GfmTableDelimiterCell`][Name::GfmTableDelimiterCell]
//! * [`GfmTableDelimiterCellValue`][Name::GfmTableDelimiterCellValue]
//! * [`GfmTableDelimiterFiller`][Name::GfmTableDelimiterFiller]
//! * [`GfmTableDelimiterMarker`][Name::GfmTableDelimiterMarker]
//! * [`GfmTableDelimiterRow`][Name::GfmTableDelimiterRow]
//! * [`GfmTableHead`][Name::GfmTableHead]
//! * [`GfmTableRow`][Name::GfmTableRow]
//! * [`LineEnding`][Name::LineEnding]
//!
//! ## References
//!
//! * [`micromark-extension-gfm-table`](https://github.com/micromark/micromark-extension-gfm-table)
//! * [*§ 4.10 Tables (extension)* in `GFM`](https://github.github.com/gfm/#tables-extension-)
//!
//! [flow]: crate::mdx_parser::construct::flow
//! [text]: crate::mdx_parser::construct::text
//! [attention]: crate::mdx_parser::construct::attention
//! [raw_text]: crate::mdx_parser::construct::raw_text
//! [html_table]: https://html.spec.whatwg.org/multipage/tables.html#the-table-element
//! [html_tbody]: https://html.spec.whatwg.org/multipage/tables.html#the-tbody-element
//! [html_td]: https://html.spec.whatwg.org/multipage/tables.html#the-td-element
//! [html_th]: https://html.spec.whatwg.org/multipage/tables.html#the-th-element
//! [html_thead]: https://html.spec.whatwg.org/multipage/tables.html#the-thead-element
//! [html_tr]: https://html.spec.whatwg.org/multipage/tables.html#the-tr-element

use crate::mdx_parser::construct::partial_space_or_tab::{space_or_tab, space_or_tab_min_max};
use crate::mdx_parser::event::{Content, Event, Kind, Link, Name};
use crate::mdx_parser::resolve::Name as ResolveName;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::Subresult;
use crate::mdx_parser::tokenizer::{ByteSet, LINE_STOP, Tokenizer, with_bytes};
use crate::mdx_parser::util::{constant::TAB_SIZE, skip::opt_back as skip_opt_back};
use alloc::vec;

/// Bytes that data in a cell stops at.
const CELL_STOP: ByteSet = with_bytes(LINE_STOP, b" \\|");

/// Start of a GFM table.
///
/// If there is a valid table row or table head before, then we try to parse
/// another row.
/// Otherwise, we try to parse a head.
///
/// ```markdown
/// > | | a |
///     ^
///   | | - |
/// > | | b |
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.parse_state.options.constructs.gfm_table {
        if !tokenizer.pierce
            && !tokenizer.events.is_empty()
            && matches!(
                tokenizer.events[skip_opt_back(
                    &tokenizer.events,
                    tokenizer.events.len() - 1,
                    &[Name::LineEnding, Name::SpaceOrTab],
                )]
                .name,
                Name::GfmTableHead | Name::GfmTableRow
            )
        {
            State::Retry(StateName::GfmTableBodyRowStart)
        } else {
            State::Retry(StateName::GfmTableHeadRowBefore)
        }
    } else {
        State::Nok
    }
}

/// Before table head row.
///
/// ```markdown
/// > | | a |
///     ^
///   | | - |
///   | | b |
/// ```
pub fn head_row_before(tokenizer: &mut Tokenizer) -> State {
    tokenizer.enter(Name::GfmTableHead);
    tokenizer.enter(Name::GfmTableRow);
    if matches!(tokenizer.current, Some(b'\t' | b' ')) {
        tokenizer.attempt(State::Next(StateName::GfmTableHeadRowStart), State::Nok);
        State::Retry(space_or_tab_min_max(
            tokenizer,
            0,
            if tokenizer.parse_state.options.constructs.code_indented {
                TAB_SIZE - 1
            } else {
                usize::MAX
            },
        ))
    } else {
        State::Retry(StateName::GfmTableHeadRowStart)
    }
}

/// Before table head row, after whitespace.
///
/// ```markdown
/// > | | a |
///     ^
///   | | - |
///   | | b |
/// ```
pub fn head_row_start(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        // 4+ spaces.
        Some(b'\t' | b' ') => State::Nok,
        Some(b'|') => State::Retry(StateName::GfmTableHeadRowBreak),
        _ => {
            tokenizer.tokenize_state.seen = true;
            // Count the first character, that isn’t a pipe, double.
            tokenizer.tokenize_state.size_b += 1;
            State::Retry(StateName::GfmTableHeadRowBreak)
        }
    }
}

/// At break in table head row.
///
/// ```markdown
/// > | | a |
///     ^
///       ^
///         ^
///   | | - |
///   | | b |
/// ```
pub fn head_row_break(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => {
            tokenizer.tokenize_state.seen = false;
            tokenizer.tokenize_state.size = 0;
            tokenizer.tokenize_state.size_b = 0;
            State::Nok
        }
        Some(b'\n') => {
            // If anything other than one pipe (ignoring whitespace) was used, it’s fine.
            if tokenizer.tokenize_state.size_b > 1 {
                tokenizer.tokenize_state.size_b = 0;
                // Feel free to interrupt:
                tokenizer.interrupt = true;
                tokenizer.exit(Name::GfmTableRow);
                tokenizer.enter(Name::LineEnding);
                tokenizer.consume();
                tokenizer.exit(Name::LineEnding);
                State::Next(StateName::GfmTableHeadDelimiterStart)
            } else {
                tokenizer.tokenize_state.seen = false;
                tokenizer.tokenize_state.size = 0;
                tokenizer.tokenize_state.size_b = 0;
                State::Nok
            }
        }
        Some(b'\t' | b' ') => {
            tokenizer.attempt(State::Next(StateName::GfmTableHeadRowBreak), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        }
        _ => {
            tokenizer.tokenize_state.size_b += 1;

            // Whether a delimiter was seen.
            if tokenizer.tokenize_state.seen {
                tokenizer.tokenize_state.seen = false;
                // Header cell count.
                tokenizer.tokenize_state.size += 1;
            }

            if tokenizer.current == Some(b'|') {
                tokenizer.enter(Name::GfmTableCellDivider);
                tokenizer.consume();
                tokenizer.exit(Name::GfmTableCellDivider);
                // Whether a delimiter was seen.
                tokenizer.tokenize_state.seen = true;
                State::Next(StateName::GfmTableHeadRowBreak)
            } else {
                // Anything else is cell data.
                tokenizer.enter(Name::Data);
                State::Retry(StateName::GfmTableHeadRowData)
            }
        }
    }
}

/// In table head row data.
///
/// ```markdown
/// > | | a |
///       ^
///   | | - |
///   | | b |
/// ```
pub fn head_row_data(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\t' | b'\n' | b' ' | b'|') => {
            tokenizer.exit(Name::Data);
            State::Retry(StateName::GfmTableHeadRowBreak)
        }
        Some(b'\\') => {
            tokenizer.consume();
            State::Next(StateName::GfmTableHeadRowEscape)
        }
        _ => {
            tokenizer.consume_run(&CELL_STOP);
            State::Next(StateName::GfmTableHeadRowData)
        }
    }
}

/// In table head row escape.
///
/// ```markdown
/// > | | a\-b |
///         ^
///   | | ---- |
///   | | c    |
/// ```
pub fn head_row_escape(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\\' | b'|') => {
            tokenizer.consume();
            State::Next(StateName::GfmTableHeadRowData)
        }
        _ => State::Retry(StateName::GfmTableHeadRowData),
    }
}

/// Before delimiter row.
///
/// ```markdown
///   | | a |
/// > | | - |
///     ^
///   | | b |
/// ```
pub fn head_delimiter_start(tokenizer: &mut Tokenizer) -> State {
    // Reset `interrupt`.
    tokenizer.interrupt = false;

    if tokenizer.lazy || tokenizer.pierce {
        State::Retry(StateName::GfmTableHeadDelimiterNok)
    } else {
        tokenizer.enter(Name::GfmTableDelimiterRow);
        // Track if we’ve seen a `:` or `|`.
        tokenizer.tokenize_state.seen = false;

        match tokenizer.current {
            Some(b'\t' | b' ') => {
                tokenizer.attempt(
                    State::Next(StateName::GfmTableHeadDelimiterBefore),
                    State::Next(StateName::GfmTableHeadDelimiterNok),
                );

                State::Retry(space_or_tab_min_max(
                    tokenizer,
                    0,
                    if tokenizer.parse_state.options.constructs.code_indented {
                        TAB_SIZE - 1
                    } else {
                        usize::MAX
                    },
                ))
            }
            _ => State::Retry(StateName::GfmTableHeadDelimiterBefore),
        }
    }
}

/// Before delimiter row, after optional whitespace.
///
/// Reused when a `|` is found later, to parse another cell.
///
/// ```markdown
///   | | a |
/// > | | - |
///     ^
///   | | b |
/// ```
pub fn head_delimiter_before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'-' | b':') => State::Retry(StateName::GfmTableHeadDelimiterValueBefore),
        Some(b'|') => {
            tokenizer.tokenize_state.seen = true;
            // If we start with a pipe, we open a cell marker.
            tokenizer.enter(Name::GfmTableCellDivider);
            tokenizer.consume();
            tokenizer.exit(Name::GfmTableCellDivider);
            State::Next(StateName::GfmTableHeadDelimiterCellBefore)
        }
        // More whitespace / empty row not allowed at start.
        _ => State::Retry(StateName::GfmTableHeadDelimiterNok),
    }
}

/// After `|`, before delimiter cell.
///
/// ```markdown
///   | | a |
/// > | | - |
///      ^
/// ```
pub fn head_delimiter_cell_before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\t' | b' ') => {
            tokenizer.attempt(State::Next(StateName::GfmTableHeadDelimiterValueBefore), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        }
        _ => State::Retry(StateName::GfmTableHeadDelimiterValueBefore),
    }
}

/// Before delimiter cell value.
///
/// ```markdown
///   | | a |
/// > | | - |
///       ^
/// ```
pub fn head_delimiter_value_before(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => State::Retry(StateName::GfmTableHeadDelimiterCellAfter),
        Some(b':') => {
            // Align: left.
            tokenizer.tokenize_state.size_b += 1;
            tokenizer.tokenize_state.seen = true;
            tokenizer.enter(Name::GfmTableDelimiterMarker);
            tokenizer.consume();
            tokenizer.exit(Name::GfmTableDelimiterMarker);
            State::Next(StateName::GfmTableHeadDelimiterLeftAlignmentAfter)
        }
        Some(b'-') => {
            // Align: none.
            tokenizer.tokenize_state.size_b += 1;
            State::Retry(StateName::GfmTableHeadDelimiterLeftAlignmentAfter)
        }
        _ => State::Retry(StateName::GfmTableHeadDelimiterNok),
    }
}

/// After delimiter cell left alignment marker.
///
/// ```markdown
///   | | a  |
/// > | | :- |
///        ^
/// ```
pub fn head_delimiter_left_alignment_after(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'-') => {
            tokenizer.enter(Name::GfmTableDelimiterFiller);
            State::Retry(StateName::GfmTableHeadDelimiterFiller)
        }
        // Anything else is not ok after the left-align colon.
        _ => State::Retry(StateName::GfmTableHeadDelimiterNok),
    }
}

/// In delimiter cell filler.
///
/// ```markdown
///   | | a |
/// > | | - |
///       ^
/// ```
pub fn head_delimiter_filler(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'-') => {
            tokenizer.consume();
            State::Next(StateName::GfmTableHeadDelimiterFiller)
        }
        Some(b':') => {
            // Align is `center` if it was `left`, `right` otherwise.
            tokenizer.tokenize_state.seen = true;
            tokenizer.exit(Name::GfmTableDelimiterFiller);
            tokenizer.enter(Name::GfmTableDelimiterMarker);
            tokenizer.consume();
            tokenizer.exit(Name::GfmTableDelimiterMarker);
            State::Next(StateName::GfmTableHeadDelimiterRightAlignmentAfter)
        }
        _ => {
            tokenizer.exit(Name::GfmTableDelimiterFiller);
            State::Retry(StateName::GfmTableHeadDelimiterRightAlignmentAfter)
        }
    }
}

/// After delimiter cell right alignment marker.
///
/// ```markdown
///   | |  a |
/// > | | -: |
///         ^
/// ```
pub fn head_delimiter_right_alignment_after(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\t' | b' ') => {
            tokenizer.attempt(State::Next(StateName::GfmTableHeadDelimiterCellAfter), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        }
        _ => State::Retry(StateName::GfmTableHeadDelimiterCellAfter),
    }
}

/// After delimiter cell.
///
/// ```markdown
///   | |  a |
/// > | | -: |
///          ^
/// ```
pub fn head_delimiter_cell_after(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            // Exit when:
            // * there was no `:` or `|` at all (it’s a thematic break or setext
            //   underline instead)
            // * the header cell count is not the delimiter cell count
            if !tokenizer.tokenize_state.seen || tokenizer.tokenize_state.size != tokenizer.tokenize_state.size_b {
                State::Retry(StateName::GfmTableHeadDelimiterNok)
            } else {
                // Reset.
                tokenizer.tokenize_state.seen = false;
                tokenizer.tokenize_state.size = 0;
                tokenizer.tokenize_state.size_b = 0;
                tokenizer.exit(Name::GfmTableDelimiterRow);
                tokenizer.exit(Name::GfmTableHead);
                tokenizer.register_resolver(ResolveName::GfmTable);
                State::Ok
            }
        }
        Some(b'|') => State::Retry(StateName::GfmTableHeadDelimiterBefore),
        _ => State::Retry(StateName::GfmTableHeadDelimiterNok),
    }
}

/// In delimiter row, at a disallowed byte.
///
/// ```markdown
///   | | a |
/// > | | x |
///       ^
/// ```
pub fn head_delimiter_nok(tokenizer: &mut Tokenizer) -> State {
    // Reset.
    tokenizer.tokenize_state.seen = false;
    tokenizer.tokenize_state.size = 0;
    tokenizer.tokenize_state.size_b = 0;
    State::Nok
}

/// Before table body row.
///
/// ```markdown
///   | | a |
///   | | - |
/// > | | b |
///     ^
/// ```
pub fn body_row_start(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.lazy {
        State::Nok
    } else {
        tokenizer.enter(Name::GfmTableRow);

        match tokenizer.current {
            Some(b'\t' | b' ') => {
                tokenizer.attempt(State::Next(StateName::GfmTableBodyRowBreak), State::Nok);
                // We’re parsing a body row.
                // If we’re here, we already attempted blank lines and indented
                // code.
                // So parse as much whitespace as needed:
                State::Retry(space_or_tab_min_max(tokenizer, 0, usize::MAX))
            }
            _ => State::Retry(StateName::GfmTableBodyRowBreak),
        }
    }
}

/// At break in table body row.
///
/// ```markdown
///   | | a |
///   | | - |
/// > | | b |
///     ^
///       ^
///         ^
/// ```
pub fn body_row_break(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::GfmTableRow);
            State::Ok
        }
        Some(b'\t' | b' ') => {
            tokenizer.attempt(State::Next(StateName::GfmTableBodyRowBreak), State::Nok);
            State::Retry(space_or_tab(tokenizer))
        }
        Some(b'|') => {
            tokenizer.enter(Name::GfmTableCellDivider);
            tokenizer.consume();
            tokenizer.exit(Name::GfmTableCellDivider);
            State::Next(StateName::GfmTableBodyRowBreak)
        }
        // Anything else is cell content.
        _ => {
            tokenizer.enter(Name::Data);
            State::Retry(StateName::GfmTableBodyRowData)
        }
    }
}

/// In table body row data.
///
/// ```markdown
///   | | a |
///   | | - |
/// > | | b |
///       ^
/// ```
pub fn body_row_data(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\t' | b'\n' | b' ' | b'|') => {
            tokenizer.exit(Name::Data);
            State::Retry(StateName::GfmTableBodyRowBreak)
        }
        _ => {
            let name = if tokenizer.current == Some(b'\\') {
                StateName::GfmTableBodyRowEscape
            } else {
                StateName::GfmTableBodyRowData
            };
            tokenizer.consume();
            State::Next(name)
        }
    }
}

/// In table body row escape.
///
/// ```markdown
///   | | a    |
///   | | ---- |
/// > | | b\-c |
///         ^
/// ```
pub fn body_row_escape(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\\' | b'|') => {
            tokenizer.consume();
            State::Next(StateName::GfmTableBodyRowData)
        }
        _ => State::Retry(StateName::GfmTableBodyRowData),
    }
}

/// Resolve GFM table.
pub fn resolve(tokenizer: &mut Tokenizer) -> Option<Subresult> {
    let mut index = 0;
    let mut in_first_cell_awaiting_pipe = true;
    let mut in_row = false;
    let mut in_delimiter_row = false;
    let mut last_cell = (0, 0, 0, 0);
    let mut cell = (0, 0, 0, 0);
    let mut after_head_awaiting_first_body_row = false;
    let mut last_table_end = 0;
    let mut last_table_has_body = false;

    while index < tokenizer.events.len() {
        let event = &tokenizer.events[index];

        if event.kind == Kind::Enter {
            // Start of head.
            if event.name == Name::GfmTableHead {
                after_head_awaiting_first_body_row = false;

                // Inject previous (body end and) table end.
                if last_table_end != 0 {
                    flush_table_end(tokenizer, last_table_end, last_table_has_body);
                    last_table_has_body = false;
                    last_table_end = 0;
                }

                // Inject table start.
                let enter = Event {
                    kind: Kind::Enter,
                    name: Name::GfmTable,
                    point: tokenizer.events[index].point.clone(),
                    link: None,
                };
                tokenizer.map.add(index, 0, vec![enter]);
            } else if matches!(event.name, Name::GfmTableRow | Name::GfmTableDelimiterRow) {
                in_delimiter_row = event.name == Name::GfmTableDelimiterRow;
                in_row = true;
                in_first_cell_awaiting_pipe = true;
                last_cell = (0, 0, 0, 0);
                cell = (0, index + 1, 0, 0);

                // Inject table body start.
                if after_head_awaiting_first_body_row {
                    after_head_awaiting_first_body_row = false;
                    last_table_has_body = true;
                    let enter = Event {
                        kind: Kind::Enter,
                        name: Name::GfmTableBody,
                        point: tokenizer.events[index].point.clone(),
                        link: None,
                    };
                    tokenizer.map.add(index, 0, vec![enter]);
                }
            }
            // Cell data.
            else if in_row
                && matches!(
                    event.name,
                    Name::Data | Name::GfmTableDelimiterMarker | Name::GfmTableDelimiterFiller
                )
            {
                in_first_cell_awaiting_pipe = false;

                // First value in cell.
                if cell.2 == 0 {
                    if last_cell.1 != 0 {
                        cell.0 = cell.1;
                        flush_cell(tokenizer, last_cell, in_delimiter_row, None);
                        last_cell = (0, 0, 0, 0);
                    }

                    cell.2 = index;
                }
            } else if event.name == Name::GfmTableCellDivider {
                if in_first_cell_awaiting_pipe {
                    in_first_cell_awaiting_pipe = false;
                } else {
                    if last_cell.1 != 0 {
                        cell.0 = cell.1;
                        flush_cell(tokenizer, last_cell, in_delimiter_row, None);
                    }

                    last_cell = cell;
                    cell = (last_cell.1, index, 0, 0);
                }
            }
        // Exit events.
        } else if event.name == Name::GfmTableHead {
            after_head_awaiting_first_body_row = true;
            last_table_end = index;
        } else if matches!(event.name, Name::GfmTableRow | Name::GfmTableDelimiterRow) {
            in_row = false;
            last_table_end = index;
            if last_cell.1 != 0 {
                cell.0 = cell.1;
                flush_cell(tokenizer, last_cell, in_delimiter_row, Some(index));
            } else if cell.1 != 0 {
                flush_cell(tokenizer, cell, in_delimiter_row, Some(index));
            }
        } else if in_row
            && (matches!(
                event.name,
                Name::Data | Name::GfmTableDelimiterMarker | Name::GfmTableDelimiterFiller
            ))
        {
            cell.3 = index;
        }

        index += 1;
    }

    if last_table_end != 0 {
        flush_table_end(tokenizer, last_table_end, last_table_has_body);
    }

    tokenizer.map.consume(&mut tokenizer.events);
    None
}

/// Generate a cell.
fn flush_cell(
    tokenizer: &mut Tokenizer,
    range: (usize, usize, usize, usize),
    in_delimiter_row: bool,
    row_end: Option<usize>,
) {
    let group_name = if in_delimiter_row {
        Name::GfmTableDelimiterCell
    } else {
        Name::GfmTableCell
    };
    let value_name = if in_delimiter_row {
        Name::GfmTableDelimiterCellValue
    } else {
        Name::GfmTableCellText
    };

    // Insert an exit for the previous cell, if there is one.
    //
    // ```markdown
    // > | | aa | bb | cc |
    //          ^-- exit
    //           ^^^^-- this cell
    // ```
    if range.0 != 0 {
        tokenizer.map.add(
            range.0,
            0,
            vec![Event {
                kind: Kind::Exit,
                name: group_name.clone(),
                point: tokenizer.events[range.0].point.clone(),
                link: None,
            }],
        );
    }

    // Insert enter of this cell.
    //
    // ```markdown
    // > | | aa | bb | cc |
    //           ^-- enter
    //           ^^^^-- this cell
    // ```
    tokenizer.map.add(
        range.1,
        0,
        vec![Event {
            kind: Kind::Enter,
            name: group_name.clone(),
            point: tokenizer.events[range.1].point.clone(),
            link: None,
        }],
    );

    // Insert text start at first data start and end at last data end, and
    // remove events between.
    //
    // ```markdown
    // > | | aa | bb | cc |
    //            ^-- enter
    //             ^-- exit
    //           ^^^^-- this cell
    // ```
    if range.2 != 0 {
        tokenizer.map.add(
            range.2,
            0,
            vec![Event {
                kind: Kind::Enter,
                name: value_name.clone(),
                point: tokenizer.events[range.2].point.clone(),
                link: None,
            }],
        );
        debug_assert_ne!(range.3, 0);

        if !in_delimiter_row {
            tokenizer.events[range.2].link = Some(Link {
                previous: None,
                next: None,
                content: Content::Text,
            });

            // To do: positional info of the remaining `data` nodes likely have
            // to be fixed.
            if range.3 > range.2 + 1 {
                let a = range.2 + 1;
                let b = range.3 - range.2 - 1;
                tokenizer.map.add(a, b, vec![]);
            }
        }

        tokenizer.map.add(
            range.3 + 1,
            0,
            vec![Event {
                kind: Kind::Exit,
                name: value_name,
                point: tokenizer.events[range.3].point.clone(),
                link: None,
            }],
        );
    }

    // Insert an exit for the last cell, if at the row end.
    //
    // ```markdown
    // > | | aa | bb | cc |
    //                    ^-- exit
    //               ^^^^^^-- this cell (the last one contains two “between” parts)
    // ```
    if let Some(row_end) = row_end {
        tokenizer.map.add(
            row_end,
            0,
            vec![Event {
                kind: Kind::Exit,
                name: group_name,
                point: tokenizer.events[row_end].point.clone(),
                link: None,
            }],
        );
    }
}

/// Generate table end (and table body end).
fn flush_table_end(tokenizer: &mut Tokenizer, index: usize, body: bool) {
    let mut exits = vec![];

    if body {
        exits.push(Event {
            kind: Kind::Exit,
            name: Name::GfmTableBody,
            point: tokenizer.events[index].point.clone(),
            link: None,
        });
    }

    exits.push(Event {
        kind: Kind::Exit,
        name: Name::GfmTable,
        point: tokenizer.events[index].point.clone(),
        link: None,
    });

    tokenizer.map.add(index + 1, 0, exits);
}
