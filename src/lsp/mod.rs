//! Language Server Protocol implementation for rumdl
//!
//! This module provides a Language Server Protocol (LSP) implementation for rumdl,
//! enabling real-time markdown linting in editors and IDEs.
//!
//! Following Ruff's approach, this is built directly into the main rumdl binary
//! and can be started with `rumdl server`.

pub mod server;
pub mod types;

pub use server::RumdlLanguageServer;
pub use types::{warning_to_code_action, warning_to_diagnostic, RumdlLspConfig};

use anyhow::Result;
use tokio::net::TcpListener;
use tower_lsp::{LspService, Server};

/// Start the Language Server Protocol server
/// This is the main entry point for `rumdl server`
pub async fn start_server() -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(RumdlLanguageServer::new);

    log::info!("Starting rumdl Language Server Protocol server");

    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}

/// Start the LSP server over TCP (useful for debugging)
pub async fn start_tcp_server(port: u16) -> Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    log::info!("rumdl LSP server listening on 127.0.0.1:{}", port);

    loop {
        let (stream, _) = listener.accept().await?;
        let (service, socket) = LspService::new(RumdlLanguageServer::new);

        tokio::spawn(async move {
            let (read, write) = tokio::io::split(stream);
            Server::new(read, write, socket).serve(service).await;
        });
    }
}
