//! File processing and linting logic.
//!
//! This module handles file discovery, core linting/fixing, and embedded markdown processing.

mod discovery;
mod doc_comments;
mod embedded;
mod fix_reporting;
mod preflight;
mod processing;

pub use discovery::*;
pub use doc_comments::format_doc_comment_blocks;
pub use fix_reporting::*;
pub use preflight::*;
pub use processing::*;

#[cfg(test)]
mod tests;
