//! File processing and linting logic.
//!
//! This module handles file discovery, core linting/fixing, and embedded markdown processing.

mod discovery;
mod embedded;
mod processing;

pub use discovery::*;
pub use processing::*;

#[cfg(test)]
mod tests;
