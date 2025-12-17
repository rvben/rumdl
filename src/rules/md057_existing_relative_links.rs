//!
//! Rule MD057: Existing relative links
//!
//! See [docs/md057.md](../../docs/md057.md) for full documentation, configuration, and examples.

use crate::rule::{CrossFileScope, LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::element_cache::ElementCache;
use crate::workspace_index::{CrossFileLinkIndex, FileIndex};
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};

mod md057_config;
use md057_config::MD057Config;

// Thread-safe cache for file existence checks to avoid redundant filesystem operations
static FILE_EXISTENCE_CACHE: LazyLock<Arc<Mutex<HashMap<PathBuf, bool>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

// Reset the file existence cache (typically between rule runs)
fn reset_file_existence_cache() {
    if let Ok(mut cache) = FILE_EXISTENCE_CACHE.lock() {
        cache.clear();
    }
}

// Check if a file exists with caching
fn file_exists_with_cache(path: &Path) -> bool {
    match FILE_EXISTENCE_CACHE.lock() {
        Ok(mut cache) => *cache.entry(path.to_path_buf()).or_insert_with(|| path.exists()),
        Err(_) => path.exists(), // Fallback to uncached check on mutex poison
    }
}

/// Check if a file exists, also trying markdown extensions for extensionless links.
/// This supports wiki-style links like `[Link](page)` that resolve to `page.md`.
fn file_exists_or_markdown_extension(path: &Path) -> bool {
    // First, check exact path
    if file_exists_with_cache(path) {
        return true;
    }

    // If the path has no extension, try adding markdown extensions
    if path.extension().is_none() {
        for ext in MARKDOWN_EXTENSIONS {
            // MARKDOWN_EXTENSIONS includes the dot, e.g., ".md"
            let path_with_ext = path.with_extension(&ext[1..]);
            if file_exists_with_cache(&path_with_ext) {
                return true;
            }
        }
    }

    false
}

// Regex to match the start of a link - simplified for performance
static LINK_START_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!?\[[^\]]*\]").unwrap());

/// Regex to extract the URL from an angle-bracketed markdown link
/// Format: `](<URL>)` or `](<URL> "title")`
/// This handles URLs with parentheses like `](<path/(with)/parens.md>)`
static URL_EXTRACT_ANGLE_BRACKET_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\]\(\s*<([^>]+)>(#[^\)\s]*)?\s*(?:"[^"]*")?\s*\)"#).unwrap());

/// Regex to extract the URL from a normal markdown link (without angle brackets)
/// Format: `](URL)` or `](URL "title")`
static URL_EXTRACT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("\\]\\(\\s*([^>\\)\\s#]+)(#[^)\\s]*)?\\s*(?:\"[^\"]*\")?\\s*\\)").unwrap());

/// Regex to detect URLs with explicit schemes (should not be checked as relative links)
/// Matches: scheme:// or scheme: (per RFC 3986)
/// This covers http, https, ftp, file, smb, mailto, tel, data, macappstores, etc.
static PROTOCOL_DOMAIN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([a-zA-Z][a-zA-Z0-9+.-]*://|[a-zA-Z][a-zA-Z0-9+.-]*:|www\.)").unwrap());

// Current working directory
static CURRENT_DIR: LazyLock<PathBuf> = LazyLock::new(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

/// Convert a hex digit (0-9, a-f, A-F) to its numeric value.
/// Returns None for non-hex characters.
#[inline]
fn hex_digit_to_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

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

/// Rule MD057: Existing relative links should point to valid files or directories.
#[derive(Debug, Default, Clone)]
pub struct MD057ExistingRelativeLinks {
    /// Base directory for resolving relative links
    base_path: Arc<Mutex<Option<PathBuf>>>,
}

impl MD057ExistingRelativeLinks {
    /// Create a new instance with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the base path for resolving relative links
    pub fn with_path<P: AsRef<Path>>(self, path: P) -> Self {
        let path = path.as_ref();
        let dir_path = if path.is_file() {
            path.parent().map(|p| p.to_path_buf())
        } else {
            Some(path.to_path_buf())
        };

        if let Ok(mut guard) = self.base_path.lock() {
            *guard = dir_path;
        }
        self
    }

    pub fn from_config_struct(_config: MD057Config) -> Self {
        Self::default()
    }

    /// Check if a URL is external or should be skipped for validation.
    ///
    /// Returns `true` (skip validation) for:
    /// - URLs with protocols: `https://`, `http://`, `ftp://`, `mailto:`, etc.
    /// - Bare domains: `www.example.com`, `example.com`
    /// - Template variables: `{{URL}}`, `{{% include %}}`
    /// - Absolute web URL paths: `/api/docs`, `/blog/post.html`
    ///
    /// Returns `false` (validate) for:
    /// - Relative filesystem paths: `./file.md`, `../parent/file.md`, `file.md`
    #[inline]
    fn is_external_url(&self, url: &str) -> bool {
        if url.is_empty() {
            return false;
        }

        // Quick checks for common external URL patterns
        if PROTOCOL_DOMAIN_REGEX.is_match(url) || url.starts_with("www.") {
            return true;
        }

        // Skip template variables (Handlebars/Mustache/Jinja2 syntax)
        // Examples: {{URL}}, {{#URL}}, {{> partial}}, {{% include %}}, {{ variable }}
        if url.starts_with("{{") || url.starts_with("{%") {
            return true;
        }

        // Bare domain check (e.g., "example.com")
        // Note: We intentionally DON'T skip all TLDs like .org, .net, etc.
        // Links like [text](nodejs.org/path) without a protocol are broken -
        // they'll be treated as relative paths by markdown renderers.
        // Flagging them helps users find missing protocols.
        // We only skip .com as a minimal safety net for the most common case.
        if url.ends_with(".com") {
            return true;
        }

        // Absolute URL paths (e.g., /api/docs, /blog/post.html) are treated as web paths
        // and skipped. These are typically routes for published documentation sites,
        // not filesystem paths that can be validated locally.
        if url.starts_with('/') {
            return true;
        }

        // Framework path aliases (resolved by build tools like Vite, webpack, etc.)
        // These are not filesystem paths but module/asset aliases
        // Examples: ~/assets/image.png, @images/photo.jpg, @/components/Button.vue
        if url.starts_with('~') || url.starts_with('@') {
            return true;
        }

        // All other cases (relative paths, etc.) are not external
        false
    }

    /// Check if the URL is a fragment-only link (internal document link)
    #[inline]
    fn is_fragment_only_link(&self, url: &str) -> bool {
        url.starts_with('#')
    }

    /// Decode URL percent-encoded sequences in a path.
    /// Converts `%20` to space, `%2F` to `/`, etc.
    /// Returns the original string if decoding fails or produces invalid UTF-8.
    fn url_decode(path: &str) -> String {
        // Quick check: if no percent sign, return as-is
        if !path.contains('%') {
            return path.to_string();
        }

        let bytes = path.as_bytes();
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
        String::from_utf8(result).unwrap_or_else(|_| path.to_string())
    }

    /// Strip query parameters and fragments from a URL for file existence checking.
    /// URLs like `path/to/image.png?raw=true` or `file.md#section` should check
    /// for `path/to/image.png` or `file.md` respectively.
    ///
    /// Note: In standard URLs, query parameters (`?`) come before fragments (`#`),
    /// so we check for `?` first. If a URL has both, only the query is stripped here
    /// (fragments are handled separately by the regex in `contribute_to_index`).
    fn strip_query_and_fragment(url: &str) -> &str {
        // Find the first occurrence of '?' or '#', whichever comes first
        // This handles both standard URLs (? before #) and edge cases (# before ?)
        let query_pos = url.find('?');
        let fragment_pos = url.find('#');

        match (query_pos, fragment_pos) {
            (Some(q), Some(f)) => {
                // Both exist - strip at whichever comes first
                &url[..q.min(f)]
            }
            (Some(q), None) => &url[..q],
            (None, Some(f)) => &url[..f],
            (None, None) => url,
        }
    }

    /// Resolve a relative link against a provided base path
    fn resolve_link_path_with_base(link: &str, base_path: &Path) -> PathBuf {
        base_path.join(link)
    }

    /// Process a single link and check if it exists
    fn process_link_with_base(
        &self,
        url: &str,
        line_num: usize,
        column: usize,
        base_path: &Path,
        warnings: &mut Vec<LintWarning>,
    ) {
        // Skip empty URLs
        if url.is_empty() {
            return;
        }

        // Skip external URLs and fragment-only links (optimized order)
        if self.is_external_url(url) || self.is_fragment_only_link(url) {
            return;
        }

        // Strip query parameters and fragments before checking file existence
        // URLs like `path/to/image.png?raw=true` should check for `path/to/image.png`
        let file_path = Self::strip_query_and_fragment(url);

        // URL-decode the path to handle percent-encoded characters
        // e.g., `penguin%20with%20space.jpg` -> `penguin with space.jpg`
        let decoded_path = Self::url_decode(file_path);

        // Resolve the relative link against the base path
        let resolved_path = Self::resolve_link_path_with_base(&decoded_path, base_path);
        // Check if the file exists, also trying markdown extensions for extensionless links
        if !file_exists_or_markdown_extension(&resolved_path) {
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: line_num,
                column,
                end_line: line_num,
                end_column: column + url.len(),
                message: format!("Relative link '{url}' does not exist"),
                severity: Severity::Warning,
                fix: None, // No automatic fix for missing files
            });
        }
    }
}

impl Rule for MD057ExistingRelativeLinks {
    fn name(&self) -> &'static str {
        "MD057"
    }

    fn description(&self) -> &'static str {
        "Relative links should point to existing files"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Link
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        ctx.content.is_empty() || !ctx.likely_has_links_or_images()
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;

        // Early returns for performance
        if content.is_empty() || !content.contains('[') {
            return Ok(Vec::new());
        }

        // Quick check for any potential links before expensive operations
        if !content.contains("](") {
            return Ok(Vec::new());
        }

        // Reset the file existence cache for a fresh run
        reset_file_existence_cache();

        let mut warnings = Vec::new();

        // Determine base path for resolving relative links
        // ALWAYS compute from ctx.source_file for each file - do not reuse cached base_path
        // This ensures each file resolves links relative to its own directory
        let base_path: Option<PathBuf> = {
            // First check if base_path was explicitly set via with_path() (for tests)
            let explicit_base = self.base_path.lock().ok().and_then(|g| g.clone());
            if explicit_base.is_some() {
                explicit_base
            } else if let Some(ref source_file) = ctx.source_file {
                // Resolve symlinks to get the actual file location
                // This ensures relative links are resolved from the target's directory,
                // not the symlink's directory
                let resolved_file = source_file.canonicalize().unwrap_or_else(|_| source_file.clone());
                resolved_file
                    .parent()
                    .map(|p| p.to_path_buf())
                    .or_else(|| Some(CURRENT_DIR.clone()))
            } else {
                // No source file available - cannot validate relative links
                None
            }
        };

        // If we still don't have a base path, we can't validate relative links
        let Some(base_path) = base_path else {
            return Ok(warnings);
        };

        // Use LintContext links instead of expensive regex parsing
        if !ctx.links.is_empty() {
            // Use LineIndex for correct position calculation across all line ending types
            let line_index = &ctx.line_index;

            // Create element cache once for all links
            let element_cache = ElementCache::new(content);

            // Pre-collect lines to avoid repeated line iteration
            let lines: Vec<&str> = content.lines().collect();

            for link in &ctx.links {
                let line_idx = link.line - 1;
                if line_idx >= lines.len() {
                    continue;
                }

                let line = lines[line_idx];

                // Quick check for link pattern in this line
                if !line.contains("](") {
                    continue;
                }

                // Find all links in this line using optimized regex
                for link_match in LINK_START_REGEX.find_iter(line) {
                    let start_pos = link_match.start();
                    let end_pos = link_match.end();

                    // Calculate absolute position using LineIndex
                    let line_start_byte = line_index.get_line_start_byte(line_idx + 1).unwrap_or(0);
                    let absolute_start_pos = line_start_byte + start_pos;

                    // Skip if this link is in a code span
                    if element_cache.is_in_code_span(absolute_start_pos) {
                        continue;
                    }

                    // Find the URL part after the link text
                    // Try angle-bracket regex first (handles URLs with parens like `<path/(with)/parens.md>`)
                    // Then fall back to normal URL regex
                    let caps_and_url = URL_EXTRACT_ANGLE_BRACKET_REGEX
                        .captures_at(line, end_pos - 1)
                        .and_then(|caps| caps.get(1).map(|g| (caps, g)))
                        .or_else(|| {
                            URL_EXTRACT_REGEX
                                .captures_at(line, end_pos - 1)
                                .and_then(|caps| caps.get(1).map(|g| (caps, g)))
                        });

                    if let Some((_caps, url_group)) = caps_and_url {
                        let url = url_group.as_str().trim();

                        // Calculate column position
                        let column = start_pos + 1;

                        // Process and validate the link
                        self.process_link_with_base(url, link.line, column, &base_path, &mut warnings);
                    }
                }
            }
        }

        // Also process images - they have URLs already parsed
        for image in &ctx.images {
            let url = image.url.as_ref();
            self.process_link_with_base(url, image.line, image.start_col + 1, &base_path, &mut warnings);
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        // No configurable options for this rule
        None
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD057Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }

    fn cross_file_scope(&self) -> CrossFileScope {
        CrossFileScope::Workspace
    }

    fn contribute_to_index(&self, ctx: &crate::lint_context::LintContext, index: &mut FileIndex) {
        let content = ctx.content;

        // Early returns for performance
        if content.is_empty() || !content.contains("](") {
            return;
        }

        // Pre-collect lines to avoid repeated line iteration
        let lines: Vec<&str> = content.lines().collect();
        let element_cache = ElementCache::new(content);
        let line_index = &ctx.line_index;

        for link in &ctx.links {
            let line_idx = link.line - 1;
            if line_idx >= lines.len() {
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
                // The regex separates URL and fragment: group 1 excludes #, group 2 captures #fragment
                // Try angle-bracket regex first (handles URLs with parens)
                let caps_result = URL_EXTRACT_ANGLE_BRACKET_REGEX
                    .captures_at(line, end_pos - 1)
                    .or_else(|| URL_EXTRACT_REGEX.captures_at(line, end_pos - 1));

                if let Some(caps) = caps_result
                    && let Some(url_group) = caps.get(1)
                {
                    let file_path = url_group.as_str().trim();

                    // Skip empty, external, template variables, absolute URL paths,
                    // framework aliases, or fragment-only URLs
                    if file_path.is_empty()
                        || PROTOCOL_DOMAIN_REGEX.is_match(file_path)
                        || file_path.starts_with("www.")
                        || file_path.starts_with('#')
                        || file_path.starts_with("{{")
                        || file_path.starts_with("{%")
                        || file_path.starts_with('/')
                        || file_path.starts_with('~')
                        || file_path.starts_with('@')
                    {
                        continue;
                    }

                    // Strip query parameters before indexing (e.g., `file.md?raw=true` -> `file.md`)
                    let file_path = Self::strip_query_and_fragment(file_path);

                    // Get fragment from capture group 2 (includes # prefix)
                    let fragment = caps.get(2).map(|m| m.as_str().trim_start_matches('#')).unwrap_or("");

                    // Only index markdown file links for cross-file validation
                    // Non-markdown files (images, media) are validated via filesystem in check()
                    if is_markdown_file(file_path) {
                        index.add_cross_file_link(CrossFileLinkIndex {
                            target_path: file_path.to_string(),
                            fragment: fragment.to_string(),
                            line: link.line,
                            column: start_pos + 1,
                        });
                    }
                }
            }
        }
    }

    fn cross_file_check(
        &self,
        file_path: &Path,
        file_index: &FileIndex,
        workspace_index: &crate::workspace_index::WorkspaceIndex,
    ) -> LintResult {
        let mut warnings = Vec::new();

        // Get the directory containing this file for resolving relative links
        let file_dir = file_path.parent();

        for cross_link in &file_index.cross_file_links {
            // Resolve the relative path
            let target_path = if cross_link.target_path.starts_with('/') {
                // Absolute path from workspace root (e.g., "/CONTRIBUTING.md")
                // Walk up from the current file's directory to find the workspace root
                let stripped = cross_link.target_path.trim_start_matches('/');
                resolve_absolute_link(file_path, stripped)
            } else if let Some(dir) = file_dir {
                dir.join(&cross_link.target_path)
            } else {
                Path::new(&cross_link.target_path).to_path_buf()
            };

            // Normalize the path (handle .., ., etc.)
            let target_path = normalize_path(&target_path);

            // Check if the target markdown file exists in the workspace index
            if !workspace_index.contains_file(&target_path) {
                // File not in index - check filesystem directly for case-insensitive filesystems
                if !target_path.exists() {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: cross_link.line,
                        column: cross_link.column,
                        end_line: cross_link.line,
                        end_column: cross_link.column + cross_link.target_path.len(),
                        message: format!("Relative link '{}' does not exist", cross_link.target_path),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
            }
        }

        Ok(warnings)
    }
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

/// Resolve an absolute link (e.g., "/CONTRIBUTING.md") relative to the workspace root.
///
/// Absolute paths in markdown (starting with "/") are relative to the workspace/repo root,
/// not the filesystem root. This function walks up from the current file's directory
/// to find where the target file exists.
fn resolve_absolute_link(file_path: &Path, stripped_path: &str) -> PathBuf {
    // Walk up from the file's directory, checking each ancestor for the target
    let mut current = file_path.parent();
    while let Some(dir) = current {
        let candidate = dir.join(stripped_path);
        if candidate.exists() {
            return candidate;
        }
        current = dir.parent();
    }

    // If not found by walking up, return the path relative to the file's directory
    // (this will likely fail the existence check later, which is correct behavior)
    file_path
        .parent()
        .map(|d| d.join(stripped_path))
        .unwrap_or_else(|| PathBuf::from(stripped_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_strip_query_and_fragment() {
        // Test query parameter stripping
        assert_eq!(
            MD057ExistingRelativeLinks::strip_query_and_fragment("file.png?raw=true"),
            "file.png"
        );
        assert_eq!(
            MD057ExistingRelativeLinks::strip_query_and_fragment("file.png?raw=true&version=1"),
            "file.png"
        );
        assert_eq!(
            MD057ExistingRelativeLinks::strip_query_and_fragment("file.png?"),
            "file.png"
        );

        // Test fragment stripping
        assert_eq!(
            MD057ExistingRelativeLinks::strip_query_and_fragment("file.md#section"),
            "file.md"
        );
        assert_eq!(
            MD057ExistingRelativeLinks::strip_query_and_fragment("file.md#"),
            "file.md"
        );

        // Test both query and fragment (query comes first, per RFC 3986)
        assert_eq!(
            MD057ExistingRelativeLinks::strip_query_and_fragment("file.md?raw=true#section"),
            "file.md"
        );

        // Test no query or fragment
        assert_eq!(
            MD057ExistingRelativeLinks::strip_query_and_fragment("file.png"),
            "file.png"
        );

        // Test with path
        assert_eq!(
            MD057ExistingRelativeLinks::strip_query_and_fragment("path/to/image.png?raw=true"),
            "path/to/image.png"
        );
        assert_eq!(
            MD057ExistingRelativeLinks::strip_query_and_fragment("path/to/image.png?raw=true#anchor"),
            "path/to/image.png"
        );

        // Edge case: fragment before query (non-standard but possible)
        assert_eq!(
            MD057ExistingRelativeLinks::strip_query_and_fragment("file.md#section?query"),
            "file.md"
        );
    }

    #[test]
    fn test_url_decode() {
        // Simple space encoding
        assert_eq!(
            MD057ExistingRelativeLinks::url_decode("penguin%20with%20space.jpg"),
            "penguin with space.jpg"
        );

        // Path with encoded spaces
        assert_eq!(
            MD057ExistingRelativeLinks::url_decode("assets/my%20file%20name.png"),
            "assets/my file name.png"
        );

        // Multiple encoded characters
        assert_eq!(
            MD057ExistingRelativeLinks::url_decode("hello%20world%21.md"),
            "hello world!.md"
        );

        // Lowercase hex
        assert_eq!(MD057ExistingRelativeLinks::url_decode("%2f%2e%2e"), "/..");

        // Uppercase hex
        assert_eq!(MD057ExistingRelativeLinks::url_decode("%2F%2E%2E"), "/..");

        // Mixed case hex
        assert_eq!(MD057ExistingRelativeLinks::url_decode("%2f%2E%2e"), "/..");

        // No encoding - return as-is
        assert_eq!(
            MD057ExistingRelativeLinks::url_decode("normal-file.md"),
            "normal-file.md"
        );

        // Incomplete percent encoding - leave as-is
        assert_eq!(MD057ExistingRelativeLinks::url_decode("file%2.txt"), "file%2.txt");

        // Percent at end - leave as-is
        assert_eq!(MD057ExistingRelativeLinks::url_decode("file%"), "file%");

        // Invalid hex digits - leave as-is
        assert_eq!(MD057ExistingRelativeLinks::url_decode("file%GG.txt"), "file%GG.txt");

        // Plus sign (should NOT be decoded - that's form encoding, not URL encoding)
        assert_eq!(MD057ExistingRelativeLinks::url_decode("file+name.txt"), "file+name.txt");

        // Empty string
        assert_eq!(MD057ExistingRelativeLinks::url_decode(""), "");

        // UTF-8 multi-byte characters (é = C3 A9 in UTF-8)
        assert_eq!(MD057ExistingRelativeLinks::url_decode("caf%C3%A9.md"), "café.md");

        // Multiple consecutive encoded characters
        assert_eq!(MD057ExistingRelativeLinks::url_decode("%20%20%20"), "   ");

        // Encoded path separators
        assert_eq!(
            MD057ExistingRelativeLinks::url_decode("path%2Fto%2Ffile.md"),
            "path/to/file.md"
        );

        // Mixed encoded and non-encoded
        assert_eq!(
            MD057ExistingRelativeLinks::url_decode("hello%20world/foo%20bar.md"),
            "hello world/foo bar.md"
        );

        // Special characters that are commonly encoded
        assert_eq!(MD057ExistingRelativeLinks::url_decode("file%5B1%5D.md"), "file[1].md");

        // Percent at position that looks like encoding but isn't valid
        assert_eq!(MD057ExistingRelativeLinks::url_decode("100%pure.md"), "100%pure.md");
    }

    #[test]
    fn test_url_encoded_filenames() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create a file with spaces in the name
        let file_with_spaces = base_path.join("penguin with space.jpg");
        File::create(&file_with_spaces)
            .unwrap()
            .write_all(b"image data")
            .unwrap();

        // Create a subdirectory with spaces
        let subdir = base_path.join("my images");
        std::fs::create_dir(&subdir).unwrap();
        let nested_file = subdir.join("photo 1.png");
        File::create(&nested_file).unwrap().write_all(b"photo data").unwrap();

        // Test content with URL-encoded links
        let content = r#"
# Test Document with URL-Encoded Links

![Penguin](penguin%20with%20space.jpg)
![Photo](my%20images/photo%201.png)
![Missing](missing%20file.jpg)
"#;

        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only have one warning for the missing file
        assert_eq!(
            result.len(),
            1,
            "Should only warn about missing%20file.jpg. Got: {result:?}"
        );
        assert!(
            result[0].message.contains("missing%20file.jpg"),
            "Warning should mention the URL-encoded filename"
        );
    }

    #[test]
    fn test_external_urls() {
        let rule = MD057ExistingRelativeLinks::new();

        // Common web protocols
        assert!(rule.is_external_url("https://example.com"));
        assert!(rule.is_external_url("http://example.com"));
        assert!(rule.is_external_url("ftp://example.com"));
        assert!(rule.is_external_url("www.example.com"));
        assert!(rule.is_external_url("example.com"));

        // Special URI schemes (issue #192)
        assert!(rule.is_external_url("file:///path/to/file"));
        assert!(rule.is_external_url("smb://server/share"));
        assert!(rule.is_external_url("macappstores://apps.apple.com/"));
        assert!(rule.is_external_url("mailto:user@example.com"));
        assert!(rule.is_external_url("tel:+1234567890"));
        assert!(rule.is_external_url("data:text/plain;base64,SGVsbG8="));
        assert!(rule.is_external_url("javascript:void(0)"));
        assert!(rule.is_external_url("ssh://git@github.com/repo"));
        assert!(rule.is_external_url("git://github.com/repo.git"));

        // Template variables should be skipped (not checked as relative links)
        assert!(rule.is_external_url("{{URL}}")); // Handlebars/Mustache
        assert!(rule.is_external_url("{{#URL}}")); // Handlebars block helper
        assert!(rule.is_external_url("{{> partial}}")); // Handlebars partial
        assert!(rule.is_external_url("{{ variable }}")); // Mustache with spaces
        assert!(rule.is_external_url("{{% include %}}")); // Jinja2/Hugo shortcode
        assert!(rule.is_external_url("{{")); // Even partial matches (regex edge case)

        // Absolute web URL paths should be skipped (not validated)
        // These are typically routes for published documentation sites
        assert!(rule.is_external_url("/api/v1/users"));
        assert!(rule.is_external_url("/blog/2024/release.html"));
        assert!(rule.is_external_url("/react/hooks/use-state.html"));
        assert!(rule.is_external_url("/pkg/runtime"));
        assert!(rule.is_external_url("/doc/go1compat"));
        assert!(rule.is_external_url("/index.html"));
        assert!(rule.is_external_url("/assets/logo.png"));

        // Framework path aliases should be skipped (resolved by build tools)
        // Tilde prefix (common in Vite, Nuxt, Astro for project root)
        assert!(rule.is_external_url("~/assets/image.png"));
        assert!(rule.is_external_url("~/components/Button.vue"));
        assert!(rule.is_external_url("~assets/logo.svg")); // Nuxt style without /

        // @ prefix (common in Vue, webpack, Vite aliases)
        assert!(rule.is_external_url("@/components/Header.vue"));
        assert!(rule.is_external_url("@images/photo.jpg"));
        assert!(rule.is_external_url("@assets/styles.css"));

        // Relative paths should NOT be external (should be validated)
        assert!(!rule.is_external_url("./relative/path.md"));
        assert!(!rule.is_external_url("relative/path.md"));
        assert!(!rule.is_external_url("../parent/path.md"));
    }

    #[test]
    fn test_framework_path_aliases() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Test content with framework path aliases (should all be skipped)
        let content = r#"
# Framework Path Aliases

![Image 1](~/assets/penguin.jpg)
![Image 2](~assets/logo.svg)
![Image 3](@images/photo.jpg)
![Image 4](@/components/icon.svg)
[Link](@/pages/about.md)

This is a [real missing link](missing.md) that should be flagged.
"#;

        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only have one warning for the real missing link
        assert_eq!(
            result.len(),
            1,
            "Should only warn about missing.md, not framework aliases. Got: {result:?}"
        );
        assert!(
            result[0].message.contains("missing.md"),
            "Warning should be for missing.md"
        );
    }

    #[test]
    fn test_url_decode_security_path_traversal() {
        // Ensure URL decoding doesn't enable path traversal attacks
        // The decoded path is still validated against the base path
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create a file in the temp directory
        let file_in_base = base_path.join("safe.md");
        File::create(&file_in_base).unwrap().write_all(b"# Safe").unwrap();

        // Test with encoded path traversal attempt
        // Use a path that definitely won't exist on any platform (not /etc/passwd which exists on Linux)
        // %2F = /, so ..%2F..%2Fnonexistent%2Ffile = ../../nonexistent/file
        // %252F = %2F (double encoded), so ..%252F..%252F = ..%2F..%2F (literal, won't decode to ..)
        let content = r#"
[Traversal attempt](..%2F..%2Fnonexistent_dir_12345%2Fmissing.md)
[Double encoded](..%252F..%252Fnonexistent%252Ffile.md)
[Safe link](safe.md)
"#;

        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // The traversal attempts should still be flagged as missing
        // (they don't exist relative to base_path after decoding)
        assert_eq!(
            result.len(),
            2,
            "Should have warnings for traversal attempts. Got: {result:?}"
        );
    }

    #[test]
    fn test_url_encoded_utf8_filenames() {
        // Test with actual UTF-8 encoded filenames
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create files with unicode names
        let cafe_file = base_path.join("café.md");
        File::create(&cafe_file).unwrap().write_all(b"# Cafe").unwrap();

        let content = r#"
[Café link](caf%C3%A9.md)
[Missing unicode](r%C3%A9sum%C3%A9.md)
"#;

        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only warn about the missing file
        assert_eq!(
            result.len(),
            1,
            "Should only warn about missing résumé.md. Got: {result:?}"
        );
        assert!(
            result[0].message.contains("r%C3%A9sum%C3%A9.md"),
            "Warning should mention the URL-encoded filename"
        );
    }

    #[test]
    fn test_no_warnings_without_base_path() {
        let rule = MD057ExistingRelativeLinks::new();
        let content = "[Link](missing.md)";

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should have no warnings without base path");
    }

    #[test]
    fn test_existing_and_missing_links() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create an existing file
        let exists_path = base_path.join("exists.md");
        File::create(&exists_path).unwrap().write_all(b"# Test File").unwrap();

        // Verify the file exists
        assert!(exists_path.exists(), "exists.md should exist for this test");

        // Create test content with both existing and missing links
        let content = r#"
# Test Document

[Valid Link](exists.md)
[Invalid Link](missing.md)
[External Link](https://example.com)
[Media Link](image.jpg)
        "#;

        // Initialize rule with the base path (default: check all files including media)
        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

        // Test the rule
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should have two warnings: missing.md and image.jpg (both don't exist)
        assert_eq!(result.len(), 2);
        let messages: Vec<_> = result.iter().map(|w| w.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("missing.md")));
        assert!(messages.iter().any(|m| m.contains("image.jpg")));
    }

    #[test]
    fn test_angle_bracket_links() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create an existing file
        let exists_path = base_path.join("exists.md");
        File::create(&exists_path).unwrap().write_all(b"# Test File").unwrap();

        // Create test content with angle bracket links
        let content = r#"
# Test Document

[Valid Link](<exists.md>)
[Invalid Link](<missing.md>)
[External Link](<https://example.com>)
    "#;

        // Test with default settings
        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should have one warning for missing.md
        assert_eq!(result.len(), 1, "Should have exactly one warning");
        assert!(
            result[0].message.contains("missing.md"),
            "Warning should mention missing.md"
        );
    }

    #[test]
    fn test_angle_bracket_links_with_parens() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create directory structure with parentheses in path
        let app_dir = base_path.join("app");
        std::fs::create_dir(&app_dir).unwrap();
        let upload_dir = app_dir.join("(upload)");
        std::fs::create_dir(&upload_dir).unwrap();
        let page_file = upload_dir.join("page.tsx");
        File::create(&page_file)
            .unwrap()
            .write_all(b"export default function Page() {}")
            .unwrap();

        // Create test content with angle bracket links containing parentheses
        let content = r#"
# Test Document with Paths Containing Parens

[Upload Page](<app/(upload)/page.tsx>)
[Unix pipe](<https://en.wikipedia.org/wiki/Pipeline_(Unix)>)
[Missing](<app/(missing)/file.md>)
"#;

        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only have one warning for the missing file
        assert_eq!(
            result.len(),
            1,
            "Should have exactly one warning for missing file. Got: {result:?}"
        );
        assert!(
            result[0].message.contains("app/(missing)/file.md"),
            "Warning should mention app/(missing)/file.md"
        );
    }

    #[test]
    fn test_all_file_types_checked() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create a test with various file types - all should be checked
        let content = r#"
[Image Link](image.jpg)
[Video Link](video.mp4)
[Markdown Link](document.md)
[PDF Link](file.pdf)
"#;

        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should warn about all missing files regardless of extension
        assert_eq!(result.len(), 4, "Should have warnings for all missing files");
    }

    #[test]
    fn test_code_span_detection() {
        let rule = MD057ExistingRelativeLinks::new();

        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        let rule = rule.with_path(base_path);

        // Test with document structure
        let content = "This is a [link](nonexistent.md) and `[not a link](not-checked.md)` in code.";

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only find the real link, not the one in code
        assert_eq!(result.len(), 1, "Should only flag the real link");
        assert!(result[0].message.contains("nonexistent.md"));
    }

    #[test]
    fn test_inline_code_spans() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create test content with links in inline code spans
        let content = r#"
# Test Document

This is a normal link: [Link](missing.md)

This is a code span with a link: `[Link](another-missing.md)`

Some more text with `inline code [Link](yet-another-missing.md) embedded`.

    "#;

        // Initialize rule with the base path
        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

        // Test the rule
        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only have warning for the normal link, not for links in code spans
        assert_eq!(result.len(), 1, "Should have exactly one warning");
        assert!(
            result[0].message.contains("missing.md"),
            "Warning should be for missing.md"
        );
        assert!(
            !result.iter().any(|w| w.message.contains("another-missing.md")),
            "Should not warn about link in code span"
        );
        assert!(
            !result.iter().any(|w| w.message.contains("yet-another-missing.md")),
            "Should not warn about link in inline code"
        );
    }

    #[test]
    fn test_extensionless_link_resolution() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Create a markdown file WITHOUT specifying .md extension in the link
        let page_path = base_path.join("page.md");
        File::create(&page_path).unwrap().write_all(b"# Page").unwrap();

        // Test content with extensionless link that should resolve to page.md
        let content = r#"
# Test Document

[Link without extension](page)
[Link with extension](page.md)
[Missing link](nonexistent)
"#;

        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should only have warning for nonexistent link
        // Both "page" and "page.md" should resolve to the same file
        assert_eq!(result.len(), 1, "Should only warn about nonexistent link");
        assert!(
            result[0].message.contains("nonexistent"),
            "Warning should be for 'nonexistent' not 'page'"
        );
    }

    // Cross-file validation tests
    #[test]
    fn test_cross_file_scope() {
        let rule = MD057ExistingRelativeLinks::new();
        assert_eq!(rule.cross_file_scope(), CrossFileScope::Workspace);
    }

    #[test]
    fn test_contribute_to_index_extracts_markdown_links() {
        let rule = MD057ExistingRelativeLinks::new();
        let content = r#"
# Document

[Link to docs](./docs/guide.md)
[Link with fragment](./other.md#section)
[External link](https://example.com)
[Image link](image.png)
[Media file](video.mp4)
"#;

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let mut index = FileIndex::new();
        rule.contribute_to_index(&ctx, &mut index);

        // Should only index markdown file links
        assert_eq!(index.cross_file_links.len(), 2);

        // Check first link
        assert_eq!(index.cross_file_links[0].target_path, "./docs/guide.md");
        assert_eq!(index.cross_file_links[0].fragment, "");

        // Check second link (with fragment)
        assert_eq!(index.cross_file_links[1].target_path, "./other.md");
        assert_eq!(index.cross_file_links[1].fragment, "section");
    }

    #[test]
    fn test_contribute_to_index_skips_external_and_anchors() {
        let rule = MD057ExistingRelativeLinks::new();
        let content = r#"
# Document

[External](https://example.com)
[Another external](http://example.org)
[Fragment only](#section)
[FTP link](ftp://files.example.com)
[Mail link](mailto:test@example.com)
[WWW link](www.example.com)
"#;

        let ctx = crate::lint_context::LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let mut index = FileIndex::new();
        rule.contribute_to_index(&ctx, &mut index);

        // Should not index any of these
        assert_eq!(index.cross_file_links.len(), 0);
    }

    #[test]
    fn test_cross_file_check_valid_link() {
        use crate::workspace_index::WorkspaceIndex;

        let rule = MD057ExistingRelativeLinks::new();

        // Create a workspace index with the target file
        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(PathBuf::from("docs/guide.md"), FileIndex::new());

        // Create file index with a link to an existing file
        let mut file_index = FileIndex::new();
        file_index.add_cross_file_link(CrossFileLinkIndex {
            target_path: "guide.md".to_string(),
            fragment: "".to_string(),
            line: 5,
            column: 1,
        });

        // Run cross-file check from docs/index.md
        let warnings = rule
            .cross_file_check(Path::new("docs/index.md"), &file_index, &workspace_index)
            .unwrap();

        // Should have no warnings - file exists
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_cross_file_check_missing_link() {
        use crate::workspace_index::WorkspaceIndex;

        let rule = MD057ExistingRelativeLinks::new();

        // Create an empty workspace index
        let workspace_index = WorkspaceIndex::new();

        // Create file index with a link to a missing file
        let mut file_index = FileIndex::new();
        file_index.add_cross_file_link(CrossFileLinkIndex {
            target_path: "missing.md".to_string(),
            fragment: "".to_string(),
            line: 5,
            column: 1,
        });

        // Run cross-file check
        let warnings = rule
            .cross_file_check(Path::new("docs/index.md"), &file_index, &workspace_index)
            .unwrap();

        // Should have one warning for the missing file
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("missing.md"));
        assert!(warnings[0].message.contains("does not exist"));
    }

    #[test]
    fn test_cross_file_check_parent_path() {
        use crate::workspace_index::WorkspaceIndex;

        let rule = MD057ExistingRelativeLinks::new();

        // Create a workspace index with the target file at the root
        let mut workspace_index = WorkspaceIndex::new();
        workspace_index.insert_file(PathBuf::from("readme.md"), FileIndex::new());

        // Create file index with a parent path link
        let mut file_index = FileIndex::new();
        file_index.add_cross_file_link(CrossFileLinkIndex {
            target_path: "../readme.md".to_string(),
            fragment: "".to_string(),
            line: 5,
            column: 1,
        });

        // Run cross-file check from docs/guide.md
        let warnings = rule
            .cross_file_check(Path::new("docs/guide.md"), &file_index, &workspace_index)
            .unwrap();

        // Should have no warnings - file exists at normalized path
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_normalize_path_function() {
        // Test simple cases
        assert_eq!(
            normalize_path(Path::new("docs/guide.md")),
            PathBuf::from("docs/guide.md")
        );

        // Test current directory removal
        assert_eq!(
            normalize_path(Path::new("./docs/guide.md")),
            PathBuf::from("docs/guide.md")
        );

        // Test parent directory resolution
        assert_eq!(
            normalize_path(Path::new("docs/sub/../guide.md")),
            PathBuf::from("docs/guide.md")
        );

        // Test multiple parent directories
        assert_eq!(normalize_path(Path::new("a/b/c/../../d.md")), PathBuf::from("a/d.md"));
    }

    #[test]
    fn test_resolve_absolute_link() {
        // Create a temporary directory structure for testing
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let root = temp_dir.path();

        // Create root-level file
        let contributing = root.join("CONTRIBUTING.md");
        File::create(&contributing).expect("Failed to create CONTRIBUTING.md");

        // Create nested directory with a markdown file
        let docs = root.join("docs");
        std::fs::create_dir(&docs).expect("Failed to create docs dir");
        let readme = docs.join("README.md");
        File::create(&readme).expect("Failed to create README.md");

        // Test: absolute link from nested file to root file
        // From docs/README.md, link to /CONTRIBUTING.md should resolve to root/CONTRIBUTING.md
        let resolved = resolve_absolute_link(&readme, "CONTRIBUTING.md");
        assert!(resolved.exists(), "Should find CONTRIBUTING.md at workspace root");
        assert_eq!(resolved, contributing);

        // Test: file that doesn't exist should not resolve (returns path relative to file's dir)
        let nonexistent = resolve_absolute_link(&readme, "NONEXISTENT.md");
        assert!(!nonexistent.exists(), "Should not find nonexistent file");
    }
}
