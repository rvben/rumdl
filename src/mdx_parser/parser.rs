//! Turn bytes of markdown into events.

use crate::mdx_parser::ParseOptions;
use crate::mdx_parser::event::{Event, Point};
use crate::mdx_parser::message;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::subtokenize;
use crate::mdx_parser::tokenizer::Tokenizer;
use alloc::{string::String, vec, vec::Vec};
use core::cell::Cell;

/// Info needed, in all content types, when parsing markdown.
///
/// Importantly, this contains a set of known definitions.
/// It also references the input value as bytes (`u8`).
#[derive(Debug)]
pub struct ParseState<'a> {
    /// Configuration.
    pub options: &'a ParseOptions,
    /// Bytes of MDX expressions and ESM that may still be read, if limited.
    pub mdx_parse_budget: Option<Cell<usize>>,
    /// List of chars.
    pub bytes: &'a [u8],
    /// Set of defined definition identifiers.
    pub definitions: Vec<String>,
    /// Set of defined GFM footnote definition identifiers.
    pub gfm_footnote_definitions: Vec<String>,
}

/// Turn a string of markdown into events.
///
/// Passes the bytes back so the compiler can access the source.
pub fn parse<'a>(value: &'a str, options: &'a ParseOptions) -> Result<(Vec<Event>, ParseState<'a>), message::Message> {
    let bytes = value.as_bytes();

    let mut parse_state = ParseState {
        options,
        bytes,
        mdx_parse_budget: options
            .mdx_parse_budget_factor
            .map(|factor| Cell::new(factor.saturating_mul(bytes.len()))),
        definitions: vec![],
        gfm_footnote_definitions: vec![],
    };

    let start = Point {
        line: 1,
        column: 1,
        index: 0,
        vs: 0,
    };
    let mut tokenizer = Tokenizer::new(start, &parse_state);

    let state = tokenizer.push(
        (0, 0),
        (parse_state.bytes.len(), 0),
        State::Next(StateName::DocumentStart),
    );
    let mut result = tokenizer.flush(state, true)?;
    let mut events = tokenizer.events;

    loop {
        // Sorted and unique, so `label_end` can binary search them.
        let fn_defs = &mut parse_state.gfm_footnote_definitions;
        let defs = &mut parse_state.definitions;
        if !result.gfm_footnote_definitions.is_empty() {
            fn_defs.append(&mut result.gfm_footnote_definitions);
            fn_defs.sort_unstable();
            fn_defs.dedup();
        }
        if !result.definitions.is_empty() {
            defs.append(&mut result.definitions);
            defs.sort_unstable();
            defs.dedup();
        }

        if result.done {
            return Ok((events, parse_state));
        }

        result = subtokenize(&mut events, &parse_state, None)?;
    }
}
