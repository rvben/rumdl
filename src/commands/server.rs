//! Handler for the `server` command.

use colored::*;

use rumdl_lib::exit_codes::exit;

/// Handle the server command: start the LSP server.
pub fn handle_server(port: Option<u16>, stdio: bool, verbose: bool, config: Option<String>) {
    // If verbose flag is set, increase log level to Debug
    // (logging is already initialized in main() via RUST_LOG)
    if verbose {
        log::set_max_level(log::LevelFilter::Debug);
    }

    // Validate config file exists if provided
    if let Some(config_path) = &config
        && !std::path::Path::new(config_path).exists()
    {
        eprintln!(
            "{}: Configuration file not found: {}",
            "Error".red().bold(),
            config_path
        );
        exit::tool_error();
    }

    // Start the LSP server
    let runtime = tokio::runtime::Runtime::new().unwrap_or_else(|e| {
        eprintln!("{}: Failed to create Tokio runtime: {}", "Error".red().bold(), e);
        exit::tool_error();
    });

    runtime.block_on(async {
        if let Some(port) = port {
            // TCP mode for debugging
            if let Err(e) = rumdl_lib::lsp::start_tcp_server(port, config.as_deref()).await {
                eprintln!("Failed to start LSP server on port {port}: {e}");
                exit::tool_error();
            }
        } else {
            // Standard LSP mode over stdio (default behavior)
            // Note: stdio flag is for explicit documentation, behavior is the same
            let _ = stdio; // Suppress unused variable warning
            if let Err(e) = rumdl_lib::lsp::start_server(config.as_deref()).await {
                eprintln!("Failed to start LSP server: {e}");
                exit::tool_error();
            }
        }
    });
}
