//!
//! This module defines configuration structures, loading logic, and provenance tracking for rumdl.
//! Supports TOML, pyproject.toml, and markdownlint config formats, and provides merging and override logic.

pub mod flavor;
pub use flavor::*;

pub mod types;
pub use types::*;

pub mod source_tracking;
pub use source_tracking::*;

mod loading;
pub(crate) use loading::rumdl_configs_in_dir;

pub mod registry;
pub use registry::*;

pub mod validation;
pub use validation::*;

mod parsers;
pub use parsers::is_global_value_key;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "../config_intelligent_merge_tests.rs"]
mod config_intelligent_merge_tests;
