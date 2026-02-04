//! Per-language code block linting and formatting using external tools.
//!
//! This module provides infrastructure for running external formatters and linters
//! on fenced code blocks in markdown files, based on their language tags.
//!
//! # Overview
//!
//! When enabled, rumdl can process code blocks using external tools:
//! - **Lint mode** (`rumdl check`): Run linters and report diagnostics
//! - **Format mode** (`rumdl check --fix` / `rumdl fmt`): Run formatters and update content
//!
//! # Configuration
//!
//! Code block tools are disabled by default. Enable and configure in `.rumdl.toml`:
//!
//! ```toml
//! [code-block-tools]
//! enabled = true
//! normalize-language = "linguist"  # Resolve aliases like "py" -> "python"
//! on-error = "fail"                # or "skip" / "warn"
//! timeout = 30000                  # ms per tool
//!
//! [code-block-tools.languages.python]
//! lint = ["ruff:check"]
//! format = ["ruff:format"]
//!
//! [code-block-tools.languages.shell]
//! lint = ["shellcheck"]
//! format = ["shfmt"]
//!
//! [code-block-tools.language-aliases]
//! py = "python"
//! bash = "shell"
//! ```
//!
//! # Built-in Tools
//!
//! Common tools are pre-configured:
//! - `ruff:check`, `ruff:format` - Python
//! - `prettier`, `prettier:json`, etc. - JavaScript, JSON, YAML, etc.
//! - `shellcheck`, `shfmt` - Shell scripts
//! - `rustfmt` - Rust
//! - `gofmt`, `goimports` - Go
//! - And many more (see [`registry`] module)
//!
//! # Custom Tools
//!
//! Define custom tools in configuration:
//!
//! ```toml
//! [code-block-tools.tools.my-formatter]
//! command = ["my-tool", "--format"]
//! stdin = true
//! stdout = true
//! ```
//!
//! # Language Resolution
//!
//! With `normalize-language = "linguist"` (default), common aliases are resolved:
//! - `py`, `python3` → `python`
//! - `bash`, `sh`, `zsh` → `shell`
//! - `js`, `node` → `javascript`
//!
//! See [`linguist`] module for the full list.

pub mod config;
pub mod executor;
pub mod linguist;
pub mod processor;
pub mod registry;

pub use config::{CodeBlockToolsConfig, LanguageToolConfig, NormalizeLanguage, OnError, ToolDefinition};
pub use executor::{ExecutorError, ToolExecutor, ToolOutput};
pub use linguist::LinguistResolver;
pub use processor::{
    CodeBlockDiagnostic, CodeBlockResult, CodeBlockToolProcessor, DiagnosticSeverity, FencedCodeBlockInfo,
    ProcessorError,
};
pub use registry::ToolRegistry;
