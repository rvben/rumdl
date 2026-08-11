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

use crate::config::{Config, MarkdownFlavor};
use crate::discovery::{ExcludeMatchers, MarkdownWalkOptions, MarkdownWorkspaceScan};
use crate::lsp::server::{ConfigResolver, DocumentEntry};
use crate::lsp::types::{IndexState, IndexUpdate, RelintRequest};
use crate::rule::Rule;
use crate::workspace_index::{FileIndex, WorkspaceIndex};

/// Walk options for workspace indexing, derived from the resolved config.
///
/// Mirrors CLI discovery (gitignore handling driven by
/// `global.respect_gitignore`, hidden files included, `.markdownlintignore`
/// honored) with one deliberate divergence: `.git`/`node_modules`/`target`
/// are always skipped as an editor-performance safety net, even when not
/// gitignored.
pub(super) fn index_walk_options(config: &Config) -> MarkdownWalkOptions {
    MarkdownWalkOptions {
        respect_gitignore: config.global.respect_gitignore,
        skip_vendor_dirs: true,
    }
}

/// The rules that contribute to the cross-file index, built from the resolved
/// config so each one indexes with the settings the workspace configured.
///
/// Deliberately not filtered by the enabled-rule set: the index is what
/// navigation, completion and rename read, so disabling a rule's diagnostics is
/// not a request to lose heading anchors in the editor.
///
/// The membership is `CrossFileScope::Workspace`, but the scope is a method on
/// a constructed rule, so deriving it means building all of them and discarding
/// all but these two. The index resolves its rules once per directory, and once
/// per file under `.editorconfig`, which made that discard the dominant cost of
/// a scan. `cross_file_rules_match_the_workspace_scope` pins the list against
/// the scope every rule declares, so a third one cannot join unnoticed.
pub(super) fn cross_file_rules(config: &Config) -> Vec<Box<dyn Rule>> {
    vec![
        crate::rules::MD051LinkFragments::from_config(config),
        crate::rules::MD057ExistingRelativeLinks::from_config(config),
    ]
}

/// The configuration-derived objects needed to interpret one indexed file.
/// Kept together so cached rules cannot accidentally be paired with a flavor
/// from another configuration scope.
struct IndexConfiguration {
    config: Config,
    rules: Vec<Box<dyn Rule>>,
}

impl IndexConfiguration {
    fn new(config: Config) -> Self {
        let rules = cross_file_rules(&config);
        Self { config, rules }
    }

    fn build_file_index(&self, content: &str, path: &Path) -> FileIndex {
        IndexWorker::build_file_index(content, &self.rules, self.config.get_flavor_for_file(path), Some(path))
    }
}

/// A file update waiting out its debounce window.
struct PendingUpdate {
    /// The content to index once the window closes.
    content: String,
    /// When the update was queued, which starts the window.
    queued_at: Instant,
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
    /// Debouncing: path -> the update waiting out its window
    pending: HashMap<PathBuf, PendingUpdate>,
    /// Debounce duration
    debounce_duration: Duration,
    /// Sender to request re-linting of files (back to server)
    relint_tx: mpsc::Sender<RelintRequest>,
    /// Shared per-file configuration policy used by diagnostics.
    config_resolver: ConfigResolver,
    /// The server's document store, so a scan of the workspace indexes what an
    /// editor is showing rather than what was last written to disk.
    documents: Arc<RwLock<HashMap<Url, DocumentEntry>>>,
}

/// The state an index worker shares with the server that spawned it.
///
/// Each handle is the server's own, so the worker reads what the editor is
/// currently working with rather than a copy taken at startup.
pub(crate) struct SharedIndexState {
    pub(crate) workspace_index: Arc<RwLock<WorkspaceIndex>>,
    pub(crate) index_state: Arc<RwLock<IndexState>>,
    pub(crate) workspace_roots: Arc<RwLock<Vec<PathBuf>>>,
    pub(crate) config_resolver: ConfigResolver,
    pub(crate) documents: Arc<RwLock<HashMap<Url, DocumentEntry>>>,
}

impl IndexWorker {
    /// Create a new index worker
    pub(crate) fn new(
        rx: mpsc::Receiver<IndexUpdate>,
        client: Client,
        relint_tx: mpsc::Sender<RelintRequest>,
        shared: SharedIndexState,
    ) -> Self {
        let SharedIndexState {
            workspace_index,
            index_state,
            workspace_roots,
            config_resolver,
            documents,
        } = shared;
        Self {
            rx,
            workspace_index,
            index_state,
            client,
            workspace_roots,
            pending: HashMap::new(),
            debounce_duration: Duration::from_millis(100),
            relint_tx,
            config_resolver,
            documents,
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
                            self.pending.insert(path, PendingUpdate {
                                content,
                                queued_at: Instant::now(),
                            });
                        }
                        Some(IndexUpdate::FileRemoved { path }) => {
                            // A change is debounced and a removal is not, so an
                            // update queued moments earlier is still waiting
                            // here. Flushing it afterwards would put the path
                            // back in an index that no longer covers it, and
                            // nothing would take it out again.
                            self.pending.remove(&path);
                            self.handle_file_removed(&path).await;
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
            .filter(|(_, pending)| now.duration_since(pending.queued_at) >= self.debounce_duration)
            .map(|(path, _)| path.clone())
            .collect();

        if ready.is_empty() {
            return;
        }

        let mut directory_configs: HashMap<PathBuf, IndexConfiguration> = HashMap::new();
        for path in ready {
            if let Some(pending) = self.pending.remove(&path) {
                let directory = path.parent().unwrap_or(&path);
                if let Some(index_config) = directory_configs.get(directory) {
                    self.update_single_file(&path, &pending.content, index_config).await;
                    continue;
                }

                let config = self.config_resolver.resolve_effective_config_for_file(&path).await;
                let index_config = IndexConfiguration::new(config);
                self.update_single_file(&path, &pending.content, &index_config).await;
                directory_configs.insert(directory.to_path_buf(), index_config);
            }
        }
    }

    /// Update a single file in the index
    async fn update_single_file(&self, path: &Path, content: &str, index_config: &IndexConfiguration) {
        let Ok(file_index) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            index_config.build_file_index(content, path)
        })) else {
            log::error!("Panic while indexing {}: skipping", path.display());
            return;
        };

        // What the index held for this file, so the update can answer whether it
        // changed anything a cross-file check reads. Typing in a paragraph
        // rewrites the entry with the same links and anchors, and re-linting
        // every open file that links here on each pause in typing would cost a
        // full lint per file for an answer that cannot have changed.
        let previous = {
            let index = self.workspace_index.read().await;
            index.get_file(path).cloned()
        };
        let changed = previous
            .as_ref()
            .is_none_or(|previous| previous.extracted_data_differs(&file_index));
        // Whether a link is involved on either side, so this file is worth
        // re-linting itself. A link removed is as much a change as one added:
        // the diagnostic it produced is on screen until something recomputes it.
        let links_involved = !file_index.cross_file_links.is_empty()
            || previous.is_some_and(|previous| !previous.cross_file_links.is_empty());

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

        if !changed {
            return;
        }

        // Get new dependents after updating
        let new_dependents = {
            let index = self.workspace_index.read().await;
            index.get_dependents(path)
        };

        // Request re-lint of affected files (union of old and new dependents)
        let mut affected: std::collections::HashSet<PathBuf> = old_dependents.into_iter().collect();
        affected.extend(new_dependents);

        // The file itself: its own cross-file diagnostics were computed against
        // the entry this update just replaced, which for the document being
        // typed in is the one the editor holds.
        if links_involved {
            affected.insert(path.to_path_buf());
        }

        for dep_path in affected {
            self.request_relint(RelintRequest::File(dep_path)).await;
        }
    }

    /// Ask the server to publish a document's diagnostics again.
    ///
    /// A closed channel means the server is gone, which happens on shutdown and
    /// is not worth a warning; the request has nowhere useful to arrive.
    async fn request_relint(&self, request: RelintRequest) {
        if self.relint_tx.send(request).await.is_err() {
            log::debug!("Re-lint channel closed; skipping re-lint request");
        }
    }

    /// Build a FileIndex from content, parsing with the file's Markdown flavor so
    /// the index (anchors, cross-file links, and the symbols built from it) matches
    /// what diagnostics and the document outline see.
    ///
    /// The rules themselves say what a file contributes, through the same builder
    /// the CLI uses, so the editor and the command line agree on which anchors
    /// exist. Hand-rolling it here made them disagree: anchors were always
    /// generated GitHub-style whatever the flavor and never deduplicated, HTML
    /// and attribute anchors were missing entirely, and the inline-disable state
    /// cross-file checks honor was never exported, so a `<!-- rumdl-disable -->`
    /// held in the editor while the CLI honored it.
    ///
    /// Build `rules` with [`cross_file_rules`].
    pub(super) fn build_file_index(
        content: &str,
        rules: &[Box<dyn Rule>],
        flavor: MarkdownFlavor,
        path: Option<&Path>,
    ) -> FileIndex {
        crate::build_file_index_only(content, rules, flavor, path.map(Path::to_path_buf))
    }

    /// Drop a file from the index, whether it was deleted or stopped being one
    /// the index covers.
    async fn handle_file_removed(&self, path: &Path) {
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
            self.request_relint(RelintRequest::File(dep_path)).await;
        }
    }

    /// The content of every document an editor holds, keyed by the path it
    /// indexes under.
    ///
    /// The index is keyed by path, so it answers with one version of a file
    /// however many URI spellings name it, which is what the update messages
    /// keyed by path already assume.
    async fn open_buffers(&self) -> HashMap<PathBuf, String> {
        self.documents
            .read()
            .await
            .iter()
            .filter(|(_, entry)| !entry.from_disk)
            .filter_map(|(uri, entry)| Some((crate::lsp::resolve_uri(uri)?, entry.content.clone())))
            .collect()
    }

    /// Perform a full rescan of the workspace
    async fn full_rescan(&mut self) {
        // Every waiting update is about to be superseded: a scan reads the
        // filesystem for the disk-originated ones and the editor's own buffer
        // for the rest, both of which are at least as new as what is waiting
        // here, because a document is stored before its update is queued.
        self.pending.clear();

        // File selection remains a workspace-level decision. Once selected,
        // each document is interpreted with its own effective configuration.
        let roots = self.workspace_roots.read().await.clone();
        let config = self.config_resolver.workspace_config().await;
        let options = index_walk_options(&config);
        let includes = config.global.include.clone();
        let excludes = ExcludeMatchers::new(&config.global.exclude);
        for (pattern, error) in &excludes.invalid {
            log::warn!("Invalid exclude pattern '{pattern}': {error}");
        }
        let mut files = scan_markdown_files(&roots, options, includes, excludes).await;

        // A document an editor holds belongs in the index whatever discovery says
        // about it, because opening one indexes it: a scan that dropped it would
        // be the rescan taking it back out. Its content comes from the buffer as
        // well, since the filesystem holds the last save, and answering
        // cross-file questions from that describes a version of the file the
        // editor stopped showing.
        let open_buffers = self.open_buffers().await;
        let mut current: std::collections::HashSet<PathBuf> = files.iter().cloned().collect();
        for path in open_buffers.keys() {
            // Except where the file is gone, which no buffer speaks for: a
            // rename reaches the server as a deletion of the old path, and the
            // document can still be open under it when this runs. Asked as
            // whether a file is there rather than whether anything is, because a
            // directory that took the name answers the weaker question and the
            // scan would never hand such a path back.
            if tokio::fs::metadata(path).await.is_ok_and(|meta| meta.is_file()) && current.insert(path.clone()) {
                files.push(path.clone());
            }
        }
        let total = files.len();

        // Evict entries the scan no longer covers (deleted files, newly excluded
        // or gitignored ones) so navigation and completions stop surfacing them.
        {
            let removed = self.workspace_index.write().await.retain_only(&current);
            if removed > 0 {
                log::info!("Workspace rescan evicted {removed} stale index entries");
            }
        }

        if total == 0 {
            *self.index_state.write().await = IndexState::Ready;
            self.request_relint(RelintRequest::AllOpen).await;
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

        // Files in one directory share every setting that can affect the
        // workspace index. `.editorconfig` can vary lint-only settings between
        // neighbors, but it cannot change Markdown flavor or either
        // workspace-scoped rule, an invariant pinned by an integration test.
        // Cache by directory so a large scan constructs those rules once.
        let mut directory_configs: HashMap<PathBuf, IndexConfiguration> = HashMap::new();

        // Index each file, an open document from the buffer read above.
        for (i, path) in files.iter().enumerate() {
            let content = match open_buffers.get(path) {
                Some(buffer) => Some(buffer.clone()),
                None => tokio::fs::read_to_string(path).await.ok(),
            };
            if let Some(content) = content {
                let directory = path.parent().unwrap_or(path);
                let file_index = if let Some(index_config) = directory_configs.get(directory) {
                    index_config.build_file_index(&content, path)
                } else {
                    let config = self.config_resolver.resolve_effective_config_for_file(path).await;
                    let index_config = IndexConfiguration::new(config);
                    let file_index = index_config.build_file_index(&content, path);
                    directory_configs.insert(directory.to_path_buf(), index_config);
                    file_index
                };

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

        // Every document opened while the scan ran was linted with cross-file
        // checks skipped, because those are gated on the index being ready.
        // Nothing else recomputes them, so without this the editor shows an
        // incomplete answer until the file is edited.
        self.request_relint(RelintRequest::AllOpen).await;
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
/// Applies the shared discovery semantics (gitignore handling per config,
/// `.markdownlintignore`, hidden files included, vendor dirs skipped) plus
/// config `include` and `exclude` patterns. Runs the (synchronous) filesystem
/// walk on a blocking thread.
async fn scan_markdown_files(
    roots: &[PathBuf],
    options: MarkdownWalkOptions,
    includes: Vec<String>,
    excludes: ExcludeMatchers,
) -> Vec<PathBuf> {
    let roots = roots.to_vec();
    tokio::task::spawn_blocking(move || collect_markdown_files(&roots, &options, &includes, &excludes))
        .await
        .unwrap_or_else(|e| {
            log::warn!("Workspace scan task failed: {e}");
            Vec::new()
        })
}

/// Collect the files selected by the production workspace-index configuration.
fn collect_markdown_files(
    roots: &[PathBuf],
    options: &MarkdownWalkOptions,
    includes: &[String],
    excludes: &ExcludeMatchers,
) -> Vec<PathBuf> {
    MarkdownWorkspaceScan::new(options, includes, excludes).collect(roots)
}

/// Whether `path` should be excluded from the workspace index based on the
/// production full-scan configuration.
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
pub(super) fn path_is_ignored_for_index(
    roots: &[PathBuf],
    path: &Path,
    options: &MarkdownWalkOptions,
    includes: &[String],
    excludes: &ExcludeMatchers,
) -> bool {
    MarkdownWorkspaceScan::new(options, includes, excludes).path_is_ignored(roots, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::CrossFileScope;

    /// Index `content` the way the worker does, with the default configuration.
    fn build_index(content: &str, flavor: MarkdownFlavor) -> FileIndex {
        let rules = cross_file_rules(&Config::default());
        IndexWorker::build_file_index(content, &rules, flavor, None)
    }

    /// `cross_file_rules` names its members rather than deriving them, so this
    /// is what keeps the list honest: a rule that starts declaring
    /// `CrossFileScope::Workspace` fails here until the index builds it too,
    /// and one that stops declaring it fails until the index drops it.
    #[test]
    fn cross_file_rules_match_the_workspace_scope() {
        let config = Config::default();
        let names = |rules: &[Box<dyn Rule>]| rules.iter().map(|rule| rule.name().to_string()).collect::<Vec<_>>();

        let declared = crate::rules::all_rules(&config)
            .into_iter()
            .filter(|rule| rule.cross_file_scope() == CrossFileScope::Workspace)
            .collect::<Vec<_>>();

        assert!(
            !declared.is_empty(),
            "control: the scope must be reachable, or this test says nothing"
        );
        assert_eq!(names(&declared), names(&cross_file_rules(&config)));
    }

    #[test]
    fn test_build_file_index() {
        let content = r#"
# Main Heading

Some text.

## Sub Heading {#sub}

More text with [link](./other.md#section).
"#;

        let index = build_index(content, MarkdownFlavor::default());

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
    fn test_build_file_index_respects_flavor() {
        // `# -8<- [start:x]` is a heading in Standard markdown but a MkDocs snippet
        // marker. The index must parse with the file's flavor so anchors, cross-file
        // navigation, and workspace symbols all agree with the document outline.
        let content = "# Real\n\n# -8<- [start:section]\n";

        let standard = build_index(content, MarkdownFlavor::Standard);
        assert_eq!(
            standard.headings.len(),
            2,
            "Standard treats the snippet line as a heading"
        );

        let mkdocs = build_index(content, MarkdownFlavor::MkDocs);
        assert_eq!(mkdocs.headings.len(), 1, "MkDocs excludes the snippet marker");
        assert_eq!(mkdocs.headings[0].text, "Real");
    }

    #[test]
    fn test_build_file_index_column_positions() {
        // Verify that column positions are correct (fix for issue #234)
        let content = "See [link](./file.md) here.\n";

        let index = build_index(content, MarkdownFlavor::default());

        assert_eq!(index.cross_file_links.len(), 1);
        assert_eq!(index.cross_file_links[0].target_path, "./file.md");
        assert_eq!(index.cross_file_links[0].line, 1);
        // "See [link](" = 11 chars, so column 12 is where "./file.md" starts
        assert_eq!(index.cross_file_links[0].column, 12);
    }

    #[test]
    fn test_build_file_index_multiple_links() {
        let content = "First [a](./a.md) and [b](./b.md#section) links.\n";

        let index = build_index(content, MarkdownFlavor::default());

        assert_eq!(index.cross_file_links.len(), 2);

        let find = |target: &str| {
            index
                .cross_file_links
                .iter()
                .find(|link| link.target_path == target)
                .unwrap_or_else(|| panic!("no indexed link to {target}: {:?}", index.cross_file_links))
        };

        // Only MD057 indexes a link with no fragment, and it points at the
        // destination: "First [a](" = 10 chars, column 11.
        assert_eq!(find("./a.md").column, 11);

        // MD051 indexes a link that carries one and points at the link itself,
        // where its cross-file diagnostic belongs: "First [a](./a.md) and " = 22
        // chars, column 23. It contributes first, so its position is the one kept.
        let fragment_link = find("./b.md");
        assert_eq!(fragment_link.fragment, "section");
        assert_eq!(fragment_link.column, 23);
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

        let mut files = collect_markdown_files(
            &[root.to_path_buf()],
            &index_walk_options(&Config::default()),
            &[],
            &ExcludeMatchers::new(&[]),
        );
        files.sort();

        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert_eq!(names, vec!["README.md".to_string()]);
    }

    #[test]
    fn test_collect_markdown_files_applies_config_excludes() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("README.md"), "# Readme\n").unwrap();
        fs::create_dir(root.join("drafts")).unwrap();
        fs::write(root.join("drafts").join("wip.md"), "# WIP\n").unwrap();

        // A bare directory pattern must exclude the directory's contents,
        // matching CLI behavior.
        let excludes = ExcludeMatchers::new(&["drafts".to_string()]);
        let names: Vec<String> = collect_markdown_files(
            &[root.to_path_buf()],
            &index_walk_options(&Config::default()),
            &[],
            &excludes,
        )
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();

        assert_eq!(names, vec!["README.md".to_string()]);
    }

    #[test]
    fn test_collect_markdown_files_honors_absolute_exclude_patterns() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        // Canonicalize the way production does, so the pattern built below has
        // the shape an expanded `~` produces (on Windows that means no verbatim
        // `\\?\` prefix, which would match nothing).
        let root = crate::discovery::canonicalize_for_matching(dir.path()).unwrap();

        fs::write(root.join("README.md"), "# Readme\n").unwrap();
        fs::create_dir(root.join("drafts")).unwrap();
        fs::write(root.join("drafts").join("wip.md"), "# WIP\n").unwrap();

        // An absolute pattern - what a `~/...` pattern expands to - must
        // exclude in the workspace scan just as it does in the CLI walk.
        let pattern = format!("{}/drafts", root.to_string_lossy().replace('\\', "/"));
        let names: Vec<String> = collect_markdown_files(
            std::slice::from_ref(&root),
            &index_walk_options(&Config::default()),
            &[],
            &ExcludeMatchers::new(&[pattern]),
        )
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();

        assert_eq!(names, vec!["README.md".to_string()]);
    }

    #[test]
    fn test_collect_markdown_files_can_disable_gitignore() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join(".gitignore"), "ignored.md\n").unwrap();
        fs::write(root.join("ignored.md"), "# Ignored\n").unwrap();

        let mut config = Config::default();
        config.global.respect_gitignore = false;
        let names: Vec<String> = collect_markdown_files(
            &[root.to_path_buf()],
            &index_walk_options(&config),
            &[],
            &ExcludeMatchers::new(&[]),
        )
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();

        assert_eq!(names, vec!["ignored.md".to_string()]);
    }

    #[test]
    fn test_collect_markdown_files_includes_hidden_files() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir(root.join(".github")).unwrap();
        fs::write(root.join(".github").join("PULL_REQUEST_TEMPLATE.md"), "# PR\n").unwrap();
        fs::write(root.join("README.md"), "# Readme\n").unwrap();

        let mut names: Vec<String> = collect_markdown_files(
            &[root.to_path_buf()],
            &index_walk_options(&Config::default()),
            &[],
            &ExcludeMatchers::new(&[]),
        )
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();
        names.sort();

        // Hidden files lint in the CLI, so the index must cover them too.
        assert_eq!(
            names,
            vec!["PULL_REQUEST_TEMPLATE.md".to_string(), "README.md".to_string()]
        );
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

        let mut names: Vec<String> = collect_markdown_files(
            &[root.to_path_buf()],
            &index_walk_options(&Config::default()),
            &[],
            &ExcludeMatchers::new(&[]),
        )
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();
        names.sort();

        assert_eq!(names, vec!["guide.markdown".to_string(), "top.md".to_string()]);
    }

    #[test]
    fn test_workspace_index_applies_includes_to_scan_and_watch_events() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir(root.join("docs")).unwrap();
        fs::create_dir(root.join("templates")).unwrap();
        fs::write(root.join("README.md"), "# Readme\n").unwrap();
        fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
        fs::write(root.join("templates/page.md.jinja"), "# Template\n").unwrap();

        let roots = vec![root.clone()];
        let options = index_walk_options(&Config::default());
        let includes = vec!["docs/**".to_string(), "templates/**/*.md.jinja".to_string()];
        let excludes = ExcludeMatchers::new(&[]);

        let names: Vec<String> = collect_markdown_files(&roots, &options, &includes, &excludes)
            .iter()
            .map(|path| path.strip_prefix(&root).unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["docs/guide.md", "templates/page.md.jinja"]);

        assert!(path_is_ignored_for_index(
            &roots,
            &root.join("README.md"),
            &options,
            &includes,
            &excludes
        ));
        assert!(!path_is_ignored_for_index(
            &roots,
            &root.join("templates/page.md.jinja"),
            &options,
            &includes,
            &excludes
        ));
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
        let options = index_walk_options(&Config::default());
        let no_excludes = ExcludeMatchers::new(&[]);

        // Tracked files are not ignored.
        assert!(!path_is_ignored_for_index(
            &roots,
            &root.join("README.md"),
            &options,
            &[],
            &no_excludes
        ));
        assert!(!path_is_ignored_for_index(
            &roots,
            &root.join("docs/guide.md"),
            &options,
            &[],
            &no_excludes
        ));

        // Gitignored file and file inside a gitignored directory.
        assert!(path_is_ignored_for_index(
            &roots,
            &root.join("draft.md"),
            &options,
            &[],
            &no_excludes
        ));
        assert!(path_is_ignored_for_index(
            &roots,
            &root.join("build/out.md"),
            &options,
            &[],
            &no_excludes
        ));

        // Hidden files are indexed, matching the CLI which lints them.
        assert!(!path_is_ignored_for_index(
            &roots,
            &root.join(".hidden.md"),
            &options,
            &[],
            &no_excludes
        ));

        // Dependency/output dirs are always skipped, even without a gitignore rule
        // and without the file existing.
        assert!(path_is_ignored_for_index(
            &roots,
            &root.join("node_modules/dep.md"),
            &options,
            &[],
            &no_excludes
        ));
        assert!(path_is_ignored_for_index(
            &roots,
            &root.join("target/doc.md"),
            &options,
            &[],
            &no_excludes
        ));

        // Config exclude patterns are honored, matched root-relative.
        let excludes = ExcludeMatchers::new(&["docs".to_string()]);
        assert!(path_is_ignored_for_index(
            &roots,
            &root.join("docs/guide.md"),
            &options,
            &[],
            &excludes
        ));
        assert!(!path_is_ignored_for_index(
            &roots,
            &root.join("README.md"),
            &options,
            &[],
            &excludes
        ));

        // Paths outside every workspace root are not filtered.
        let outside = dir.path().parent().unwrap().join("elsewhere.md");
        assert!(!path_is_ignored_for_index(
            &roots,
            &outside,
            &options,
            &[],
            &no_excludes
        ));
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
        let options = index_walk_options(&Config::default());
        let no_excludes = ExcludeMatchers::new(&[]);

        assert!(path_is_ignored_for_index(
            &roots,
            &root.join("docs/generated.md"),
            &options,
            &[],
            &no_excludes
        ));
        assert!(!path_is_ignored_for_index(
            &roots,
            &root.join("docs/manual.md"),
            &options,
            &[],
            &no_excludes
        ));
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
        let options = index_walk_options(&Config::default());
        let no_excludes = ExcludeMatchers::new(&[]);

        // Files directly under the workspace are indexed despite the `target`
        // ancestor in the absolute path.
        assert!(!path_is_ignored_for_index(
            &roots,
            &root.join("README.md"),
            &options,
            &[],
            &no_excludes
        ));
        // A `target` directory *inside* the workspace is still excluded.
        assert!(path_is_ignored_for_index(
            &roots,
            &root.join("target/out.md"),
            &options,
            &[],
            &no_excludes
        ));
    }

    /// The guard deciding whether an index update is worth re-linting for.
    ///
    /// Every keystroke rewrites the typed file's entry, so answering "changed"
    /// for a prose edit would lint every file linking here on each pause in
    /// typing, for an answer that cannot have moved.
    #[test]
    fn test_extracted_data_differs_ignores_a_prose_only_edit() {
        let before = build_index(
            "# Guide\n\nProse.\n\nSee [other](./other.md#section).\n",
            MarkdownFlavor::default(),
        );
        let after = build_index(
            "# Guide\n\nProse, now with a clause typed into it.\n\nSee [other](./other.md#section).\n",
            MarkdownFlavor::default(),
        );

        // Control: the two really are different documents, so a `false` here is
        // the guard answering rather than the test comparing a value to itself.
        assert_ne!(before.content_hash, after.content_hash);
        assert!(!before.extracted_data_differs(&after));
    }

    #[test]
    fn test_extracted_data_differs_reports_a_renamed_heading() {
        // What a file's dependents read: rename the anchor they link to and
        // their diagnostics change, with no event in their own documents.
        let before = build_index("# Setup\n", MarkdownFlavor::default());
        let after = build_index("# Installation\n", MarkdownFlavor::default());

        assert!(before.extracted_data_differs(&after));
    }

    #[test]
    fn test_extracted_data_differs_reports_a_new_link() {
        // What the typed file itself reads: a link just written has no
        // diagnostic yet, and nothing but this update will ask for one.
        let before = build_index("# Guide\n\nProse.\n", MarkdownFlavor::default());
        let after = build_index("# Guide\n\nSee [other](./other.md#nope).\n", MarkdownFlavor::default());

        assert!(before.extracted_data_differs(&after));
    }
}
