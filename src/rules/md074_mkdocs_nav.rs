//!
//! Rule MD074: MkDocs nav validation
//!
//! See [docs/md074.md](../../docs/md074.md) for full documentation, configuration, and examples.

use crate::rule::{LintError, LintResult, LintWarning, Rule, RuleCategory, Severity};
use crate::rule_config_serde::RuleConfig;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

mod md074_config;
pub use md074_config::{MD074Config, NavValidation};

/// Cache to track which mkdocs.yml files have been validated in this session.
/// Prevents redundant validation when checking multiple files in the same project.
static VALIDATED_PROJECTS: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Rule MD074: MkDocs nav validation
///
/// Validates that MkDocs nav entries in mkdocs.yml point to existing files.
/// Only active when the markdown flavor is set to "mkdocs".
#[derive(Debug, Clone)]
pub struct MD074MkDocsNav {
    config: MD074Config,
}

impl Default for MD074MkDocsNav {
    fn default() -> Self {
        Self::new()
    }
}

impl MD074MkDocsNav {
    pub fn new() -> Self {
        Self {
            config: MD074Config::default(),
        }
    }

    pub fn from_config_struct(config: MD074Config) -> Self {
        Self { config }
    }

    /// Clear the validation cache. Useful for testing.
    #[cfg(test)]
    pub fn clear_cache() {
        if let Ok(mut cache) = VALIDATED_PROJECTS.lock() {
            cache.clear();
        }
    }

    /// Find mkdocs.yml or mkdocs.yaml in the directory or parent directories
    fn find_mkdocs_yml(start_path: &Path) -> Option<PathBuf> {
        let mut current = if start_path.is_file() {
            start_path.parent()?.to_path_buf()
        } else {
            start_path.to_path_buf()
        };

        loop {
            // Check both .yml and .yaml extensions
            for filename in &["mkdocs.yml", "mkdocs.yaml"] {
                let mkdocs_path = current.join(filename);
                if mkdocs_path.exists() {
                    // Canonicalize to get consistent path for caching
                    return mkdocs_path.canonicalize().ok();
                }
            }

            if !current.pop() {
                break;
            }
        }

        None
    }

    /// Parse mkdocs.yml and extract configuration
    fn parse_mkdocs_yml(path: &Path) -> Result<MkDocsConfig, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

        serde_yml::from_str(&content).map_err(|e| format!("Failed to parse {}: {e}", path.display()))
    }

    /// Recursively extract all file paths from nav structure
    /// Returns tuples of (file_path, nav_location_description)
    fn extract_nav_paths(nav: &[NavItem], prefix: &str) -> Vec<(String, String)> {
        let mut paths = Vec::new();

        for item in nav {
            match item {
                NavItem::Path(path) => {
                    let nav_path = if prefix.is_empty() {
                        path.clone()
                    } else {
                        format!("{prefix} > {path}")
                    };
                    paths.push((path.clone(), nav_path));
                }
                NavItem::Section { name, children } => {
                    let new_prefix = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{prefix} > {name}")
                    };
                    paths.extend(Self::extract_nav_paths(children, &new_prefix));
                }
                NavItem::NamedPath { name, path } => {
                    let nav_path = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{prefix} > {name}")
                    };
                    paths.push((path.clone(), nav_path));
                }
            }
        }

        paths
    }

    /// Collect all markdown files in docs_dir recursively
    fn collect_docs_files(docs_dir: &Path) -> HashSet<PathBuf> {
        Self::collect_docs_files_recursive(docs_dir, docs_dir)
    }

    /// Recursive helper that preserves the original docs_dir for relative path calculation
    fn collect_docs_files_recursive(current_dir: &Path, root_docs_dir: &Path) -> HashSet<PathBuf> {
        let mut files = HashSet::new();

        let entries = match std::fs::read_dir(current_dir) {
            Ok(entries) => entries,
            Err(_) => return files,
        };

        for entry in entries.flatten() {
            let path = entry.path();

            // Skip hidden directories and files
            if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                continue;
            }

            if path.is_dir() {
                files.extend(Self::collect_docs_files_recursive(&path, root_docs_dir));
            } else if path.is_file()
                && let Some(ext) = path.extension()
            {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                if ext_lower == "md" || ext_lower == "markdown" {
                    // Get path relative to docs_dir, normalized with forward slashes
                    if let Ok(relative) = path.strip_prefix(root_docs_dir) {
                        let normalized = Self::normalize_path(relative);
                        files.insert(normalized);
                    }
                }
            }
        }

        files
    }

    /// Normalize a path to use forward slashes (for cross-platform consistency)
    fn normalize_path(path: &Path) -> PathBuf {
        let path_str = path.to_string_lossy();
        PathBuf::from(path_str.replace('\\', "/"))
    }

    /// Normalize a nav path string for comparison
    fn normalize_nav_path(path: &str) -> PathBuf {
        PathBuf::from(path.replace('\\', "/"))
    }

    /// Check if a path looks like an external URL
    fn is_external_url(path: &str) -> bool {
        path.starts_with("http://") || path.starts_with("https://") || path.starts_with("//") || path.contains("://")
    }

    /// Check if a path is absolute (starts with /)
    fn is_absolute_path(path: &str) -> bool {
        path.starts_with('/')
    }

    /// Perform the actual validation of mkdocs.yml nav entries
    fn validate_nav(&self, mkdocs_path: &Path, mkdocs_config: &MkDocsConfig) -> Vec<LintWarning> {
        let mut warnings = Vec::new();
        let mkdocs_file = mkdocs_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "mkdocs.yml".to_string());

        // Get docs_dir relative to mkdocs.yml location
        let mkdocs_dir = mkdocs_path.parent().unwrap_or(Path::new("."));
        let docs_dir = if Path::new(&mkdocs_config.docs_dir).is_absolute() {
            PathBuf::from(&mkdocs_config.docs_dir)
        } else {
            mkdocs_dir.join(&mkdocs_config.docs_dir)
        };

        if !docs_dir.exists() {
            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 1,
                message: format!(
                    "docs_dir '{}' does not exist (from {})",
                    mkdocs_config.docs_dir,
                    mkdocs_path.display()
                ),
                severity: Severity::Warning,
                fix: None,
            });
            return warnings;
        }

        // Extract all nav paths
        let nav_paths = Self::extract_nav_paths(&mkdocs_config.nav, "");

        // Track referenced files for omitted_files check (normalized paths)
        let mut referenced_files: HashSet<PathBuf> = HashSet::new();

        // Validate each nav entry
        for (file_path, nav_location) in &nav_paths {
            // Skip external URLs
            if Self::is_external_url(file_path) {
                continue;
            }

            // Check for absolute links
            if Self::is_absolute_path(file_path) {
                if self.config.absolute_links == NavValidation::Warn {
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 1,
                        message: format!("Absolute path in nav '{nav_location}': {file_path} (in {mkdocs_file})"),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
                continue;
            }

            let normalized_path = Self::normalize_nav_path(file_path);

            // Check if file exists
            if self.config.not_found == NavValidation::Warn {
                let full_path = docs_dir.join(&normalized_path);

                // Handle directory entries (e.g., "api/" -> "api/index.md")
                let (actual_path, is_dir_entry) = if file_path.ends_with('/') || full_path.is_dir() {
                    let index_path = normalized_path.join("index.md");
                    (docs_dir.join(&index_path), true)
                } else {
                    (full_path, false)
                };

                // Track the actual file that would be served
                if is_dir_entry {
                    referenced_files.insert(normalized_path.join("index.md"));
                } else {
                    referenced_files.insert(normalized_path.clone());
                }

                if !actual_path.exists() {
                    let display_path = if is_dir_entry {
                        format!(
                            "{} (resolves to {}/index.md)",
                            file_path,
                            file_path.trim_end_matches('/')
                        )
                    } else {
                        file_path.to_string()
                    };
                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 1,
                        message: format!(
                            "Nav entry '{nav_location}' points to non-existent file: {display_path} (in {mkdocs_file})"
                        ),
                        severity: Severity::Warning,
                        fix: None,
                    });
                }
            } else {
                // Still track referenced files even if not validating
                if file_path.ends_with('/') {
                    referenced_files.insert(normalized_path.join("index.md"));
                } else {
                    referenced_files.insert(normalized_path);
                }
            }
        }

        // Check for omitted files
        if self.config.omitted_files == NavValidation::Warn {
            let all_docs = Self::collect_docs_files(&docs_dir);

            for doc_file in all_docs {
                if !referenced_files.contains(&doc_file) {
                    // Skip common files that are often intentionally not in nav
                    let file_name = doc_file.file_name().map(|n| n.to_string_lossy());
                    if let Some(name) = &file_name {
                        let name_lower = name.to_lowercase();
                        // Skip index files in root, README files, and other common non-nav files
                        if (doc_file.parent().is_none() || doc_file.parent() == Some(Path::new("")))
                            && (name_lower == "index.md" || name_lower == "readme.md")
                        {
                            continue;
                        }
                    }

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 1,
                        message: format!("File not referenced in nav: {} (in {mkdocs_file})", doc_file.display()),
                        severity: Severity::Info,
                        fix: None,
                    });
                }
            }
        }

        warnings
    }
}

/// MkDocs configuration structure (partial - only fields we need for validation)
#[derive(Debug)]
struct MkDocsConfig {
    /// Documentation directory (default: "docs")
    docs_dir: String,

    /// Navigation structure
    nav: Vec<NavItem>,
}

fn default_docs_dir() -> String {
    "docs".to_string()
}

/// Navigation item in mkdocs.yml
/// MkDocs nav can be:
/// - A simple string: "index.md"
/// - A named path: { "Home": "index.md" }
/// - A section with children: { "Section": [...] }
#[derive(Debug)]
enum NavItem {
    /// Simple path: "index.md"
    Path(String),

    /// Section with children: { "Section Name": [...] }
    Section { name: String, children: Vec<NavItem> },

    /// Named path: { "Page Title": "path/to/page.md" }
    NamedPath { name: String, path: String },
}

impl NavItem {
    /// Parse a NavItem from a serde_yml::Value
    fn from_yaml_value(value: &serde_yml::Value) -> Option<NavItem> {
        match value {
            serde_yml::Value::String(s) => Some(NavItem::Path(s.clone())),
            serde_yml::Value::Mapping(map) => {
                if map.len() != 1 {
                    return None;
                }

                let (key, val) = map.iter().next()?;
                let name = key.as_str()?.to_string();

                match val {
                    serde_yml::Value::String(path) => Some(NavItem::NamedPath {
                        name,
                        path: path.clone(),
                    }),
                    serde_yml::Value::Sequence(seq) => {
                        let children: Vec<NavItem> = seq.iter().filter_map(NavItem::from_yaml_value).collect();
                        Some(NavItem::Section { name, children })
                    }
                    serde_yml::Value::Null => {
                        // Handle case like "- Section:" with no value (treated as empty section)
                        Some(NavItem::Section {
                            name,
                            children: Vec::new(),
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for MkDocsConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawMkDocsConfig {
            #[serde(default = "default_docs_dir")]
            docs_dir: String,
            #[serde(default)]
            nav: Option<serde_yml::Value>,
        }

        let raw = RawMkDocsConfig::deserialize(deserializer)?;

        let nav = match raw.nav {
            Some(serde_yml::Value::Sequence(seq)) => seq.iter().filter_map(NavItem::from_yaml_value).collect(),
            _ => Vec::new(),
        };

        Ok(MkDocsConfig {
            docs_dir: raw.docs_dir,
            nav,
        })
    }
}

impl Rule for MD074MkDocsNav {
    fn name(&self) -> &'static str {
        "MD074"
    }

    fn description(&self) -> &'static str {
        "MkDocs nav entries should point to existing files"
    }

    fn category(&self) -> RuleCategory {
        // Use Other to bypass content-based filtering since this rule
        // validates mkdocs.yml, not links in the markdown content
        RuleCategory::Other
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Only run for MkDocs flavor
        ctx.flavor != crate::config::MarkdownFlavor::MkDocs
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        // Only run for MkDocs flavor
        if ctx.flavor != crate::config::MarkdownFlavor::MkDocs {
            return Ok(Vec::new());
        }

        // Need source file path to find mkdocs.yml
        let Some(source_file) = &ctx.source_file else {
            return Ok(Vec::new());
        };

        // Find mkdocs.yml (returns canonicalized path for consistent caching)
        let Some(mkdocs_path) = Self::find_mkdocs_yml(source_file) else {
            return Ok(Vec::new());
        };

        // Check if we've already validated this mkdocs.yml in this session
        // This prevents duplicate warnings when checking multiple files in the same project
        if let Ok(mut cache) = VALIDATED_PROJECTS.lock() {
            // Use insert() return value to atomically check-and-insert (avoids TOCTOU race)
            if !cache.insert(mkdocs_path.clone()) {
                // Already validated, skip
                return Ok(Vec::new());
            }
        }
        // If lock is poisoned, continue with validation (just without caching)

        // Parse mkdocs.yml
        let mkdocs_config = match Self::parse_mkdocs_yml(&mkdocs_path) {
            Ok(config) => config,
            Err(e) => {
                // Return a warning about the parse failure instead of silently failing
                return Ok(vec![LintWarning {
                    rule_name: Some(self.name().to_string()),
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 1,
                    message: e,
                    severity: Severity::Warning,
                    fix: None,
                }]);
            }
        };

        // Perform validation
        Ok(self.validate_nav(&mkdocs_path, &mkdocs_config))
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        // This rule doesn't provide automatic fixes
        Ok(ctx.content.to_string())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let default_config = MD074Config::default();
        let json_value = serde_json::to_value(&default_config).ok()?;
        let toml_value = crate::rule_config_serde::json_to_toml_value(&json_value)?;

        if let toml::Value::Table(table) = toml_value {
            if !table.is_empty() {
                Some((MD074Config::RULE_NAME.to_string(), toml::Value::Table(table)))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD074Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn setup_test() {
        MD074MkDocsNav::clear_cache();
    }

    #[test]
    fn test_find_mkdocs_yml() {
        setup_test();
        let temp_dir = tempdir().unwrap();
        let mkdocs_path = temp_dir.path().join("mkdocs.yml");
        fs::write(&mkdocs_path, "site_name: Test").unwrap();

        let subdir = temp_dir.path().join("docs");
        fs::create_dir_all(&subdir).unwrap();
        let file_in_subdir = subdir.join("test.md");

        let found = MD074MkDocsNav::find_mkdocs_yml(&file_in_subdir);
        assert!(found.is_some());
        // Canonicalized paths should match
        assert_eq!(found.unwrap(), mkdocs_path.canonicalize().unwrap());
    }

    #[test]
    fn test_find_mkdocs_yaml_extension() {
        setup_test();
        let temp_dir = tempdir().unwrap();
        let mkdocs_path = temp_dir.path().join("mkdocs.yaml"); // .yaml extension
        fs::write(&mkdocs_path, "site_name: Test").unwrap();

        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        let file_in_docs = docs_dir.join("test.md");

        let found = MD074MkDocsNav::find_mkdocs_yml(&file_in_docs);
        assert!(found.is_some(), "Should find mkdocs.yaml");
        assert_eq!(found.unwrap(), mkdocs_path.canonicalize().unwrap());
    }

    #[test]
    fn test_parse_simple_nav() {
        setup_test();
        let temp_dir = tempdir().unwrap();
        let mkdocs_path = temp_dir.path().join("mkdocs.yml");

        let mkdocs_content = r#"
site_name: Test
docs_dir: docs
nav:
  - Home: index.md
  - Guide: guide.md
  - Section:
    - Page 1: section/page1.md
    - Page 2: section/page2.md
"#;
        fs::write(&mkdocs_path, mkdocs_content).unwrap();

        let config = MD074MkDocsNav::parse_mkdocs_yml(&mkdocs_path).unwrap();
        assert_eq!(config.docs_dir, "docs");
        assert_eq!(config.nav.len(), 3);

        let paths = MD074MkDocsNav::extract_nav_paths(&config.nav, "");
        assert_eq!(paths.len(), 4);

        // Check paths are extracted correctly
        let path_strs: Vec<&str> = paths.iter().map(|(p, _)| p.as_str()).collect();
        assert!(path_strs.contains(&"index.md"));
        assert!(path_strs.contains(&"guide.md"));
        assert!(path_strs.contains(&"section/page1.md"));
        assert!(path_strs.contains(&"section/page2.md"));
    }

    #[test]
    fn test_parse_deeply_nested_nav() {
        setup_test();
        let temp_dir = tempdir().unwrap();
        let mkdocs_path = temp_dir.path().join("mkdocs.yml");

        let mkdocs_content = r#"
site_name: Test
nav:
  - Level 1:
    - Level 2:
      - Level 3:
        - Deep Page: deep/nested/page.md
"#;
        fs::write(&mkdocs_path, mkdocs_content).unwrap();

        let config = MD074MkDocsNav::parse_mkdocs_yml(&mkdocs_path).unwrap();
        let paths = MD074MkDocsNav::extract_nav_paths(&config.nav, "");

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].0, "deep/nested/page.md");
        assert!(paths[0].1.contains("Level 1"));
        assert!(paths[0].1.contains("Level 2"));
        assert!(paths[0].1.contains("Level 3"));
    }

    #[test]
    fn test_parse_nav_with_external_urls() {
        setup_test();
        let temp_dir = tempdir().unwrap();
        let mkdocs_path = temp_dir.path().join("mkdocs.yml");

        let mkdocs_content = r#"
site_name: Test
docs_dir: docs
nav:
  - Home: index.md
  - GitHub: https://github.com/example/repo
  - External: http://example.com
  - Protocol Relative: //example.com/path
"#;
        fs::write(&mkdocs_path, mkdocs_content).unwrap();

        let config = MD074MkDocsNav::parse_mkdocs_yml(&mkdocs_path).unwrap();
        let paths = MD074MkDocsNav::extract_nav_paths(&config.nav, "");

        // All 4 entries are extracted
        assert_eq!(paths.len(), 4);

        // Verify external URL detection
        assert!(!MD074MkDocsNav::is_external_url("index.md"));
        assert!(MD074MkDocsNav::is_external_url("https://github.com/example/repo"));
        assert!(MD074MkDocsNav::is_external_url("http://example.com"));
        assert!(MD074MkDocsNav::is_external_url("//example.com/path"));
    }

    #[test]
    fn test_parse_nav_with_empty_section() {
        setup_test();
        let temp_dir = tempdir().unwrap();
        let mkdocs_path = temp_dir.path().join("mkdocs.yml");

        // Empty section (null value)
        let mkdocs_content = r#"
site_name: Test
nav:
  - Empty Section:
  - Home: index.md
"#;
        fs::write(&mkdocs_path, mkdocs_content).unwrap();

        let result = MD074MkDocsNav::parse_mkdocs_yml(&mkdocs_path);
        assert!(result.is_ok(), "Should handle empty sections");
    }

    #[test]
    fn test_nav_not_found_validation() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        // Create mkdocs.yml
        let mkdocs_content = r#"
site_name: Test
docs_dir: docs
nav:
  - Home: index.md
  - Missing: missing.md
"#;
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        // Create docs directory with only index.md
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("index.md"), "# Home").unwrap();

        // Create a test markdown file
        let test_file = docs_dir.join("test.md");
        fs::write(&test_file, "# Test").unwrap();

        let rule = MD074MkDocsNav::new();
        let ctx =
            crate::lint_context::LintContext::new("# Test", crate::config::MarkdownFlavor::MkDocs, Some(test_file));

        let result = rule.check(&ctx).unwrap();

        // Should have 1 warning for missing.md
        assert_eq!(result.len(), 1, "Should warn about missing nav entry. Got: {result:?}");
        assert!(result[0].message.contains("missing.md"));
    }

    #[test]
    fn test_absolute_links_validation() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        let mkdocs_content = r#"
site_name: Test
docs_dir: docs
nav:
  - Absolute: /absolute/path.md
"#;
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        let test_file = docs_dir.join("test.md");
        fs::write(&test_file, "# Test").unwrap();

        let config = MD074Config {
            not_found: NavValidation::Ignore,
            omitted_files: NavValidation::Ignore,
            absolute_links: NavValidation::Warn,
        };
        let rule = MD074MkDocsNav::from_config_struct(config);

        let ctx =
            crate::lint_context::LintContext::new("# Test", crate::config::MarkdownFlavor::MkDocs, Some(test_file));

        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Should warn about absolute path. Got: {result:?}");
        assert!(result[0].message.contains("Absolute path"));
    }

    #[test]
    fn test_omitted_files_validation() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        let mkdocs_content = r#"
site_name: Test
docs_dir: docs
nav:
  - Home: index.md
"#;
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("index.md"), "# Home").unwrap();
        fs::write(docs_dir.join("unlisted.md"), "# Unlisted").unwrap();

        // Create subdirectory with file
        let subdir = docs_dir.join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("nested.md"), "# Nested").unwrap();

        let test_file = docs_dir.join("test.md");
        fs::write(&test_file, "# Test").unwrap();

        let config = MD074Config {
            not_found: NavValidation::Ignore,
            omitted_files: NavValidation::Warn,
            absolute_links: NavValidation::Ignore,
        };
        let rule = MD074MkDocsNav::from_config_struct(config);

        let ctx =
            crate::lint_context::LintContext::new("# Test", crate::config::MarkdownFlavor::MkDocs, Some(test_file));

        let result = rule.check(&ctx).unwrap();

        // Should warn about unlisted.md, test.md, and subdir/nested.md
        // (index.md in root is skipped)
        assert!(result.len() >= 2, "Should warn about omitted files. Got: {result:?}");

        let messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("unlisted.md")),
            "Should mention unlisted.md"
        );
    }

    #[test]
    fn test_omitted_files_with_subdirectories() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        let mkdocs_content = r#"
site_name: Test
docs_dir: docs
nav:
  - Home: index.md
  - API:
    - Overview: api/overview.md
"#;
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("index.md"), "# Home").unwrap();

        let api_dir = docs_dir.join("api");
        fs::create_dir_all(&api_dir).unwrap();
        fs::write(api_dir.join("overview.md"), "# Overview").unwrap();
        fs::write(api_dir.join("unlisted.md"), "# Unlisted API").unwrap();

        let test_file = docs_dir.join("index.md");

        let config = MD074Config {
            not_found: NavValidation::Warn,
            omitted_files: NavValidation::Warn,
            absolute_links: NavValidation::Ignore,
        };
        let rule = MD074MkDocsNav::from_config_struct(config);

        let ctx =
            crate::lint_context::LintContext::new("# Home", crate::config::MarkdownFlavor::MkDocs, Some(test_file));

        let result = rule.check(&ctx).unwrap();

        // Should only warn about api/unlisted.md, not api/overview.md
        let messages: Vec<&str> = result.iter().map(|w| w.message.as_str()).collect();

        // api/overview.md should NOT be reported (it's in nav)
        assert!(
            !messages.iter().any(|m| m.contains("overview.md")),
            "Should NOT warn about api/overview.md (it's in nav). Got: {messages:?}"
        );

        // api/unlisted.md SHOULD be reported
        assert!(
            messages.iter().any(|m| m.contains("unlisted.md")),
            "Should warn about api/unlisted.md. Got: {messages:?}"
        );
    }

    #[test]
    fn test_skips_non_mkdocs_flavor() {
        setup_test();
        let rule = MD074MkDocsNav::new();
        let ctx = crate::lint_context::LintContext::new("# Test", crate::config::MarkdownFlavor::Standard, None);

        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Should skip non-MkDocs flavor");
    }

    #[test]
    fn test_skips_external_urls_in_validation() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        let mkdocs_content = r#"
site_name: Test
docs_dir: docs
nav:
  - Home: index.md
  - GitHub: https://github.com/example
  - Docs: http://docs.example.com
"#;
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("index.md"), "# Home").unwrap();

        let test_file = docs_dir.join("index.md");

        let rule = MD074MkDocsNav::new();
        let ctx =
            crate::lint_context::LintContext::new("# Home", crate::config::MarkdownFlavor::MkDocs, Some(test_file));

        let result = rule.check(&ctx).unwrap();

        // Should NOT warn about external URLs as missing files
        assert!(
            result.is_empty(),
            "Should not warn about external URLs. Got: {result:?}"
        );
    }

    #[test]
    fn test_cache_prevents_duplicate_validation() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        let mkdocs_content = r#"
site_name: Test
docs_dir: docs
nav:
  - Home: index.md
  - Missing: missing.md
"#;
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("index.md"), "# Home").unwrap();
        fs::write(docs_dir.join("other.md"), "# Other").unwrap();

        let rule = MD074MkDocsNav::new();

        // First file check
        let ctx1 = crate::lint_context::LintContext::new(
            "# Home",
            crate::config::MarkdownFlavor::MkDocs,
            Some(docs_dir.join("index.md")),
        );
        let result1 = rule.check(&ctx1).unwrap();
        assert_eq!(result1.len(), 1, "First check should return warnings");

        // Second file check - same project
        let ctx2 = crate::lint_context::LintContext::new(
            "# Other",
            crate::config::MarkdownFlavor::MkDocs,
            Some(docs_dir.join("other.md")),
        );
        let result2 = rule.check(&ctx2).unwrap();
        assert!(result2.is_empty(), "Second check should return no warnings (cached)");
    }

    #[test]
    fn test_invalid_mkdocs_yml_returns_warning() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        // Invalid YAML
        let mkdocs_content = "site_name: Test\nnav: [[[invalid yaml";
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        let test_file = docs_dir.join("test.md");
        fs::write(&test_file, "# Test").unwrap();

        let rule = MD074MkDocsNav::new();
        let ctx =
            crate::lint_context::LintContext::new("# Test", crate::config::MarkdownFlavor::MkDocs, Some(test_file));

        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Should return parse error warning");
        assert!(
            result[0].message.contains("Failed to parse"),
            "Should mention parse failure"
        );
    }

    #[test]
    fn test_missing_docs_dir_returns_warning() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        let mkdocs_content = r#"
site_name: Test
docs_dir: nonexistent
nav:
  - Home: index.md
"#;
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        // Create a file but not in the docs_dir
        let other_dir = temp_dir.path().join("other");
        fs::create_dir_all(&other_dir).unwrap();
        let test_file = other_dir.join("test.md");
        fs::write(&test_file, "# Test").unwrap();

        let rule = MD074MkDocsNav::new();
        let ctx =
            crate::lint_context::LintContext::new("# Test", crate::config::MarkdownFlavor::MkDocs, Some(test_file));

        let result = rule.check(&ctx).unwrap();

        assert_eq!(result.len(), 1, "Should warn about missing docs_dir");
        assert!(
            result[0].message.contains("does not exist"),
            "Should mention docs_dir doesn't exist"
        );
    }

    #[test]
    fn test_default_docs_dir() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        // mkdocs.yml without docs_dir specified - should default to "docs"
        let mkdocs_content = r#"
site_name: Test
nav:
  - Home: index.md
"#;
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        let config = MD074MkDocsNav::parse_mkdocs_yml(&temp_dir.path().join("mkdocs.yml")).unwrap();
        assert_eq!(config.docs_dir, "docs", "Should default to 'docs'");
    }

    #[test]
    fn test_path_normalization() {
        // Test that paths are normalized consistently
        let path1 = MD074MkDocsNav::normalize_path(Path::new("api/overview.md"));
        let path2 = MD074MkDocsNav::normalize_nav_path("api/overview.md");
        assert_eq!(path1, path2);

        // Windows-style paths should be normalized
        let win_path = MD074MkDocsNav::normalize_nav_path("api\\overview.md");
        assert_eq!(win_path, PathBuf::from("api/overview.md"));
    }

    #[test]
    fn test_skips_hidden_files_and_directories() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        let mkdocs_content = r#"
site_name: Test
docs_dir: docs
nav:
  - Home: index.md
"#;
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("index.md"), "# Home").unwrap();

        // Create hidden file and directory
        fs::write(docs_dir.join(".hidden.md"), "# Hidden").unwrap();
        let hidden_dir = docs_dir.join(".hidden_dir");
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("secret.md"), "# Secret").unwrap();

        let collected = MD074MkDocsNav::collect_docs_files(&docs_dir);

        assert_eq!(collected.len(), 1, "Should only find index.md");
        assert!(
            !collected.iter().any(|p| p.to_string_lossy().contains("hidden")),
            "Should not include hidden files"
        );
    }

    #[test]
    fn test_is_external_url() {
        assert!(MD074MkDocsNav::is_external_url("https://example.com"));
        assert!(MD074MkDocsNav::is_external_url("http://example.com"));
        assert!(MD074MkDocsNav::is_external_url("//example.com"));
        assert!(MD074MkDocsNav::is_external_url("ftp://files.example.com"));
        assert!(!MD074MkDocsNav::is_external_url("index.md"));
        assert!(!MD074MkDocsNav::is_external_url("path/to/file.md"));
        assert!(!MD074MkDocsNav::is_external_url("/absolute/path.md"));
    }

    #[test]
    fn test_is_absolute_path() {
        assert!(MD074MkDocsNav::is_absolute_path("/absolute/path.md"));
        assert!(MD074MkDocsNav::is_absolute_path("/index.md"));
        assert!(!MD074MkDocsNav::is_absolute_path("relative/path.md"));
        assert!(!MD074MkDocsNav::is_absolute_path("index.md"));
        assert!(!MD074MkDocsNav::is_absolute_path("https://example.com"));
    }

    #[test]
    fn test_directory_nav_entries() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        // Nav with directory entry (trailing slash)
        let mkdocs_content = r#"
site_name: Test
docs_dir: docs
nav:
  - Home: index.md
  - API: api/
"#;
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("index.md"), "# Home").unwrap();

        // Create api directory WITHOUT index.md
        let api_dir = docs_dir.join("api");
        fs::create_dir_all(&api_dir).unwrap();

        let test_file = docs_dir.join("index.md");

        let rule = MD074MkDocsNav::new();
        let ctx =
            crate::lint_context::LintContext::new("# Home", crate::config::MarkdownFlavor::MkDocs, Some(test_file));

        let result = rule.check(&ctx).unwrap();

        // Should warn that api/index.md doesn't exist
        assert_eq!(
            result.len(),
            1,
            "Should warn about missing api/index.md. Got: {result:?}"
        );
        assert!(result[0].message.contains("api/"), "Should mention api/ in warning");
        assert!(
            result[0].message.contains("index.md"),
            "Should mention index.md in warning"
        );
    }

    #[test]
    fn test_directory_nav_entries_with_index() {
        setup_test();
        let temp_dir = tempdir().unwrap();

        // Nav with directory entry (trailing slash)
        let mkdocs_content = r#"
site_name: Test
docs_dir: docs
nav:
  - Home: index.md
  - API: api/
"#;
        fs::write(temp_dir.path().join("mkdocs.yml"), mkdocs_content).unwrap();

        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("index.md"), "# Home").unwrap();

        // Create api directory WITH index.md
        let api_dir = docs_dir.join("api");
        fs::create_dir_all(&api_dir).unwrap();
        fs::write(api_dir.join("index.md"), "# API").unwrap();

        let test_file = docs_dir.join("index.md");

        let rule = MD074MkDocsNav::new();
        let ctx =
            crate::lint_context::LintContext::new("# Home", crate::config::MarkdownFlavor::MkDocs, Some(test_file));

        let result = rule.check(&ctx).unwrap();

        // Should not warn - api/index.md exists
        assert!(
            result.is_empty(),
            "Should not warn when api/index.md exists. Got: {result:?}"
        );
    }
}
