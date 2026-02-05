//! Handler for the `version` command.

/// Print version information.
pub fn handle_version() {
    println!("rumdl {}", env!("CARGO_PKG_VERSION"));
}
