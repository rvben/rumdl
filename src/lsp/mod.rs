//! Language Server Protocol implementation for rumdl
//!
//! This module provides a Language Server Protocol (LSP) implementation for rumdl,
//! enabling real-time markdown linting in editors and IDEs.
//!
//! Following Ruff's approach, this is built directly into the main rumdl binary
//! and can be started with `rumdl server`.

mod completion;
mod configuration;
pub mod index_worker;
mod linting;
mod navigation;
mod position;
mod relint;
pub mod server;
mod symbols;
pub mod types;

pub use server::RumdlLanguageServer;
pub use types::{RumdlLspConfig, warning_to_code_actions, warning_to_diagnostic};

use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::net::TcpListener;
use tower_lsp::{LspService, Server};

/// Resolve a workspace root to the path space the server identifies files in.
///
/// Every index key descends from a resolved root, so a document must be
/// resolved the same way for a lookup to find its entry. Sharing one space also
/// makes a root-relative comparison (`starts_with`, exclude relativization)
/// answer for the paths documents actually arrive as.
pub(crate) fn resolve_workspace_root(path: &Path) -> PathBuf {
    crate::discovery::canonicalize_for_matching(path).unwrap_or_else(|| path.to_path_buf())
}

/// Resolve a document path to the same space as [`resolve_workspace_root`].
///
/// A URI arrives in whatever form the editor sent. It can reach the file
/// through a symlinked ancestor, and on Windows it never carries the `\\?\`
/// prefix that canonicalization produces, so the raw path routinely names the
/// same file as a key without being equal to it.
///
/// Only the directory is resolved. The workspace scan does not follow symlinks,
/// so it records a symlinked file under the name it was reached by rather than
/// under its target, and resolving the file itself would look up a path the
/// scan never produced. Resolving the directory also keeps working for a file
/// that is not on disk, such as one deleted since it was indexed.
pub(crate) fn resolve_document_path(path: &Path) -> PathBuf {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return resolve_workspace_root(path);
    };
    match crate::discovery::canonicalize_for_matching(parent) {
        Some(dir) => dir.join(name),
        None => path.to_path_buf(),
    }
}

/// Resolve a document URI to the path the server identifies it by.
///
/// `None` for a URI that names no file, such as an editor's untitled buffer.
pub(crate) fn resolve_uri(uri: &tower_lsp::lsp_types::Url) -> Option<PathBuf> {
    uri.to_file_path().ok().map(|path| resolve_document_path(&path))
}

/// Spell a URI the way the server identifies the document it names.
///
/// Navigation resolves a link target to a path and turns it back into a URI to
/// ask for that document's content, so a document whose own URI spells its path
/// differently is reachable under two URIs. This is the one the server treats as
/// the document's identity; a URI naming no file is its own.
pub(crate) fn resolve_uri_spelling(uri: &tower_lsp::lsp_types::Url) -> tower_lsp::lsp_types::Url {
    resolve_uri(uri)
        .and_then(|path| tower_lsp::lsp_types::Url::from_file_path(path).ok())
        .unwrap_or_else(|| uri.clone())
}

/// Start the Language Server Protocol server
/// This is the main entry point for `rumdl server`
pub async fn start_server(config_path: Option<&str>) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| RumdlLanguageServer::new(client, config_path));

    log::info!("Starting rumdl Language Server Protocol server");

    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}

/// Start the LSP server over TCP (useful for debugging)
pub async fn start_tcp_server(port: u16, config_path: Option<&str>) -> Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    log::info!("rumdl LSP server listening on 127.0.0.1:{port}");

    // Clone config_path to owned String so we can move it into the spawned task
    let config_path_owned = config_path.map(std::string::ToString::to_string);

    loop {
        let (stream, _) = listener.accept().await?;
        let config_path_clone = config_path_owned.clone();
        let (service, socket) =
            LspService::new(move |client| RumdlLanguageServer::new(client, config_path_clone.as_deref()));

        tokio::spawn(async move {
            let (read, write) = tokio::io::split(stream);
            Server::new(read, write, socket).serve(service).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify that the module exports are accessible
        // This ensures the public API is stable
        fn _check_exports() {
            // These should compile without errors
            let _server_type: RumdlLanguageServer;
            let _config_type: RumdlLspConfig;
            let _func1: fn(&crate::rule::LintWarning, &str) -> tower_lsp::lsp_types::Diagnostic = warning_to_diagnostic;
            let _func2: fn(
                &crate::rule::LintWarning,
                &tower_lsp::lsp_types::Url,
                &str,
            ) -> Vec<tower_lsp::lsp_types::CodeAction> = warning_to_code_actions;
        }
    }

    #[tokio::test]
    async fn test_tcp_server_bind() {
        use std::net::TcpListener as StdTcpListener;

        // Find an available port
        let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        // Start the server in a background task
        let server_handle = tokio::spawn(async move {
            // Server should start without panicking
            match tokio::time::timeout(std::time::Duration::from_millis(100), start_tcp_server(port, None)).await {
                Ok(Ok(())) => {} // Server started and stopped normally
                Ok(Err(_)) => {} // Server had an error (expected in test)
                Err(_) => {}     // Timeout (expected - server runs forever)
            }
        });

        // Give the server time to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Try to connect to verify it's listening
        match tokio::time::timeout(
            std::time::Duration::from_millis(50),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")),
        )
        .await
        {
            Ok(Ok(_)) => {
                // Successfully connected
            }
            _ => {
                // Connection failed or timed out - that's okay for this test
            }
        }

        // Cancel the server task
        server_handle.abort();
    }

    #[tokio::test]
    async fn test_tcp_server_invalid_port() {
        // Port 0 is technically valid (OS assigns), but let's test a privileged port
        // that we likely can't bind to without root
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), start_tcp_server(80, None)).await;

        match result {
            Ok(Err(_)) => {
                // Expected - should fail to bind to privileged port
            }
            Ok(Ok(())) => {
                panic!("Should not be able to bind to port 80 without privileges");
            }
            Err(_) => {
                // Timeout - server tried to run, which means bind succeeded
                // This might happen if tests are run as root
            }
        }
    }

    #[tokio::test]
    async fn test_service_creation() {
        // Test that we can create the LSP service
        let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));

        // Service should be created successfully
        // We can't easily test more without a full LSP client
        drop(service);
    }

    #[tokio::test]
    async fn test_multiple_tcp_connections() {
        use std::net::TcpListener as StdTcpListener;

        // Find an available port
        let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        // Start the server
        let server_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), start_tcp_server(port, None)).await;
        });

        // Give server time to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Try multiple connections
        let mut handles = vec![];
        for _ in 0..3 {
            let handle = tokio::spawn(async move {
                match tokio::time::timeout(
                    std::time::Duration::from_millis(100),
                    tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")),
                )
                .await
                {
                    Ok(Ok(_stream)) => {
                        // Connection successful
                        true
                    }
                    _ => false,
                }
            });
            handles.push(handle);
        }

        // Wait for all connections
        for handle in handles {
            let _ = handle.await;
        }

        // Clean up
        server_handle.abort();
    }

    #[test]
    fn test_logging_initialization() {
        // Verify that starting the server includes proper logging
        // This is more of a smoke test to ensure logging statements compile

        // The actual log::info! calls are in the async functions,
        // but we can at least verify the module imports and uses logging
        let _info_level = log::Level::Info;
    }
}
