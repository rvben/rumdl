//! Output formatting module for rumdl
//!
//! This module provides different output formats for linting results,
//! similar to how Ruff handles multiple output formats.

use crate::rule::LintWarning;
use std::io::{self, Write};

pub mod formatters;

// Re-export formatters
pub use formatters::*;

/// Trait for output formatters
pub trait OutputFormatter {
    /// Format a collection of warnings for output
    fn format_warnings(&self, warnings: &[LintWarning], file_path: &str) -> String;

    /// Format a summary of results across multiple files
    fn format_summary(&self, _files_processed: usize, _total_warnings: usize, _duration_ms: u64) -> Option<String> {
        // Default: no summary
        None
    }

    /// Whether this formatter should use colors
    fn use_colors(&self) -> bool {
        false
    }
}

/// Available output formats
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    /// Default human-readable format with colors and context
    Text,
    /// Concise format: file:line:col: [RULE] message
    Concise,
    /// Grouped format: violations grouped by file
    Grouped,
    /// JSON format (existing)
    Json,
    /// JSON Lines format (one JSON object per line)
    JsonLines,
    /// GitHub Actions annotation format
    GitHub,
    /// GitLab Code Quality format
    GitLab,
    /// Pylint-compatible format: file:line:column: CODE message
    Pylint,
    /// Azure Pipeline logging format
    Azure,
    /// SARIF 2.1.0 format
    Sarif,
    /// JUnit XML format
    Junit,
}

impl OutputFormat {
    /// Parse output format from string
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "text" | "full" => Ok(OutputFormat::Text),
            "concise" => Ok(OutputFormat::Concise),
            "grouped" => Ok(OutputFormat::Grouped),
            "json" => Ok(OutputFormat::Json),
            "json-lines" | "jsonlines" => Ok(OutputFormat::JsonLines),
            "github" => Ok(OutputFormat::GitHub),
            "gitlab" => Ok(OutputFormat::GitLab),
            "pylint" => Ok(OutputFormat::Pylint),
            "azure" => Ok(OutputFormat::Azure),
            "sarif" => Ok(OutputFormat::Sarif),
            "junit" => Ok(OutputFormat::Junit),
            _ => Err(format!("Unknown output format: {}", s)),
        }
    }

    /// Create a formatter instance for this format
    pub fn create_formatter(&self) -> Box<dyn OutputFormatter> {
        match self {
            OutputFormat::Text => Box::new(TextFormatter::new()),
            OutputFormat::Concise => Box::new(ConciseFormatter::new()),
            OutputFormat::Grouped => Box::new(GroupedFormatter::new()),
            OutputFormat::Json => Box::new(JsonFormatter::new()),
            OutputFormat::JsonLines => Box::new(JsonLinesFormatter::new()),
            OutputFormat::GitHub => Box::new(GitHubFormatter::new()),
            OutputFormat::GitLab => Box::new(GitLabFormatter::new()),
            OutputFormat::Pylint => Box::new(PylintFormatter::new()),
            OutputFormat::Azure => Box::new(AzureFormatter::new()),
            OutputFormat::Sarif => Box::new(SarifFormatter::new()),
            OutputFormat::Junit => Box::new(JunitFormatter::new()),
        }
    }
}

/// Output writer that handles stdout/stderr routing
pub struct OutputWriter {
    use_stderr: bool,
    _quiet: bool,
    silent: bool,
}

impl OutputWriter {
    pub fn new(use_stderr: bool, quiet: bool, silent: bool) -> Self {
        Self {
            use_stderr,
            _quiet: quiet,
            silent,
        }
    }

    /// Write output to appropriate stream
    pub fn write(&self, content: &str) -> io::Result<()> {
        if self.silent {
            return Ok(());
        }

        if self.use_stderr {
            eprint!("{}", content);
            io::stderr().flush()?;
        } else {
            print!("{}", content);
            io::stdout().flush()?;
        }
        Ok(())
    }

    /// Write a line to appropriate stream
    pub fn writeln(&self, content: &str) -> io::Result<()> {
        if self.silent {
            return Ok(());
        }

        if self.use_stderr {
            eprintln!("{}", content);
        } else {
            println!("{}", content);
        }
        Ok(())
    }

    /// Write error/debug output (always to stderr unless silent)
    pub fn write_error(&self, content: &str) -> io::Result<()> {
        if self.silent {
            return Ok(());
        }

        eprintln!("{}", content);
        Ok(())
    }
}
