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
use crate::workspace_index::{FileIndex, HeadingIndex, WorkspaceIndex, extract_cross_file_links};

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
        let Ok(file_index) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Self::build_file_index(content)))
        else {
            log::error!("Panic while indexing {}: skipping", path.display());
            return;
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
    pub(super) fn build_file_index(content: &str) -> FileIndex {
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let mut file_index = FileIndex::new();

        // Extract headings from the content
        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                let auto_anchor = AnchorStyle::GitHub.generate_fragment(&heading.text);
                let is_setext = matches!(
                    heading.style,
                    crate::lint_context::types::HeadingStyle::Setext1
                        | crate::lint_context::types::HeadingStyle::Setext2
                );

                file_index.add_heading(HeadingIndex {
                    text: heading.text.clone(),
                    auto_anchor,
                    custom_anchor: heading.custom_id.clone(),
                    line: line_num + 1, // 1-indexed
                    is_setext,
                });
            }
        }

        // Extract cross-file links using the shared utility
        // This ensures consistent position tracking with MD057
        let links = extract_cross_file_links(&ctx);
        for link in links.relative {
            file_index.add_cross_file_link(link);
        }
        for link in links.root_relative {
            file_index.add_root_relative_link(link);
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
///
/// Honors `.gitignore`, `.ignore`, git global excludes, and skips hidden
/// entries so generated/ignored files don't pollute the index. Runs the
/// (synchronous) filesystem walk on a blocking thread.
async fn scan_markdown_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let roots = roots.to_vec();
    tokio::task::spawn_blocking(move || collect_markdown_files(&roots))
        .await
        .unwrap_or_else(|e| {
            log::warn!("Workspace scan task failed: {e}");
            Vec::new()
        })
}

/// Build a [`WalkBuilder`](ignore::WalkBuilder) rooted at `root` with the ignore
/// options used for workspace indexing.
///
/// Centralizing the configuration keeps the full scan ([`collect_markdown_files`])
/// and the single-path check ([`path_is_ignored_for_index`]) in exact agreement
/// about which files belong in the index: same hidden/gitignore/global/exclude
/// handling, same `node_modules`/`target` exclusion.
fn index_walk_builder(root: &Path) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        // Honor .gitignore / .ignore even outside a git repository.
        .require_git(false)
        // Always skip dependency/build output directories even when not gitignored.
        .filter_entry(|entry| {
            let name = entry.file_name().to_str().unwrap_or("");
            name != "node_modules" && name != "target"
        });
    builder
}

/// Collect markdown files from the given roots, respecting ignore files.
fn collect_markdown_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for root in roots {
        for result in index_walk_builder(root).build() {
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    if entry.file_type().is_some_and(|t| t.is_file())
                        && let Some(ext) = path.extension()
                        && is_markdown_extension(ext)
                    {
                        files.push(path.to_path_buf());
                    }
                }
                Err(e) => log::warn!("Error scanning {}: {}", root.display(), e),
            }
        }
    }

    files
}

/// Whether `path` should be excluded from the workspace index based on the same
/// ignore rules used by the full scan ([`collect_markdown_files`]).
///
/// Used to keep filesystem-watch events (`did_change_watched_files`) from
/// reintroducing generated/ignored files that the full scan skips. Files the
/// user explicitly opens or edits bypass this check, since the active document
/// must stay indexed for in-file anchor completion.
///
/// Determines ignore status by walking from the containing workspace root down
/// the chain of directories leading to `path`, using the shared
/// [`index_walk_builder`] configuration. Descent is pruned to that single chain,
/// so the walk applies the same ignore rules the full scan would (including an
/// ignored ancestor directory or a hidden entry) without traversing the tree. If
/// the walk does not yield `path`, the file must not enter the index.
///
/// `node_modules`/`target` are also checked directly so the predicate works even
/// for paths that do not exist on disk. The file must exist for the walk to
/// observe it, which holds for the create/change watch events that use this.
pub(super) fn path_is_ignored_for_index(roots: &[PathBuf], path: &Path) -> bool {
    // Use the deepest workspace root that contains the file so nested roots
    // resolve their own ignore files. Paths outside every root aren't filtered.
    let Some(root) = roots
        .iter()
        .filter(|r| path.starts_with(r))
        .max_by_key(|r| r.components().count())
    else {
        return false;
    };

    // Check `node_modules`/`target` only below the workspace root, so a workspace
    // located under a directory of that name is not wholly excluded.
    if let Ok(rel) = path.strip_prefix(root)
        && rel
            .components()
            .any(|c| matches!(c, std::path::Component::Normal(name) if name == "node_modules" || name == "target"))
    {
        return true;
    }

    let target = path.to_path_buf();
    let mut builder = index_walk_builder(root);
    // Only descend into directories that lead to `target`; everything else is
    // pruned. `target.starts_with(entry)` holds for `target` and its ancestors.
    builder.filter_entry(move |entry| target.starts_with(entry.path()));
    for entry in builder.build().flatten() {
        if entry.path() == path {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_build_file_index_column_positions() {
        // Verify that column positions are correct (fix for issue #234)
        let content = "See [link](./file.md) here.\n";

        let index = IndexWorker::build_file_index(content);

        assert_eq!(index.cross_file_links.len(), 1);
        assert_eq!(index.cross_file_links[0].target_path, "./file.md");
        assert_eq!(index.cross_file_links[0].line, 1);
        // "See [link](" = 11 chars, so column 12 is where "./file.md" starts
        assert_eq!(index.cross_file_links[0].column, 12);
    }

    #[test]
    fn test_build_file_index_multiple_links() {
        let content = "First [a](./a.md) and [b](./b.md#section) links.\n";

        let index = IndexWorker::build_file_index(content);

        assert_eq!(index.cross_file_links.len(), 2);

        // First link: "First [a](" = 10 chars, column 11
        assert_eq!(index.cross_file_links[0].target_path, "./a.md");
        assert_eq!(index.cross_file_links[0].column, 11);

        // Second link: "First [a](./a.md) and [b](" = 26 chars, column 27
        assert_eq!(index.cross_file_links[1].target_path, "./b.md");
        assert_eq!(index.cross_file_links[1].fragment, "section");
        assert_eq!(index.cross_file_links[1].column, 27);
    }

    #[test]
    fn test_collect_markdown_files_respects_gitignore() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // A tracked markdown file and a build-output one that .gitignore excludes.
        fs::write(root.join("README.md"), "# Readme\n").unwrap();
        fs::write(root.join(".gitignore"), "build/\nignored.md\n").unwrap();
        fs::write(root.join("ignored.md"), "# Ignored\n").unwrap();
        fs::create_dir(root.join("build")).unwrap();
        fs::write(root.join("build").join("generated.md"), "# Generated\n").unwrap();

        // Dependency/output dirs are skipped even when not gitignored.
        fs::create_dir(root.join("node_modules")).unwrap();
        fs::write(root.join("node_modules").join("dep.md"), "# Dep\n").unwrap();

        let mut files = collect_markdown_files(&[root.to_path_buf()]);
        files.sort();

        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert_eq!(names, vec!["README.md".to_string()]);
    }

    #[test]
    fn test_collect_markdown_files_finds_nested_markdown() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("top.md"), "# Top\n").unwrap();
        fs::create_dir(root.join("docs")).unwrap();
        fs::write(root.join("docs").join("guide.markdown"), "# Guide\n").unwrap();
        fs::write(root.join("docs").join("notes.txt"), "not markdown\n").unwrap();

        let mut names: Vec<String> = collect_markdown_files(&[root.to_path_buf()])
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        names.sort();

        assert_eq!(names, vec!["guide.markdown".to_string(), "top.md".to_string()]);
    }

    #[test]
    fn test_path_is_ignored_for_index() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::write(root.join(".gitignore"), "build/\ndraft.md\n").unwrap();

        // The check walks the file's directory, so the files must exist (as they
        // do for create/change watch events).
        fs::write(root.join("README.md"), "").unwrap();
        fs::write(root.join("draft.md"), "").unwrap();
        fs::write(root.join(".hidden.md"), "").unwrap();
        fs::create_dir(root.join("docs")).unwrap();
        fs::write(root.join("docs").join("guide.md"), "").unwrap();
        fs::create_dir(root.join("build")).unwrap();
        fs::write(root.join("build").join("out.md"), "").unwrap();

        let roots = vec![root.clone()];

        // Tracked files are not ignored.
        assert!(!path_is_ignored_for_index(&roots, &root.join("README.md")));
        assert!(!path_is_ignored_for_index(&roots, &root.join("docs/guide.md")));

        // Gitignored file and file inside a gitignored directory.
        assert!(path_is_ignored_for_index(&roots, &root.join("draft.md")));
        assert!(path_is_ignored_for_index(&roots, &root.join("build/out.md")));

        // Hidden files are excluded, matching the full scan's hidden(true).
        assert!(path_is_ignored_for_index(&roots, &root.join(".hidden.md")));

        // Dependency/output dirs are always skipped, even without a gitignore rule
        // and without the file existing.
        assert!(path_is_ignored_for_index(&roots, &root.join("node_modules/dep.md")));
        assert!(path_is_ignored_for_index(&roots, &root.join("target/doc.md")));

        // Paths outside every workspace root are not filtered.
        let outside = dir.path().parent().unwrap().join("elsewhere.md");
        assert!(!path_is_ignored_for_index(&roots, &outside));
    }

    #[test]
    fn test_path_is_ignored_for_index_honors_nested_gitignore() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir(root.join("docs")).unwrap();
        fs::write(root.join("docs").join(".gitignore"), "generated.md\n").unwrap();
        fs::write(root.join("docs").join("generated.md"), "").unwrap();
        fs::write(root.join("docs").join("manual.md"), "").unwrap();

        let roots = vec![root.clone()];

        assert!(path_is_ignored_for_index(&roots, &root.join("docs/generated.md")));
        assert!(!path_is_ignored_for_index(&roots, &root.join("docs/manual.md")));
    }

    #[test]
    fn test_path_is_ignored_for_index_workspace_under_target_dir() {
        use std::fs;

        // A workspace whose own path contains a `target` component must not have
        // all of its files treated as ignored.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("target").join("my-docs");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("README.md"), "").unwrap();
        fs::create_dir(root.join("target")).unwrap();
        fs::write(root.join("target").join("out.md"), "").unwrap();

        let roots = vec![root.clone()];

        // Files directly under the workspace are indexed despite the `target`
        // ancestor in the absolute path.
        assert!(!path_is_ignored_for_index(&roots, &root.join("README.md")));
        // A `target` directory *inside* the workspace is still excluded.
        assert!(path_is_ignored_for_index(&roots, &root.join("target/out.md")));
    }
}
