//! Shared MkDocs configuration utilities.
//!
//! Provides discovery and parsing of mkdocs.yml/mkdocs.yaml files,
//! with caching for efficient repeated lookups.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

/// Cache: canonicalized mkdocs.yml path -> resolved docs_dir (absolute)
static DOCS_DIR_CACHE: LazyLock<Mutex<HashMap<PathBuf, PathBuf>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Find mkdocs.yml or mkdocs.yaml by walking up from `start_path`.
///
/// Returns the canonicalized path to the mkdocs config file, or None if not found.
pub fn find_mkdocs_yml(start_path: &Path) -> Option<PathBuf> {
    let mut current = if start_path.is_file() {
        start_path.parent()?.to_path_buf()
    } else {
        start_path.to_path_buf()
    };

    loop {
        for filename in &["mkdocs.yml", "mkdocs.yaml"] {
            let mkdocs_path = current.join(filename);
            if mkdocs_path.exists() {
                return mkdocs_path.canonicalize().ok();
            }
        }

        if !current.pop() {
            break;
        }
    }

    None
}

/// Minimal mkdocs.yml structure for extracting docs_dir.
#[derive(Debug, Deserialize)]
struct MkDocsYmlPartial {
    #[serde(default = "default_docs_dir")]
    docs_dir: String,
}

fn default_docs_dir() -> String {
    "docs".to_string()
}

/// Resolve the `docs_dir` for a project by finding and parsing mkdocs.yml.
///
/// Results are cached by the canonicalized mkdocs.yml path to avoid
/// repeated filesystem operations and YAML parsing.
///
/// `start_path` should be the markdown file being checked or its parent directory.
/// Returns the absolute path to the docs directory, or None if no mkdocs.yml is found.
pub fn resolve_docs_dir(start_path: &Path) -> Option<PathBuf> {
    let mkdocs_path = find_mkdocs_yml(start_path)?;

    // Check cache first
    if let Ok(cache) = DOCS_DIR_CACHE.lock()
        && let Some(docs_dir) = cache.get(&mkdocs_path)
    {
        return Some(docs_dir.clone());
    }

    // Parse mkdocs.yml to get docs_dir
    let content = std::fs::read_to_string(&mkdocs_path).ok()?;
    let config: MkDocsYmlPartial = serde_yml::from_str(&content).ok()?;

    // Resolve docs_dir relative to mkdocs.yml location
    let mkdocs_dir = mkdocs_path.parent()?;
    let docs_dir = if Path::new(&config.docs_dir).is_absolute() {
        PathBuf::from(&config.docs_dir)
    } else {
        mkdocs_dir.join(&config.docs_dir)
    };

    // Only cache if the docs_dir actually exists
    if docs_dir.exists()
        && let Ok(mut cache) = DOCS_DIR_CACHE.lock()
    {
        cache.insert(mkdocs_path, docs_dir.clone());
    }

    Some(docs_dir)
}

/// Clear the docs_dir cache. Useful for testing.
#[cfg(test)]
pub fn clear_docs_dir_cache() {
    if let Ok(mut cache) = DOCS_DIR_CACHE.lock() {
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_find_mkdocs_yml() {
        let temp_dir = tempdir().unwrap();
        let mkdocs_path = temp_dir.path().join("mkdocs.yml");
        fs::write(&mkdocs_path, "site_name: test\n").unwrap();

        let sub_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&sub_dir).unwrap();

        let result = find_mkdocs_yml(&sub_dir);
        assert!(result.is_some());
    }

    #[test]
    fn test_find_mkdocs_yaml_extension() {
        let temp_dir = tempdir().unwrap();
        let mkdocs_path = temp_dir.path().join("mkdocs.yaml");
        fs::write(&mkdocs_path, "site_name: test\n").unwrap();

        let result = find_mkdocs_yml(temp_dir.path());
        assert!(result.is_some());
    }

    #[test]
    fn test_resolve_docs_dir_default() {
        clear_docs_dir_cache();
        let temp_dir = tempdir().unwrap();
        let mkdocs_path = temp_dir.path().join("mkdocs.yml");
        fs::write(&mkdocs_path, "site_name: test\n").unwrap();

        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        let result = resolve_docs_dir(temp_dir.path());
        assert!(result.is_some());
        let result_path = result.unwrap();
        assert!(result_path.ends_with("docs"));
    }

    #[test]
    fn test_resolve_docs_dir_custom() {
        clear_docs_dir_cache();
        let temp_dir = tempdir().unwrap();
        let mkdocs_path = temp_dir.path().join("mkdocs.yml");
        fs::write(&mkdocs_path, "site_name: test\ndocs_dir: documentation\n").unwrap();

        let docs_dir = temp_dir.path().join("documentation");
        fs::create_dir_all(&docs_dir).unwrap();

        let result = resolve_docs_dir(temp_dir.path());
        assert!(result.is_some());
        let result_path = result.unwrap();
        assert!(result_path.ends_with("documentation"));
    }

    #[test]
    fn test_resolve_docs_dir_no_mkdocs_yml() {
        clear_docs_dir_cache();
        let temp_dir = tempdir().unwrap();
        let result = resolve_docs_dir(temp_dir.path());
        assert!(result.is_none());
    }
}
