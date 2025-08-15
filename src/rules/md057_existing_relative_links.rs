//!
//! Rule MD057: Existing relative links
//!
//! See [docs/md057.md](../../docs/md057.md) for full documentation, configuration, and examples.

use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::utils::document_structure::{DocumentStructure, DocumentStructureExtensions};
use crate::utils::element_cache::ElementCache;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod md057_config;
use md057_config::MD057Config;

// Thread-safe cache for file existence checks to avoid redundant filesystem operations
lazy_static! {
    static ref FILE_EXISTENCE_CACHE: Arc<Mutex<HashMap<PathBuf, bool>>> = Arc::new(Mutex::new(HashMap::new()));
}

// Reset the file existence cache (typically between rule runs)
fn reset_file_existence_cache() {
    let mut cache = FILE_EXISTENCE_CACHE.lock().unwrap();
    cache.clear();
}

// Check if a file exists with caching
fn file_exists_with_cache(path: &Path) -> bool {
    let mut cache = FILE_EXISTENCE_CACHE.lock().unwrap();
    *cache.entry(path.to_path_buf()).or_insert_with(|| path.exists())
}

lazy_static! {
    // Regex to match the start of a link - simplified for performance
    static ref LINK_START_REGEX: Regex =
        Regex::new(r"!?\[[^\]]*\]").unwrap();

    /// Regex to extract the URL from a markdown link
    /// Format: `](URL)` or `](URL "title")`
    static ref URL_EXTRACT_REGEX: Regex =
        Regex::new("\\]\\(\\s*<?([^>\\)\\s#]+)(#[^)\\s]*)?\\s*(?:\"[^\"]*\")?\\s*>?\\s*\\)").unwrap();

    /// Regex to detect code fence blocks
    static ref CODE_FENCE_REGEX: Regex =
        Regex::new(r"^( {0,3})(`{3,}|~{3,})").unwrap();

    /// Regex to detect protocol and domain for external links
    static ref PROTOCOL_DOMAIN_REGEX: Regex =
        Regex::new(r"^(https?://|ftp://|mailto:|www\.)").unwrap();

    /// Regex to detect media file types
    static ref MEDIA_FILE_REGEX: Regex =
        Regex::new(r"\.(jpg|jpeg|png|gif|bmp|svg|webp|tiff|mp3|mp4|avi|mov|webm|wav|ogg|pdf)$").unwrap();

    /// Regex to detect fragment-only links
    static ref FRAGMENT_ONLY_REGEX: Regex =
        Regex::new(r"^#").unwrap();

    // Current working directory
    static ref CURRENT_DIR: PathBuf = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
}

/// Rule MD057: Existing relative links should point to valid files or directories.
#[derive(Debug, Default, Clone)]
pub struct MD057ExistingRelativeLinks {
    /// Base directory for resolving relative links
    base_path: Arc<Mutex<Option<PathBuf>>>,
    /// Configuration
    config: MD057Config,
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

        *self.base_path.lock().unwrap() = dir_path;
        self
    }

    /// Configure whether to skip checking media files
    pub fn with_skip_media_files(mut self, skip_media_files: bool) -> Self {
        self.config.skip_media_files = skip_media_files;
        self
    }

    pub fn from_config_struct(config: MD057Config) -> Self {
        Self {
            base_path: Arc::new(Mutex::new(None)),
            config,
        }
    }

    /// Check if a URL is external (optimized version)
    #[inline]
    fn is_external_url(&self, url: &str) -> bool {
        if url.is_empty() {
            return false;
        }

        // Quick checks for common external URL patterns
        if PROTOCOL_DOMAIN_REGEX.is_match(url) || url.starts_with("www.") {
            return true;
        }

        // More restrictive domain check using a simpler pattern
        if !self.is_media_file(url) && url.ends_with(".com") {
            return true;
        }

        // Absolute paths within the site are not external
        if url.starts_with('/') {
            return false;
        }

        // All other cases (relative paths, etc.) are not external
        false
    }

    /// Check if the URL is a fragment-only link (internal document link)
    #[inline]
    fn is_fragment_only_link(&self, url: &str) -> bool {
        url.starts_with('#')
    }

    /// Check if the URL has a media file extension (optimized with early returns)
    #[inline]
    fn is_media_file(&self, url: &str) -> bool {
        // Quick check before using regex
        if !url.contains('.') {
            return false;
        }
        MEDIA_FILE_REGEX.is_match(url)
    }

    /// Determine if we should skip checking this media file
    #[inline]
    fn should_skip_media_file(&self, url: &str) -> bool {
        self.config.skip_media_files && self.is_media_file(url)
    }

    /// Resolve a relative link against the base path
    fn resolve_link_path(&self, link: &str) -> Option<PathBuf> {
        self.base_path
            .lock()
            .unwrap()
            .as_ref()
            .map(|base_path| base_path.join(link))
    }

    /// Process a single link and check if it exists
    fn process_link(&self, url: &str, line_num: usize, column: usize, warnings: &mut Vec<LintWarning>) {
        // Skip empty URLs
        if url.is_empty() {
            return;
        }

        // Skip external URLs and fragment-only links (optimized order)
        if self.is_external_url(url) || self.is_fragment_only_link(url) {
            return;
        }

        // Skip media files if configured to do so
        if self.should_skip_media_file(url) {
            return;
        }

        // Resolve the relative link against the base path
        if let Some(resolved_path) = self.resolve_link_path(url) {
            // Check if the file exists (with caching to avoid filesystem calls)
            if !file_exists_with_cache(&resolved_path) {
                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
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
        let content = ctx.content;
        content.is_empty() || !content.contains('[') || !content.contains("](")
    }

    /// Optimized implementation using document structure
    fn check_with_structure(
        &self,
        ctx: &crate::lint_context::LintContext,
        structure: &DocumentStructure,
    ) -> LintResult {
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

        // Cache base path lookup to avoid repeated mutex operations
        let base_path = {
            let base_path_guard = self.base_path.lock().unwrap();
            if base_path_guard.is_some() {
                base_path_guard.clone()
            } else {
                // Try to determine the base path from the file being processed (cached)
                static CACHED_FILE_PATH: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
                CACHED_FILE_PATH
                    .get_or_init(|| {
                        if let Ok(file_path) = env::var("RUMDL_FILE_PATH") {
                            let path = Path::new(&file_path);
                            if path.exists() {
                                path.parent()
                                    .map(|p| p.to_path_buf())
                                    .or_else(|| Some(CURRENT_DIR.clone()))
                            } else {
                                Some(CURRENT_DIR.clone())
                            }
                        } else {
                            Some(CURRENT_DIR.clone())
                        }
                    })
                    .clone()
            }
        };

        // If we still don't have a base path, we can't validate relative links
        if base_path.is_none() {
            return Ok(warnings);
        }

        // Use DocumentStructure links instead of expensive regex parsing
        if !structure.links.is_empty() {
            // Pre-compute line positions for efficient absolute position calculation
            let mut line_positions = Vec::new();
            let mut pos = 0;
            line_positions.push(0);
            for ch in content.chars() {
                pos += ch.len_utf8();
                if ch == '\n' {
                    line_positions.push(pos);
                }
            }

            // Create element cache once for all links
            let element_cache = ElementCache::new(content);

            // Pre-collect lines to avoid repeated line iteration
            let lines: Vec<&str> = content.lines().collect();

            for link in &structure.links {
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

                    // Calculate absolute position efficiently using pre-computed positions
                    let absolute_start_pos = if line_idx < line_positions.len() {
                        line_positions[line_idx] + start_pos
                    } else {
                        // Fallback for edge cases
                        content.lines().take(line_idx).map(|l| l.len() + 1).sum::<usize>() + start_pos
                    };

                    // Skip if this link is in a code span
                    if element_cache.is_in_code_span(absolute_start_pos) {
                        continue;
                    }

                    // Find the URL part after the link text
                    if let Some(caps) = URL_EXTRACT_REGEX.captures_at(line, end_pos - 1)
                        && let Some(url_group) = caps.get(1)
                    {
                        let url = url_group.as_str().trim();

                        // Calculate column position
                        let column = start_pos + 1;

                        // Process and validate the link
                        self.process_link(url, link.line, column, &mut warnings);
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let content = ctx.content;
        // If document structure is available, use the optimized version
        let structure = DocumentStructure::new(content);
        self.check_with_structure(ctx, &structure)

        // The code below is now unreachable because we always use the document structure
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD057Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

impl DocumentStructureExtensions for MD057ExistingRelativeLinks {
    fn has_relevant_elements(
        &self,
        _ctx: &crate::lint_context::LintContext,
        _doc_structure: &DocumentStructure,
    ) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_external_urls() {
        let rule = MD057ExistingRelativeLinks::new();

        assert!(rule.is_external_url("https://example.com"));
        assert!(rule.is_external_url("http://example.com"));
        assert!(rule.is_external_url("ftp://example.com"));
        assert!(rule.is_external_url("www.example.com"));
        assert!(rule.is_external_url("example.com"));

        assert!(!rule.is_external_url("./relative/path.md"));
        assert!(!rule.is_external_url("relative/path.md"));
        assert!(!rule.is_external_url("../parent/path.md"));
    }

    #[test]
    fn test_media_files() {
        // Test with default settings (skip_media_files = true)
        let rule_default = MD057ExistingRelativeLinks::new();

        // Test media file identification
        assert!(
            rule_default.is_media_file("image.jpg"),
            "image.jpg should be identified as a media file"
        );
        assert!(
            rule_default.is_media_file("video.mp4"),
            "video.mp4 should be identified as a media file"
        );
        assert!(
            rule_default.is_media_file("document.pdf"),
            "document.pdf should be identified as a media file"
        );
        assert!(
            rule_default.is_media_file("path/to/audio.mp3"),
            "path/to/audio.mp3 should be identified as a media file"
        );

        assert!(
            !rule_default.is_media_file("document.md"),
            "document.md should not be identified as a media file"
        );
        assert!(
            !rule_default.is_media_file("code.rs"),
            "code.rs should not be identified as a media file"
        );

        // Test media file skipping with default settings (skip_media_files = true)
        assert!(
            rule_default.should_skip_media_file("image.jpg"),
            "image.jpg should be skipped with default settings"
        );
        assert!(
            !rule_default.should_skip_media_file("document.md"),
            "document.md should not be skipped"
        );

        // Test media file skipping with skip_media_files = false
        let rule_no_skip = MD057ExistingRelativeLinks::new().with_skip_media_files(false);
        assert!(
            !rule_no_skip.should_skip_media_file("image.jpg"),
            "image.jpg should not be skipped when skip_media_files is false"
        );
    }

    #[test]
    fn test_no_warnings_without_base_path() {
        let rule = MD057ExistingRelativeLinks::new();
        let content = "[Link](missing.md)";

        let ctx = crate::lint_context::LintContext::new(content);
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

        // Initialize rule with the base path
        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);

        // Test the rule
        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should have one warning for the missing.md link but not for the media file
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing.md"));

        // Test with document structure
        let structure = DocumentStructure::new(content);
        let result_with_structure = rule.check_with_structure(&ctx, &structure).unwrap();

        // Results should be the same
        assert_eq!(result.len(), result_with_structure.len());
        assert!(result_with_structure[0].message.contains("missing.md"));
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

        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check(&ctx).unwrap();

        // Should have one warning for missing.md
        assert_eq!(result.len(), 1, "Should have exactly one warning");
        assert!(
            result[0].message.contains("missing.md"),
            "Warning should mention missing.md"
        );
    }

    #[test]
    fn test_media_file_handling() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();

        // Explicitly check that image.jpg doesn't exist in the test directory
        let image_path = base_path.join("image.jpg");
        assert!(
            !image_path.exists(),
            "Test precondition failed: image.jpg should not exist"
        );

        // Create a test content with a media link - make sure it's very explicit
        let content = "[Media Link](image.jpg)";

        // Test with skip_media_files = true (default)
        let rule_skip_media = MD057ExistingRelativeLinks::new().with_path(base_path);

        let ctx = crate::lint_context::LintContext::new(content);
        let result_skip = rule_skip_media.check(&ctx).unwrap();

        // Should have no warnings when media files are skipped
        assert_eq!(
            result_skip.len(),
            0,
            "Should have no warnings when skip_media_files is true"
        );

        // Test with skip_media_files = false
        let rule_check_all = MD057ExistingRelativeLinks::new()
            .with_path(base_path)
            .with_skip_media_files(false);

        let ctx = crate::lint_context::LintContext::new(content);
        let result_all = rule_check_all.check(&ctx).unwrap();

        // Should warn about the missing media file
        assert_eq!(
            result_all.len(),
            1,
            "Should have one warning when skip_media_files is false"
        );
        assert!(
            result_all[0].message.contains("image.jpg"),
            "Warning should mention image.jpg"
        );
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
        let structure = DocumentStructure::new(content);

        let ctx = crate::lint_context::LintContext::new(content);
        let result = rule.check_with_structure(&ctx, &structure).unwrap();

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
        let ctx = crate::lint_context::LintContext::new(content);
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
}
