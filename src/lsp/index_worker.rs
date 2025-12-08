//! Background worker for workspace index management
//!
//! This module provides a background task that manages the workspace index
//! for cross-file analysis. It handles debouncing rapid file updates and
//! efficiently updates the index without blocking the main LSP server.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{RwLock, mpsc};
use tower_lsp::Client;
use tower_lsp::lsp_types::*;

use crate::config::MarkdownFlavor;
use crate::lint_context::LintContext;
use crate::lsp::types::{IndexState, IndexUpdate};
use crate::utils::anchor_styles::AnchorStyle;
use crate::workspace_index::{CrossFileLinkIndex, FileIndex, HeadingIndex, WorkspaceIndex};

/// Supported markdown file extensions
const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdx", "mkd", "mkdn", "mdown", "mdwn", "qmd", "rmd"];

/// Check if a file extension is a markdown extension
#[inline]
fn is_markdown_extension(ext: &std::ffi::OsStr) -> bool {
    ext.to_str()
        .is_some_and(|s| MARKDOWN_EXTENSIONS.contains(&s.to_lowercase().as_str()))
}

/// Background worker for managing the workspace index
///
/// Receives updates via a channel and maintains the workspace index
/// with debouncing to avoid excessive re-indexing during rapid edits.
pub struct IndexWorker {
    /// Receiver for index update messages
    rx: mpsc::Receiver<IndexUpdate>,
    /// The workspace index being maintained
    workspace_index: Arc<RwLock<WorkspaceIndex>>,
    /// Current state of the index (building/ready/error)
    index_state: Arc<RwLock<IndexState>>,
    /// LSP client for progress reporting
    client: Client,
    /// Workspace root folders
    workspace_roots: Arc<RwLock<Vec<PathBuf>>>,
    /// Debouncing: path -> (content, last_update_time)
    pending: HashMap<PathBuf, (String, Instant)>,
    /// Debounce duration
    debounce_duration: Duration,
    /// Sender to request re-linting of files (back to server)
    relint_tx: mpsc::Sender<PathBuf>,
}

impl IndexWorker {
    /// Create a new index worker
    pub fn new(
        rx: mpsc::Receiver<IndexUpdate>,
        workspace_index: Arc<RwLock<WorkspaceIndex>>,
        index_state: Arc<RwLock<IndexState>>,
        client: Client,
        workspace_roots: Arc<RwLock<Vec<PathBuf>>>,
        relint_tx: mpsc::Sender<PathBuf>,
    ) -> Self {
        Self {
            rx,
            workspace_index,
            index_state,
            client,
            workspace_roots,
            pending: HashMap::new(),
            debounce_duration: Duration::from_millis(100),
            relint_tx,
        }
    }

    /// Run the index worker event loop
    pub async fn run(mut self) {
        let mut debounce_interval = tokio::time::interval(Duration::from_millis(50));

        loop {
            tokio::select! {
                // Receive updates from main server
                msg = self.rx.recv() => {
                    match msg {
                        Some(IndexUpdate::FileChanged { path, content }) => {
                            self.pending.insert(path, (content, Instant::now()));
                        }
                        Some(IndexUpdate::FileDeleted { path }) => {
                            self.handle_file_deleted(&path).await;
                        }
                        Some(IndexUpdate::FullRescan) => {
                            self.full_rescan().await;
                        }
                        Some(IndexUpdate::Shutdown) | None => {
                            log::info!("Index worker shutting down");
                            break;
                        }
                    }
                }

                // Process debounced updates periodically
                _ = debounce_interval.tick() => {
                    self.process_pending_updates().await;
                }
            }
        }
    }

    /// Process pending updates that have been debounced long enough
    async fn process_pending_updates(&mut self) {
        let now = Instant::now();
        let ready: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, (_, time))| now.duration_since(*time) >= self.debounce_duration)
            .map(|(path, _)| path.clone())
            .collect();

        for path in ready {
            if let Some((content, _)) = self.pending.remove(&path) {
                self.update_single_file(&path, &content).await;
            }
        }
    }

    /// Update a single file in the index
    async fn update_single_file(&self, path: &Path, content: &str) {
        // Build FileIndex using LintContext
        let file_index =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Self::build_file_index(content))) {
                Ok(index) => index,
                Err(_) => {
                    log::error!("Panic while indexing {}: skipping", path.display());
                    return;
                }
            };

        // Get old dependents before updating
        let old_dependents = {
            let index = self.workspace_index.read().await;
            index.get_dependents(path)
        };

        // Update the index
        {
            let mut index = self.workspace_index.write().await;
            index.update_file(path, file_index);
        }

        // Get new dependents after updating
        let new_dependents = {
            let index = self.workspace_index.read().await;
            index.get_dependents(path)
        };

        // Request re-lint of affected files (union of old and new dependents)
        let mut affected: std::collections::HashSet<PathBuf> = old_dependents.into_iter().collect();
        affected.extend(new_dependents);

        for dep_path in affected {
            if self.relint_tx.send(dep_path.clone()).await.is_err() {
                log::warn!("Failed to send re-lint request for {}", dep_path.display());
            }
        }
    }

    /// Build a FileIndex from content
    fn build_file_index(content: &str) -> FileIndex {
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let mut file_index = FileIndex::new();

        // Extract headings from the content
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                let auto_anchor = AnchorStyle::GitHub.generate_fragment(&heading.text);

                file_index.add_heading(HeadingIndex {
                    text: heading.text.clone(),
                    auto_anchor,
                    custom_anchor: heading.custom_id.clone(),
                    line: line_num + 1, // 1-indexed
                });
            }
        }

        // Extract cross-file links
        // Use pulldown-cmark to find links pointing to other files
        for link in find_cross_file_links(content) {
            file_index.add_cross_file_link(link);
        }

        file_index
    }

    /// Handle a file deletion
    async fn handle_file_deleted(&self, path: &Path) {
        // Remove pending update for this file
        // (self.pending is not accessible here directly, but FileDeleted is handled immediately)

        // Get dependents before removing
        let dependents = {
            let index = self.workspace_index.read().await;
            index.get_dependents(path)
        };

        // Remove from index
        {
            let mut index = self.workspace_index.write().await;
            index.remove_file(path);
        }

        // Request re-lint of dependent files (they now have broken links)
        for dep_path in dependents {
            if self.relint_tx.send(dep_path.clone()).await.is_err() {
                log::warn!("Failed to send re-lint request for {}", dep_path.display());
            }
        }
    }

    /// Perform a full rescan of the workspace
    async fn full_rescan(&mut self) {
        // Clear pending updates
        self.pending.clear();

        // Find all markdown files in workspace roots
        let roots = self.workspace_roots.read().await.clone();
        let files = scan_markdown_files(&roots).await;
        let total = files.len();

        if total == 0 {
            *self.index_state.write().await = IndexState::Ready;
            return;
        }

        // Set initial building state
        *self.index_state.write().await = IndexState::Building {
            progress: 0.0,
            files_indexed: 0,
            total_files: total,
        };

        // Report progress start
        self.report_progress_begin(total).await;

        // Index each file
        for (i, path) in files.iter().enumerate() {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                let file_index = Self::build_file_index(&content);

                let mut index = self.workspace_index.write().await;
                index.update_file(path, file_index);
            }

            // Report progress every 10 files or at end
            if i % 10 == 0 || i == total - 1 {
                let progress = ((i + 1) as f32 / total as f32) * 100.0;
                *self.index_state.write().await = IndexState::Building {
                    progress,
                    files_indexed: i + 1,
                    total_files: total,
                };
                self.report_progress_update(i + 1, total).await;
            }
        }

        // Mark as ready
        *self.index_state.write().await = IndexState::Ready;
        self.report_progress_done().await;

        log::info!("Workspace indexing complete: {total} files indexed");
    }

    /// Report progress begin via LSP
    async fn report_progress_begin(&self, total: usize) {
        let token = NumberOrString::String("rumdl-index".to_string());

        // Request progress token creation
        if self
            .client
            .send_request::<request::WorkDoneProgressCreate>(WorkDoneProgressCreateParams { token: token.clone() })
            .await
            .is_err()
        {
            log::debug!("Client does not support work done progress");
            return;
        }

        // Send begin notification
        self.client
            .send_notification::<notification::Progress>(ProgressParams {
                token,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(WorkDoneProgressBegin {
                    title: "Indexing workspace".to_string(),
                    cancellable: Some(false),
                    message: Some(format!("Scanning {total} markdown files...")),
                    percentage: Some(0),
                })),
            })
            .await;
    }

    /// Report progress update via LSP
    async fn report_progress_update(&self, indexed: usize, total: usize) {
        let token = NumberOrString::String("rumdl-index".to_string());
        let percentage = ((indexed as f32 / total as f32) * 100.0) as u32;

        self.client
            .send_notification::<notification::Progress>(ProgressParams {
                token,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(WorkDoneProgressReport {
                    cancellable: Some(false),
                    message: Some(format!("Indexed {indexed}/{total} files")),
                    percentage: Some(percentage),
                })),
            })
            .await;
    }

    /// Report progress done via LSP
    async fn report_progress_done(&self) {
        let token = NumberOrString::String("rumdl-index".to_string());

        self.client
            .send_notification::<notification::Progress>(ProgressParams {
                token,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(WorkDoneProgressEnd {
                    message: Some("Indexing complete".to_string()),
                })),
            })
            .await;
    }
}

/// Scan workspace roots for markdown files
async fn scan_markdown_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for root in roots {
        if let Err(e) = collect_markdown_files_recursive(root, &mut files).await {
            log::warn!("Error scanning {}: {}", root.display(), e);
        }
    }

    files
}

/// Recursively collect markdown files from a directory
async fn collect_markdown_files_recursive(dir: &PathBuf, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let file_type = entry.file_type().await?;

        if file_type.is_dir() {
            // Skip hidden directories and common non-source directories
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') && name != "node_modules" && name != "target" {
                Box::pin(collect_markdown_files_recursive(&path, files)).await?;
            }
        } else if file_type.is_file()
            && let Some(ext) = path.extension()
            && is_markdown_extension(ext)
        {
            files.push(path);
        }
    }

    Ok(())
}

/// Find cross-file links in content
fn find_cross_file_links(content: &str) -> Vec<CrossFileLinkIndex> {
    use pulldown_cmark::{Event, Parser, Tag};

    let mut links = Vec::new();
    let parser = Parser::new(content);

    let mut current_line = 1;
    let mut current_col = 1;

    for event in parser {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) | Event::Start(Tag::Image { dest_url, .. }) => {
                let url = dest_url.as_ref();

                // Skip external URLs and anchor-only links
                if url.starts_with("http://")
                    || url.starts_with("https://")
                    || url.starts_with("mailto:")
                    || url.starts_with('#')
                {
                    continue;
                }

                // Parse the URL for file path and fragment
                let (file_path, fragment) = if let Some(hash_pos) = url.find('#') {
                    (&url[..hash_pos], &url[hash_pos + 1..])
                } else {
                    (url, "")
                };

                // Only track links that point to files (with or without fragments)
                if !file_path.is_empty() {
                    links.push(CrossFileLinkIndex {
                        target_path: file_path.to_string(),
                        fragment: fragment.to_string(),
                        line: current_line,
                        column: current_col,
                    });
                }
            }
            Event::Text(text) => {
                // Update position tracking
                for ch in text.chars() {
                    if ch == '\n' {
                        current_line += 1;
                        current_col = 1;
                    } else {
                        current_col += 1;
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                current_line += 1;
                current_col = 1;
            }
            _ => {}
        }
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_cross_file_links() {
        let content = r#"
# Heading

See [other file](./other.md) for more info.

Also check [section](./guide.md#install) and [external](https://example.com).

[Same file anchor](#local) is ignored.
"#;

        let links = find_cross_file_links(content);

        assert_eq!(links.len(), 2);

        assert_eq!(links[0].target_path, "./other.md");
        assert_eq!(links[0].fragment, "");

        assert_eq!(links[1].target_path, "./guide.md");
        assert_eq!(links[1].fragment, "install");
    }

    #[test]
    fn test_find_cross_file_links_images() {
        let content = r#"
![Image](./images/photo.png)

![External](https://example.com/img.png)
"#;

        let links = find_cross_file_links(content);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_path, "./images/photo.png");
    }

    #[test]
    fn test_build_file_index() {
        let content = r#"
# Main Heading

Some text.

## Sub Heading {#sub}

More text with [link](./other.md#section).
"#;

        let index = IndexWorker::build_file_index(content);

        assert_eq!(index.headings.len(), 2);
        assert_eq!(index.headings[0].text, "Main Heading");
        assert!(index.headings[0].custom_anchor.is_none());

        // HeadingInfo.text has the custom ID stripped; the custom_id is stored separately
        assert_eq!(index.headings[1].text, "Sub Heading");
        assert_eq!(index.headings[1].custom_anchor, Some("sub".to_string()));

        assert_eq!(index.cross_file_links.len(), 1);
        assert_eq!(index.cross_file_links[0].target_path, "./other.md");
        assert_eq!(index.cross_file_links[0].fragment, "section");
    }
}
