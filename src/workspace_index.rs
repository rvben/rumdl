//! Workspace-wide index for cross-file analysis
//!
//! This module provides infrastructure for rules that need to validate
//! references across multiple files, such as MD051 which validates that
//! cross-file link fragments point to valid headings.
//!
//! The index is built in parallel and designed for minimal memory overhead.
//!
//! ## Cache Format
//!
//! The workspace index can be persisted to disk for faster startup on
//! repeated runs. The cache format includes a version header to detect
//! incompatible format changes:
//!
//! ```text
//! [4 bytes: magic "RWSI" - Rumdl Workspace Index]
//! [4 bytes: format version (u32 little-endian)]
//! [N bytes: postcard-serialized WorkspaceIndex]
//! ```

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::lint_context::LintContext;
use crate::utils::element_cache::ElementCache;

// =============================================================================
// URL Decoding Helper
// =============================================================================

/// Convert a hex digit character to its numeric value (0-15)
fn hex_digit_to_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// URL-decode a string, handling percent-encoded characters.
/// Returns the decoded string, or the original if decoding fails.
/// Used for matching URL-encoded CJK fragments against raw anchors.
fn url_decode(s: &str) -> String {
    // Fast path: no percent signs means no encoding
    if !s.contains('%') {
        return s.to_string();
    }

    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            // Try to parse the two hex digits following %
            let hex1 = bytes[i + 1];
            let hex2 = bytes[i + 2];
            if let (Some(d1), Some(d2)) = (hex_digit_to_value(hex1), hex_digit_to_value(hex2)) {
                result.push(d1 * 16 + d2);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }

    // Convert to UTF-8, falling back to original if invalid
    String::from_utf8(result).unwrap_or_else(|_| s.to_string())
}

// =============================================================================
// Shared cross-file link extraction utilities
//
// These regexes and helpers are the canonical implementation for extracting
// cross-file links. Both MD057 and LSP use this shared code path for correct
// position tracking.
// =============================================================================

/// Regex to match the start of a link
static LINK_START_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!?\[[^\]]*\]").unwrap());

/// Regex to extract the URL from an angle-bracketed markdown link
/// Format: `](<URL>)` or `](<URL> "title")`
static URL_EXTRACT_ANGLE_BRACKET_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\]\(\s*<([^>]+)>(#[^\)\s]*)?\s*(?:"[^"]*")?\s*\)"#).unwrap());

/// Regex to extract the URL from a normal markdown link (without angle brackets)
/// Format: `](URL)` or `](URL "title")`
static URL_EXTRACT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"]\(\s*([^>)\s#]+)(#[^)\s]*)?\s*(?:"[^"]*")?\s*\)"#).unwrap());

/// Regex to detect URLs with explicit schemes
static PROTOCOL_DOMAIN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([a-zA-Z][a-zA-Z0-9+.-]*://|[a-zA-Z][a-zA-Z0-9+.-]*:|www\.)").unwrap());

/// Supported markdown file extensions
const MARKDOWN_EXTENSIONS: &[&str] = &[
    ".md",
    ".markdown",
    ".mdx",
    ".mkd",
    ".mkdn",
    ".mdown",
    ".mdwn",
    ".qmd",
    ".rmd",
];

/// Check if a path has a markdown extension (case-insensitive)
#[inline]
fn is_markdown_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();
    MARKDOWN_EXTENSIONS.iter().any(|ext| path_lower.ends_with(ext))
}

/// Strip query parameters and fragments from a URL path
/// Returns the path portion before `?` or `#`
fn strip_query_and_fragment(url: &str) -> &str {
    let query_pos = url.find('?');
    let fragment_pos = url.find('#');

    match (query_pos, fragment_pos) {
        (Some(q), Some(f)) => &url[..q.min(f)],
        (Some(q), None) => &url[..q],
        (None, Some(f)) => &url[..f],
        (None, None) => url,
    }
}

/// Extract cross-file links from content using correct regex-based position tracking.
///
/// This is the canonical implementation used by both MD057 and LSP to ensure
/// consistent and correct column positions for diagnostic reporting.
///
/// Returns a vector of `CrossFileLinkIndex` entries, one for each markdown file
/// link found in the content.
pub fn extract_cross_file_links(ctx: &LintContext) -> Vec<CrossFileLinkIndex> {
    let content = ctx.content;

    // Early returns for performance
    if content.is_empty() || !content.contains("](") {
        return Vec::new();
    }

    let mut links = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let element_cache = ElementCache::new(content);
    let line_index = &ctx.line_index;

    // Track which lines we've already processed to avoid duplicates
    // (ctx.links may have multiple entries for the same line)
    let mut processed_lines = HashSet::new();

    for link in &ctx.links {
        let line_idx = link.line - 1;
        if line_idx >= lines.len() {
            continue;
        }

        // Skip if we've already processed this line
        if !processed_lines.insert(line_idx) {
            continue;
        }

        let line = lines[line_idx];
        if !line.contains("](") {
            continue;
        }

        // Find all links in this line
        for link_match in LINK_START_REGEX.find_iter(line) {
            let start_pos = link_match.start();
            let end_pos = link_match.end();

            // Calculate absolute position for code span detection
            let line_start_byte = line_index.get_line_start_byte(line_idx + 1).unwrap_or(0);
            let absolute_start_pos = line_start_byte + start_pos;

            // Skip if in code span
            if element_cache.is_in_code_span(absolute_start_pos) {
                continue;
            }

            // Extract the URL (group 1) and fragment (group 2)
            // Try angle-bracket regex first (handles URLs with parens)
            let caps_result = URL_EXTRACT_ANGLE_BRACKET_REGEX
                .captures_at(line, end_pos - 1)
                .or_else(|| URL_EXTRACT_REGEX.captures_at(line, end_pos - 1));

            if let Some(caps) = caps_result
                && let Some(url_group) = caps.get(1)
            {
                let file_path = url_group.as_str().trim();

                // Skip empty, external, template variables, absolute URL paths,
                // framework aliases, fragment-only URLs, or rustdoc intra-doc links
                if file_path.is_empty()
                    || PROTOCOL_DOMAIN_REGEX.is_match(file_path)
                    || file_path.starts_with("www.")
                    || file_path.starts_with('#')
                    || file_path.starts_with("{{")
                    || file_path.starts_with("{%")
                    || file_path.starts_with('/')
                    || file_path.starts_with('~')
                    || file_path.starts_with('@')
                    || (file_path.starts_with('`') && file_path.ends_with('`'))
                {
                    continue;
                }

                // Strip query parameters before indexing
                let file_path = strip_query_and_fragment(file_path);

                // Get fragment from capture group 2 (includes # prefix)
                let fragment = caps.get(2).map(|m| m.as_str().trim_start_matches('#')).unwrap_or("");

                // Only index markdown file links for cross-file validation
                if is_markdown_file(file_path) {
                    links.push(CrossFileLinkIndex {
                        target_path: file_path.to_string(),
                        fragment: fragment.to_string(),
                        line: link.line,
                        column: url_group.start() + 1,
                    });
                }
            }
        }
    }

    links
}

/// Magic bytes identifying a workspace index cache file
#[cfg(feature = "native")]
const CACHE_MAGIC: &[u8; 4] = b"RWSI";

/// Cache format version - increment when WorkspaceIndex serialization changes
#[cfg(feature = "native")]
const CACHE_FORMAT_VERSION: u32 = 5;

/// Cache file name within the version directory
#[cfg(feature = "native")]
const CACHE_FILE_NAME: &str = "workspace_index.bin";

/// Workspace-wide index for cross-file analysis
///
/// Contains pre-extracted information from all markdown files in the workspace,
/// enabling rules to validate cross-file references efficiently.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WorkspaceIndex {
    /// Map from file path to its extracted data
    files: HashMap<PathBuf, FileIndex>,
    /// Reverse dependency graph: target file → files that link to it
    /// Used to efficiently re-lint dependent files when a target changes
    reverse_deps: HashMap<PathBuf, HashSet<PathBuf>>,
    /// Version counter for cache invalidation (incremented on any change)
    version: u64,
}

/// Index data extracted from a single file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileIndex {
    /// Headings in this file with their anchors
    pub headings: Vec<HeadingIndex>,
    /// Reference links in this file (for cross-file analysis)
    pub reference_links: Vec<ReferenceLinkIndex>,
    /// Cross-file links in this file (for MD051 cross-file validation)
    pub cross_file_links: Vec<CrossFileLinkIndex>,
    /// Defined reference IDs (e.g., from [ref]: url definitions)
    /// Used to filter out reference links that have explicit definitions
    pub defined_references: HashSet<String>,
    /// Content hash for change detection
    pub content_hash: String,
    /// O(1) anchor lookup: lowercased anchor → heading index
    /// Includes both auto-generated and custom anchors
    anchor_to_heading: HashMap<String, usize>,
    /// HTML anchors defined via <a id="..."> or <element id="..."> tags
    /// Stored lowercase for case-insensitive matching
    html_anchors: HashSet<String>,
    /// Attribute anchors defined via { #id } syntax (kramdown/MkDocs attr_list)
    /// Can appear on any element, not just headings
    /// Stored lowercase for case-insensitive matching
    attribute_anchors: HashSet<String>,
    /// Rules disabled for the entire file (from inline comments)
    /// Used by cross-file rules to respect inline disable directives
    pub file_disabled_rules: HashSet<String>,
    /// Rules disabled at specific lines (line number -> set of rule names)
    /// Merges both persistent disables and line-specific disables
    pub line_disabled_rules: HashMap<usize, HashSet<String>>,
}

/// Information about a heading for cross-file lookup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadingIndex {
    /// The heading text (e.g., "Installation Guide")
    pub text: String,
    /// Auto-generated anchor (e.g., "installation-guide")
    pub auto_anchor: String,
    /// Custom anchor if present (e.g., "install")
    pub custom_anchor: Option<String>,
    /// Line number (1-indexed)
    pub line: usize,
}

/// Information about a reference link for cross-file analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceLinkIndex {
    /// The reference ID (the part in [text][ref])
    pub reference_id: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
}

/// Information about a cross-file link for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossFileLinkIndex {
    /// The target file path (relative, as it appears in the link)
    pub target_path: String,
    /// The fragment/anchor being linked to (without #)
    pub fragment: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
}

/// Information about a vulnerable anchor (heading without custom ID)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerableAnchor {
    /// File path where the heading is located
    pub file: PathBuf,
    /// Line number of the heading
    pub line: usize,
    /// The heading text
    pub text: String,
}

impl WorkspaceIndex {
    /// Create a new empty workspace index
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the current version (for cache invalidation)
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Get the number of indexed files
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Check if a file is in the index
    pub fn contains_file(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    /// Get the index data for a specific file
    pub fn get_file(&self, path: &Path) -> Option<&FileIndex> {
        self.files.get(path)
    }

    /// Insert or update a file's index data
    pub fn insert_file(&mut self, path: PathBuf, index: FileIndex) {
        self.files.insert(path, index);
        self.version = self.version.wrapping_add(1);
    }

    /// Remove a file from the index
    pub fn remove_file(&mut self, path: &Path) -> Option<FileIndex> {
        // Clean up reverse deps for this file
        self.clear_reverse_deps_for(path);

        let result = self.files.remove(path);
        if result.is_some() {
            self.version = self.version.wrapping_add(1);
        }
        result
    }

    /// Build a map of all "vulnerable" anchors across the workspace
    ///
    /// A vulnerable anchor is an auto-generated anchor for a heading that
    /// does NOT have a custom anchor defined. These are problematic for
    /// translated content because the anchor changes when the heading is translated.
    ///
    /// Returns: Map from lowercase anchor → Vec of VulnerableAnchor info
    /// Multiple files can have headings with the same auto-generated anchor,
    /// so we collect all occurrences.
    pub fn get_vulnerable_anchors(&self) -> HashMap<String, Vec<VulnerableAnchor>> {
        let mut vulnerable: HashMap<String, Vec<VulnerableAnchor>> = HashMap::new();

        for (file_path, file_index) in &self.files {
            for heading in &file_index.headings {
                // Only include headings WITHOUT custom anchors
                if heading.custom_anchor.is_none() && !heading.auto_anchor.is_empty() {
                    let anchor_key = heading.auto_anchor.to_lowercase();
                    vulnerable.entry(anchor_key).or_default().push(VulnerableAnchor {
                        file: file_path.clone(),
                        line: heading.line,
                        text: heading.text.clone(),
                    });
                }
            }
        }

        vulnerable
    }

    /// Get all headings across the workspace (for debugging/testing)
    pub fn all_headings(&self) -> impl Iterator<Item = (&Path, &HeadingIndex)> {
        self.files
            .iter()
            .flat_map(|(path, index)| index.headings.iter().map(move |h| (path.as_path(), h)))
    }

    /// Iterate over all files in the index
    pub fn files(&self) -> impl Iterator<Item = (&Path, &FileIndex)> {
        self.files.iter().map(|(p, i)| (p.as_path(), i))
    }

    /// Clear the entire index
    pub fn clear(&mut self) {
        self.files.clear();
        self.reverse_deps.clear();
        self.version = self.version.wrapping_add(1);
    }

    /// Update a file's index and maintain reverse dependencies
    ///
    /// This method:
    /// 1. Removes this file as a source (dependent) from all reverse deps
    /// 2. Inserts the new file index
    /// 3. Builds new reverse deps from cross_file_links
    pub fn update_file(&mut self, path: &Path, index: FileIndex) {
        // Remove this file as a source (dependent) from all target entries
        // Note: We don't remove it as a target - other files may still link to it
        self.clear_reverse_deps_as_source(path);

        // Build new reverse deps from cross_file_links
        for link in &index.cross_file_links {
            let target = self.resolve_target_path(path, &link.target_path);
            self.reverse_deps.entry(target).or_default().insert(path.to_path_buf());
        }

        self.files.insert(path.to_path_buf(), index);
        self.version = self.version.wrapping_add(1);
    }

    /// Get files that depend on (link to) the given file
    ///
    /// Returns a list of file paths that contain links targeting this file.
    /// Used to re-lint dependent files when a target file changes.
    pub fn get_dependents(&self, path: &Path) -> Vec<PathBuf> {
        self.reverse_deps
            .get(path)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Check if a file needs re-indexing based on its content hash
    ///
    /// Returns `true` if the file is not in the index or has a different hash.
    pub fn is_file_stale(&self, path: &Path, current_hash: &str) -> bool {
        self.files
            .get(path)
            .map(|f| f.content_hash != current_hash)
            .unwrap_or(true)
    }

    /// Retain only files that exist in the given set, removing deleted files
    ///
    /// This prunes stale entries from the cache for files that no longer exist.
    /// Returns the number of files removed.
    pub fn retain_only(&mut self, current_files: &std::collections::HashSet<PathBuf>) -> usize {
        let before_count = self.files.len();

        // Collect files to remove
        let to_remove: Vec<PathBuf> = self
            .files
            .keys()
            .filter(|path| !current_files.contains(*path))
            .cloned()
            .collect();

        // Remove each file properly (clears reverse deps)
        for path in &to_remove {
            self.remove_file(path);
        }

        before_count - self.files.len()
    }

    /// Save the workspace index to a cache file
    ///
    /// Uses postcard for efficient binary serialization with:
    /// - Magic header for file type validation
    /// - Format version for compatibility detection
    /// - Atomic writes (temp file + rename) to prevent corruption
    #[cfg(feature = "native")]
    pub fn save_to_cache(&self, cache_dir: &Path) -> std::io::Result<()> {
        use std::fs;
        use std::io::Write;

        // Ensure cache directory exists
        fs::create_dir_all(cache_dir)?;

        // Serialize the index data using postcard
        let encoded = postcard::to_allocvec(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        // Build versioned cache file: [magic][version][data]
        let mut cache_data = Vec::with_capacity(8 + encoded.len());
        cache_data.extend_from_slice(CACHE_MAGIC);
        cache_data.extend_from_slice(&CACHE_FORMAT_VERSION.to_le_bytes());
        cache_data.extend_from_slice(&encoded);

        // Write atomically: write to temp file then rename
        let final_path = cache_dir.join(CACHE_FILE_NAME);
        let temp_path = cache_dir.join(format!("{}.tmp.{}", CACHE_FILE_NAME, std::process::id()));

        // Write to temp file
        {
            let mut file = fs::File::create(&temp_path)?;
            file.write_all(&cache_data)?;
            file.sync_all()?;
        }

        // Atomic rename
        fs::rename(&temp_path, &final_path)?;

        log::debug!(
            "Saved workspace index to cache: {} files, {} bytes (format v{})",
            self.files.len(),
            cache_data.len(),
            CACHE_FORMAT_VERSION
        );

        Ok(())
    }

    /// Load the workspace index from a cache file
    ///
    /// Returns `None` if:
    /// - Cache file doesn't exist
    /// - Magic header doesn't match
    /// - Format version is incompatible
    /// - Data is corrupted
    #[cfg(feature = "native")]
    pub fn load_from_cache(cache_dir: &Path) -> Option<Self> {
        use std::fs;

        let path = cache_dir.join(CACHE_FILE_NAME);
        let data = fs::read(&path).ok()?;

        // Validate header: need at least 8 bytes for magic + version
        if data.len() < 8 {
            log::warn!("Workspace index cache too small, discarding");
            let _ = fs::remove_file(&path);
            return None;
        }

        // Check magic header
        if &data[0..4] != CACHE_MAGIC {
            log::warn!("Workspace index cache has invalid magic header, discarding");
            let _ = fs::remove_file(&path);
            return None;
        }

        // Check format version
        let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        if version != CACHE_FORMAT_VERSION {
            log::info!(
                "Workspace index cache format version mismatch (got {version}, expected {CACHE_FORMAT_VERSION}), rebuilding"
            );
            let _ = fs::remove_file(&path);
            return None;
        }

        // Deserialize the index data using postcard
        match postcard::from_bytes::<Self>(&data[8..]) {
            Ok(index) => {
                log::debug!(
                    "Loaded workspace index from cache: {} files (format v{})",
                    index.files.len(),
                    version
                );
                Some(index)
            }
            Err(e) => {
                log::warn!("Failed to deserialize workspace index cache: {e}");
                let _ = fs::remove_file(&path);
                None
            }
        }
    }

    /// Remove a file as a source from all reverse dependency entries
    ///
    /// This removes the file from being listed as a dependent in all target entries.
    /// Used when updating a file (we need to remove old outgoing links before adding new ones).
    fn clear_reverse_deps_as_source(&mut self, path: &Path) {
        for deps in self.reverse_deps.values_mut() {
            deps.remove(path);
        }
        // Clean up empty entries
        self.reverse_deps.retain(|_, deps| !deps.is_empty());
    }

    /// Remove a file completely from reverse dependency tracking
    ///
    /// Removes the file as both a source (dependent) and as a target.
    /// Used when deleting a file from the index.
    fn clear_reverse_deps_for(&mut self, path: &Path) {
        // Remove as source (dependent)
        self.clear_reverse_deps_as_source(path);

        // Also remove as target
        self.reverse_deps.remove(path);
    }

    /// Resolve a relative path from a source file to an absolute target path
    fn resolve_target_path(&self, source_file: &Path, relative_target: &str) -> PathBuf {
        // Get the directory containing the source file
        let source_dir = source_file.parent().unwrap_or(Path::new(""));

        // Join with the relative target and normalize
        let target = source_dir.join(relative_target);

        // Normalize the path (handle .., ., etc.)
        Self::normalize_path(&target)
    }

    /// Normalize a path by resolving . and .. components
    fn normalize_path(path: &Path) -> PathBuf {
        let mut components = Vec::new();

        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    // Go up one level if possible
                    if !components.is_empty() {
                        components.pop();
                    }
                }
                std::path::Component::CurDir => {
                    // Skip current directory markers
                }
                _ => {
                    components.push(component);
                }
            }
        }

        components.iter().collect()
    }
}

impl FileIndex {
    /// Create a new empty file index
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a file index with the given content hash
    pub fn with_hash(content_hash: String) -> Self {
        Self {
            content_hash,
            ..Default::default()
        }
    }

    /// Add a heading to the index
    ///
    /// Also updates the anchor lookup map for O(1) anchor queries
    pub fn add_heading(&mut self, heading: HeadingIndex) {
        let index = self.headings.len();

        // Add auto-generated anchor to lookup map (lowercased for case-insensitive matching)
        self.anchor_to_heading.insert(heading.auto_anchor.to_lowercase(), index);

        // Add custom anchor if present
        if let Some(ref custom) = heading.custom_anchor {
            self.anchor_to_heading.insert(custom.to_lowercase(), index);
        }

        self.headings.push(heading);
    }

    /// Check if an anchor exists in this file (O(1) lookup)
    ///
    /// Returns true if the anchor matches any of:
    /// - Auto-generated heading anchors
    /// - Custom heading anchors (from {#id} syntax on headings)
    /// - HTML anchors (from <a id="..."> or <element id="...">)
    /// - Attribute anchors (from { #id } syntax on non-heading elements)
    ///
    /// Matching is case-insensitive. URL-encoded anchors (e.g., CJK characters
    /// like `%E6%97%A5%E6%9C%AC%E8%AA%9E` for `日本語`) are decoded before matching.
    pub fn has_anchor(&self, anchor: &str) -> bool {
        let lower = anchor.to_lowercase();

        // Fast path: try exact match first
        if self.anchor_to_heading.contains_key(&lower)
            || self.html_anchors.contains(&lower)
            || self.attribute_anchors.contains(&lower)
        {
            return true;
        }

        // Slow path: if anchor contains percent-encoding, try decoded version
        if anchor.contains('%') {
            let decoded = url_decode(anchor).to_lowercase();
            if decoded != lower {
                return self.anchor_to_heading.contains_key(&decoded)
                    || self.html_anchors.contains(&decoded)
                    || self.attribute_anchors.contains(&decoded);
            }
        }

        false
    }

    /// Add an HTML anchor (from <a id="..."> or <element id="..."> tags)
    pub fn add_html_anchor(&mut self, anchor: String) {
        if !anchor.is_empty() {
            self.html_anchors.insert(anchor.to_lowercase());
        }
    }

    /// Add an attribute anchor (from { #id } syntax on non-heading elements)
    pub fn add_attribute_anchor(&mut self, anchor: String) {
        if !anchor.is_empty() {
            self.attribute_anchors.insert(anchor.to_lowercase());
        }
    }

    /// Get the heading index for an anchor (O(1) lookup)
    ///
    /// Returns the index into `self.headings` if found.
    pub fn get_heading_by_anchor(&self, anchor: &str) -> Option<&HeadingIndex> {
        self.anchor_to_heading
            .get(&anchor.to_lowercase())
            .and_then(|&idx| self.headings.get(idx))
    }

    /// Add a reference link to the index
    pub fn add_reference_link(&mut self, link: ReferenceLinkIndex) {
        self.reference_links.push(link);
    }

    /// Check if a rule is disabled at a specific line
    ///
    /// Used by cross-file rules to respect inline disable directives.
    /// Checks both file-wide disables and line-specific disables.
    pub fn is_rule_disabled_at_line(&self, rule_name: &str, line: usize) -> bool {
        // Check file-wide disables (highest priority)
        if self.file_disabled_rules.contains("*") || self.file_disabled_rules.contains(rule_name) {
            return true;
        }

        // Check line-specific disables
        if let Some(rules) = self.line_disabled_rules.get(&line) {
            return rules.contains("*") || rules.contains(rule_name);
        }

        false
    }

    /// Add a cross-file link to the index (deduplicates by target_path, fragment, line)
    pub fn add_cross_file_link(&mut self, link: CrossFileLinkIndex) {
        // Deduplicate: multiple rules may contribute the same link with different columns
        // (e.g., MD051 uses link start, MD057 uses URL start)
        let is_duplicate = self.cross_file_links.iter().any(|existing| {
            existing.target_path == link.target_path && existing.fragment == link.fragment && existing.line == link.line
        });
        if !is_duplicate {
            self.cross_file_links.push(link);
        }
    }

    /// Add a defined reference ID (e.g., from [ref]: url)
    pub fn add_defined_reference(&mut self, ref_id: String) {
        self.defined_references.insert(ref_id);
    }

    /// Check if a reference ID has an explicit definition
    pub fn has_defined_reference(&self, ref_id: &str) -> bool {
        self.defined_references.contains(ref_id)
    }

    /// Check if the content hash matches
    pub fn hash_matches(&self, hash: &str) -> bool {
        self.content_hash == hash
    }

    /// Get the number of headings
    pub fn heading_count(&self) -> usize {
        self.headings.len()
    }

    /// Get the number of reference links
    pub fn reference_link_count(&self) -> usize {
        self.reference_links.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_index_basic() {
        let mut index = WorkspaceIndex::new();
        assert_eq!(index.file_count(), 0);
        assert_eq!(index.version(), 0);

        let mut file_index = FileIndex::with_hash("abc123".to_string());
        file_index.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: None,
            line: 1,
        });

        index.insert_file(PathBuf::from("docs/install.md"), file_index);
        assert_eq!(index.file_count(), 1);
        assert_eq!(index.version(), 1);

        assert!(index.contains_file(Path::new("docs/install.md")));
        assert!(!index.contains_file(Path::new("docs/other.md")));
    }

    #[test]
    fn test_vulnerable_anchors() {
        let mut index = WorkspaceIndex::new();

        // File 1: heading without custom anchor (vulnerable)
        let mut file1 = FileIndex::new();
        file1.add_heading(HeadingIndex {
            text: "Getting Started".to_string(),
            auto_anchor: "getting-started".to_string(),
            custom_anchor: None,
            line: 1,
        });
        index.insert_file(PathBuf::from("docs/guide.md"), file1);

        // File 2: heading with custom anchor (not vulnerable)
        let mut file2 = FileIndex::new();
        file2.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: Some("install".to_string()),
            line: 1,
        });
        index.insert_file(PathBuf::from("docs/install.md"), file2);

        let vulnerable = index.get_vulnerable_anchors();
        assert_eq!(vulnerable.len(), 1);
        assert!(vulnerable.contains_key("getting-started"));
        assert!(!vulnerable.contains_key("installation"));

        let anchors = vulnerable.get("getting-started").unwrap();
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].file, PathBuf::from("docs/guide.md"));
        assert_eq!(anchors[0].text, "Getting Started");
    }

    #[test]
    fn test_vulnerable_anchors_multiple_files_same_anchor() {
        // Multiple files can have headings with the same auto-generated anchor
        // get_vulnerable_anchors() should collect all of them
        let mut index = WorkspaceIndex::new();

        // File 1: has "Installation" heading (vulnerable)
        let mut file1 = FileIndex::new();
        file1.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: None,
            line: 1,
        });
        index.insert_file(PathBuf::from("docs/en/guide.md"), file1);

        // File 2: also has "Installation" heading with same anchor (vulnerable)
        let mut file2 = FileIndex::new();
        file2.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: None,
            line: 5,
        });
        index.insert_file(PathBuf::from("docs/fr/guide.md"), file2);

        // File 3: has "Installation" but WITH custom anchor (not vulnerable)
        let mut file3 = FileIndex::new();
        file3.add_heading(HeadingIndex {
            text: "Installation".to_string(),
            auto_anchor: "installation".to_string(),
            custom_anchor: Some("install".to_string()),
            line: 10,
        });
        index.insert_file(PathBuf::from("docs/de/guide.md"), file3);

        let vulnerable = index.get_vulnerable_anchors();
        assert_eq!(vulnerable.len(), 1); // One unique anchor
        assert!(vulnerable.contains_key("installation"));

        let anchors = vulnerable.get("installation").unwrap();
        // Should have 2 entries (en and fr), NOT 3 (de has custom anchor)
        assert_eq!(anchors.len(), 2, "Should collect both vulnerable anchors");

        // Verify both files are represented
        let files: std::collections::HashSet<_> = anchors.iter().map(|a| &a.file).collect();
        assert!(files.contains(&PathBuf::from("docs/en/guide.md")));
        assert!(files.contains(&PathBuf::from("docs/fr/guide.md")));
    }

    #[test]
    fn test_file_index_hash() {
        let index = FileIndex::with_hash("hash123".to_string());
        assert!(index.hash_matches("hash123"));
        assert!(!index.hash_matches("other"));
    }

    #[test]
    fn test_version_increment() {
        let mut index = WorkspaceIndex::new();
        assert_eq!(index.version(), 0);

        index.insert_file(PathBuf::from("a.md"), FileIndex::new());
        assert_eq!(index.version(), 1);

        index.insert_file(PathBuf::from("b.md"), FileIndex::new());
        assert_eq!(index.version(), 2);

        index.remove_file(Path::new("a.md"));
        assert_eq!(index.version(), 3);

        // Removing non-existent file doesn't increment
        index.remove_file(Path::new("nonexistent.md"));
        assert_eq!(index.version(), 3);
    }

    #[test]
    fn test_reverse_deps_basic() {
        let mut index = WorkspaceIndex::new();

        // File A links to file B
        let mut file_a = FileIndex::new();
        file_a.add_cross_file_link(CrossFileLinkIndex {
            target_path: "b.md".to_string(),
            fragment: "section".to_string(),
            line: 10,
            column: 5,
        });
        index.update_file(Path::new("docs/a.md"), file_a);

        // Check that B has A as a dependent
        let dependents = index.get_dependents(Path::new("docs/b.md"));
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0], PathBuf::from("docs/a.md"));

        // A has no dependents
        let a_dependents = index.get_dependents(Path::new("docs/a.md"));
        assert!(a_dependents.is_empty());
    }

    #[test]
    fn test_reverse_deps_multiple() {
        let mut index = WorkspaceIndex::new();

        // Files A and C both link to B
        let mut file_a = FileIndex::new();
        file_a.add_cross_file_link(CrossFileLinkIndex {
            target_path: "../b.md".to_string(),
            fragment: "".to_string(),
            line: 1,
            column: 1,
        });
        index.update_file(Path::new("docs/sub/a.md"), file_a);

        let mut file_c = FileIndex::new();
        file_c.add_cross_file_link(CrossFileLinkIndex {
            target_path: "b.md".to_string(),
            fragment: "".to_string(),
            line: 1,
            column: 1,
        });
        index.update_file(Path::new("docs/c.md"), file_c);

        // B should have both A and C as dependents
        let dependents = index.get_dependents(Path::new("docs/b.md"));
        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains(&PathBuf::from("docs/sub/a.md")));
        assert!(dependents.contains(&PathBuf::from("docs/c.md")));
    }

    #[test]
    fn test_reverse_deps_update_clears_old() {
        let mut index = WorkspaceIndex::new();

        // File A initially links to B
        let mut file_a = FileIndex::new();
        file_a.add_cross_file_link(CrossFileLinkIndex {
            target_path: "b.md".to_string(),
            fragment: "".to_string(),
            line: 1,
            column: 1,
        });
        index.update_file(Path::new("docs/a.md"), file_a);

        // Verify B has A as dependent
        assert_eq!(index.get_dependents(Path::new("docs/b.md")).len(), 1);

        // Update A to link to C instead of B
        let mut file_a_updated = FileIndex::new();
        file_a_updated.add_cross_file_link(CrossFileLinkIndex {
            target_path: "c.md".to_string(),
            fragment: "".to_string(),
            line: 1,
            column: 1,
        });
        index.update_file(Path::new("docs/a.md"), file_a_updated);

        // B should no longer have A as dependent
        assert!(index.get_dependents(Path::new("docs/b.md")).is_empty());

        // C should now have A as dependent
        let c_deps = index.get_dependents(Path::new("docs/c.md"));
        assert_eq!(c_deps.len(), 1);
        assert_eq!(c_deps[0], PathBuf::from("docs/a.md"));
    }

    #[test]
    fn test_reverse_deps_remove_file() {
        let mut index = WorkspaceIndex::new();

        // File A links to B
        let mut file_a = FileIndex::new();
        file_a.add_cross_file_link(CrossFileLinkIndex {
            target_path: "b.md".to_string(),
            fragment: "".to_string(),
            line: 1,
            column: 1,
        });
        index.update_file(Path::new("docs/a.md"), file_a);

        // Verify B has A as dependent
        assert_eq!(index.get_dependents(Path::new("docs/b.md")).len(), 1);

        // Remove file A
        index.remove_file(Path::new("docs/a.md"));

        // B should no longer have any dependents
        assert!(index.get_dependents(Path::new("docs/b.md")).is_empty());
    }

    #[test]
    fn test_normalize_path() {
        // Test .. handling
        let path = Path::new("docs/sub/../other.md");
        let normalized = WorkspaceIndex::normalize_path(path);
        assert_eq!(normalized, PathBuf::from("docs/other.md"));

        // Test . handling
        let path2 = Path::new("docs/./other.md");
        let normalized2 = WorkspaceIndex::normalize_path(path2);
        assert_eq!(normalized2, PathBuf::from("docs/other.md"));

        // Test multiple ..
        let path3 = Path::new("a/b/c/../../d.md");
        let normalized3 = WorkspaceIndex::normalize_path(path3);
        assert_eq!(normalized3, PathBuf::from("a/d.md"));
    }

    #[test]
    fn test_clear_clears_reverse_deps() {
        let mut index = WorkspaceIndex::new();

        // File A links to B
        let mut file_a = FileIndex::new();
        file_a.add_cross_file_link(CrossFileLinkIndex {
            target_path: "b.md".to_string(),
            fragment: "".to_string(),
            line: 1,
            column: 1,
        });
        index.update_file(Path::new("docs/a.md"), file_a);

        // Verify B has A as dependent
        assert_eq!(index.get_dependents(Path::new("docs/b.md")).len(), 1);

        // Clear the index
        index.clear();

        // Both files and reverse deps should be cleared
        assert_eq!(index.file_count(), 0);
        assert!(index.get_dependents(Path::new("docs/b.md")).is_empty());
    }

    #[test]
    fn test_is_file_stale() {
        let mut index = WorkspaceIndex::new();

        // Non-existent file is always stale
        assert!(index.is_file_stale(Path::new("nonexistent.md"), "hash123"));

        // Add a file with known hash
        let file_index = FileIndex::with_hash("hash123".to_string());
        index.insert_file(PathBuf::from("docs/test.md"), file_index);

        // Same hash means not stale
        assert!(!index.is_file_stale(Path::new("docs/test.md"), "hash123"));

        // Different hash means stale
        assert!(index.is_file_stale(Path::new("docs/test.md"), "different_hash"));
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_cache_roundtrip() {
        use std::fs;

        // Create a temp directory
        let temp_dir = std::env::temp_dir().join("rumdl_test_cache_roundtrip");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create an index with some data
        let mut index = WorkspaceIndex::new();

        let mut file1 = FileIndex::with_hash("abc123".to_string());
        file1.add_heading(HeadingIndex {
            text: "Test Heading".to_string(),
            auto_anchor: "test-heading".to_string(),
            custom_anchor: Some("test".to_string()),
            line: 1,
        });
        file1.add_cross_file_link(CrossFileLinkIndex {
            target_path: "./other.md".to_string(),
            fragment: "section".to_string(),
            line: 5,
            column: 3,
        });
        index.update_file(Path::new("docs/file1.md"), file1);

        let mut file2 = FileIndex::with_hash("def456".to_string());
        file2.add_heading(HeadingIndex {
            text: "Another Heading".to_string(),
            auto_anchor: "another-heading".to_string(),
            custom_anchor: None,
            line: 1,
        });
        index.update_file(Path::new("docs/other.md"), file2);

        // Save to cache
        index.save_to_cache(&temp_dir).expect("Failed to save cache");

        // Verify cache file exists
        assert!(temp_dir.join("workspace_index.bin").exists());

        // Load from cache
        let loaded = WorkspaceIndex::load_from_cache(&temp_dir).expect("Failed to load cache");

        // Verify data matches
        assert_eq!(loaded.file_count(), 2);
        assert!(loaded.contains_file(Path::new("docs/file1.md")));
        assert!(loaded.contains_file(Path::new("docs/other.md")));

        // Check file1 details
        let file1_loaded = loaded.get_file(Path::new("docs/file1.md")).unwrap();
        assert_eq!(file1_loaded.content_hash, "abc123");
        assert_eq!(file1_loaded.headings.len(), 1);
        assert_eq!(file1_loaded.headings[0].text, "Test Heading");
        assert_eq!(file1_loaded.headings[0].custom_anchor, Some("test".to_string()));
        assert_eq!(file1_loaded.cross_file_links.len(), 1);
        assert_eq!(file1_loaded.cross_file_links[0].target_path, "./other.md");

        // Check reverse deps were serialized correctly
        let dependents = loaded.get_dependents(Path::new("docs/other.md"));
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0], PathBuf::from("docs/file1.md"));

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_cache_missing_file() {
        let temp_dir = std::env::temp_dir().join("rumdl_test_cache_missing");
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Should return None for non-existent cache
        let result = WorkspaceIndex::load_from_cache(&temp_dir);
        assert!(result.is_none());
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_cache_corrupted_file() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join("rumdl_test_cache_corrupted");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Write corrupted data (too small for header)
        fs::write(temp_dir.join("workspace_index.bin"), b"bad").unwrap();

        // Should return None for corrupted cache (and remove the file)
        let result = WorkspaceIndex::load_from_cache(&temp_dir);
        assert!(result.is_none());

        // Corrupted file should be removed
        assert!(!temp_dir.join("workspace_index.bin").exists());

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_cache_invalid_magic() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join("rumdl_test_cache_invalid_magic");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Write data with wrong magic header
        let mut data = Vec::new();
        data.extend_from_slice(b"XXXX"); // Wrong magic
        data.extend_from_slice(&1u32.to_le_bytes()); // Version 1
        data.extend_from_slice(&[0; 100]); // Some garbage data
        fs::write(temp_dir.join("workspace_index.bin"), &data).unwrap();

        // Should return None for invalid magic
        let result = WorkspaceIndex::load_from_cache(&temp_dir);
        assert!(result.is_none());

        // File should be removed
        assert!(!temp_dir.join("workspace_index.bin").exists());

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_cache_version_mismatch() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join("rumdl_test_cache_version_mismatch");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Write data with correct magic but wrong version
        let mut data = Vec::new();
        data.extend_from_slice(b"RWSI"); // Correct magic
        data.extend_from_slice(&999u32.to_le_bytes()); // Future version
        data.extend_from_slice(&[0; 100]); // Some garbage data
        fs::write(temp_dir.join("workspace_index.bin"), &data).unwrap();

        // Should return None for version mismatch
        let result = WorkspaceIndex::load_from_cache(&temp_dir);
        assert!(result.is_none());

        // File should be removed to trigger rebuild
        assert!(!temp_dir.join("workspace_index.bin").exists());

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_cache_atomic_write() {
        use std::fs;

        // Test that atomic writes work (no temp files left behind)
        let temp_dir = std::env::temp_dir().join("rumdl_test_cache_atomic");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let index = WorkspaceIndex::new();
        index.save_to_cache(&temp_dir).expect("Failed to save");

        // Only the final cache file should exist, no temp files
        let entries: Vec<_> = fs::read_dir(&temp_dir).unwrap().collect();
        assert_eq!(entries.len(), 1);
        assert!(temp_dir.join("workspace_index.bin").exists());

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_has_anchor_auto_generated() {
        let mut file_index = FileIndex::new();
        file_index.add_heading(HeadingIndex {
            text: "Installation Guide".to_string(),
            auto_anchor: "installation-guide".to_string(),
            custom_anchor: None,
            line: 1,
        });

        // Should find by auto-generated anchor
        assert!(file_index.has_anchor("installation-guide"));

        // Case-insensitive matching
        assert!(file_index.has_anchor("Installation-Guide"));
        assert!(file_index.has_anchor("INSTALLATION-GUIDE"));

        // Should not find non-existent anchor
        assert!(!file_index.has_anchor("nonexistent"));
    }

    #[test]
    fn test_has_anchor_custom() {
        let mut file_index = FileIndex::new();
        file_index.add_heading(HeadingIndex {
            text: "Installation Guide".to_string(),
            auto_anchor: "installation-guide".to_string(),
            custom_anchor: Some("install".to_string()),
            line: 1,
        });

        // Should find by auto-generated anchor
        assert!(file_index.has_anchor("installation-guide"));

        // Should also find by custom anchor
        assert!(file_index.has_anchor("install"));
        assert!(file_index.has_anchor("Install")); // case-insensitive

        // Should not find non-existent anchor
        assert!(!file_index.has_anchor("nonexistent"));
    }

    #[test]
    fn test_get_heading_by_anchor() {
        let mut file_index = FileIndex::new();
        file_index.add_heading(HeadingIndex {
            text: "Installation Guide".to_string(),
            auto_anchor: "installation-guide".to_string(),
            custom_anchor: Some("install".to_string()),
            line: 10,
        });
        file_index.add_heading(HeadingIndex {
            text: "Configuration".to_string(),
            auto_anchor: "configuration".to_string(),
            custom_anchor: None,
            line: 20,
        });

        // Get by auto anchor
        let heading = file_index.get_heading_by_anchor("installation-guide");
        assert!(heading.is_some());
        assert_eq!(heading.unwrap().text, "Installation Guide");
        assert_eq!(heading.unwrap().line, 10);

        // Get by custom anchor
        let heading = file_index.get_heading_by_anchor("install");
        assert!(heading.is_some());
        assert_eq!(heading.unwrap().text, "Installation Guide");

        // Get second heading
        let heading = file_index.get_heading_by_anchor("configuration");
        assert!(heading.is_some());
        assert_eq!(heading.unwrap().text, "Configuration");
        assert_eq!(heading.unwrap().line, 20);

        // Non-existent
        assert!(file_index.get_heading_by_anchor("nonexistent").is_none());
    }

    #[test]
    fn test_anchor_lookup_many_headings() {
        // Test that O(1) lookup works with many headings
        let mut file_index = FileIndex::new();

        // Add 100 headings
        for i in 0..100 {
            file_index.add_heading(HeadingIndex {
                text: format!("Heading {i}"),
                auto_anchor: format!("heading-{i}"),
                custom_anchor: Some(format!("h{i}")),
                line: i + 1,
            });
        }

        // Verify all can be found
        for i in 0..100 {
            assert!(file_index.has_anchor(&format!("heading-{i}")));
            assert!(file_index.has_anchor(&format!("h{i}")));

            let heading = file_index.get_heading_by_anchor(&format!("heading-{i}"));
            assert!(heading.is_some());
            assert_eq!(heading.unwrap().line, i + 1);
        }
    }

    // =============================================================================
    // Tests for extract_cross_file_links utility
    // =============================================================================

    #[test]
    fn test_extract_cross_file_links_basic() {
        use crate::config::MarkdownFlavor;

        let content = "# Test\n\nSee [link](./other.md) for info.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let links = extract_cross_file_links(&ctx);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_path, "./other.md");
        assert_eq!(links[0].fragment, "");
        assert_eq!(links[0].line, 3);
        // "See [link](" = 11 chars, so column 12 is where "./other.md" starts
        assert_eq!(links[0].column, 12);
    }

    #[test]
    fn test_extract_cross_file_links_with_fragment() {
        use crate::config::MarkdownFlavor;

        let content = "Check [guide](./guide.md#install) here.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let links = extract_cross_file_links(&ctx);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_path, "./guide.md");
        assert_eq!(links[0].fragment, "install");
        assert_eq!(links[0].line, 1);
        // "Check [guide](" = 14 chars, so column 15 is where "./guide.md" starts
        assert_eq!(links[0].column, 15);
    }

    #[test]
    fn test_extract_cross_file_links_multiple_on_same_line() {
        use crate::config::MarkdownFlavor;

        let content = "See [a](a.md) and [b](b.md) here.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let links = extract_cross_file_links(&ctx);

        assert_eq!(links.len(), 2);

        assert_eq!(links[0].target_path, "a.md");
        assert_eq!(links[0].line, 1);
        // "See [a](" = 8 chars, so column 9
        assert_eq!(links[0].column, 9);

        assert_eq!(links[1].target_path, "b.md");
        assert_eq!(links[1].line, 1);
        // "See [a](a.md) and [b](" = 22 chars, so column 23
        assert_eq!(links[1].column, 23);
    }

    #[test]
    fn test_extract_cross_file_links_angle_brackets() {
        use crate::config::MarkdownFlavor;

        let content = "See [link](<path/with (parens).md>) here.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let links = extract_cross_file_links(&ctx);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_path, "path/with (parens).md");
        assert_eq!(links[0].line, 1);
        // "See [link](<" = 12 chars, so column 13
        assert_eq!(links[0].column, 13);
    }

    #[test]
    fn test_extract_cross_file_links_skips_external() {
        use crate::config::MarkdownFlavor;

        let content = r#"
[external](https://example.com)
[mailto](mailto:test@example.com)
[local](./local.md)
[fragment](#section)
[absolute](/docs/page.md)
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let links = extract_cross_file_links(&ctx);

        // Only the local markdown link should be extracted
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_path, "./local.md");
    }

    #[test]
    fn test_extract_cross_file_links_skips_non_markdown() {
        use crate::config::MarkdownFlavor;

        let content = r#"
[image](./photo.png)
[doc](./readme.md)
[pdf](./document.pdf)
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let links = extract_cross_file_links(&ctx);

        // Only markdown files are indexed for cross-file validation
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_path, "./readme.md");
    }

    #[test]
    fn test_extract_cross_file_links_skips_code_spans() {
        use crate::config::MarkdownFlavor;

        let content = "Normal [link](./file.md) and `[code](./ignored.md)` here.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let links = extract_cross_file_links(&ctx);

        // Only the link outside code span should be extracted
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_path, "./file.md");
    }

    #[test]
    fn test_extract_cross_file_links_with_query_params() {
        use crate::config::MarkdownFlavor;

        let content = "See [doc](./file.md?raw=true) here.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let links = extract_cross_file_links(&ctx);

        assert_eq!(links.len(), 1);
        // Query params should be stripped
        assert_eq!(links[0].target_path, "./file.md");
    }

    #[test]
    fn test_extract_cross_file_links_empty_content() {
        use crate::config::MarkdownFlavor;

        let content = "";
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let links = extract_cross_file_links(&ctx);

        assert!(links.is_empty());
    }

    #[test]
    fn test_extract_cross_file_links_no_links() {
        use crate::config::MarkdownFlavor;

        let content = "# Just a heading\n\nSome text without links.\n";
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let links = extract_cross_file_links(&ctx);

        assert!(links.is_empty());
    }

    #[test]
    fn test_extract_cross_file_links_position_accuracy_issue_234() {
        // This test verifies the fix for GitHub issue #234
        // The LSP was reporting incorrect column positions for MD057 diagnostics
        use crate::config::MarkdownFlavor;

        let content = r#"# Test Document

Here is a [broken link](nonexistent-file.md) that should trigger MD057.

And another [link](also-missing.md) on this line.
"#;
        let ctx = LintContext::new(content, MarkdownFlavor::default(), None);
        let links = extract_cross_file_links(&ctx);

        assert_eq!(links.len(), 2);

        // First link: "Here is a [broken link](" = 24 chars, column 25
        assert_eq!(links[0].target_path, "nonexistent-file.md");
        assert_eq!(links[0].line, 3);
        assert_eq!(links[0].column, 25);

        // Second link: "And another [link](" = 19 chars, column 20
        assert_eq!(links[1].target_path, "also-missing.md");
        assert_eq!(links[1].line, 5);
        assert_eq!(links[1].column, 20);
    }
}
