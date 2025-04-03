use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

lazy_static! {
    // Match markdown links: [text](url) or [text](url "title") or [text](<url>)
    // Updated to better handle angle brackets in URLs and capture fragments separately
    static ref LINK_REGEX: FancyRegex = FancyRegex::new(r#"(?<!\\)\[([^\]]*)\]\(\s*<?([^">\s#]+)(#[^)\s"]*)?(?:\s+"[^"]*")??\s*>?\s*\)"#).unwrap();
    static ref CODE_FENCE_REGEX: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();
    // Protocol-based URLs
    static ref PROTOCOL_REGEX: Regex = Regex::new(r"^(https?://|ftp://|mailto:|tel:)").unwrap();
    // Domain-based URLs without protocol (www.example.com or example.com)
    // Updated to more precisely match domain patterns and avoid matching common filenames
    static ref DOMAIN_REGEX: Regex = Regex::new(r"^(www\.[a-zA-Z0-9]|(^[a-zA-Z0-9][a-zA-Z0-9-]*\.[a-zA-Z]{2,}))").unwrap();
    // Media files pattern - extensions that typically don't need to exist locally
    static ref MEDIA_FILES_REGEX: Regex = Regex::new(r"\.(pdf|mp4|mp3|avi|mov|flv|wmv|webm|ogg|wav|flac|aac|m4a|jpg|jpeg|png|gif|bmp|svg|webp|tiff|ico)$").unwrap();
    // Fragment-only links pattern (links to headings within the same document)
    static ref FRAGMENT_ONLY_REGEX: Regex = Regex::new(r"^#").unwrap();
}

/// Rule MD057: Relative links should point to existing files
///
/// This rule checks if relative links in Markdown files point to files that actually exist
/// in the file system. It helps identify broken links to other files.
#[derive(Debug, Clone)]
pub struct MD057ExistingRelativeLinks {
    /// Base directory for resolving relative links
    base_path: RefCell<Option<PathBuf>>,
    /// Skip checking media files
    skip_media_files: bool,
}

impl Default for MD057ExistingRelativeLinks {
    fn default() -> Self {
        Self {
            base_path: RefCell::new(None),
            skip_media_files: true,
        }
    }
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
        
        *self.base_path.borrow_mut() = dir_path;
        self
    }
    
    /// Configure whether to skip checking media files
    pub fn with_skip_media_files(mut self, skip_media_files: bool) -> Self {
        self.skip_media_files = skip_media_files;
        self
    }
    
    /// Check if a URL is external
    fn is_external_url(&self, url: &str) -> bool {
        let debug = false; // Debug output disabled for normal operation
        
        if url.is_empty() {
            if debug {
                // println!("is_external_url: URL is empty");
            }
            return false;
        }
        
        // If it starts with a protocol (http://, https://, ftp://, etc.), it's external
        if PROTOCOL_REGEX.is_match(url) {
            if debug {
                // println!("is_external_url: URL '{}' matches protocol pattern", url);
            }
            return true;
        }
        
        // Check for www. prefix which indicates an external URL
        if url.starts_with("www.") {
            if debug {
                // println!("is_external_url: URL '{}' starts with www.", url);
            }
            return true;
        }
        
        // More restrictive domain check - must contain a dot and end with known TLD
        // But not check for media files extensions which are handled separately
        if !self.is_media_file(url) && url.contains('.') && 
           url.split('.').last().map_or(false, |tld| 
               ["com", "org", "net", "io", "edu", "gov", "co", "uk", "de", 
                "ru", "jp", "cn", "br", "in", "fr", "it", "nl", "ca", "es", "au", "ch"]
                   .contains(&tld)) {
            if debug {
                // println!("is_external_url: URL '{}' matches domain pattern with valid TLD", url);
            }
            return true;
        }
        
        // Check for absolute paths
        if url.starts_with('/') {
            if debug {
                // println!("is_external_url: URL '{}' is an absolute path, not external", url);
            }
            return false; // Absolute paths within the site are not external
        }
        
        // All other cases (relative paths, etc.) are not external
        if debug {
            // println!("is_external_url: URL '{}' is not external", url);
        }
        false
    }
    
    /// Check if the URL is a fragment-only link (internal document link)
    fn is_fragment_only_link(&self, url: &str) -> bool {
        FRAGMENT_ONLY_REGEX.is_match(url)
    }
    
    /// Check if the URL has a media file extension
    fn is_media_file(&self, url: &str) -> bool {
        MEDIA_FILES_REGEX.is_match(url)
    }
    
    /// Determine if we should skip checking this media file
    fn should_skip_media_file(&self, url: &str) -> bool {
        self.skip_media_files && self.is_media_file(url)
    }
    
    /// Resolve a relative link against the base path
    fn resolve_link_path(&self, link: &str) -> Option<PathBuf> {
        self.base_path.borrow().as_ref().map(|base_path| base_path.join(link))
    }
    
    /// Detect inline code spans in a line and return their ranges
    fn compute_inline_code_spans(&self, line: &str) -> Vec<(usize, usize)> {
        if !line.contains('`') {
            return Vec::new();
        }

        let mut spans = Vec::new();
        let mut in_code = false;
        let mut code_start = 0;

        for (i, c) in line.chars().enumerate() {
            if c == '`' {
                if !in_code {
                    code_start = i;
                    in_code = true;
                } else {
                    spans.push((code_start, i + 1)); // Include the closing backtick
                    in_code = false;
                }
            }
        }

        spans
    }
    
    /// Check if a position is within an inline code span
    fn is_in_code_span(&self, spans: &[(usize, usize)], pos: usize) -> bool {
        spans.iter().any(|&(start, end)| pos >= start && pos < end)
    }
}

impl Rule for MD057ExistingRelativeLinks {
    fn name(&self) -> &'static str {
        "MD057"
    }

    fn description(&self) -> &'static str {
        "Relative links should point to existing files"
    }

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();
        let debug = false; // Debug output disabled for normal operation

        // If no base path is set, we can't validate relative links
        if self.base_path.borrow().is_none() {
            return Ok(warnings);
        }

        let base_path = self.base_path.borrow();
        if debug {
            // println!("Base path: {:?}", base_path);
        }

        for (line_num, line) in content.lines().enumerate() {
            // Handle code block boundaries
            if let Some(cap) = CODE_FENCE_REGEX.captures(line) {
                let marker = cap[0].to_string();
                if !in_code_block {
                    in_code_block = true;
                    code_fence_marker = marker;
                } else if line.trim().starts_with(&code_fence_marker) {
                    in_code_block = false;
                    code_fence_marker.clear();
                }
                continue;
            }

            // Skip lines in code blocks
            if in_code_block {
                continue;
            }

            // Skip processing if the rule is disabled for this line
            if crate::rule::is_rule_disabled_at_line(content, self.name(), line_num) {
                continue;
            }
            
            // Detect inline code spans in this line
            let inline_code_spans = self.compute_inline_code_spans(line);

            // Find all links in the line
            if let Ok(matches) = LINK_REGEX.captures_iter(line).collect::<Result<Vec<_>, _>>() {
                for cap in matches {
                    if let (Some(full_match), Some(_text_match), Some(url_match)) = 
                           (cap.get(0), cap.get(1), cap.get(2)) {
                        // Skip links inside inline code spans
                        if self.is_in_code_span(&inline_code_spans, full_match.start()) {
                            if debug {
                                // println!("Skipping link inside inline code span");
                            }
                            continue;
                        }
                        
                        let mut url = url_match.as_str().trim();
                        
                        if debug {
                            // println!("Found URL: '{}'", url);
                        }
                        
                        // Clean the URL - remove trailing '>' if present
                        if url.ends_with('>') {
                            url = &url[..url.len() - 1];
                            if debug {
                                // println!("Cleaned URL: '{}'", url);
                            }
                        }
                        
                        // Skip empty or external URLs
                        if url.is_empty() || self.is_external_url(url) {
                            if debug {
                                // println!("Skipping URL '{}': empty or external", url);
                            }
                            continue;
                        }
                        
                        // Skip fragment-only links (internal document links)
                        if self.is_fragment_only_link(url) {
                            if debug {
                                // println!("Skipping URL '{}': fragment-only link", url);
                            }
                            continue;
                        }
                        
                        // Check if it's a media file (for debugging)
                        let is_media = self.is_media_file(url);
                        let should_skip = self.should_skip_media_file(url);
                        
                        if debug {
                            // println!("URL '{}': is_media={}, should_skip={}", url, is_media, should_skip);
                        }
                        
                        // Skip media files if configured to do so
                        if should_skip {
                            if debug {
                                // println!("URL '{}' is a media file and should be skipped", url);
                            }
                            continue;
                        }
                        
                        // Resolve the relative link against the base path
                        if let Some(resolved_path) = self.resolve_link_path(url) {
                            let exists = resolved_path.exists();
                            
                            if debug {
                                // println!("Resolved path: {:?}", resolved_path);
                                // println!("Path exists? {}", exists);
                            }
                            
                            // Check if the file exists
                            if !exists {
                                let full_match = cap.get(0).unwrap();
                                
                                warnings.push(LintWarning {
            rule_name: Some(self.name()),
                                    line: line_num + 1,
                                    column: full_match.start() + 1,
                                    message: format!("Relative link '{}' does not exist", url),
                                    severity: Severity::Warning,
                                    fix: None, // No automatic fix for missing files
                                });
                                
                                if debug {
                                    // println!("Added warning for non-existent file: {}", url);
                                }
                            } else if debug {
                                // println!("File exists: {}", url);
                            }
                        }
                    }
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, _content: &str) -> Result<String, LintError> {
        // No automatic fix is provided for this rule
        // as creating missing files is beyond the scope of a linter
        Err(LintError::FixFailed(
            "Cannot automatically fix missing files".to_string(),
        ))
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
        assert!(rule_default.is_media_file("image.jpg"), "image.jpg should be identified as a media file");
        assert!(rule_default.is_media_file("video.mp4"), "video.mp4 should be identified as a media file");
        assert!(rule_default.is_media_file("document.pdf"), "document.pdf should be identified as a media file");
        assert!(rule_default.is_media_file("path/to/audio.mp3"), "path/to/audio.mp3 should be identified as a media file");
        
        assert!(!rule_default.is_media_file("document.md"), "document.md should not be identified as a media file");
        assert!(!rule_default.is_media_file("code.rs"), "code.rs should not be identified as a media file");
        
        // Test media file skipping with default settings (skip_media_files = true)
        assert!(rule_default.should_skip_media_file("image.jpg"), "image.jpg should be skipped with default settings");
        assert!(!rule_default.should_skip_media_file("document.md"), "document.md should not be skipped");
        
        // Test media file skipping with skip_media_files = false
        let rule_no_skip = MD057ExistingRelativeLinks::new().with_skip_media_files(false);
        assert!(!rule_no_skip.should_skip_media_file("image.jpg"), "image.jpg should not be skipped when skip_media_files is false");
    }

    #[test]
    fn test_no_warnings_without_base_path() {
        let rule = MD057ExistingRelativeLinks::new();
        let content = "[Link](missing.md)";
        
        let result = rule.check(content).unwrap();
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
        let result = rule.check(content).unwrap();
        
        // Should have one warning for the missing.md link but not for the media file
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing.md"));
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
        let rule = MD057ExistingRelativeLinks::new()
            .with_path(base_path);
        
        let result = rule.check(content).unwrap();
        
        // Should have one warning for missing.md
        assert_eq!(result.len(), 1, "Should have exactly one warning");
        assert!(result[0].message.contains("missing.md"), "Warning should mention missing.md");
    }

    #[test]
    fn test_media_file_handling() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();
        
        // Explicitly check that image.jpg doesn't exist in the test directory
        let image_path = base_path.join("image.jpg");
        assert!(!image_path.exists(), "Test precondition failed: image.jpg should not exist");
        
        // Create a test content with a media link - make sure it's very explicit
        let content = "[Media Link](image.jpg)";
        
        // Test with skip_media_files = true (default)
        let rule_skip_media = MD057ExistingRelativeLinks::new()
            .with_path(base_path);
        
        let result_skip = rule_skip_media.check(content).unwrap();
        
        // Should have no warnings when media files are skipped
        assert_eq!(result_skip.len(), 0, "Should have no warnings when skip_media_files is true");
        
        // Test with skip_media_files = false
        let rule_check_all = MD057ExistingRelativeLinks::new()
            .with_path(base_path)
            .with_skip_media_files(false);
        
        // Debug: Verify media file identification and handling
        if false { // Set to false to disable debug output in tests
            // println!("Is 'image.jpg' a media file? {}", rule_check_all.is_media_file("image.jpg"));
            // println!("Should skip 'image.jpg'? {}", rule_check_all.should_skip_media_file("image.jpg"));
            // println!("Skip media files setting: {}", rule_check_all.skip_media_files);
        }
        
        // Ensure the file still doesn't exist
        assert!(!image_path.exists(), "image.jpg should not exist for this test");
        
        // Debug: Verify the path resolution
        if let Some(resolved) = rule_check_all.resolve_link_path("image.jpg") {
            if false { // Set to false to disable debug output
                // println!("Resolved path: {:?}", resolved);
                // println!("Path exists? {}", resolved.exists());
            } else {
                // println!("Failed to resolve path");
            }
        } else {
            // println!("Failed to resolve path");
        }
        
        let result_all = rule_check_all.check(content).unwrap();
        
        if false { // Set to false to disable debug output in tests
            // println!("Number of warnings: {}", result_all.len());
            // for (i, warning) in result_all.iter().enumerate() {
            //     println!("Warning {}: {}", i, warning.message);
            // }
        }
        
        // Should warn about the missing media file
        assert_eq!(result_all.len(), 1, "Should have one warning when skip_media_files is false");
        assert!(result_all[0].message.contains("image.jpg"), "Warning should mention image.jpg");
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
        let result = rule.check(content).unwrap();
        
        // Should only have warning for the normal link, not for links in code spans
        assert_eq!(result.len(), 1, "Should have exactly one warning");
        assert!(result[0].message.contains("missing.md"), "Warning should be for missing.md");
        assert!(!result.iter().any(|w| w.message.contains("another-missing.md")), 
               "Should not warn about link in code span");
        assert!(!result.iter().any(|w| w.message.contains("yet-another-missing.md")), 
               "Should not warn about link in inline code");
    }
    
    #[test]
    fn test_compute_inline_code_spans() {
        let rule = MD057ExistingRelativeLinks::new();
        
        // Test with no backticks
        let spans = rule.compute_inline_code_spans("No code spans here");
        assert!(spans.is_empty(), "Should have no spans when no backticks are present");
        
        // Test with a simple code span
        let spans = rule.compute_inline_code_spans("Text with `code span` in it");
        assert_eq!(spans.len(), 1, "Should detect one code span");
        assert_eq!(spans[0], (10, 21), "Code span should be at the correct position");
        
        // Test with multiple code spans
        let spans = rule.compute_inline_code_spans("Multiple `code` spans `in one` line");
        assert_eq!(spans.len(), 2, "Should detect two code spans");
        assert_eq!(spans[0], (9, 15), "First code span should be at the correct position");
        assert_eq!(spans[1], (22, 30), "Second code span should be at the correct position");
        
        // Test with unbalanced backticks
        let spans = rule.compute_inline_code_spans("Unbalanced `backtick");
        assert!(spans.is_empty(), "Should not detect unbalanced backticks as spans");
    }
} 