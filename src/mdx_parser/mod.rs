//! Internal MDX parser, adapted from markdown-rs 1.0.0 (MIT).
//!
//! See README.md for provenance and the changes maintained here.

// Preserve upstream visibility and unused parser capabilities to keep the
// imported source easy to compare with upstream. This is a private module.
#![forbid(unsafe_code)]
#![allow(dead_code, unreachable_pub, unused_imports)]
// Keep upstream code mechanically comparable instead of mixing stylistic
// modernization into the parser import.
#![allow(
    clippy::box_collection,
    clippy::collapsible_if,
    clippy::implicit_saturating_sub,
    clippy::manual_is_multiple_of,
    clippy::manual_pattern_char_comparison,
    clippy::manual_repeat_n,
    clippy::uninlined_format_args,
    clippy::unnecessary_map_or
)]

mod configuration;
mod construct;
mod event;
pub mod mdast;
pub mod message;
mod parser;
mod resolve;
mod state;
mod subtokenize;
mod to_mdast;
mod tokenizer;
pub mod unist;
mod util;

pub use configuration::{Constructs, ParseOptions};
pub(crate) use util::character_reference::{decode_named, decode_numeric};
pub use util::mdx::{
    EsmParse as MdxEsmParse, ExpressionKind as MdxExpressionKind, ExpressionParse as MdxExpressionParse,
    Signal as MdxSignal,
};

pub fn to_mdast(value: &str, options: &ParseOptions) -> Result<mdast::Node, message::Message> {
    let (events, parse_state) = parser::parse(value, options)?;
    to_mdast::compile(&events, parse_state.bytes)
}

#[cfg(test)]
mod tests;

// Keep the renderer available only as an oracle for upstream regression tests.
#[cfg(test)]
mod to_html;
#[cfg(test)]
pub use configuration::{CompileOptions, Options};
#[cfg(test)]
pub use util::line_ending::LineEnding;

#[cfg(test)]
pub fn to_html(value: &str) -> alloc::string::String {
    to_html_with_options(value, &Options::default()).unwrap()
}

#[cfg(test)]
pub fn to_html_with_options(value: &str, options: &Options) -> Result<alloc::string::String, message::Message> {
    let (events, parse_state) = parser::parse(value, &options.parse)?;
    Ok(to_html::compile(&events, parse_state.bytes, &options.compile))
}

#[cfg(test)]
mod mdx_collect_tests;
#[cfg(test)]
mod mdx_parse_budget_tests;
#[cfg(test)]
mod pending_regressions;

/// Parse the AST and preserve exact source ranges for JSX attribute expressions.
/// Their collected values and stops can omit indentation and contain virtual spaces.
pub fn to_mdast_with_attribute_ranges(
    value: &str,
    options: &ParseOptions,
) -> Result<(mdast::Node, alloc::vec::Vec<(usize, usize)>), message::Message> {
    use event::{Kind, Name};

    let (events, parse_state) = parser::parse(value, options)?;
    let root = to_mdast::compile(&events, parse_state.bytes)?;
    let mut ranges = alloc::vec::Vec::new();
    let mut start = 0;
    for event in &events {
        if matches!(
            event.name,
            Name::MdxJsxTagAttributeExpression | Name::MdxJsxTagAttributeValueExpression
        ) {
            // Attribute-expression bodies are opaque JavaScript, so these tokens
            // never nest even when the JavaScript itself contains JSX.
            match event.kind {
                Kind::Enter => start = event.point.index,
                Kind::Exit => ranges.push((start, event.point.index)),
            }
        }
    }
    Ok((root, ranges))
}
