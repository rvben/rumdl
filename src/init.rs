//!
//! This module provides initialization utilities for rumdl, such as creating default configuration files.

use std::fs;
use std::io;
use std::path::Path;
use thiserror::Error;

/// Error type for initialization operations
#[derive(Error, Debug)]
pub enum InitError {
    #[error("Failed to access file {path}: {source}")]
    IoError { source: io::Error, path: String },
}

/// Create a default configuration file at the specified path.
///
/// Returns `true` if the file was created, or `false` if it already exists.
///
/// # Errors
///
/// Returns an error if the file cannot be created due to permissions or other I/O errors.
pub fn create_default_config(path: &str) -> Result<bool, InitError> {
    if Path::new(path).exists() {
        return Ok(false);
    }

    let default_config = r#"# rumdl configuration file

[general]
# Maximum line length for line-based rules
line_length = 80

[rules]
# Rules to disable (comma-separated list of rule IDs)
disabled = []

# Rule-specific configuration
[rules.MD007]
# Number of spaces for list indentation
indent = 2

[rules.MD013]
# Enable line length checking for code blocks
code_blocks = true
# Enable line length checking for tables
tables = false
# Enable line length checking for headings
headings = true
# Enable strict line length checking (no exceptions)
strict = false

[rules.MD022]
# Number of blank lines required before headings
lines_above = 1
# Number of blank lines required after headings
lines_below = 1

[rules.MD024]
# Allow headings with the same content if they're not siblings
allow_different_nesting = true

[rules.MD029]
# Style for ordered list markers (one = 1., ordered = 1, 2, 3, ordered_parenthesis = 1), 2), 3))
style = "one"

[rules.MD035]
# Style for horizontal rules (----, ***, etc.)
style = "---"

[rules.MD048]
# Style for code fence markers (``` or ~~~)
style = "```"

[rules.MD049]
# Style for emphasis (asterisk or underscore)
style = "*"

[rules.MD050]
# Style for strong emphasis (asterisk or underscore)
style = "**"
"#;

    fs::write(path, default_config).map_err(|e| InitError::IoError {
        source: e,
        path: path.to_string(),
    })?;

    Ok(true)
}
