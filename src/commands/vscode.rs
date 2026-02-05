//! Handler for the `vscode` command.

use colored::*;
use rumdl_lib::exit_codes::exit;

/// Handle VS Code extension installation, update, and status.
pub fn handle_vscode(force: bool, update: bool, status: bool) {
    match rumdl_lib::vscode::handle_vscode_command(force, update, status) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{}: {}", "Error".red().bold(), e);
            exit::tool_error();
        }
    }
}
