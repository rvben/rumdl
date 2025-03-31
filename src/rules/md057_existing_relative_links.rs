use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

lazy_static! {
    // Match markdown links: [text](url) or [text](url "title")
    static ref LINK_REGEX: FancyRegex = FancyRegex::new(r#"(?<!\\)\[([^\]]*)\]\(([^)\s"]+)(?:\s+"[^"]*")?(?:#[^)]*)??\)"#).unwrap();
    static ref CODE_FENCE_REGEX: Regex = Regex::new(r"^(`{3,}|~{3,})").unwrap();
    // Protocol-based URLs
    static ref PROTOCOL_REGEX: Regex = Regex::new(r"^(https?://|ftp://|mailto:|tel:)").unwrap();
    // Domain-based URLs without protocol (www.example.com or example.com)
    static ref DOMAIN_REGEX: Regex = Regex::new(r"^(www\.[a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9-]*\.(com|org|net|io|edu|gov|co|uk|de|ru|jp|cn|br|in|fr|it|nl|ca|es|au|ch))").unwrap();
}

/// Rule MD057: Relative links should point to existing files
///
/// This rule checks if relative links in Markdown files point to files that actually exist
/// in the file system. It helps identify broken links to other files.
#[derive(Debug, Clone)]
pub struct MD057ExistingRelativeLinks {
    /// Base directory for resolving relative links
    base_path: RefCell<Option<PathBuf>>,
}

impl Default for MD057ExistingRelativeLinks {
    fn default() -> Self {
        Self {
            base_path: RefCell::new(None),
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
    
    /// Check if a URL is external
    fn is_external_url(&self, url: &str) -> bool {
        if url.is_empty() {
            return false;
        }
        
        // If it starts with a protocol (http://, https://, ftp://, etc.), it's external
        if PROTOCOL_REGEX.is_match(url) {
            return true;
        }
        
        // If it has a domain-like structure (www.example.com or example.com), it's external
        if DOMAIN_REGEX.is_match(url) {
            return true;
        }
        
        // Check for absolute paths
        if url.starts_with('/') {
            return false; // Absolute paths within the site are not external
        }
        
        // All other cases (relative paths, etc.) are not external
        false
    }
    
    /// Resolve a relative link against the base path
    fn resolve_link_path(&self, link: &str) -> Option<PathBuf> {
        if let Some(base_path) = self.base_path.borrow().as_ref() {
            Some(base_path.join(link))
        } else {
            None
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

    fn check(&self, content: &str) -> LintResult {
        let _line_index = LineIndex::new(content.to_string());
        let mut warnings = Vec::new();
        let mut in_code_block = false;
        let mut code_fence_marker = String::new();

        // If no base path is set, we can't validate relative links
        if self.base_path.borrow().is_none() {
            return Ok(warnings);
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

            // Find all links in the line
            if let Ok(matches) = LINK_REGEX.captures_iter(line).collect::<Result<Vec<_>, _>>() {
                for cap in matches {
                    // Skip processing if the rule is disabled for this line
                    if crate::rule::is_rule_disabled_at_line(content, self.name(), line_num) {
                        continue;
                    }
                    
                    if let (Some(text_match), Some(url_match)) = (cap.get(1), cap.get(2)) {
                        let _text = text_match.as_str();
                        let url = url_match.as_str().trim();
                        
                        // Skip empty or external URLs
                        if url.is_empty() || self.is_external_url(url) {
                            continue;
                        }
                        
                        // Resolve the relative link against the base path
                        if let Some(resolved_path) = self.resolve_link_path(url) {
                            // Check if the file exists
                            if !resolved_path.exists() {
                                let full_match = cap.get(0).unwrap();
                                
                                warnings.push(LintWarning {
                                    line: line_num + 1,
                                    column: full_match.start() + 1,
                                    message: format!("Relative link '{}' does not exist", url),
                                    severity: Severity::Warning,
                                    fix: None, // No automatic fix for missing files
                                });
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
        
        // Create test content with both existing and missing links
        let content = r#"
# Test Document

[Valid Link](exists.md)
[Invalid Link](missing.md)
[External Link](https://example.com)
        "#;
        
        // Initialize rule with the base path
        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
        
        // Test the rule
        let result = rule.check(content).unwrap();
        
        // Should have one warning for the missing.md link
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing.md"));
    }

    #[test]
    fn test_code_blocks() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();
        
        // Create test content with links in code blocks
        let content = r#"
# Test Document

[Invalid Link](missing.md)

```markdown
[Another Invalid Link](also-missing.md)
```
        "#;
        
        // Initialize rule with the base path
        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
        
        // Test the rule
        let result = rule.check(content).unwrap();
        
        // Should only have one warning for the link outside the code block
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("missing.md"));
        assert!(!result[0].message.contains("also-missing.md"));
    }

    #[test]
    fn test_disabled_rule() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();
        
        // Create test content with disabled rule
        let content = r#"
# Test Document

<!-- markdownlint-disable MD057 -->
[Invalid Link](missing.md)
<!-- markdownlint-enable MD057 -->

[Another Invalid Link](also-missing.md)
        "#;
        
        // Initialize rule with the base path
        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
        
        // Test the rule
        let result = rule.check(content).unwrap();
        
        // Should only have one warning for the link after enabling the rule
        assert_eq!(result.len(), 1, "Expected 1 warning, got {}", result.len());
        assert!(result[0].message.contains("also-missing.md"), 
                "Expected warning about also-missing.md, got: {}", result[0].message);
        assert_eq!(result[0].line, 8, "Warning should be on line 8");
        
        // Verify that the exact disabled link was not detected
        for warning in &result {
            // Check for the exact link "missing.md", not a substring match
            let msg = &warning.message;
            assert!(!msg.contains("'missing.md'"), 
                   "Found warning for disabled link 'missing.md' in message: {}", msg);
        }
    }

    #[test]
    fn test_links_with_titles() {
        // Create a temporary directory for test files
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path();
        
        // Create an existing file
        let exists_path = base_path.join("exists.md");
        File::create(&exists_path).unwrap().write_all(b"# Test File").unwrap();
        
        // Create test content with links that have titles
        let content = r#"
# Test Document with Titled Links

[Valid Link](exists.md "This is a valid link")
[Invalid Link](missing.md "This is an invalid link")
[External Link](https://example.com "External site")
        "#;
        
        // Initialize rule with the base path
        let rule = MD057ExistingRelativeLinks::new().with_path(base_path);
        
        // Test the rule
        let result = rule.check(content).unwrap();
        
        // Should have one warning for the missing.md link
        assert_eq!(result.len(), 1, "Expected only one warning for missing.md");
        assert!(result[0].message.contains("missing.md"), "Warning should mention missing.md not the title");
        assert!(!result[0].message.contains("This is an invalid link"), "Warning should not include the title text");
    }
} 