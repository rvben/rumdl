//! Handler for the `code-block-tools-docs` command.
//!
//! Generates or checks the built-in tools table in `docs/code-block-tools.md` from the
//! registry, mirroring the `schema generate`/`schema check` workflow. All rendering
//! logic lives in `rumdl_lib::code_block_tools`; this handler only does file I/O and
//! maps results to exit codes.

use colored::*;
use std::fs;
use std::path::PathBuf;

use rumdl_lib::code_block_tools::{render_builtin_tools_table, splice_builtin_tools_docs};
use rumdl_lib::exit_codes::exit;

use crate::CodeBlockToolsDocsAction;

/// Handle the `code-block-tools-docs` subcommand.
pub fn handle_code_block_tools_docs(action: CodeBlockToolsDocsAction) {
    match action {
        CodeBlockToolsDocsAction::Print => {
            print!("{}", render_builtin_tools_table());
        }
        CodeBlockToolsDocsAction::Generate => {
            let path = docs_path();
            let existing = read_docs(&path);
            let updated = splice(&existing);

            if existing == updated {
                println!("Built-in tools docs already up-to-date: {}", path.display());
            } else {
                fs::write(&path, &updated).unwrap_or_else(|e| {
                    eprintln!("{}: Failed to write docs file: {}", "Error".red().bold(), e);
                    exit::tool_error();
                });
                println!("Built-in tools docs updated: {}", path.display());
            }
        }
        CodeBlockToolsDocsAction::Check => {
            let path = docs_path();
            let existing = read_docs(&path);
            let updated = splice(&existing);

            if existing != updated {
                eprintln!(
                    "{}: Built-in tools docs are out of date: {}",
                    "Error".red().bold(),
                    path.display()
                );
                eprintln!("Run 'make sync-code-block-tools' to update them.");
                exit::tool_error();
            }
            println!("Built-in tools docs are up-to-date: {}", path.display());
        }
    }
}

/// Read the docs file, exiting with a clear message if it is missing.
fn read_docs(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_else(|_| {
        eprintln!("{}: Docs file not found: {}", "Error".red().bold(), path.display());
        exit::tool_error();
    })
}

/// Splice the generated table/count into the docs text, exiting on malformed markers.
fn splice(existing: &str) -> String {
    splice_builtin_tools_docs(existing).unwrap_or_else(|e| {
        eprintln!("{}: Failed to update built-in tools docs: {e}", "Error".red().bold());
        exit::tool_error();
    })
}

/// Locate `docs/code-block-tools.md` relative to the project root (nearest Cargo.toml).
fn docs_path() -> PathBuf {
    let start = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("{}: Failed to get current directory: {}", "Error".red().bold(), e);
        exit::tool_error();
    });

    let mut current = start.clone();
    loop {
        if current.join("Cargo.toml").exists() {
            return current.join("docs").join("code-block-tools.md");
        }
        if !current.pop() {
            return start.join("docs").join("code-block-tools.md");
        }
    }
}
