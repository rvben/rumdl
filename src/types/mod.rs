//! Type-safe configuration types for rumdl.
//!
//! This module contains newtype wrappers and validation types that enforce
//! constraints on configuration values at both compile time and runtime.

mod heading_level;

pub use heading_level::{HeadingLevel, HeadingLevelError};
