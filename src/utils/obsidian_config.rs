//! Obsidian vault configuration utilities.
//!
//! Provides discovery and parsing of `.obsidian/app.json` files,
//! with caching for efficient repeated lookups.
//!
//! Mirrors the pattern used by `mkdocs_config.rs` for MkDocs projects.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

/// Cache: canonicalized vault root -> resolved attachment folder (absolute)
static ATTACHMENT_DIR_CACHE: LazyLock<Mutex<HashMap<PathBuf, AttachmentResolution>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Result of resolving the Obsidian attachment folder configuration.
#[derive(Debug, Clone)]
pub enum AttachmentResolution {
    /// Absolute path to a fixed attachment folder (vault root or named folder)
    Fixed(PathBuf),
    /// Relative to each file's directory (`./ ` prefix in config)
    RelativeToFile(String),
}

/// Minimal `.obsidian/app.json` structure for extracting attachment settings.
#[derive(Debug, Deserialize)]
struct ObsidianAppConfig {
    #[serde(default, rename = "attachmentFolderPath")]
    attachment_folder_path: String,
}

/// Find an Obsidian vault root by walking up from `start_path`.
///
/// Returns the vault root directory (parent of `.obsidian/`), or None if not found.
pub fn find_obsidian_vault(start_path: &Path) -> Option<PathBuf> {
    let mut current = if start_path.is_file() {
        start_path.parent()?.to_path_buf()
    } else {
        start_path.to_path_buf()
    };

    loop {
        let obsidian_dir = current.join(".obsidian");
        if obsidian_dir.is_dir() {
            return current.canonicalize().ok();
        }

        if !current.pop() {
            break;
        }
    }

    None
}

/// Resolve the attachment folder for a given file in an Obsidian vault.
///
/// Reads `.obsidian/app.json` to determine the `attachmentFolderPath` setting:
/// - `""` (empty/absent): vault root
/// - `"FolderName"`: `<vault-root>/FolderName/`
/// - `"./"`: same folder as current file
/// - `"./subfolder"`: `<file-dir>/subfolder/`
///
/// Results are cached by vault root path.
///
/// `start_path` should be the markdown file being checked or its parent directory.
/// `file_dir` is the directory containing the file being checked (for `./` resolution).
///
/// Returns the absolute path to the attachment folder, or None if no vault is found.
pub fn resolve_attachment_folder(start_path: &Path, file_dir: &Path) -> Option<PathBuf> {
    let vault_root = find_obsidian_vault(start_path)?;

    // Check cache first
    if let Ok(cache) = ATTACHMENT_DIR_CACHE.lock() {
        if let Some(resolution) = cache.get(&vault_root) {
            return Some(match resolution {
                AttachmentResolution::Fixed(path) => path.clone(),
                AttachmentResolution::RelativeToFile(subfolder) => {
                    if subfolder.is_empty() {
                        file_dir.to_path_buf()
                    } else {
                        file_dir.join(subfolder)
                    }
                }
            });
        }
    }

    // Parse .obsidian/app.json
    let app_json_path = vault_root.join(".obsidian").join("app.json");
    let attachment_folder_path = if app_json_path.exists() {
        std::fs::read_to_string(&app_json_path)
            .ok()
            .and_then(|content| serde_json::from_str::<ObsidianAppConfig>(&content).ok())
            .map(|config| config.attachment_folder_path)
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Resolve and cache
    let resolution = if attachment_folder_path.is_empty() {
        AttachmentResolution::Fixed(vault_root.clone())
    } else if let Some(relative) = attachment_folder_path.strip_prefix("./") {
        AttachmentResolution::RelativeToFile(relative.to_string())
    } else {
        AttachmentResolution::Fixed(vault_root.join(&attachment_folder_path))
    };

    let result = match &resolution {
        AttachmentResolution::Fixed(path) => path.clone(),
        AttachmentResolution::RelativeToFile(subfolder) => {
            if subfolder.is_empty() {
                file_dir.to_path_buf()
            } else {
                file_dir.join(subfolder)
            }
        }
    };

    if let Ok(mut cache) = ATTACHMENT_DIR_CACHE.lock() {
        cache.insert(vault_root, resolution);
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_find_obsidian_vault() {
        let temp = tempdir().unwrap();
        let vault = temp.path().join("my-vault");
        fs::create_dir_all(vault.join(".obsidian")).unwrap();
        fs::create_dir_all(vault.join("notes/subfolder")).unwrap();

        // From a file in the vault root
        let result = find_obsidian_vault(&vault.join("test.md"));
        assert!(result.is_some());

        // From a nested subfolder
        let result = find_obsidian_vault(&vault.join("notes/subfolder/deep.md"));
        assert!(result.is_some());

        // From outside the vault
        let result = find_obsidian_vault(temp.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_attachment_folder_vault_root() {
        let temp = tempdir().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir_all(vault.join(".obsidian")).unwrap();
        fs::write(vault.join(".obsidian/app.json"), r#"{"attachmentFolderPath": ""}"#).unwrap();

        let file_dir = vault.join("notes");
        fs::create_dir_all(&file_dir).unwrap();

        let result = resolve_attachment_folder(&file_dir.join("test.md"), &file_dir);
        assert!(result.is_some());
        let resolved = result.unwrap();
        assert_eq!(resolved.canonicalize().unwrap(), vault.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_attachment_folder_named_folder() {
        let temp = tempdir().unwrap();
        let vault = temp.path().join("vault2");
        fs::create_dir_all(vault.join(".obsidian")).unwrap();
        fs::create_dir_all(vault.join("Attachments")).unwrap();
        fs::write(
            vault.join(".obsidian/app.json"),
            r#"{"attachmentFolderPath": "Attachments"}"#,
        )
        .unwrap();

        let file_dir = vault.join("notes");
        fs::create_dir_all(&file_dir).unwrap();

        let result = resolve_attachment_folder(&file_dir.join("test.md"), &file_dir);
        assert!(result.is_some());
        let resolved = result.unwrap();
        assert!(resolved.ends_with("Attachments"));
    }

    #[test]
    fn test_resolve_attachment_folder_relative_to_file() {
        let temp = tempdir().unwrap();
        let vault = temp.path().join("vault3");
        fs::create_dir_all(vault.join(".obsidian")).unwrap();
        fs::write(vault.join(".obsidian/app.json"), r#"{"attachmentFolderPath": "./"}"#).unwrap();

        let file_dir = vault.join("notes");
        fs::create_dir_all(&file_dir).unwrap();

        let result = resolve_attachment_folder(&file_dir.join("test.md"), &file_dir);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), file_dir);
    }

    #[test]
    fn test_resolve_attachment_folder_subfolder_under_file() {
        let temp = tempdir().unwrap();
        let vault = temp.path().join("vault4");
        fs::create_dir_all(vault.join(".obsidian")).unwrap();
        fs::write(
            vault.join(".obsidian/app.json"),
            r#"{"attachmentFolderPath": "./assets"}"#,
        )
        .unwrap();

        let file_dir = vault.join("notes");
        fs::create_dir_all(&file_dir).unwrap();

        let result = resolve_attachment_folder(&file_dir.join("test.md"), &file_dir);
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("assets"));
    }

    #[test]
    fn test_resolve_attachment_folder_no_app_json() {
        let temp = tempdir().unwrap();
        let vault = temp.path().join("vault5");
        fs::create_dir_all(vault.join(".obsidian")).unwrap();
        // No app.json - should default to vault root

        let result = resolve_attachment_folder(&vault.join("test.md"), &vault);
        assert!(result.is_some());
        let resolved = result.unwrap();
        assert_eq!(resolved.canonicalize().unwrap(), vault.canonicalize().unwrap());
    }

    #[test]
    fn test_no_vault_returns_none() {
        let temp = tempdir().unwrap();
        let result = resolve_attachment_folder(&temp.path().join("test.md"), temp.path());
        assert!(result.is_none());
    }
}
