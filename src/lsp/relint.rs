//! Re-linting of open documents when the workspace index changes
//!
//! Most diagnostics answer a question about the document alone, so the editor
//! event that changes the text is also the event that recomputes them. The
//! cross-file rules (MD051, MD057, MD062) answer against the workspace index
//! instead, and the index changes for reasons no editor event describes: another
//! file's headings moved, a linked file was deleted, or the initial workspace
//! scan finished after a document was already opened and linted without it.
//!
//! The index worker reports those changes as [`RelintRequest`]s. This module
//! consumes them and publishes the affected documents' diagnostics again.

use std::collections::HashSet;
use std::path::PathBuf;

use tokio::sync::mpsc;
use tower_lsp::lsp_types::Url;

use super::server::RumdlLanguageServer;
use super::types::RelintRequest;

impl RumdlLanguageServer {
    /// Consume the index worker's re-lint requests until the server shuts down.
    ///
    /// Requests arrive in bursts: one index update can name every file linking
    /// to the one that changed. They are drained into a single set first, so a
    /// document named twice in a burst is linted once.
    pub(super) async fn run_relint_worker(self, mut requests: mpsc::Receiver<RelintRequest>) {
        while let Some(first) = requests.recv().await {
            let mut paths: HashSet<PathBuf> = HashSet::new();
            let mut all_open = false;
            let mut request = Some(first);

            while let Some(current) = request {
                match current {
                    RelintRequest::File(path) => {
                        paths.insert(path);
                    }
                    RelintRequest::AllOpen => all_open = true,
                }
                request = requests.try_recv().ok();
            }

            self.republish_open_documents(&paths, all_open).await;
        }

        log::debug!("Re-lint worker stopped: the index worker is gone");
    }

    /// Publish diagnostics again for the open documents a re-lint burst names.
    ///
    /// Only documents the editor has open: a file cached from disk to answer a
    /// navigation request has no diagnostics on screen, and publishing for it
    /// would put some there.
    async fn republish_open_documents(&self, paths: &HashSet<PathBuf>, all_open: bool) {
        // Matched by resolved path rather than by URI, because the index keys
        // files the same way. A document opened through a symlinked ancestor is
        // stored under the editor's spelling and indexed under the resolved one.
        let targets: Vec<Url> = {
            let documents = self.documents.read().await;
            documents
                .iter()
                .filter(|(_, entry)| !entry.from_disk)
                .filter(|(uri, _)| all_open || super::resolve_uri(uri).is_some_and(|path| paths.contains(&path)))
                .map(|(uri, _)| uri.clone())
                .collect()
        };

        if targets.is_empty() {
            return;
        }

        if *self.client_supports_pull_diagnostics.read().await {
            // A pull client is never sent diagnostics; it asks. Telling it the
            // answers it holds may have changed is the only way to make it ask
            // again, since nothing about its documents changed on its side.
            //
            // Sent to any pull client rather than only to one advertising
            // `workspace.diagnostic.refreshSupport`: that capability arrives
            // under a key this LSP types version does not read, so gating on it
            // would silence the refresh for every real client. A client that
            // does not want it answers with an error, which is not a failure of
            // this server.
            if let Err(e) = self.client.workspace_diagnostic_refresh().await {
                log::debug!("Client did not accept workspace/diagnostic/refresh: {e}");
            }
            return;
        }

        log::debug!("Re-linting {} open document(s) after an index change", targets.len());
        for uri in targets {
            // External tools are re-run only for a whole-index change, matching
            // did_open. A per-file re-lint follows a keystroke somewhere in the
            // workspace and matches did_change, which does not re-run them.
            self.republish_diagnostics(uri, all_open).await;
        }
    }

    /// Lint an open document again and publish the result.
    ///
    /// Distinct from `update_diagnostics` in that the text is read here rather
    /// than passed in: this runs without an editor event, so there is no text to
    /// be handed. The document's version is checked again after the lint, since
    /// a keystroke during it would make these diagnostics describe text the
    /// editor no longer holds. That keystroke publishes its own diagnostics and
    /// queues its own index update, so dropping this result loses nothing.
    async fn republish_diagnostics(&self, uri: Url, run_external_tools: bool) {
        let Some((text, version)) = ({
            let documents = self.documents.read().await;
            documents
                .get(&uri)
                .filter(|entry| !entry.from_disk)
                .map(|entry| (entry.content.clone(), entry.version))
        }) else {
            return;
        };

        match self.lint_document(&uri, &text, run_external_tools).await {
            Ok(diagnostics) => {
                let current = {
                    let documents = self.documents.read().await;
                    documents.get(&uri).and_then(|entry| entry.version)
                };
                if current != version {
                    log::debug!("Dropping re-lint of {uri}: the document changed while it ran");
                    return;
                }
                self.client.publish_diagnostics(uri, diagnostics, version).await;
            }
            Err(e) => {
                log::error!("Failed to re-lint {uri}: {e}");
            }
        }
    }
}

#[cfg(test)]
#[path = "relint_tests.rs"]
mod tests;
