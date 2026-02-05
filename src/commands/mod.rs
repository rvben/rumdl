//! Command handlers for the rumdl CLI.
//!
//! Each subcommand has its own module with a public handler function
//! that `main()` dispatches to.

pub mod check;
pub mod clean;
pub mod completions;
pub mod config;
pub mod explain;
pub mod import;
pub mod init;
pub mod rule;
pub mod schema;
pub mod server;
pub mod version;
pub mod vscode;
