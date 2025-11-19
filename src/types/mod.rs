//! Type-safe configuration types for rumdl.
//!
//! This module contains newtype wrappers and validation types that enforce
//! constraints on configuration values at both compile time and runtime.

mod br_spaces;
mod heading_level;
mod indent_size;
mod line_length;
mod non_negative_usize;
mod positive_usize;

pub use br_spaces::{BrSpaces, BrSpacesError};
pub use heading_level::{HeadingLevel, HeadingLevelError};
pub use indent_size::{IndentSize, IndentSizeError};
pub use line_length::LineLength;
pub use non_negative_usize::{NonNegativeUsize, NonNegativeUsizeError};
pub use positive_usize::{PositiveUsize, PositiveUsizeError};
